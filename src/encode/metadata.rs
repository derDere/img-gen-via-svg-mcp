//! Physical resolution recorded in an encoded file.
//!
//! The encoders this server uses do not expose a density setting, so the two
//! formats whose container makes it a well-defined byte edit are patched here,
//! and every other format reports that it cannot carry the value.

/// Writes a pHYs chunk into an encoded PNG.
///
/// Returns false when the input is not a PNG this function recognises.
pub fn set_png_density(data: &mut Vec<u8>, dpi: f32) -> bool {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if data.len() < 8 + 25 || data[..8] != SIGNATURE {
        return false;
    }
    // IHDR is always the first chunk: 4 length + 4 type + 13 data + 4 CRC.
    let insert_at = 8 + 25;
    if &data[12..16] != b"IHDR" {
        return false;
    }

    let pixels_per_metre = (dpi / 0.0254).round().max(1.0) as u32;
    let mut chunk = Vec::with_capacity(21);
    chunk.extend_from_slice(&9u32.to_be_bytes());
    chunk.extend_from_slice(b"pHYs");
    chunk.extend_from_slice(&pixels_per_metre.to_be_bytes());
    chunk.extend_from_slice(&pixels_per_metre.to_be_bytes());
    chunk.push(1); // unit: metre
    let crc = crc32(&chunk[4..]);
    chunk.extend_from_slice(&crc.to_be_bytes());

    data.splice(insert_at..insert_at, chunk);
    true
}

/// Patches the density fields of a JPEG's JFIF APP0 segment.
///
/// Returns false when the file carries no JFIF segment to patch.
pub fn set_jpeg_density(data: &mut [u8], dpi: f32) -> bool {
    let dots = dpi.round().clamp(1.0, 65535.0) as u16;
    let mut index = 2; // past SOI
    while index + 4 <= data.len() {
        if data[index] != 0xFF {
            return false;
        }
        let marker = data[index + 1];
        let length = u16::from_be_bytes([data[index + 2], data[index + 3]]) as usize;
        if marker == 0xE0 && index + 4 + 5 <= data.len() && &data[index + 4..index + 9] == b"JFIF\0"
        {
            // units at +11, X density at +12..14, Y density at +14..16.
            let base = index + 4;
            if base + 16 <= data.len() {
                data[base + 7] = 1; // units: dots per inch
                data[base + 8..base + 10].copy_from_slice(&dots.to_be_bytes());
                data[base + 10..base + 12].copy_from_slice(&dots.to_be_bytes());
                return true;
            }
            return false;
        }
        if marker == 0xD8 || marker == 0xD9 {
            return false;
        }
        index += 2 + length;
    }
    false
}

/// The CRC-32 the PNG specification prescribes.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crc_matches_the_reference_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn refuses_data_that_is_not_a_png() {
        let mut data = b"not a png at all, really not".to_vec();
        assert!(!set_png_density(&mut data, 300.0));
    }
}
