//! ICO, single-image and multi-resolution.

use crate::error::{ErrorCode, Result, ToolError};
use image::{ExtendedColorType, RgbaImage};
use serde_json::json;
use std::io::Cursor;

/// The largest edge length the ICO format can express.
pub const MAX_EDGE: u32 = 256;

/// Packs one or more images into an ICO file.
///
/// Every image is stored as PNG inside the container, which is what modern
/// consumers expect and what keeps the alpha channel intact.
pub fn encode(images: &[RgbaImage]) -> Result<Vec<u8>> {
    use image::codecs::ico::{IcoEncoder, IcoFrame};

    if images.is_empty() {
        return Err(ToolError::new(
            ErrorCode::EncodeFailed,
            "An ICO file needs at least one image.",
        ));
    }

    let mut encoded = Vec::with_capacity(images.len());
    for image in images {
        if image.width() > MAX_EDGE || image.height() > MAX_EDGE {
            return Err(ToolError::new(
                ErrorCode::EncodeFailed,
                format!(
                    "ICO holds images up to {MAX_EDGE}×{MAX_EDGE}; one is {}×{}.",
                    image.width(),
                    image.height()
                ),
            )
            .with_detail(json!({ "width": image.width(), "height": image.height() })));
        }
        let mut png = Cursor::new(Vec::new());
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(image.as_raw(), image.width(), image.height(), ExtendedColorType::Rgba8)
            .map_err(|e| {
                ToolError::new(
                    ErrorCode::EncodeFailed,
                    format!("An ICO entry failed to encode: {e}"),
                )
            })?;
        encoded.push((png.into_inner(), image.width(), image.height()));
    }

    let frames: Vec<IcoFrame<'_>> = encoded
        .iter()
        .map(|(png, width, height)| {
            IcoFrame::with_encoded(png, *width, *height, ExtendedColorType::Rgba8).map_err(|e| {
                ToolError::new(ErrorCode::EncodeFailed, format!("An ICO frame is invalid: {e}"))
            })
        })
        .collect::<Result<_>>()?;

    let mut out = Cursor::new(Vec::new());
    IcoEncoder::new(&mut out).encode_images(&frames).map_err(|e| {
        ToolError::new(ErrorCode::EncodeFailed, format!("The ICO file failed to encode: {e}"))
    })?;
    Ok(out.into_inner())
}

use image::ImageEncoder;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_several_sizes_and_reads_them_back() {
        let images: Vec<RgbaImage> =
            [16u32, 32, 48].iter().map(|s| RgbaImage::new(*s, *s)).collect();
        let bytes = encode(&images).unwrap();
        assert_eq!(&bytes[..4], &[0, 0, 1, 0], "an ICO file starts with its own magic");
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 3, "three entries were written");
    }

    #[test]
    fn refuses_an_entry_above_the_format_limit() {
        let error = encode(&[RgbaImage::new(512, 512)]).unwrap_err();
        assert_eq!(error.code, ErrorCode::EncodeFailed);
    }
}
