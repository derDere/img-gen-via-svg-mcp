//! Raster input for `convert_image`: a file, or Base64 with an optional
//! `data:` prefix.

use crate::error::{ErrorCode, Result, ToolError, invalid_input};
use base64::Engine;
use image::DynamicImage;
use std::path::Path;

/// A decoded raster image together with the format it arrived in.
#[derive(Debug)]
pub struct DecodedRaster {
    /// The pixels.
    pub image: DynamicImage,
    /// The format the bytes were encoded in, when it could be determined.
    pub format: Option<image::ImageFormat>,
}

/// Reads and decodes a raster image from disk.
pub fn from_path(path: &Path) -> Result<DecodedRaster> {
    let bytes = std::fs::read(path).map_err(|e| {
        let code = if e.kind() == std::io::ErrorKind::NotFound {
            ErrorCode::InputNotFound
        } else {
            ErrorCode::InputUnreadable
        };
        ToolError::new(code, format!("Cannot read {}: {}", path.display(), e))
    })?;
    decode(&bytes, None)
}

/// Decodes a raster image from a Base64 string, tolerating a `data:` prefix.
pub fn from_base64(text: &str, hint: Option<image::ImageFormat>) -> Result<DecodedRaster> {
    let payload = text.split_once(";base64,").map(|(_, rest)| rest).unwrap_or(text);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .map_err(|e| invalid_input(format!("input_base64 is not valid Base64: {e}")))?;
    decode(&bytes, hint)
}

/// Decodes raster bytes, refusing an SVG with a pointer to the right tool.
pub fn decode(bytes: &[u8], hint: Option<image::ImageFormat>) -> Result<DecodedRaster> {
    if looks_like_svg(bytes) {
        return Err(invalid_input("The input is an SVG document, not a raster image.")
            .with_hint("Use render_svg, which rasterises vector input at an exact pixel size."));
    }
    let format = hint.or_else(|| image::guess_format(bytes).ok());
    let image = match format {
        Some(format) => image::load_from_memory_with_format(bytes, format),
        None => image::load_from_memory(bytes),
    }
    .map_err(|e| {
        ToolError::new(
            ErrorCode::InputUnreadable,
            format!("The raster input cannot be decoded: {e}"),
        )
    })?;
    Ok(DecodedRaster { image, format })
}

/// Recognises an SVG document by its first non-whitespace markup.
fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(1024)];
    let text = String::from_utf8_lossy(head);
    let trimmed = text.trim_start();
    trimmed.starts_with("<svg")
        || (trimmed.starts_with("<?xml") && text.contains("<svg"))
        || (trimmed.starts_with("<!DOCTYPE svg"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_svg_input_with_a_pointer_to_render_svg() {
        let error = decode(b"<svg xmlns='http://www.w3.org/2000/svg'/>", None).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidInput);
        assert!(error.hint.unwrap().contains("render_svg"));
    }
}
