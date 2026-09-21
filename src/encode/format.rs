//! Output formats, what they can carry, and how they are named.

use crate::error::{ErrorCode, Result, ToolError, invalid_input};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A single-image output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// Portable Network Graphics. The default.
    Png,
    /// JPEG. No alpha channel.
    Jpeg,
    /// Windows bitmap, written with an alpha channel when one is present.
    Bmp,
    /// GIF, at most 256 colours.
    Gif,
    /// TIFF.
    Tiff,
    /// WebP, lossless only.
    Webp,
    /// Windows icon, one image; `render_icon` builds multi-resolution files.
    Ico,
    /// Truevision TGA.
    Tga,
    /// Quite OK Image format.
    Qoi,
    /// AVIF, available only in a build with the `avif` feature.
    Avif,
    /// Portable anymap. No alpha channel.
    Pnm,
    /// Farbfeld.
    Farbfeld,
    /// OpenEXR, 32-bit float.
    Openexr,
    /// Radiance HDR. No alpha channel.
    Hdr,
}

impl Format {
    /// Every format this server knows about.
    pub const ALL: [Format; 14] = [
        Format::Png,
        Format::Jpeg,
        Format::Bmp,
        Format::Gif,
        Format::Tiff,
        Format::Webp,
        Format::Ico,
        Format::Tga,
        Format::Qoi,
        Format::Avif,
        Format::Pnm,
        Format::Farbfeld,
        Format::Openexr,
        Format::Hdr,
    ];

    /// The wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Bmp => "bmp",
            Self::Gif => "gif",
            Self::Tiff => "tiff",
            Self::Webp => "webp",
            Self::Ico => "ico",
            Self::Tga => "tga",
            Self::Qoi => "qoi",
            Self::Avif => "avif",
            Self::Pnm => "pnm",
            Self::Farbfeld => "farbfeld",
            Self::Openexr => "openexr",
            Self::Hdr => "hdr",
        }
    }

    /// The extension a file of this format is normally given.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Bmp => "bmp",
            Self::Gif => "gif",
            Self::Tiff => "tiff",
            Self::Webp => "webp",
            Self::Ico => "ico",
            Self::Tga => "tga",
            Self::Qoi => "qoi",
            Self::Avif => "avif",
            Self::Pnm => "pnm",
            Self::Farbfeld => "ff",
            Self::Openexr => "exr",
            Self::Hdr => "hdr",
        }
    }

    /// The media type used for inline image content.
    pub fn mime_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Bmp => "image/bmp",
            Self::Gif => "image/gif",
            Self::Tiff => "image/tiff",
            Self::Webp => "image/webp",
            Self::Ico => "image/x-icon",
            Self::Tga => "image/x-tga",
            Self::Qoi => "image/qoi",
            Self::Avif => "image/avif",
            Self::Pnm => "image/x-portable-anymap",
            Self::Farbfeld => "image/x-farbfeld",
            Self::Openexr => "image/x-exr",
            Self::Hdr => "image/vnd.radiance",
        }
    }

    /// Whether the format can carry an alpha channel.
    ///
    /// This is the table §4.6 of the specification is built on: a format that
    /// answers `false` here forces the canvas to be flattened.
    pub fn has_alpha(self) -> bool {
        !matches!(self, Self::Jpeg | Self::Pnm | Self::Hdr)
    }

    /// Whether this build can encode the format.
    pub fn is_available(self) -> bool {
        match self {
            Self::Avif => cfg!(feature = "avif"),
            _ => true,
        }
    }

    /// The `image` crate's identifier for the format.
    pub fn to_image_format(self) -> image::ImageFormat {
        match self {
            Self::Png => image::ImageFormat::Png,
            Self::Jpeg => image::ImageFormat::Jpeg,
            Self::Bmp => image::ImageFormat::Bmp,
            Self::Gif => image::ImageFormat::Gif,
            Self::Tiff => image::ImageFormat::Tiff,
            Self::Webp => image::ImageFormat::WebP,
            Self::Ico => image::ImageFormat::Ico,
            Self::Tga => image::ImageFormat::Tga,
            Self::Qoi => image::ImageFormat::Qoi,
            Self::Avif => image::ImageFormat::Avif,
            Self::Pnm => image::ImageFormat::Pnm,
            Self::Farbfeld => image::ImageFormat::Farbfeld,
            Self::Openexr => image::ImageFormat::OpenExr,
            Self::Hdr => image::ImageFormat::Hdr,
        }
    }

    /// The format a file extension names, when it names one.
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" | "jpe" => Some(Self::Jpeg),
            "bmp" | "dib" => Some(Self::Bmp),
            "gif" => Some(Self::Gif),
            "tif" | "tiff" => Some(Self::Tiff),
            "webp" => Some(Self::Webp),
            "ico" => Some(Self::Ico),
            "tga" => Some(Self::Tga),
            "qoi" => Some(Self::Qoi),
            "avif" => Some(Self::Avif),
            "pnm" | "ppm" | "pgm" | "pbm" | "pam" => Some(Self::Pnm),
            "ff" | "farbfeld" => Some(Self::Farbfeld),
            "exr" => Some(Self::Openexr),
            "hdr" => Some(Self::Hdr),
            _ => None,
        }
    }
}

/// Decides the output format from the explicit parameter and the target path.
///
/// An explicit format that contradicts the extension is refused rather than
/// silently preferred, because either the caller or the path is wrong and this
/// server does not guess which.
pub fn resolve(explicit: Option<Format>, output_path: Option<&Path>) -> Result<Format> {
    let from_path = output_path
        .and_then(Path::extension)
        .and_then(|e| e.to_str())
        .and_then(Format::from_extension);

    let format = match (explicit, from_path) {
        (Some(explicit), Some(from_path)) if explicit != from_path => {
            return Err(invalid_input(format!(
                "format is {} but the output path ends in .{}, which is {}.",
                explicit.as_str(),
                output_path.and_then(Path::extension).and_then(|e| e.to_str()).unwrap_or_default(),
                from_path.as_str()
            ))
            .with_hint("Drop the format parameter, or give the path the matching extension."));
        }
        (Some(explicit), _) => explicit,
        (None, Some(from_path)) => from_path,
        (None, None) => Format::Png,
    };

    if !format.is_available() {
        return Err(ToolError::new(
            ErrorCode::UnsupportedFormat,
            format!("This build cannot encode {}.", format.as_str()),
        )
        .with_detail(serde_json::json!({ "format": format.as_str() }))
        .with_hint("Rebuild with the matching Cargo feature, or choose another format."));
    }
    Ok(format)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_the_format_from_the_extension() {
        assert_eq!(resolve(None, Some(Path::new("/tmp/a.webp"))).unwrap(), Format::Webp);
        assert_eq!(resolve(None, Some(Path::new("/tmp/a.JPG"))).unwrap(), Format::Jpeg);
        assert_eq!(resolve(None, None).unwrap(), Format::Png);
    }

    #[test]
    fn refuses_a_contradiction_instead_of_choosing() {
        let error = resolve(Some(Format::Png), Some(Path::new("/tmp/a.jpg"))).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn every_canonical_extension_names_its_own_format() {
        for format in Format::ALL {
            assert_eq!(
                Format::from_extension(format.extension()),
                Some(format),
                "{} does not round-trip through its extension",
                format.as_str()
            );
        }
    }

    #[test]
    fn only_three_formats_lack_alpha() {
        let without: Vec<&str> =
            Format::ALL.iter().filter(|f| !f.has_alpha()).map(|f| f.as_str()).collect();
        assert_eq!(without, vec!["jpeg", "pnm", "hdr"]);
    }
}
