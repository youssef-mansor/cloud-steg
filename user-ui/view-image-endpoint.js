// Add after viewable endpoint in server.js

app.post('/api/view-image', async (req, res) => {
    if (!req.session.username) {
        return res.status(401).json({ error: 'Not logged in' });
    }

    try {
        const { filename } = req.body;
        const username = req.session.username;

        const stegImagePath = path.join(__dirname, 'data', username, 'viewable', filename);
        const metadataPath = `${stegImagePath}.json`;

        // Load metadata
        const metadata = JSON.parse(await fs.readFile(metadataPath, 'utf8'));
        const { from: sender, originalImage, viewCount } = metadata;

        // Derive key
        const steg = require('./steg');
        const key = steg.deriveKey(sender, username, originalImage);

        // Extract encrypted data
        const encryptedData = await steg.extractDataFromImage(stegImagePath);

        // Decrypt to get original image
        const { imageBuffer, metadata: decryptedMetadata } = steg.decryptData(encryptedData, key);

        // Decrement view count
        const newViewCount = viewCount - 1;

        if (newViewCount <= 0) {
            // Delete steg image and metadata
            await fs.unlink(stegImagePath);
            await fs.unlink(metadataPath);
        } else {
            // Update metadata with new count
            metadata.viewCount = newViewCount;
            await fs.writeFile(metadataPath, JSON.stringify(metadata, null, 2));
        }

        // Return original image
        res.set('Content-Type', 'image/png');
        res.send(imageBuffer);

    } catch (e) {
        console.error('View image error:', e);
        res.status(500).json({ error: e.message });
    }
});
