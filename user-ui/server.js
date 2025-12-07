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
    : ['http://10.40.45.27:3000', 'http://10.10.36.216:3000', 'http://10.40.54.163:3000'];

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

// Broadcast request to all servers, return first successful response
async function broadcastRequest(path, options = {}) {
    const promises = serverEndpoints.map(async (server) => {
        try {
            const url = `${server}${path}`;
            const response = await axios({ url, ...options, timeout: 3000 });
            return response;
        } catch (e) {
            return null;
        }
    });

    const results = await Promise.all(promises);
    const successResponse = results.find(r => r !== null);

    if (successResponse) {
        return successResponse;
    }

    throw new Error('No server available');
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

        // Upload thumbnail to server
        try {
            const FormData = require('form-data');
            const formData = new FormData();
            formData.append('image', thumbnailBuffer, {
                filename: originalName,
                contentType: 'image/png'
            });

            const response = await broadcastRequest(`/upload_image/${username}`, {
                method: 'POST',
                data: formData,
                headers: formData.getHeaders()
            });

            console.log(`✅ Upload complete: original + thumbnail saved locally, thumbnail uploaded to server`);
        } catch (e) {
            console.warn('Server upload failed, but local save succeeded:', e.message);
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

        // Return LOCAL thumbnail images (128x128) for fast display
        const userDir = path.join(__dirname, 'data', username, 'images');
        let localThumbnails = [];
        try {
            const files = await fs.readdir(userDir);
            // Only thumbnails for fast loading in browse view
            localThumbnails = files.filter(f => f.includes('-thumb-') && f.match(/\.(png|jpg|jpeg|webp)$/i));
        } catch (e) {
            localThumbnails = [];
        }

        res.json({ images: localThumbnails, count: localThumbnails.length });

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

        // Fallback to server if local not found
        try {
            const response = await broadcastRequest(`/image/${username}/${filename}`, {
                method: 'GET',
                responseType: 'arraybuffer'
            });

            res.set('Content-Type', 'image/png');
            res.send(Buffer.from(response.data));
        } catch (e) {
            res.status(404).json({ error: 'Image not found' });
        }

    } catch (e) {
        console.error('Image serve error:', e.message);
        res.status(404).json({ error: 'Image not found' });
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
        const userDir = path.join(__dirname, 'data', username, 'requests');
        await fs.mkdir(userDir, { recursive: true });

        const request = {
            from: fromUser,
            image: image,
            timestamp: Date.now()
        };

        const requestFile = path.join(userDir, `${Date.now()}-${fromUser}.json`);
        await fs.writeFile(requestFile, JSON.stringify(request, null, 2));

        console.log(`📬 Request saved locally: ${fromUser} -> ${username} for ${image}`);

        // BROADCAST to all servers to ensure delivery
        try {
            await broadcastRequest('/request-notification', {
                method: 'POST',
                data: { to: username, from: fromUser, image },
                headers: { 'Content-Type': 'application/json' }
            });
            console.log(`📡 Request broadcasted to all servers`);
        } catch (e) {
            console.warn('Server broadcast failed:', e.message);
        }

        res.json({ success: true, message: 'Request sent' });
    } catch (e) {
        console.error('Request send error:', e);
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

        const originalImagePath = path.join(__dirname, 'data', approver, 'images', requestedImage);
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

        const requesterViewableDir = path.join(__dirname, 'data', requester, 'viewable');
        await fs.mkdir(requesterViewableDir, { recursive: true });

        const stegFilename = `steg-${Date.now()}.png`;
        const stegImagePath = path.join(requesterViewableDir, stegFilename);
        await fs.writeFile(stegImagePath, stegImageBuffer);

        const metadataPath = path.join(requesterViewableDir, `${stegFilename}.json`);
        await fs.writeFile(metadataPath, JSON.stringify(metadata, null, 2));

        await fs.unlink(requestPath);
        await fs.unlink(coverTempPath).catch(() => { });

        console.log(`✅ Approved: ${approver} -> ${requester}, image: ${requestedImage}, views: ${viewCount}`);

        res.json({ success: true, message: 'Image approved and encrypted' });

    } catch (e) {
        console.error('Approval error:', e);
        res.status(500).json({ error: e.message });
    }
})

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
