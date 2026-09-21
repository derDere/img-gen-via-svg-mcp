//! ICNS, the Apple icon container.
//!
//! No target platform of this server consumes an ICNS file; it is here so that
//! a caller producing assets for macOS does not need a second tool.

use crate::error::{ErrorCode, Result, ToolError};
use ::icns::{IconFamily, Image, PixelFormat};
use image::RgbaImage;
use serde_json::json;

/// The edge lengths the ICNS format defines.
pub const SUPPORTED_SIZES: [u32; 8] = [16, 32, 48, 64, 128, 256, 512, 1024];

/// Packs images into an ICNS file.
///
/// Every image must be square and one of [`SUPPORTED_SIZES`]; the container has
/// a fixed slot per size and cannot express anything else.
pub fn encode(images: &[RgbaImage]) -> Result<Vec<u8>> {
    if images.is_empty() {
        return Err(ToolError::new(
            ErrorCode::EncodeFailed,
            "An ICNS file needs at least one image.",
        ));
    }

    let mut family = IconFamily::new();
    for image in images {
        if image.width() != image.height() || !SUPPORTED_SIZES.contains(&image.width()) {
            return Err(ToolError::new(
                ErrorCode::EncodeFailed,
                format!(
                    "ICNS holds square images of {:?} pixels; one is {}×{}.",
                    SUPPORTED_SIZES,
                    image.width(),
                    image.height()
                ),
            )
            .with_detail(json!({ "width": image.width(), "height": image.height(), "supported": SUPPORTED_SIZES })));
        }
        let icon = Image::from_data(
            PixelFormat::RGBA,
            image.width(),
            image.height(),
            image.as_raw().clone(),
        )
        .map_err(|e| {
            ToolError::new(ErrorCode::EncodeFailed, format!("An ICNS entry is invalid: {e}"))
        })?;
        family.add_icon(&icon).map_err(|e| {
            ToolError::new(
                ErrorCode::EncodeFailed,
                format!("A {}×{} image has no ICNS slot: {e}", image.width(), image.height()),
            )
        })?;
    }

    let mut out = Vec::new();
    family.write(&mut out).map_err(|e| {
        ToolError::new(ErrorCode::EncodeFailed, format!("The ICNS file failed to encode: {e}"))
    })?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_container_with_the_expected_magic() {
        let images: Vec<RgbaImage> = [16u32, 32].iter().map(|s| RgbaImage::new(*s, *s)).collect();
        let bytes = encode(&images).unwrap();
        assert_eq!(&bytes[..4], b"icns");
    }

    #[test]
    fn refuses_a_size_the_container_cannot_express() {
        let error = encode(&[RgbaImage::new(24, 24)]).unwrap_err();
        assert_eq!(error.code, ErrorCode::EncodeFailed);
    }
}
