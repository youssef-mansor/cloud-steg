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

// ... unchanged embed/extract functions

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
