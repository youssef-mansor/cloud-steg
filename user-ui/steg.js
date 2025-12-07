const CryptoJS = require('crypto-js');
const sharp = require('sharp');
const fs = require('fs').promises;

/**
 * Derive encryption key from sender, recipient, and image name
 */
function deriveKey(sender, recipient, imageName) {
    const secret = `${sender}-${recipient}-${imageName}`;
    return CryptoJS.SHA256(secret).toString();
}

/**
 * Encrypt image data and metadata
 */
function encryptData(imageBuffer, metadata, key) {
    const data = {
        image: imageBuffer.toString('base64'),
        metadata
    };

    const jsonData = JSON.stringify(data);
    const encrypted = CryptoJS.AES.encrypt(jsonData, key).toString();

    return encrypted;
}

/**
 * Decrypt data to get image and metadata
 */
function decryptData(encryptedData, key) {
    try {
        const decrypted = CryptoJS.AES.decrypt(encryptedData, key);
        const jsonData = decrypted.toString(CryptoJS.enc.Utf8);

        if (!jsonData) throw new Error('Decryption failed (empty result)');

        const data = JSON.parse(jsonData);

        return {
            imageBuffer: Buffer.from(data.image, 'base64'),
            metadata: data.metadata
        };
    } catch (e) {
        console.error("Decryption failed:", e.message);
        throw new Error('Failed to decrypt image. Wrong recipient or password?');
    }
}

/**
 * Embed encrypted data into cover image using LSB steganography
 */
async function embedDataInImage(coverImagePath, secretData) {
    // Read cover image
    const coverBuffer = await fs.readFile(coverImagePath);
    const coverImage = sharp(coverBuffer);
    const metadata = await coverImage.metadata();

    // Get raw pixel data
    const { data: pixels, info } = await coverImage
        .raw()
        .toBuffer({ resolveWithObject: true });

    // Convert secret data to binary
    const secretBinary = stringToBinary(secretData);

    // Embed length header (32 bits for data length)
    const lengthBinary = secretBinary.length.toString(2).padStart(32, '0');
    const fullBinary = lengthBinary + secretBinary;

    // Check if cover image has enough capacity
    const maxCapacity = pixels.length; // One bit per byte
    if (fullBinary.length > maxCapacity) {
        throw new Error('Cover image too small for secret data');
    }

    // Embed data in LSB
    for (let i = 0; i < fullBinary.length; i++) {
        pixels[i] = (pixels[i] & 0xFE) | parseInt(fullBinary[i]);
    }

    // Create steg image
    const stegImage = await sharp(pixels, {
        raw: {
            width: info.width,
            height: info.height,
            channels: info.channels
        }
    }).png().toBuffer();

    return stegImage;
}

/**
 * Extract encrypted data from steg image
 */
async function extractDataFromImage(stegImagePath) {
    // Read steg image
    const stegBuffer = await fs.readFile(stegImagePath);
    const stegImage = sharp(stegBuffer);

    // Get raw pixel data
    const { data: pixels } = await stegImage.raw().toBuffer({ resolveWithObject: true });

    // Extract length header (first 32 bits)
    let lengthBinary = '';
    for (let i = 0; i < 32; i++) {
        lengthBinary += (pixels[i] & 1).toString();
    }
    const dataLength = parseInt(lengthBinary, 2);

    // Extract secret data
    let secretBinary = '';
    for (let i = 32; i < 32 + dataLength; i++) {
        secretBinary += (pixels[i] & 1).toString();
    }

    // Convert binary to string
    const secretData = binaryToString(secretBinary);

    return secretData;
}

/**
 * Convert string to binary string (UTF-8 safe)
 */
function stringToBinary(str) {
    // UTF-8 encode first
    const utf8Bytes = unescape(encodeURIComponent(str));
    let binary = '';
    for (let i = 0; i < utf8Bytes.length; i++) {
        const charCode = utf8Bytes.charCodeAt(i);
        binary += charCode.toString(2).padStart(8, '0');
    }
    return binary;
}

/**
 * Convert binary string to string (UTF-8 safe)
 */
function binaryToString(binary) {
    let bytes = '';
    for (let i = 0; i < binary.length; i += 8) {
        const byte = binary.substr(i, 8);
        bytes += String.fromCharCode(parseInt(byte, 2));
    }
    // UTF-8 decode
    try {
        return decodeURIComponent(escape(bytes));
    } catch (e) {
        return bytes; // Fallback
    }
}

module.exports = {
    deriveKey,
    encryptData,
    decryptData,
    embedDataInImage,
    extractDataFromImage
};
