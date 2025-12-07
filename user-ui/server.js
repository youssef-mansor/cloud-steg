const express = require('express');
const session = require('express-session');
const axios = require('axios');
const multer = require('multer');
const sharp = require('sharp');
const path = require('path');
const fs = require('fs').promises;

const app = express();
const PORT = process.env.PORT || 5000;

// Server endpoints (active servers in cluster)
const serverEndpoints = process.env.SERVER_ENDPOINTS
    ? process.env.SERVER_ENDPOINTS.split(',')
    : [
        'http://10.40.45.27:3000',
        'http://10.40.36.216:3000',  // Fixed: was 10.10.36.216
        'http://10.40.54.163:3000'
    ];

// Middleware
app.use(express.json());
app.use(express.static(path.join(__dirname, 'public')));
app.use(session({
    secret: 'cloud-steg-secret',
    resave: false,
    saveUninitialized: false,
    cookie: { maxAge: 24 * 60 * 60 * 1000 } // 24 hours
}));

// Multer for image uploads
const upload = multer({
    storage: multer.memoryStorage(),
    limits: { fileSize: 5 * 1024 * 1024 }, // 5MB max
    fileFilter: (req, file, cb) => {
        if (file.mimetype.startsWith('image/')) {
            cb(null, true);
        } else {
            cb(new Error('Only images allowed'));
        }
    }
});

// Heartbeat tracking
const heartbeatIntervals = new Map();

// ============== Helper Functions ==============

async function findLeader() {
    const promises = serverEndpoints.map(async (server) => {
        try {
            const response = await axios.get(`${server}/`, { timeout: 2000 });
            if (response.data.is_leader) {
                return server;
            }
        } catch (e) {
            return null;
        }
        return null;
    });

    const results = await Promise.all(promises);
    const leader = results.find(s => s !== null);
    return leader || serverEndpoints[0];
}

// Broadcast request to all servers, return the best response (from leader)
async function broadcastRequest(path, options = {}) {
    const promises = serverEndpoints.map(async (server) => {
        try {
            const url = `${server}${path}`;
            const response = await axios({ url, ...options, timeout: 5000 }); // Increased timeout
            return { server, response };
        } catch (e) {
            console.log(`❌ ${server}${path} failed: ${e.message}`);
            return null;
        }
    });

    const results = await Promise.all(promises);
    const successResponses = results.filter(r => r !== null);

    console.log(`📡 Broadcast to ${path}: ${successResponses.length}/${serverEndpoints.length} responded`);

    if (successResponses.length === 0) {
        throw new Error('No server available');
    }

    // Prefer responses with data (from leader)
    const responseWithData = successResponses.find(r => {
        const data = r.response.data;
        const hasData = (data.users && data.users.length > 0) ||
            (data.images && data.images.length > 0) ||
            (data.online_clients && data.online_clients.length > 0);

        if (hasData) {
            console.log(`✅ Using response from ${r.server} with data`);
        }
        return hasData;
    });

    // Return response with data, or first successful response as fallback
    return responseWithData ? responseWithData.response : successResponses[0].response;
}

async function ensureUserDirectory(username) {
    const userDir = path.join(__dirname, 'data', username);
    const dirs = ['images', 'requests', 'viewable'];

    for (const dir of dirs) {
        await fs.mkdir(path.join(userDir, dir), { recursive: true });
    }
    return userDir;
}

function startHeartbeat(username, addr) {
    stopHeartbeat(username);

    const interval = setInterval(async () => {
        try {
            await broadcastRequest('/heartbeat', {
                method: 'POST',
                data: { username, addr },
                headers: { 'Content-Type': 'application/json' }
            });
            console.log(`💓 Heartbeat sent for ${username} at ${addr}`);
        } catch (e) {
            console.error(`Failed to send heartbeat for ${username}:`, e.message);
        }
    }, 10000);

    heartbeatIntervals.set(username, interval);
    console.log(`✅ Heartbeat started for ${username}`);
}

function stopHeartbeat(username) {
    const interval = heartbeatIntervals.get(username);
    if (interval) {
        clearInterval(interval);
        heartbeatIntervals.delete(username);
        console.log(`⏹️  Heartbeat stopped for ${username}`);
    }
}

// ============== Registration ==============

const CLIENT_IP = process.env.CLIENT_IP || '10.40.48.133';
const CLIENT_PORT = parseInt(process.env.PORT) || 8000;

app.post('/api/register', async (req, res) => {
    const { username } = req.body;

    if (!username) {
        return res.status(400).json({ error: 'Username required' });
    }

    try {
        const addr = `${CLIENT_IP}:${CLIENT_PORT}`;
        console.log(`📝 Registering ${username} at ${addr}`);

        const response = await broadcastRequest('/register', {
            method: 'POST',
            data: { username, addr },
            headers: { 'Content-Type': 'application/json' }
        });

        res.json(response.data);

    } catch (e) {
        console.error('Registration error:', e.message);
        const errorMsg = e.response?.data?.error || e.message;
        res.status(500).json({ error: errorMsg });
    }
});

// ============== Session Endpoints ==============

app.post('/api/login', async (req, res) => {
    const { username } = req.body;

    if (!username) {
        return res.status(400).json({ error: 'Username required' });
    }

    try {
        const usersResponse = await broadcastRequest('/users', { method: 'GET' });
        const user = usersResponse.data.users.find(u => u.username === username);

        if (!user) {
            return res.status(404).json({ error: 'User not registered. Please register first.' });
        }

        const userAddr = user.addr;
        req.session.username = username;
        req.session.addr = userAddr;

        await ensureUserDirectory(username);
        startHeartbeat(username, userAddr);

        // Sync view count updates from cluster (for offline period)
        try {
            const notesResponse = await broadcastRequest(`/get_note/${username}`, { method: 'GET' });

            if (notesResponse.data && notesResponse.data.notes && notesResponse.data.notes.length > 0) {
                console.log(`📥 Found ${notesResponse.data.notes.length} pending view count updates for ${username}`);

                for (const note of notesResponse.data.notes) {
                    const { image_filename, view_count_edit } = note;

                    const viewableDir = path.join(__dirname, 'data', username, 'viewable');
                    try {
                        const files = await fs.readdir(viewableDir);

                        for (const file of files) {
                            if (!file.endsWith('.json')) continue;

                            const metadataPath = path.join(viewableDir, file);
                            const metadata = JSON.parse(await fs.readFile(metadataPath, 'utf8'));

                            if (metadata.originalImage === image_filename) {
                                metadata.viewCount = view_count_edit;
                                await fs.writeFile(metadataPath, JSON.stringify(metadata, null, 2));
                                console.log(`✅ Updated view count for ${image_filename} to ${view_count_edit}`);
                                break;
                            }
                        }
                    } catch (e) {
                        console.warn(`Failed to apply view count update for ${image_filename}:`, e.message);
                    }
                }
            }
        } catch (e) {
            console.log('No pending view count updates or cluster unreachable:', e.message);
        }

        res.json({
            success: true,
            username,
            addr: userAddr,
            message: 'Logged in successfully'
        });

    } catch (e) {
        console.error('Login error:', e.message);
        res.status(500).json({ error: e.message });
    }
});

app.post('/api/logout', (req, res) => {
    const username = req.session.username;

    if (username) {
        stopHeartbeat(username);
    }

    req.session.destroy((err) => {
        if (err) {
            return res.status(500).json({ error: 'Failed to logout' });
        }
        res.json({ success: true, message: 'Logged out successfully' });
    });
});

app.get('/api/me', (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    res.json({
        username: req.session.username,
        addr: req.session.addr
    });
});

// ============== Image Endpoints ==============

app.get('/api/my-images', async (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    try {
        const username = req.session.username;

        // Get local ORIGINAL images ONLY (not thumbnails)
        const userDir = path.join(__dirname, 'data', username, 'images');
        let localOriginals = [];
        try {
            const files = await fs.readdir(userDir);
            localOriginals = files.filter(f => f.includes('-original-') && f.match(/\.(png|jpg|jpeg|webp)$/i));
        } catch (e) {
            localOriginals = [];
        }

        // Get server thumbnails
        let serverThumbnails = [];
        try {
            const response = await broadcastRequest(`/images/${username}`, { method: 'GET' });
            serverThumbnails = response.data.images || [];
        } catch (e) {
            serverThumbnails = [];
        }

        res.json({
            local_images: localOriginals,
            server_thumbnails: serverThumbnails,
            local_count: localOriginals.length,
            server_count: serverThumbnails.length
        });

    } catch (e) {
        res.status(500).json({ error: e.message });
    }
});

app.post('/api/upload', upload.single('image'), async (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    if (!req.file) {
        return res.status(400).json({ error: 'No image provided' });
    }

    try {
        const username = req.session.username;
        const timestamp = Date.now();
        const originalName = req.file.originalname;

        // Ensure user directory exists
        const userImagesDir = path.join(__dirname, 'data', username, 'images');
        await fs.mkdir(userImagesDir, { recursive: true });

        // Save ORIGINAL image locally
        const originalFilename = `${timestamp}-original-${originalName}`;
        const originalPath = path.join(userImagesDir, originalFilename);
        await fs.writeFile(originalPath, req.file.buffer);

        // Create 128x128 thumbnail
        const image = sharp(req.file.buffer);
        const thumbnailBuffer = await image
            .resize(128, 128, { fit: 'cover' })
            .png()
            .toBuffer();

        // Save THUMBNAIL locally
        const thumbnailFilename = `${timestamp}-thumb-${originalName.replace(/\.[^.]+$/, '.png')}`;
        const thumbnailPath = path.join(userImagesDir, thumbnailFilename);
        await fs.writeFile(thumbnailPath, thumbnailBuffer);

        // Upload thumbnail to ALL servers (broadcast)
        const uploadPromises = serverEndpoints.map(async (server) => {
            try {
                const FormData = require('form-data');
                const formData = new FormData();
                formData.append('image', thumbnailBuffer, {
                    filename: originalName,
                    contentType: 'image/png'
                });

                const response = await axios.post(`${server}/upload_image/${username}`, formData, {
                    headers: formData.getHeaders(),
                    timeout: 30000
                });
                console.log(`✅ Thumbnail uploaded to ${server}`);
                return { server, success: true };
            } catch (e) {
                console.log(`❌ Failed to upload to ${server}:`, e.message);
                return { server, success: false, error: e.message };
            }
        });

        const results = await Promise.all(uploadPromises);
        const successCount = results.filter(r => r.success).length;

        if (successCount > 0) {
            console.log(`✅ Upload complete: thumbnail uploaded to ${successCount}/${serverEndpoints.length} servers`);
        } else {
            console.warn('⚠️ Server upload failed on all servers, but local save succeeded');
        }

        res.json({
            success: true,
            message: 'Image uploaded successfully',
            original: originalFilename,
            thumbnail: thumbnailFilename
        });

    } catch (e) {
        console.error('Upload error:', e.message);
        res.status(500).json({ error: e.message });
    }
});

app.get('/api/user-images/:username', async (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    try {
        const { username } = req.params;

        // First try LOCAL thumbnail images (for same-device users)
        const userDir = path.join(__dirname, 'data', username, 'images');
        let localThumbnails = [];
        try {
            const files = await fs.readdir(userDir);
            localThumbnails = files.filter(f => f.includes('-thumb-') && f.match(/\.(png|jpg|jpeg|webp)$/i));
        } catch (e) {
            localThumbnails = [];
        }

        // If local thumbnails exist, use them
        if (localThumbnails.length > 0) {
            return res.json({ images: localThumbnails, count: localThumbnails.length });
        }

        // P2P: Fetch thumbnails directly from owner's device
        try {
            // Get owner's address from cluster
            const usersResponse = await broadcastRequest('/users', { method: 'GET' });
            const ownerUser = usersResponse.data.users.find(u => u.username === username);

            if (ownerUser) {
                const ownerURL = `http://${ownerUser.addr}`;
                console.log(`📡 Fetching ${username}'s images from P2P at ${ownerURL}`);

                const response = await axios.get(`${ownerURL}/p2p-images?username=${username}`, { timeout: 5000 });
                if (response.data && response.data.images) {
                    return res.json(response.data);
                }
            }
        } catch (e) {
            console.log(`P2P fetch failed for ${username}:`, e.message);
        }

        // Fallback: try cluster server (though it's usually empty)
        try {
            const response = await broadcastRequest(`/images/${username}`, { method: 'GET' });
            if (response.data && response.data.images) {
                return res.json(response.data);
            }
        } catch (e) {
            console.log(`Failed to get images from server for ${username}:`, e.message);
        }

        res.json({ images: [], count: 0 });

    } catch (e) {
        res.status(500).json({ error: e.message });
    }
});

app.get('/api/image/:username/:filename', async (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    try {
        const { username, filename } = req.params;

        // Always try local first for fast serving
        const localPath = path.join(__dirname, 'data', username, 'images', filename);
        try {
            await fs.access(localPath);
            return res.sendFile(localPath);
        } catch (e) {
            // Not found locally
        }

        // P2P: Fetch from owner's device
        try {
            const usersResponse = await broadcastRequest('/users', { method: 'GET' });
            const ownerUser = usersResponse.data.users.find(u => u.username === username);

            if (ownerUser) {
                const ownerURL = `http://${ownerUser.addr}`;
                console.log(`📡 Fetching ${filename} from ${username}'s device at ${ownerURL}`);

                const response = await axios.get(`${ownerURL}/p2p-image/${filename}`, {
                    responseType: 'arraybuffer',
                    timeout: 5000
                });

                res.set('Content-Type', response.headers['content-type'] || 'image/png');
                return res.send(response.data);
            }
        } catch (e) {
            console.log(`P2P image fetch failed:`, e.message);
        }

        // Last resort: try server
        try {
            const serverUrl = `${serverEndpoints[0]}/image/${username}/${filename}`;
            const response = await axios.get(serverUrl, {
                responseType: 'arraybuffer',
                timeout: 5000
            });
            res.set('Content-Type', response.headers['content-type'] || 'image/png');
            return res.send(response.data);
        } catch (e) {
            // Server also failed
        }

        res.status(404).json({ error: 'Image not found' });
    } catch (e) {
        console.error('Image serve error:', e.message);
        res.status(404).json({ error: 'Image not found' });
    }
});

// P2P endpoint: serve local thumbnails to other devices for a specific user
app.get('/p2p-images', async (req, res) => {
    try {
        const { username } = req.query;

        if (!username) {
            return res.status(400).json({ error: 'Username required' });
        }

        // Return ONLY this user's thumbnails
        const userImagesDir = path.join(__dirname, 'data', username, 'images');
        let thumbnails = [];

        try {
            const files = await fs.readdir(userImagesDir);
            thumbnails = files.filter(f => f.includes('-thumb-') && f.match(/\.(png|jpg|jpeg|webp)$/i));
        } catch (e) {
            // User has no images folder
        }

        res.json({ images: thumbnails, count: thumbnails.length });
    } catch (e) {
        res.status(500).json({ error: e.message });
    }
});

// P2P endpoint: serve individual image file (prioritize thumbnails)
app.get('/p2p-image/:filename', async (req, res) => {
    try {
        const { filename } = req.params;
        const { username } = req.query;

        const dataDir = path.join(__dirname, 'data');

        // If username specified, look only in that user's folder
        const usersToCheck = username ? [username] : await fs.readdir(dataDir).catch(() => []);

        for (const user of usersToCheck) {
            const imagePath = path.join(dataDir, user, 'images', filename);
            try {
                await fs.access(imagePath);
                return res.sendFile(imagePath);
            } catch (e) {
                // Try next user
            }
        }

        res.status(404).json({ error: 'Image not found' });
    } catch (e) {
        res.status(500).json({ error: e.message });
    }
});

// ============== Discovery Endpoints ==============

app.get('/api/discover', async (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    try {
        const response = await broadcastRequest('/discover', { method: 'GET' });

        const filtered = response.data.online_clients.filter(
            client => client.username !== req.session.username
        );

        res.json({
            ...response.data,
            online_clients: filtered,
            count: filtered.length
        });

    } catch (e) {
        res.status(500).json({ error: e.message });
    }
});

// ============== Request Endpoints (Placeholder) ==============

app.post('/api/request-view', async (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    const { username, image } = req.body;
    const fromUser = req.session.username;

    try {
        const request = {
            from: fromUser,
            image: image,
            timestamp: Date.now()
        };

        // Get recipient's address from cluster
        const usersResponse = await broadcastRequest('/users', { method: 'GET' });
        const recipient = usersResponse.data.users.find(u => u.username === username);

        if (!recipient) {
            return res.status(404).json({ error: 'User not found' });
        }

        // Extract recipient's UI server address (their addr is like "10.7.17.14:8000")
        const recipientAddr = recipient.addr;
        const recipientURL = `http://${recipientAddr}`;

        console.log(`📬 Sending request to ${username} at ${recipientURL}`);

        // Send request directly to recipient's UI server
        try {
            await axios.post(`${recipientURL}/receive-request`, {
                to: username,  // ADD: specify who should receive this
                from: fromUser,
                image: image,
                timestamp: request.timestamp
            }, { timeout: 5000 });

            console.log(`✅ Request delivered to ${username}`);
        } catch (e) {
            console.warn(`Failed to deliver to ${username}'s UI, storing locally:`, e.message);

            // Fallback: Save locally for same-device testing
            const userDir = path.join(__dirname, 'data', username, 'requests');
            await fs.mkdir(userDir, { recursive: true });
            const requestFile = path.join(userDir, `${Date.now()}-${fromUser}.json`);
            await fs.writeFile(requestFile, JSON.stringify(request, null, 2));
        }

        res.json({ success: true, message: 'Request sent' });
    } catch (e) {
        console.error('Request send error:', e);
        res.status(500).json({ error: e.message });
    }
});

// Endpoint to receive requests from other UI servers
app.post('/receive-request', async (req, res) => {
    try {
        const { to, from, image, timestamp } = req.body;

        if (!to) {
            return res.status(400).json({ error: 'Recipient username required' });
        }

        // Save to the SPECIFIED user's requests folder
        const requestsDir = path.join(__dirname, 'data', to, 'requests');
        await fs.mkdir(requestsDir, { recursive: true });

        const requestFile = path.join(requestsDir, `${timestamp}-${from}.json`);
        await fs.writeFile(requestFile, JSON.stringify({ from, image, timestamp }, null, 2));
        console.log(`📨 Request received for ${to} from ${from}`);

        res.json({ success: true });
    } catch (e) {
        console.error('Rejection error:', e.message);
        res.status(500).json({ error: e.message });
    }
});

// Update view count for an approved image
app.post('/api/update-view-count', async (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    try {
        const { recipient, image, viewCount } = req.body;
        const sender = req.session.username;

        if (!recipient || !image || !viewCount) {
            return res.status(400).json({ error: 'Missing required fields' });
        }

        console.log(`🔄 Updating view count for ${image} (${recipient}) to ${viewCount}`);

        // Try to update via P2P first
        const usersResponse = await broadcastRequest('/users', { method: 'GET' });
        const recipientUser = usersResponse.data.users.find(u => u.username === recipient);

        let p2pSuccess = false;
        if (recipientUser) {
            const recipientURL = `http://${recipientUser.addr}`;
            try {
                await axios.post(`${recipientURL}/update-local-view-count`, {
                    image,
                    viewCount
                }, { timeout: 5000 });
                console.log(`✅ View count updated via P2P at ${recipientURL}`);
                p2pSuccess = true;
            } catch (e) {
                console.log(`P2P update failed: ${e.message}`);
            }
        } else {
            console.log(`Recipient ${recipient} not online`);
        }

        // Try cluster as backup (non-critical - don't fail if this doesn't work)
        if (!p2pSuccess) {
            try {
                await broadcastRequest('/add_note', {
                    method: 'POST',
                    data: {
                        target_username: recipient,
                        target_image: image,
                        view_count_edit: parseInt(viewCount)
                    }
                });
                console.log(`📝 View count update stored in cluster for ${recipient}`);
            } catch (clusterErr) {
                console.warn(`⚠️ Cluster sync failed (recipient will sync on next login):`, clusterErr.message);
                // Don't fail the request - cluster is just a backup
            }
        }

        if (!p2pSuccess && !recipientUser) {
            // Only fail if recipient doesn't exist at all (neither online nor registered)
            return res.status(404).json({ error: 'Recipient not found. They will get the update when they log in next.' });
        }

        res.json({
            success: true,
            message: p2pSuccess ? 'View count updated immediately' : 'View count will update when recipient logs in'
        });

    } catch (e) {
        console.error('Update view count error:', e.message);
        res.status(500).json({ error: e.message });
    }
});

app.get('/api/requests', async (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    try {
        const username = req.session.username;
        const requestsDir = path.join(__dirname, 'data', username, 'requests');

        let requests = [];
        try {
            const files = await fs.readdir(requestsDir);
            const jsonFiles = files.filter(f => f.endsWith('.json'));

            for (const file of jsonFiles) {
                const filePath = path.join(requestsDir, file);
                const content = await fs.readFile(filePath, 'utf8');
                const request = JSON.parse(content);
                request.id = file;
                requests.push(request);
            }
        } catch (e) {
            requests = [];
        }

        res.json({ requests });
    } catch (e) {
        res.status(500).json({ error: e.message });
    }
});

// P2P endpoint: update view count for local viewable image
app.post('/update-local-view-count', async (req, res) => {
    try {
        const { image, viewCount } = req.body;

        console.log(`🔄 [P2P UPDATE] Received request to update "${image}" to viewCount=${viewCount}`);

        const dataDir = path.join(__dirname, 'data');
        const users = await fs.readdir(dataDir).catch(() => []);

        console.log(`🔍 [P2P UPDATE] Checking ${users.length} local users:`, users);

        for (const username of users) {
            const viewableDir = path.join(dataDir, username, 'viewable');
            try {
                const files = await fs.readdir(viewableDir);
                console.log(`📂 [P2P UPDATE] User ${username} has ${files.length} viewable files`);

                for (const file of files) {
                    if (!file.endsWith('.json')) continue;

                    const metadataPath = path.join(viewableDir, file);
                    const metadata = JSON.parse(await fs.readFile(metadataPath, 'utf8'));

                    console.log(`🔍 [P2P UPDATE] Checking ${file}: originalImage="${metadata.originalImage}"`);

                    // Match by originalImage field
                    if (metadata.originalImage === image) {
                        const oldCount = metadata.viewCount;
                        metadata.viewCount = parseInt(viewCount);
                        await fs.writeFile(metadataPath, JSON.stringify(metadata, null, 2));
                        console.log(`✅ [P2P UPDATE] SUCCESS! Updated ${file} from ${oldCount} to ${viewCount} for user ${username}`);
                        return res.json({ success: true });
                    }
                }
            } catch (e) {
                console.log(`⚠️  [P2P UPDATE] Error checking user ${username}:`, e.message);
            }
        }

        console.warn(`❌ [P2P UPDATE] Image "${image}" NOT FOUND in any viewable folder`);
        res.status(404).json({ error: 'Image not found' });
    } catch (e) {
        console.error('❌ [P2P UPDATE] Error:', e);
        res.status(500).json({ error: e.message });
    }
});

app.post('/api/approve', upload.single('coverImage'), async (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    try {
        const { requestId, viewCount } = req.body;
        const approver = req.session.username;

        if (!req.file || !viewCount || !requestId) {
            return res.status(400).json({ error: 'Missing required fields' });
        }

        const requestPath = path.join(__dirname, 'data', approver, 'requests', requestId);
        const requestData = JSON.parse(await fs.readFile(requestPath, 'utf8'));
        const { from: requester, image: requestedImage } = requestData;

        // Convert thumbnail name to original name if needed
        let imageName = requestedImage;
        if (imageName.includes('-thumb-')) {
            imageName = imageName.replace('-thumb-', '-original-');
        }

        // Find the actual original file (handle extension mismatch, e.g. .jpg vs .png)
        const imagesDir = path.join(__dirname, 'data', approver, 'images');
        let originalImagePath = path.join(imagesDir, imageName);

        // Check if exact file exists
        let exactFileExists = false;
        try {
            await fs.access(originalImagePath);
            exactFileExists = true;
        } catch (e) {
            exactFileExists = false;
        }

        // If exact match fails, try finding by base name (handle different extensions)
        if (!exactFileExists) {
            const files = await fs.readdir(imagesDir);
            // Extract the timestamp and original filename part regardless of extension
            const baseMatch = imageName.match(/^(\d+)-original-(.*)\.[^.]+$/);
            if (baseMatch) {
                const timestamp = baseMatch[1];
                const cleanName = baseMatch[2];

                const foundFile = files.find(f =>
                    f.startsWith(`${timestamp}-original-${cleanName}`)
                );

                if (foundFile) {
                    originalImagePath = path.join(imagesDir, foundFile);
                    imageName = foundFile;
                    console.log(`Found matching original file with different extension: ${foundFile}`);
                } else {
                    console.error(`Original image not found. Requested: ${requestedImage}, Looking for: ${imageName}`);
                    console.error(`Available files in ${imagesDir}:`, files.filter(f => f.includes('-original-')));
                    return res.status(404).json({ error: 'Original image file not found' });
                }
            } else {
                console.error(`Could not parse image name: ${imageName}`);
                return res.status(404).json({ error: 'Original image file not found' });
            }
        }

        const originalImageBuffer = await fs.readFile(originalImagePath);

        const metadata = {
            from: approver,
            to: requester,  // ADD: specify the recipient
            originalImage: imageName,  // Use the actual original image filename
            viewCount: parseInt(viewCount),
            timestamp: Date.now()
        };

        const steg = require('./steg');
        const key = steg.deriveKey(approver, requester, requestedImage);
        const encryptedData = steg.encryptData(originalImageBuffer, metadata, key);

        const coverTempPath = path.join(__dirname, 'data', 'temp-cover.png');
        await fs.writeFile(coverTempPath, req.file.buffer);

        const stegImageBuffer = await steg.embedDataInImage(coverTempPath, encryptedData);

        const stegFilename = `steg-${Date.now()}.png`;

        // Get requester's address and send steg image to their UI
        const usersResponse = await broadcastRequest('/users', { method: 'GET' });
        const requesterUser = usersResponse.data.users.find(u => u.username === requester);
        
        let deliverySuccess = false;

        if (requesterUser) {
            const requesterURL = `http://${requesterUser.addr}`;

            try {
                // Send steg image directly to requester's UI
                const FormData = require('form-data');
                const formData = new FormData();
                formData.append('stegImage', stegImageBuffer, { filename: stegFilename });
                formData.append('metadata', JSON.stringify(metadata));

                await axios.post(`${requesterURL}/receive-steg-image`, formData, {
                    headers: formData.getHeaders(),
                    timeout: 10000
                });

                console.log(`✅ Steg image sent to ${requester} at ${requesterURL}`);
                deliverySuccess = true;
            } catch (e) {
                console.warn(`⚠️ Failed to send to ${requester} via P2P (${requesterURL}):`, e.message);
            }
        } else {
            console.warn(`⚠️ Requester ${requester} not found in cluster`);
        }

        // If P2P delivery failed, store backup locally for later sync
        if (!deliverySuccess) {
            try {
                const requesterViewableDir = path.join(__dirname, 'data', requester, 'viewable');
                await fs.mkdir(requesterViewableDir, { recursive: true });
                const stegImagePath = path.join(requesterViewableDir, stegFilename);
                await fs.writeFile(stegImagePath, stegImageBuffer);
                const metadataPath = path.join(requesterViewableDir, `${stegFilename}.json`);
                await fs.writeFile(metadataPath, JSON.stringify(metadata, null, 2));
                console.log(`✅ Steg image stored in fallback location for ${requester}`);
                deliverySuccess = true;
            } catch (fallbackErr) {
                console.warn(`⚠️ Failed to store in fallback location:`, fallbackErr.message);
            }
        }

        // Always sync view count with cluster for consistency
        try {
            await broadcastRequest('/add_note', {
                method: 'POST',
                data: {
                    target_username: requester,
                    target_image: requestedImage,
                    view_count_edit: parseInt(viewCount)
                }
            });
            console.log(`📝 View count synchronized with cluster for ${requester}`);
        } catch (clusterErr) {
            console.warn(`⚠️ Failed to sync view count with cluster:`, clusterErr.message);
        }

        if (!deliverySuccess) {
            console.warn(`⚠️ WARNING: Image delivery incomplete - requester may retrieve on next login`);
        }

        await fs.unlink(requestPath);
        await fs.unlink(coverTempPath).catch(() => { });

        console.log(`✅ Approved: ${approver} -> ${requester}, image: ${requestedImage}, views: ${viewCount}`);

        res.json({ success: true, message: 'Image approved and encrypted' });

    } catch (e) {
        console.error('Approval error:', e);
        res.status(500).json({ error: e.message });
    }
})

// Endpoint to receive steg images from other UI servers
app.post('/receive-steg-image', upload.single('stegImage'), async (req, res) => {
    try {
        if (!req.file) {
            return res.status(400).json({ error: 'No steg image provided' });
        }

        const metadata = typeof req.body.metadata === 'string'
            ? JSON.parse(req.body.metadata)
            : req.body.metadata;
        const stegImageBuffer = req.file.buffer;

        // The recipient is the one who REQUESTED the image (opposite of metadata.from which is the approver)
        // In the approval flow: approver sends to requester
        // So we need to find who this steg image is FOR based on the cluster registration

        // Since we know metadata.from is the approver, we need to determine the requester
        // The best way is to check which local user exists, or use a 'to' field

        // For now, save to first local user (the owner of this UI instance)
        const dataDir = path.join(__dirname, 'data');
        const users = await fs.readdir(dataDir).catch(() => []);

        if (users.length === 0) {
            console.error('No users found on this device');
            return res.status(404).json({ error: 'No users on this device' });
        }

        // Save to the correct recipient user (from metadata.to field)
        let recipientUsername = null;

        // Check if metadata has 'to' field
        if (metadata.to) {
            // Check if this user exists on this device
            if (users.includes(metadata.to)) {
                recipientUsername = metadata.to;
            }
        }

        // Fallback to first user if recipient not found
        if (!recipientUsername) {
            recipientUsername = users[0];
            console.warn(`Recipient '${metadata.to}' not found on this device, saving to first user: ${recipientUsername}`);
        }

        const viewableDir = path.join(dataDir, recipientUsername, 'viewable');
        await fs.mkdir(viewableDir, { recursive: true });

        const stegFilename = req.file.originalname;
        const stegImagePath = path.join(viewableDir, stegFilename);
        await fs.writeFile(stegImagePath, stegImageBuffer);

        const metadataPath = path.join(viewableDir, `${stegFilename}.json`);
        await fs.writeFile(metadataPath, JSON.stringify(metadata, null, 2));

        console.log(`📨 Steg image received for ${recipientUsername} from ${metadata.from}`);

        res.json({ success: true });
    } catch (e) {
        console.error('Receive steg image error:', e);
        res.status(500).json({ error: e.message });
    }
});

app.post('/api/reject', async (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    try {
        const { requestId } = req.body;
        const username = req.session.username;

        if (!requestId) {
            return res.status(400).json({ error: 'Request ID required' });
        }

        const requestPath = path.join(__dirname, 'data', username, 'requests', requestId);
        await fs.unlink(requestPath);

        console.log(`❌ Rejected request: ${requestId}`);
        res.json({ success: true, message: 'Request rejected' });

    } catch (e) {
        console.error('Reject error:', e);
        res.status(500).json({ error: e.message });
    }
});

app.get('/api/viewable', async (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    try {
        const username = req.session.username;
        const viewableDir = path.join(__dirname, 'data', username, 'viewable');

        let images = [];
        // Safely create viewable directory if it does not exist
        try {
            await fs.mkdir(viewableDir, { recursive: true });
        } catch (e) {
            console.warn(`Failed to ensure viewable directory exists for ${username}:`, e.message);
        }
        // Safely create viewable directory if it doesn't exist
        try {
            await fs.mkdir(viewableDir, { recursive: true });
        } catch (e) {
            console.warn(`Failed to ensure viewable directory exists for ${username}:`, e.message);
        }
        try {
            const files = await fs.readdir(viewableDir);
            const stegFiles = files.filter(f => f.startsWith('steg-') && f.endsWith('.png'));

            for (const file of stegFiles) {
                const metadataPath = path.join(viewableDir, `${file}.json`);
                try {
                    const metadata = JSON.parse(await fs.readFile(metadataPath, 'utf8'));
                    images.push({
                        filename: file,
                        ...metadata
                    });
                } catch (e) {
                    // Metadata missing
                }
            }
        } catch (e) {
            // Directory doesn't exist
        }

        res.json({ images, count: images.length });

    } catch (e) {
        res.status(500).json({ error: e.message });
    }
});

app.post('/api/view-image', async (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    try {
        const { filename } = req.body;
        const username = req.session.username;

        const stegImagePath = path.join(__dirname, 'data', username, 'viewable', filename);
        const metadataPath = `${stegImagePath}.json`;

        const metadata = JSON.parse(await fs.readFile(metadataPath, 'utf8'));
        const { from: sender, originalImage, viewCount } = metadata;

        const steg = require('./steg');
        const key = steg.deriveKey(sender, username, originalImage);

        const encryptedData = await steg.extractDataFromImage(stegImagePath);
        const { imageBuffer, metadata: decryptedMetadata } = steg.decryptData(encryptedData, key);

        const newViewCount = viewCount - 1;

        if (newViewCount <= 0) {
            await fs.unlink(stegImagePath);
            await fs.unlink(metadataPath);
        } else {
            metadata.viewCount = newViewCount;
            await fs.writeFile(metadataPath, JSON.stringify(metadata, null, 2));

            // Sync updated view count back to approver
            try {
                const usersResponse = await broadcastRequest('/users', { method: 'GET' });
                const approverUser = usersResponse.data.users.find(u => u.username === sender);

                let syncSuccess = false;
                if (approverUser) {
                    // Try P2P update to approver
                    const approverURL = `http://${approverUser.addr}`;
                    try {
                        await axios.post(`${approverURL}/sync-approved-view-count`, {
                            recipient: username,
                            image: originalImage,
                            viewCount: newViewCount
                        }, { timeout: 5000 });
                        console.log(`📊 View count synced to approver ${sender} via P2P`);
                        syncSuccess = true;
                    } catch (e) {
                        console.log(`P2P sync to approver failed: ${e.message}`);
                    }
                }

                // Fallback: store in cluster if P2P failed
                if (!syncSuccess) {
                    await broadcastRequest('/add_note', {
                        method: 'POST',
                        data: {
                            target_username: sender,
                            target_image: originalImage,
                            view_count_edit: newViewCount
                        }
                    });
                    console.log(`📝 View count update stored in cluster for approver ${sender}`);
                }
            } catch (e) {
                console.log(`Failed to sync view count to approver: ${e.message}`);
            }
        }

        res.set('Content-Type', 'image/png');
        res.send(imageBuffer);

    } catch (e) {
        console.error('View image error:', e);
        res.status(500).json({ error: e.message });
    }
});

// ============== Start Server ==============

app.listen(PORT, () => {
    console.log(`🚀 User Dashboard running at http://localhost:${PORT}`);
    console.log(`📡 Monitoring servers: ${serverEndpoints.join(', ')}`);
});

// Cleanup on exit
process.on('SIGINT', () => {
    console.log('\n🛑 Shutting down...');
    heartbeatIntervals.forEach((_, username) => stopHeartbeat(username));
    process.exit(0);
});
