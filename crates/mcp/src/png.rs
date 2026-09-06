//! Pure Rust PNG encoder using `miniz_oxide` and `crc32fast`.

use crc32fast::Hasher;

/// Encodes raw RGBA8 pixels into a valid PNG file byte vector.
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let expected_len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| "Image dimensions overflow usize".to_string())?;

    if rgba.len() != expected_len {
        return Err(format!(
            "RGBA buffer length mismatch: expected {expected_len} bytes for {width}x{height}, got {}",
            rgba.len()
        ));
    }

    let mut out = Vec::new();
    // 1. Standard PNG Signature (8 bytes)
    out.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

    // 2. IHDR Chunk (Image Header)
    let mut ihdr_data = Vec::with_capacity(13);
    ihdr_data.extend_from_slice(&width.to_be_bytes());
    ihdr_data.extend_from_slice(&height.to_be_bytes());
    ihdr_data.push(8); // Bit depth: 8 bits per channel
    ihdr_data.push(6); // Color type: 6 = RGBA (Truecolor with alpha)
    ihdr_data.push(0); // Compression method: 0 (deflate)
    ihdr_data.push(0); // Filter method: 0 (standard adaptive filtering)
    ihdr_data.push(0); // Interlace method: 0 (no interlace)
    write_chunk(&mut out, b"IHDR", &ihdr_data);

    // 3. IDAT Chunk (Image Data)
    // In PNG, each scanline begins with a 1-byte filter type indicator (0 = Filter None).
    let row_len = width as usize * 4;
    let mut raw_scanlines = Vec::with_capacity(height as usize * (1 + row_len));
    for y in 0..height as usize {
        raw_scanlines.push(0); // Filter None
        let row_start = y * row_len;
        raw_scanlines.extend_from_slice(&rgba[row_start..row_start + row_len]);
    }

    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&raw_scanlines, 6);
    write_chunk(&mut out, b"IDAT", &compressed);

    // 4. IEND Chunk (Image End)
    write_chunk(&mut out, b"IEND", &[]);

    Ok(out)
}

fn write_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);

    let mut hasher = Hasher::new();
    hasher.update(chunk_type);
    hasher.update(data);
    let crc = hasher.finalize();
    out.extend_from_slice(&crc.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_png_valid_structure() {
        let width = 2;
        let height = 2;
        let rgba = vec![
            255, 0, 0, 255, // red
            0, 255, 0, 255, // green
            0, 0, 255, 255, // blue
            255, 255, 0, 255, // yellow
        ];
        let png = encode_png(width, height, &rgba).expect("PNG encoding failed");
        assert!(png.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]));
        assert!(png.windows(4).any(|w| w == b"IHDR"));
        assert!(png.windows(4).any(|w| w == b"IDAT"));
        assert!(png.windows(4).any(|w| w == b"IEND"));
    }
}
