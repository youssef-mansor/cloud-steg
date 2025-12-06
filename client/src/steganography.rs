use image::{DynamicImage, ImageBuffer, Rgba};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageMetadata {
    pub allowed_username: String,
    pub views_remaining: u32,
    pub original_username: String, // Owner of the image
}

impl ImageMetadata {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Encode image with metadata (username, view count) using steganography
pub fn encode_image_with_metadata(
    cover: DynamicImage,
    secret: DynamicImage,
    metadata: &ImageMetadata,
) -> DynamicImage {
    let mut cover = cover.to_rgba8();
    let secret = secret.to_rgba8();
    let metadata_json = metadata.to_json();
    let text_bytes = metadata_json.as_bytes();
    
    let (sw, sh) = secret.dimensions();
    let (cw, ch) = cover.dimensions();
    
    let mut cover_idx = 0;
    
    // 1. Store text length (4 bytes)
    let text_len_bytes = (text_bytes.len() as u32).to_le_bytes();
    for i in 0..4 {
        let cx = cover_idx % cw;
        let cy = cover_idx / cw;
        let pixel = cover.get_pixel_mut(cx, cy);
        pixel[0] = (pixel[0] & 0xF0) | (text_len_bytes[i as usize] >> 4);
        pixel[1] = (pixel[1] & 0xF0) | (text_len_bytes[i as usize] & 0x0F);
        cover_idx += 1;
    }
    
    // 2. Store text bytes (2 nibbles per pixel)
    for &byte in text_bytes {
        let cx = cover_idx % cw;
        let cy = cover_idx / cw;
        let pixel = cover.get_pixel_mut(cx, cy);
        pixel[0] = (pixel[0] & 0xF0) | (byte >> 4);
        pixel[1] = (pixel[1] & 0xF0) | (byte & 0x0F);
        cover_idx += 1;
    }
    
    // 3. Store image dimensions (8 bytes total)
    let width_bytes = (sw as u32).to_le_bytes();
    let height_bytes = (sh as u32).to_le_bytes();
    
    for i in 0..4 {
        let cx = cover_idx % cw;
        let cy = cover_idx / cw;
        let pixel = cover.get_pixel_mut(cx, cy);
        pixel[0] = (pixel[0] & 0xF0) | (width_bytes[i as usize] >> 4);
        pixel[1] = (pixel[1] & 0xF0) | (width_bytes[i as usize] & 0x0F);
        cover_idx += 1;
    }
    for i in 0..4 {
        let cx = cover_idx % cw;
        let cy = cover_idx / cw;
        let pixel = cover.get_pixel_mut(cx, cy);
        pixel[0] = (pixel[0] & 0xF0) | (height_bytes[i as usize] >> 4);
        pixel[1] = (pixel[1] & 0xF0) | (height_bytes[i as usize] & 0x0F);
        cover_idx += 1;
    }
    
    // 4. Encode image pixels (12 cover pixels per secret pixel)
    for y in 0..sh {
        for x in 0..sw {
            let secret_pixel = secret.get_pixel(x, y);
            
            // R channel
            for bit_pair in 0..4 {
                let cx = cover_idx % cw;
                let cy = cover_idx / cw;
                if cy >= ch { break; }
                
                let cover_pixel = cover.get_pixel_mut(cx, cy);
                let bits = (secret_pixel[0] >> (6 - bit_pair * 2)) & 0x03;
                cover_pixel[0] = (cover_pixel[0] & 0xFC) | bits;
                cover_idx += 1;
            }
            
            // G channel
            for bit_pair in 0..4 {
                let cx = cover_idx % cw;
                let cy = cover_idx / cw;
                if cy >= ch { break; }
                
                let cover_pixel = cover.get_pixel_mut(cx, cy);
                let bits = (secret_pixel[1] >> (6 - bit_pair * 2)) & 0x03;
                cover_pixel[1] = (cover_pixel[1] & 0xFC) | bits;
                cover_idx += 1;
            }
            
            // B channel
            for bit_pair in 0..4 {
                let cx = cover_idx % cw;
                let cy = cover_idx / cw;
                if cy >= ch { break; }
                
                let cover_pixel = cover.get_pixel_mut(cx, cy);
                let bits = (secret_pixel[2] >> (6 - bit_pair * 2)) & 0x03;
                cover_pixel[2] = (cover_pixel[2] & 0xFC) | bits;
                cover_idx += 1;
            }
        }
    }
    
    DynamicImage::ImageRgba8(cover)
}

/// Decode image and metadata from steganography
pub fn decode_image_with_metadata(stego: DynamicImage) -> Result<(ImageMetadata, DynamicImage), String> {
    let stego = stego.to_rgba8();
    let (cw, ch) = stego.dimensions();
    let mut cover_idx = 0;
    
    // 1. Extract text length
    let mut text_len_bytes = [0u8; 4];
    for i in 0..4 {
        let cx = cover_idx % cw;
        let cy = cover_idx / cw;
        let pixel = stego.get_pixel(cx, cy);
        text_len_bytes[i as usize] = ((pixel[0] & 0x0F) << 4) | (pixel[1] & 0x0F);
        cover_idx += 1;
    }
    let text_len = u32::from_le_bytes(text_len_bytes) as usize;
    
    // 2. Extract text bytes
    let mut text_bytes = Vec::with_capacity(text_len);
    for _ in 0..text_len {
        let cx = cover_idx % cw;
        let cy = cover_idx / cw;
        let pixel = stego.get_pixel(cx, cy);
        let byte = ((pixel[0] & 0x0F) << 4) | (pixel[1] & 0x0F);
        text_bytes.push(byte);
        cover_idx += 1;
    }
    let text = String::from_utf8_lossy(&text_bytes).to_string();
    
    // Parse metadata
    let metadata = ImageMetadata::from_json(&text)
        .map_err(|e| format!("Failed to parse metadata: {}", e))?;
    
    // 3. Extract image dimensions
    let mut width_bytes = [0u8; 4];
    let mut height_bytes = [0u8; 4];
    
    for i in 0..4 {
        let cx = cover_idx % cw;
        let cy = cover_idx / cw;
        let pixel = stego.get_pixel(cx, cy);
        width_bytes[i as usize] = ((pixel[0] & 0x0F) << 4) | (pixel[1] & 0x0F);
        cover_idx += 1;
    }
    for i in 0..4 {
        let cx = cover_idx % cw;
        let cy = cover_idx / cw;
        let pixel = stego.get_pixel(cx, cy);
        height_bytes[i as usize] = ((pixel[0] & 0x0F) << 4) | (pixel[1] & 0x0F);
        cover_idx += 1;
    }
    
    let sw = u32::from_le_bytes(width_bytes);
    let sh = u32::from_le_bytes(height_bytes);
    
    // 4. Extract image pixels
    let mut secret = ImageBuffer::new(sw, sh);
    
    for y in 0..sh {
        for x in 0..sw {
            let mut r = 0u8;
            let mut g = 0u8;
            let mut b = 0u8;
            
            // R
            for bit_pair in 0..4 {
                let cx = cover_idx % cw;
                let cy = cover_idx / cw;
                if cy >= ch { break; }
                
                let cover_pixel = stego.get_pixel(cx, cy);
                let bits = cover_pixel[0] & 0x03;
                r |= bits << (6 - bit_pair * 2);
                cover_idx += 1;
            }
            
            // G
            for bit_pair in 0..4 {
                let cx = cover_idx % cw;
                let cy = cover_idx / cw;
                if cy >= ch { break; }
                
                let cover_pixel = stego.get_pixel(cx, cy);
                let bits = cover_pixel[1] & 0x03;
                g |= bits << (6 - bit_pair * 2);
                cover_idx += 1;
            }
            
            // B
            for bit_pair in 0..4 {
                let cx = cover_idx % cw;
                let cy = cover_idx / cw;
                if cy >= ch { break; }
                
                let cover_pixel = stego.get_pixel(cx, cy);
                let bits = cover_pixel[2] & 0x03;
                b |= bits << (6 - bit_pair * 2);
                cover_idx += 1;
            }
            
            secret.put_pixel(x, y, Rgba([r, g, b, 255]));
        }
    }
    
    Ok((metadata, DynamicImage::ImageRgba8(secret)))
}

