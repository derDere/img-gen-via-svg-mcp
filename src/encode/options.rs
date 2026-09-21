//! Encoder parameters and their validation.

use crate::error::{Result, invalid_input};
use image::codecs::png::CompressionType;
use serde::Deserialize;

/// Per-format encoder settings.
///
/// Every field is optional so that the encoder can tell "the caller asked for
/// the default" from "the caller asked for nothing" — which is what lets a
/// setting that a format cannot honour be reported rather than ignored.
#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EncodeOptions {
    /// JPEG quality, 1 to 100. Defaults to 90.
    pub jpeg_quality: Option<u8>,
    /// PNG deflate effort: `fast`, `default`, `best`, `none` or `level:0`–`level:9`.
    pub png_compression: Option<String>,
    /// Whether a lossless optimisation pass runs over the encoded PNG.
    #[serde(default)]
    pub png_optimize: bool,
    /// Physical resolution recorded in the file, in dots per inch.
    pub density_metadata: Option<f32>,
}

/// The JPEG quality used when the caller names none.
pub const DEFAULT_JPEG_QUALITY: u8 = 90;

impl EncodeOptions {
    /// The JPEG quality to encode with.
    pub fn jpeg_quality(&self) -> u8 {
        self.jpeg_quality.unwrap_or(DEFAULT_JPEG_QUALITY)
    }

    /// The PNG compression to encode with.
    pub fn png_compression(&self) -> Result<CompressionType> {
        match &self.png_compression {
            Some(value) => parse_png_compression(value),
            None => Ok(CompressionType::Default),
        }
    }

    /// Checks every value, so that a bad one is refused before anything is
    /// rendered rather than after.
    pub fn validate(&self) -> Result<()> {
        if let Some(quality) = self.jpeg_quality
            && !(1..=100).contains(&quality)
        {
            return Err(invalid_input(format!(
                "jpeg_quality is {quality} but has to be between 1 and 100."
            )));
        }
        if let Some(density) = self.density_metadata
            && !(1.0..=5000.0).contains(&density)
        {
            return Err(invalid_input(format!(
                "density_metadata is {density} but has to be between 1 and 5000."
            )));
        }
        self.png_compression()?;
        Ok(())
    }
}

/// Reads the PNG compression setting.
fn parse_png_compression(value: &str) -> Result<CompressionType> {
    match value.trim().to_ascii_lowercase().as_str() {
        "fast" => Ok(CompressionType::Fast),
        "default" => Ok(CompressionType::Default),
        "best" => Ok(CompressionType::Best),
        "none" | "uncompressed" => Ok(CompressionType::Uncompressed),
        other => {
            let level = other.strip_prefix("level:").ok_or_else(|| {
                invalid_input(format!(
                    "png_compression is {value:?}; use fast, default, best, none or level:0 to level:9."
                ))
            })?;
            let level: u8 = level.trim().parse().map_err(|_| {
                invalid_input(format!("png_compression level {level:?} is not a number."))
            })?;
            if level > 9 {
                return Err(invalid_input(format!(
                    "png_compression level is {level} but has to be between 0 and 9."
                )));
            }
            Ok(CompressionType::Level(level))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_documented_compression_setting() {
        for value in ["fast", "default", "best", "none", "level:0", "level:9"] {
            assert!(parse_png_compression(value).is_ok(), "{value} should parse");
        }
        assert!(parse_png_compression("level:10").is_err());
        assert!(parse_png_compression("turbo").is_err());
    }

    #[test]
    fn rejects_an_out_of_range_quality() {
        let options = EncodeOptions { jpeg_quality: Some(0), ..EncodeOptions::default() };
        assert!(options.validate().is_err());
        assert_eq!(EncodeOptions::default().jpeg_quality(), DEFAULT_JPEG_QUALITY);
    }
}
