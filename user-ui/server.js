const express = require('express');
const session = require('express-session');
const axios = require('axios');
const multer = require('multer');
const sharp = require('sharp');
const path = require('path');
const fs = require('fs').promises;

const app = express();
const PORT = process.env.PORT || 5000;

// Server endpoints (default to remote server)
const serverEndpoints = process.env.SERVER_ENDPOINTS
    ? process.env.SERVER_ENDPOINTS.split(',')
    : ['http://10.40.45.27:3000'];

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
    for (const server of serverEndpoints) {
        try {
            const response = await axios.get(`${server}/`, { timeout: 3000 });
            if (response.data.is_leader) {
                return server;
            }
        } catch (e) {
            continue;
        }
    }
    // Return first available if no leader found
    for (const server of serverEndpoints) {
        try {
            await axios.get(`${server}/`, { timeout: 3000 });
            return server;
        } catch (e) {
            continue;
        }
    }
    return null;
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
    // Stop existing heartbeat if any
    stopHeartbeat(username);

    const interval = setInterval(async () => {
        try {
            const leader = await findLeader();
            if (leader) {
                await axios.post(`${leader}/heartbeat`, { username }, { timeout: 5000 });
                console.log(`💓 Heartbeat sent for ${username}`);
            }
        } catch (e) {
            console.error(`Failed to send heartbeat for ${username}:`, e.message);
        }
    }, 10000); // Every 10 seconds

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

const CLIENT_IP = process.env.CLIENT_IP || '10.40.44.139';
const CLIENT_PORT = parseInt(process.env.PORT) || 8000;

app.post('/api/register', async (req, res) => {
    const { username } = req.body;

    if (!username) {
        return res.status(400).json({ error: 'Username required' });
    }

    try {
        const leader = await findLeader();
        if (!leader) {
            return res.status(503).json({ error: 'No servers available' });
        }

        console.log(`📝 Registering ${username} at ${CLIENT_IP}:${CLIENT_PORT} via ${leader}`);

        const response = await axios.post(`${leader}/register`, {
            username,
            ip: CLIENT_IP,
            port: CLIENT_PORT
        }, { timeout: 5000 });

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
        // Check if user is registered
        const leader = await findLeader();
        if (!leader) {
            return res.status(503).json({ error: 'No servers available' });
        }

        const usersResponse = await axios.get(`${leader}/users`, { timeout: 5000 });
        const user = usersResponse.data.users.find(u => u.username === username);

        if (!user) {
            return res.status(404).json({ error: 'User not registered. Please register first.' });
        }

        // Construct address from ip:port (server stores them separately)
        const userAddr = `${user.ip}:${user.port}`;

        // Setup session
        req.session.username = username;
        req.session.addr = userAddr;

        // Ensure user directory exists
        await ensureUserDirectory(username);

        // Start heartbeat
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
        const leader = await findLeader();
        if (!leader) {
            return res.status(503).json({ error: 'No servers available' });
        }

        const response = await axios.get(`${leader}/images/${req.session.username}`, { timeout: 5000 });
        res.json(response.data);

    } catch (e) {
        if (e.response?.status === 404) {
            res.json({ images: [], count: 0 });
        } else {
            res.status(500).json({ error: e.message });
        }
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
        // Validate and resize to 128x128
        const image = sharp(req.file.buffer);
        const metadata = await image.metadata();

        // Resize to exactly 128x128
        const resizedBuffer = await image
            .resize(128, 128, { fit: 'cover' })
            .png()
            .toBuffer();

        // Upload to server
        const leader = await findLeader();
        if (!leader) {
            return res.status(503).json({ error: 'No servers available' });
        }

        const FormData = require('form-data');
        const formData = new FormData();
        formData.append('image', resizedBuffer, {
            filename: req.file.originalname,
            contentType: 'image/png'
        });

        const response = await axios.post(
            `${leader}/upload_image/${req.session.username}`,
            formData,
            {
                headers: formData.getHeaders(),
                timeout: 10000
            }
        );

        res.json(response.data);

    } catch (e) {
        console.error('Upload error:', e.message);
        res.status(500).json({ error: e.response?.data?.message || e.message });
    }
});

app.get('/api/user-images/:username', async (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    const { username } = req.params;

    try {
        // Try P2P first (future implementation)
        // For now, just use server

        const leader = await findLeader();
        if (!leader) {
            return res.status(503).json({ error: 'No servers available' });
        }

        const response = await axios.get(`${leader}/images/${username}`, { timeout: 5000 });
        res.json(response.data);

    } catch (e) {
        if (e.response?.status === 404) {
            res.json({ images: [], count: 0 });
        } else {
            res.status(500).json({ error: e.message });
        }
    }
});

app.get('/api/image/:username/:filename', async (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    const { username, filename } = req.params;

    try {
        const leader = await findLeader();
        if (!leader) {
            return res.status(503).json({ error: 'No servers available' });
        }

        const response = await axios.get(
            `${leader}/image/${username}/${filename}`,
            { responseType: 'arraybuffer', timeout: 10000 }
        );

        res.set('Content-Type', response.headers['content-type']);
        res.send(response.data);

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
        const leader = await findLeader();
        if (!leader) {
            return res.status(503).json({ error: 'No servers available' });
        }

        const response = await axios.get(`${leader}/discover`, { timeout: 5000 });

        // Filter out current user
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

    // TODO: Implement P2P request sending
    res.json({ success: true, message: 'Request sent (mock)' });
});

app.get('/api/requests', async (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    // TODO: Implement request retrieval
    res.json({ requests: [] });
});

app.post('/api/approve', async (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    // TODO: Implement steganography approval
    res.json({ success: true, message: 'Approved (mock)' });
});

app.get('/api/viewable', async (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    // TODO: Implement viewable images
    res.json({ images: [] });
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
