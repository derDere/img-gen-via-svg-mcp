//! Encoding a canvas into the bytes of an image file.

use crate::encode::format::Format;
use crate::encode::metadata;
use crate::encode::options::EncodeOptions;
use crate::encode::png_opt;
use crate::error::{ErrorCode, Result, ToolError};
use crate::warning::Warnings;
use image::{DynamicImage, ExtendedColorType, ImageEncoder, RgbaImage};
use serde_json::json;
use std::io::Cursor;
use tiny_skia::Pixmap;

/// Converts a rendered canvas into an image buffer.
///
/// The canvas holds premultiplied pixels; the buffer holds straight ones, so
/// the conversion is a demultiplication rather than a copy.
pub fn pixmap_to_image(pixmap: &Pixmap) -> RgbaImage {
    let mut buffer = Vec::with_capacity(pixmap.width() as usize * pixmap.height() as usize * 4);
    for pixel in pixmap.pixels() {
        let straight = pixel.demultiply();
        buffer.extend_from_slice(&[
            straight.red(),
            straight.green(),
            straight.blue(),
            straight.alpha(),
        ]);
    }
    RgbaImage::from_raw(pixmap.width(), pixmap.height(), buffer)
        .expect("the buffer holds exactly four bytes per pixel")
}

/// Encodes a canvas.
pub fn encode_pixmap(
    pixmap: &Pixmap,
    format: Format,
    options: &EncodeOptions,
    warnings: &Warnings,
) -> Result<Vec<u8>> {
    encode_image(&DynamicImage::ImageRgba8(pixmap_to_image(pixmap)), format, options, warnings)
}

/// Encodes an image buffer into the bytes of a file in the given format.
pub fn encode_image(
    image: &DynamicImage,
    format: Format,
    options: &EncodeOptions,
    warnings: &Warnings,
) -> Result<Vec<u8>> {
    options.validate()?;

    if !format.is_available() {
        return Err(ToolError::new(
            ErrorCode::UnsupportedFormat,
            format!("This build cannot encode {}.", format.as_str()),
        ));
    }

    if format != Format::Jpeg && options.jpeg_quality.is_some() {
        let note = if format == Format::Webp {
            "WebP is encoded losslessly here, so jpeg_quality has no effect."
        } else {
            "jpeg_quality applies to JPEG only and had no effect."
        };
        warnings.warn_detail(
            if format == Format::Webp { "lossless_only" } else { "option_ignored" },
            note,
            json!({ "format": format.as_str(), "option": "jpeg_quality" }),
        );
    }
    if format != Format::Png && (options.png_compression.is_some() || options.png_optimize) {
        warnings.warn_detail(
            "option_ignored",
            "png_compression and png_optimize apply to PNG only and had no effect.",
            json!({ "format": format.as_str() }),
        );
    }

    let mut bytes = match format {
        Format::Png => encode_png(image, options)?,
        Format::Jpeg => encode_jpeg(image, options)?,
        Format::Webp => encode_webp(image)?,
        Format::Gif => encode_gif(image, warnings)?,
        Format::Ico => encode_single_ico(image)?,
        other => encode_generic(image, other)?,
    };

    if format == Format::Png && options.png_optimize {
        bytes = png_opt::optimise(bytes, warnings)?;
    }

    if let Some(density) = options.density_metadata {
        let written = match format {
            Format::Png => metadata::set_png_density(&mut bytes, density),
            Format::Jpeg => metadata::set_jpeg_density(&mut bytes, density),
            _ => false,
        };
        if !written {
            warnings.warn_detail(
                "metadata_unsupported",
                format!(
                    "{} cannot carry a physical resolution here, so density_metadata was not written.",
                    format.as_str()
                ),
                json!({ "format": format.as_str(), "density_metadata": density }),
            );
        }
    }

    Ok(bytes)
}

fn encode_png(image: &DynamicImage, options: &EncodeOptions) -> Result<Vec<u8>> {
    use image::codecs::png::{FilterType, PngEncoder};
    let rgba = image.to_rgba8();
    let mut out = Cursor::new(Vec::new());
    PngEncoder::new_with_quality(&mut out, options.png_compression()?, FilterType::Adaptive)
        .write_image(rgba.as_raw(), rgba.width(), rgba.height(), ExtendedColorType::Rgba8)
        .map_err(|e| encode_failed(Format::Png, e))?;
    Ok(out.into_inner())
}

fn encode_jpeg(image: &DynamicImage, options: &EncodeOptions) -> Result<Vec<u8>> {
    use image::codecs::jpeg::JpegEncoder;
    let rgb = image.to_rgb8();
    let mut out = Cursor::new(Vec::new());
    JpegEncoder::new_with_quality(&mut out, options.jpeg_quality())
        .write_image(rgb.as_raw(), rgb.width(), rgb.height(), ExtendedColorType::Rgb8)
        .map_err(|e| encode_failed(Format::Jpeg, e))?;
    Ok(out.into_inner())
}

fn encode_webp(image: &DynamicImage) -> Result<Vec<u8>> {
    use image::codecs::webp::WebPEncoder;
    let rgba = image.to_rgba8();
    let mut out = Cursor::new(Vec::new());
    WebPEncoder::new_lossless(&mut out)
        .write_image(rgba.as_raw(), rgba.width(), rgba.height(), ExtendedColorType::Rgba8)
        .map_err(|e| encode_failed(Format::Webp, e))?;
    Ok(out.into_inner())
}

fn encode_gif(image: &DynamicImage, warnings: &Warnings) -> Result<Vec<u8>> {
    use image::codecs::gif::GifEncoder;
    let rgba = image.to_rgba8();
    let mut out = Cursor::new(Vec::new());
    {
        let mut encoder = GifEncoder::new(&mut out);
        encoder
            .encode(rgba.as_raw(), rgba.width(), rgba.height(), ExtendedColorType::Rgba8)
            .map_err(|e| encode_failed(Format::Gif, e))?;
    }
    warnings.warn_detail(
        "color_quantized",
        "GIF holds at most 256 colours, so the image was quantised, and its alpha channel reduced to fully transparent or fully opaque.",
        json!({ "format": "gif" }),
    );
    Ok(out.into_inner())
}

fn encode_single_ico(image: &DynamicImage) -> Result<Vec<u8>> {
    if image.width() > 256 || image.height() > 256 {
        return Err(ToolError::new(
            ErrorCode::EncodeFailed,
            format!(
                "ICO holds images up to 256×256; this one is {}×{}.",
                image.width(),
                image.height()
            ),
        )
        .with_detail(json!({ "width": image.width(), "height": image.height(), "limit": 256 }))
        .with_hint(
            "Render at 256 pixels or less, or use render_icon for a multi-resolution file.",
        ));
    }
    crate::encode::ico::encode(&[image.to_rgba8()])
}

fn encode_generic(image: &DynamicImage, format: Format) -> Result<Vec<u8>> {
    // Farbfeld stores 16 bits per channel, OpenEXR 32-bit floats with alpha and
    // HDR 32-bit floats without; none of those encoders accepts anything else,
    // so the conversion happens here rather than being left to the dispatcher.
    let converted;
    let image = match format {
        Format::Farbfeld => {
            converted = DynamicImage::ImageRgba16(image.to_rgba16());
            &converted
        }
        Format::Openexr => {
            converted = DynamicImage::ImageRgba32F(image.to_rgba32f());
            &converted
        }
        Format::Hdr => {
            converted = DynamicImage::ImageRgb32F(image.to_rgb32f());
            &converted
        }
        _ => image,
    };
    let mut out = Cursor::new(Vec::new());
    image.write_to(&mut out, format.to_image_format()).map_err(|e| encode_failed(format, e))?;
    Ok(out.into_inner())
}

fn encode_failed(format: Format, error: image::ImageError) -> ToolError {
    ToolError::new(
        ErrorCode::EncodeFailed,
        format!("Encoding {} failed: {}", format.as_str(), error),
    )
    .with_detail(json!({ "format": format.as_str(), "encoder_message": error.to_string() }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Color;

    fn canvas() -> Pixmap {
        let mut pixmap = Pixmap::new(8, 6).unwrap();
        pixmap.fill(Color::from_rgba8(10, 120, 200, 128));
        pixmap
    }

    #[test]
    fn demultiplies_on_the_way_out() {
        let image = pixmap_to_image(&canvas());
        let pixel = image.get_pixel(0, 0);
        assert_eq!(pixel.0[3], 128);
        assert!(pixel.0[2] > 190, "blue should survive the round trip, got {}", pixel.0[2]);
    }

    #[test]
    fn every_available_format_encodes_and_decodes_back() {
        let options = EncodeOptions::default();
        let warnings = Warnings::new();
        for format in Format::ALL {
            if !format.is_available() {
                continue;
            }
            let bytes = encode_pixmap(&canvas(), format, &options, &warnings)
                .unwrap_or_else(|e| panic!("{} failed to encode: {e}", format.as_str()));
            assert!(!bytes.is_empty(), "{} produced no bytes", format.as_str());
            let decoded = image::load_from_memory_with_format(&bytes, format.to_image_format());
            if matches!(format, Format::Avif) {
                continue; // this build has an encoder but no decoder for AVIF
            }
            let decoded =
                decoded.unwrap_or_else(|e| panic!("{} did not decode back: {e}", format.as_str()));
            assert_eq!(decoded.width(), 8, "{} lost its width", format.as_str());
            assert_eq!(decoded.height(), 6, "{} lost its height", format.as_str());
        }
    }

    #[test]
    fn png_density_metadata_is_written() {
        let options = EncodeOptions { density_metadata: Some(300.0), ..EncodeOptions::default() };
        let warnings = Warnings::new();
        let bytes = encode_pixmap(&canvas(), Format::Png, &options, &warnings).unwrap();
        assert!(bytes.windows(4).any(|w| w == b"pHYs"));
        assert!(!warnings.contains("metadata_unsupported"));
    }

    #[test]
    fn a_format_that_cannot_carry_density_says_so() {
        let options = EncodeOptions { density_metadata: Some(300.0), ..EncodeOptions::default() };
        let warnings = Warnings::new();
        encode_pixmap(&canvas(), Format::Bmp, &options, &warnings).unwrap();
        assert!(warnings.contains("metadata_unsupported"));
    }

    #[test]
    fn webp_reports_that_quality_does_not_apply() {
        let options = EncodeOptions { jpeg_quality: Some(50), ..EncodeOptions::default() };
        let warnings = Warnings::new();
        encode_pixmap(&canvas(), Format::Webp, &options, &warnings).unwrap();
        assert!(warnings.contains("lossless_only"));
    }

    #[test]
    fn jpeg_quality_changes_the_file_size() {
        let warnings = Warnings::new();
        let low = encode_pixmap(
            &canvas(),
            Format::Jpeg,
            &EncodeOptions { jpeg_quality: Some(10), ..EncodeOptions::default() },
            &warnings,
        )
        .unwrap();
        let high = encode_pixmap(
            &canvas(),
            Format::Jpeg,
            &EncodeOptions { jpeg_quality: Some(95), ..EncodeOptions::default() },
            &warnings,
        )
        .unwrap();
        assert!(low.len() < high.len(), "low quality {} vs high {}", low.len(), high.len());
    }
}
