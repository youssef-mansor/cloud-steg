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

        // Skip server upload - cluster endpoint is too slow/broken (times out >30s)
        // Images are saved locally and will be served via P2P
        console.log(`✅ Upload complete: original + thumbnail saved locally (using P2P for cross-device)`);

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

                const response = await axios.get(`${ownerURL}/p2p-images`, { timeout: 5000 });
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

// P2P endpoint: serve local thumbnails to other devices
app.get('/p2p-images', async (req, res) => {
    try {
        // Find local users and return their thumbnails
        const dataDir = path.join(__dirname, 'data');
        const users = await fs.readdir(dataDir).catch(() => []);

        let allThumbnails = [];
        for (const username of users) {
            const userImagesDir = path.join(dataDir, username, 'images');
            try {
                const files = await fs.readdir(userImagesDir);
                const thumbnails = files.filter(f => f.includes('-thumb-') && f.match(/\.(png|jpg|jpeg|webp)$/i));
                allThumbnails = allThumbnails.concat(thumbnails);
            } catch (e) {
                // Skip if no images folder
            }
        }

        res.json({ images: allThumbnails, count: allThumbnails.length });
    } catch (e) {
        res.status(500).json({ error: e.message });
    }
});

// P2P endpoint: serve individual image file
app.get('/p2p-image/:filename', async (req, res) => {
    try {
        const { filename } = req.params;

        // Find the image in any local user's folder
        const dataDir = path.join(__dirname, 'data');
        const users = await fs.readdir(dataDir).catch(() => []);

        for (const username of users) {
            const imagePath = path.join(dataDir, username, 'images', filename);
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
        console.error('Receive request error:', e);
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

        // Find the actual original file (handle extension mismatch, e.g. .jpg vs .png)
        const imagesDir = path.join(__dirname, 'data', approver, 'images');
        let originalImagePath = path.join(imagesDir, requestedImage);

        // Check if exact file exists
        let exactFileExists = false;
        try {
            await fs.access(originalImagePath);
            exactFileExists = true;
        } catch (e) {
            exactFileExists = false;
        }

        // If exact match fails, try finding by base name
        if (!exactFileExists) {
            const files = await fs.readdir(imagesDir);
            // Extract the timestamp and original filename part regardless of extension
            const baseMatch = requestedImage.match(/^(\d+)-original-(.*)\.[^.]+$/);
            if (baseMatch) {
                const timestamp = baseMatch[1];
                const cleanName = baseMatch[2];

                const foundFile = files.find(f =>
                    f.startsWith(`${timestamp}-original-${cleanName}`)
                );

                if (foundFile) {
                    originalImagePath = path.join(imagesDir, foundFile);
                    console.log(`Found matching original file with different extension: ${foundFile}`);
                } else {
                    return res.status(404).json({ error: 'Original image file not found' });
                }
            } else {
                return res.status(404).json({ error: 'Original image file not found' });
            }
        }

        const originalImageBuffer = await fs.readFile(originalImagePath);

        const metadata = {
            from: approver,
            originalImage: requestedImage,
            viewCount: parseInt(viewCount),
            timestamp: Date.now()
        };

        const steg = require('./steg');
        const key = steg.deriveKey(approver, requester, requestedImage);
        const encryptedData = steg.encryptData(originalImageBuffer, metadata, key);

        const coverTempPath = path.join(__dirname, 'data', 'temp-cover.png');
        await fs.writeFile(coverTempPath, req.file.buffer);

        const stegImageBuffer = await steg.embedDataInImage(coverTempPath, encryptedData);

        // PULL MODEL: Save steg image locally on approver's device
        // The requester will fetch it via P2P when they check their "Viewable" tab

        const servedStegDir = path.join(__dirname, 'data', approver, 'served-steg');
        await fs.mkdir(servedStegDir, { recursive: true });

        // Save using a name that identifies the recipient: recipient-timestamp.png
        const stegFilename = `${requester}-${Date.now()}.png`;
        const stegImagePath = path.join(servedStegDir, stegFilename);

        await fs.writeFile(stegImagePath, stegImageBuffer);

        // Save metadata too
        const metadataPath = path.join(servedStegDir, `${stegFilename}.json`);
        await fs.writeFile(metadataPath, JSON.stringify(metadata, null, 2));

        console.log(`✅ Steg image created and saved for ${requester} (P2P Pull): ${stegFilename}`);

        // Clean up
        await fs.unlink(requestPath);
        await fs.unlink(coverTempPath).catch(() => { });

        console.log(`✅ Approved: ${approver} -> ${requester}, image: ${requestedImage}, views: ${viewCount}`);

        res.json({ success: true, message: 'Request approved, image ready for pickup' });

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

        // Save to first local user's viewable folder
        const dataDir = path.join(__dirname, 'data');
        const users = await fs.readdir(dataDir).catch(() => []);

        if (users.length === 0) {
            console.error('No users found on this device');
            return res.status(404).json({ error: 'No users on this device' });
        }

        for (const username of users) {
            const viewableDir = path.join(dataDir, username, 'viewable');
            await fs.mkdir(viewableDir, { recursive: true });

            const stegFilename = req.file.originalname;
            const stegImagePath = path.join(viewableDir, stegFilename);
            await fs.writeFile(stegImagePath, stegImageBuffer);

            const metadataPath = path.join(viewableDir, `${stegFilename}.json`);
            await fs.writeFile(metadataPath, JSON.stringify(metadata, null, 2));

            console.log(`📨 Steg image received for ${username} from ${metadata.from}`);
            break;
        }

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

// Endpoint to list steg images waiting for a specific user (PULL Model)
app.get('/api/p2p-my-steg/:requester', async (req, res) => {
    try {
        const { requester } = req.params;
        const approver = req.session.username; // Current user is the approver holding the files

        if (!approver) return res.json({ images: [] });

        const servedStegDir = path.join(__dirname, 'data', approver, 'served-steg');

        try {
            const files = await fs.readdir(servedStegDir);
            // Filter files meant for this requester: requester-timestamp.png
            const myImages = files.filter(f => f.startsWith(`${requester}-`) && f.endsWith('.png'));

            // returning metadata is tricky without reading all json files, so we'll just return filenames
            // The client will fetch metadata separately or we infer it
            const imagesWithMeta = [];
            for (const img of myImages) {
                const metaDis = img + '.json';
                const metaPath = path.join(servedStegDir, metaDis);
                let meta = {};
                try {
                    meta = JSON.parse(await fs.readFile(metaPath, 'utf8'));
                } catch (e) { }

                imagesWithMeta.push({
                    filename: img,
                    sender: approver,
                    ...meta
                });
            }
            res.json({ images: imagesWithMeta });
        } catch (e) {
            res.json({ images: [] });
        }
    } catch (e) {
        res.status(500).json({ error: e.message });
    }
});

// Endpoint to serve the actual steg image file
app.get('/api/p2p-steg-download/:filename', async (req, res) => {
    try {
        const { filename } = req.params;
        // Search in all users' served-steg folders (in case multiple users log in same device)
        const dataDir = path.join(__dirname, 'data');
        const users = await fs.readdir(dataDir).catch(() => []);

        for (const user of users) {
            const filePath = path.join(dataDir, user, 'served-steg', filename);
            try {
                await fs.access(filePath);
                return res.sendFile(filePath);
            } catch (e) { }
        }
        res.status(404).json({ error: 'File not found' });
    } catch (e) {
        res.status(500).json({ error: e.message });
    }
});

app.get('/api/viewable', async (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    try {
        const username = req.session.username;
        let allImages = [];

        // 1. Get LOCAL viewable images (legacy/same-device)
        const viewableDir = path.join(__dirname, 'data', username, 'viewable');
        try {
            const files = await fs.readdir(viewableDir);
            const imageFiles = files.filter(f => f.match(/\.(png|jpg|jpeg|webp)$/i));

            for (const file of imageFiles) {
                const metadataPath = path.join(viewableDir, `${file}.json`);
                let metadata = {};
                try {
                    const metadataContent = await fs.readFile(metadataPath, 'utf8');
                    metadata = JSON.parse(metadataContent);
                } catch (e) { }

                allImages.push({
                    filename: file,
                    path: `/viewable-image/${username}/${file}`,
                    ...metadata,
                    source: 'local'
                });
            }
        } catch (e) { }

        // 2. Poll peers for images waiting for me (Pull Model)
        try {
            const response = await broadcastRequest(`/p2p-my-steg/${username}`, { method: 'GET' });
            // Try to aggregate from all responses (if broadcast returns array or we query all servers)
            // simplified: broadcastRequest returns "best" response.

            // Ideally we should query ALL peers. 
            // Since we don't have a "query all" function handy, we rely on the leader or random peer response. 
            // To be robust, the UI should maybe poll known peers. 
            // For now, let's assume the broadcast finds the approver.

            if (response.data && response.data.images) {
                const remoteImages = response.data.images.map(img => ({
                    ...img,
                    // Construct P2P download URL
                    path: `http://${response.from}/api/p2p-steg-download/${img.filename}`,
                    // Note: response.from might need to be the actual peer IP if broadcast proxying logic is used
                    // Actually broadcastRequest returns response.data but doesn't easily set response.from
                    // We need the peer's IP.
                    // Making a hack: ask peer to include their IP in response
                }));
                // Wait, we need the IP. Let's fix the endpoint above to include it or use discovery.
            }
        } catch (e) { }

        // Revised P2P Polling:
        // Get list of online users, query each one directly
        try {
            const usersResp = await broadcastRequest('/users', { method: 'GET' });
            const onlineUsers = usersResp.data.users.filter(u => u.username !== username);

            for (const user of onlineUsers) {
                try {
                    const peerUrl = `http://${user.addr}`;
                    const p2pResp = await axios.get(`${peerUrl}/api/p2p-my-steg/${username}`, { timeout: 2000 });
                    if (p2pResp.data.images && p2pResp.data.images.length > 0) {
                        const p2pImages = p2pResp.data.images.map(img => ({
                            ...img,
                            path: `${peerUrl}/api/p2p-steg-download/${img.filename}`,
                            source: 'p2p'
                        }));
                        allImages = allImages.concat(p2pImages);
                    }
                } catch (e) { }
            }
        } catch (e) { }

        res.json({ images: allImages });
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
