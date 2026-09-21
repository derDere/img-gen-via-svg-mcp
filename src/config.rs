//! Operator configuration.
//!
//! Configuration belongs to whoever runs the process, never to the caller. It
//! is read once at start-up from environment variables, optionally overridden
//! by a TOML file named by `IMG_SVG_MCP_CONFIG`. A per-call parameter may only
//! narrow what is configured here, never widen it.

use crate::error::{Result, ToolError, invalid_input};
use serde::Deserialize;
use std::path::PathBuf;

/// Everything the operator can set.
#[derive(Debug, Clone)]
pub struct Config {
    /// Upper bound on `width * height` for one render.
    pub max_pixels: u64,
    /// Upper bound on the size of an inline image result, in bytes.
    pub max_inline_bytes: u64,
    /// Intrinsic size used when a document declares none.
    pub default_size: (f32, f32),
    /// Wall-clock budget for one render, in milliseconds.
    pub render_timeout_ms: u64,
    /// Directories inputs may be read from. Empty means no restriction.
    pub allowed_input_dirs: Vec<PathBuf>,
    /// Directories outputs may be written to. Empty means no restriction.
    pub allowed_output_dirs: Vec<PathBuf>,
    /// Whether a symlink may lead out of an allowed directory.
    pub follow_symlinks: bool,
    /// Whether `svg_url` may be used.
    pub remote_input: bool,
    /// Whether a document may pull in resources over the network.
    pub remote_svg_references: bool,
    /// Host names remote access is limited to. Empty means any host.
    pub remote_allowlist: Vec<String>,
    /// Size limit for one fetched remote resource, in bytes.
    pub remote_max_bytes: u64,
    /// Extra font directories loaded at start-up.
    pub font_dirs: Vec<PathBuf>,
    /// Extra font files loaded at start-up.
    pub font_files: Vec<PathBuf>,
    /// Whether the platform's system fonts are ignored.
    pub skip_system_fonts: bool,
    /// Family used when a document sets no `font-family`.
    pub default_family: String,
    /// Resolution of the generic `serif` family, when pinned.
    pub serif_family: Option<String>,
    /// Resolution of the generic `sans-serif` family, when pinned.
    pub sans_serif_family: Option<String>,
    /// Resolution of the generic `cursive` family, when pinned.
    pub cursive_family: Option<String>,
    /// Resolution of the generic `fantasy` family, when pinned.
    pub fantasy_family: Option<String>,
    /// Resolution of the generic `monospace` family, when pinned.
    pub monospace_family: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_pixels: 100_000_000,
            max_inline_bytes: 5_242_880,
            default_size: (100.0, 100.0),
            render_timeout_ms: 30_000,
            allowed_input_dirs: Vec::new(),
            allowed_output_dirs: Vec::new(),
            follow_symlinks: false,
            remote_input: false,
            remote_svg_references: false,
            remote_allowlist: Vec::new(),
            remote_max_bytes: 26_214_400,
            font_dirs: Vec::new(),
            font_files: Vec::new(),
            skip_system_fonts: false,
            default_family: "Times New Roman".to_string(),
            serif_family: None,
            sans_serif_family: None,
            cursive_family: None,
            fantasy_family: None,
            monospace_family: None,
        }
    }
}

/// The subset of [`Config`] a TOML file may set. Absent keys keep the value
/// the environment produced.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    max_pixels: Option<u64>,
    max_inline_bytes: Option<u64>,
    default_size: Option<String>,
    render_timeout_ms: Option<u64>,
    allowed_input_dirs: Option<Vec<PathBuf>>,
    allowed_output_dirs: Option<Vec<PathBuf>>,
    follow_symlinks: Option<bool>,
    remote_input: Option<bool>,
    remote_svg_references: Option<bool>,
    remote_allowlist: Option<Vec<String>>,
    remote_max_bytes: Option<u64>,
    font_dirs: Option<Vec<PathBuf>>,
    font_files: Option<Vec<PathBuf>>,
    skip_system_fonts: Option<bool>,
    default_family: Option<String>,
    serif_family: Option<String>,
    sans_serif_family: Option<String>,
    cursive_family: Option<String>,
    fantasy_family: Option<String>,
    monospace_family: Option<String>,
    log_level: Option<String>,
}

impl Config {
    /// Reads the configuration from the environment and, when
    /// `IMG_SVG_MCP_CONFIG` names one, from a TOML file on top of it.
    pub fn load() -> Result<Self> {
        let mut config = Self::from_env()?;
        if let Ok(path) = std::env::var("IMG_SVG_MCP_CONFIG") {
            let text = std::fs::read_to_string(&path).map_err(|e| {
                invalid_input(format!("Configuration file {path} cannot be read: {e}"))
            })?;
            let file: FileConfig = toml::from_str(&text).map_err(|e| {
                invalid_input(format!("Configuration file {path} is not valid TOML: {e}"))
            })?;
            config.apply_file(file)?;
        }
        Ok(config)
    }

    /// Reads the configuration from environment variables alone.
    pub fn from_env() -> Result<Self> {
        let mut config = Self::default();
        if let Some(v) = env_u64("IMG_SVG_MCP_MAX_PIXELS")? {
            config.max_pixels = v;
        }
        if let Some(v) = env_u64("IMG_SVG_MCP_MAX_INLINE_BYTES")? {
            config.max_inline_bytes = v;
        }
        if let Ok(v) = std::env::var("IMG_SVG_MCP_DEFAULT_SIZE") {
            config.default_size = parse_size(&v)?;
        }
        if let Some(v) = env_u64("IMG_SVG_MCP_RENDER_TIMEOUT_MS")? {
            config.render_timeout_ms = v;
        }
        if let Ok(v) = std::env::var("IMG_SVG_MCP_ALLOWED_INPUT_DIRS") {
            config.allowed_input_dirs = split_paths(&v);
        }
        if let Ok(v) = std::env::var("IMG_SVG_MCP_ALLOWED_OUTPUT_DIRS") {
            config.allowed_output_dirs = split_paths(&v);
        }
        if let Some(v) = env_bool("IMG_SVG_MCP_FOLLOW_SYMLINKS")? {
            config.follow_symlinks = v;
        }
        if let Some(v) = env_bool("IMG_SVG_MCP_REMOTE_INPUT")? {
            config.remote_input = v;
        }
        if let Some(v) = env_bool("IMG_SVG_MCP_REMOTE_SVG_REFERENCES")? {
            config.remote_svg_references = v;
        }
        if let Ok(v) = std::env::var("IMG_SVG_MCP_REMOTE_ALLOWLIST") {
            config.remote_allowlist =
                v.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect();
        }
        if let Some(v) = env_u64("IMG_SVG_MCP_REMOTE_MAX_BYTES")? {
            config.remote_max_bytes = v;
        }
        if let Ok(v) = std::env::var("IMG_SVG_MCP_FONT_DIRS") {
            config.font_dirs = split_paths(&v);
        }
        if let Ok(v) = std::env::var("IMG_SVG_MCP_FONT_FILES") {
            config.font_files = split_paths(&v);
        }
        if let Some(v) = env_bool("IMG_SVG_MCP_SKIP_SYSTEM_FONTS")? {
            config.skip_system_fonts = v;
        }
        if let Ok(v) = std::env::var("IMG_SVG_MCP_DEFAULT_FAMILY") {
            config.default_family = v;
        }
        Ok(config)
    }

    fn apply_file(&mut self, file: FileConfig) -> Result<()> {
        macro_rules! set {
            ($($field:ident),* $(,)?) => {$(
                if let Some(v) = file.$field { self.$field = v; }
            )*};
        }
        set!(
            max_pixels,
            max_inline_bytes,
            render_timeout_ms,
            allowed_input_dirs,
            allowed_output_dirs,
            follow_symlinks,
            remote_input,
            remote_svg_references,
            remote_allowlist,
            remote_max_bytes,
            font_dirs,
            font_files,
            skip_system_fonts,
            default_family,
        );
        if let Some(v) = file.default_size {
            self.default_size = parse_size(&v)?;
        }
        for (target, value) in [
            (&mut self.serif_family, file.serif_family),
            (&mut self.sans_serif_family, file.sans_serif_family),
            (&mut self.cursive_family, file.cursive_family),
            (&mut self.fantasy_family, file.fantasy_family),
            (&mut self.monospace_family, file.monospace_family),
        ] {
            if value.is_some() {
                *target = value;
            }
        }
        let _ = file.log_level;
        Ok(())
    }
}

/// Splits a colon-separated (Windows: semicolon-separated) path list.
fn split_paths(value: &str) -> Vec<PathBuf> {
    let separator = if cfg!(windows) { ';' } else { ':' };
    value.split(separator).map(str::trim).filter(|s| !s.is_empty()).map(PathBuf::from).collect()
}

/// Parses a `WIDTHxHEIGHT` size.
fn parse_size(value: &str) -> Result<(f32, f32)> {
    let (w, h) = value
        .split_once(['x', 'X'])
        .ok_or_else(|| invalid_input(format!("Size {value} is not in WIDTHxHEIGHT form.")))?;
    let w: f32 = w.trim().parse().map_err(|_| invalid_input(format!("Bad width in {value}.")))?;
    let h: f32 = h.trim().parse().map_err(|_| invalid_input(format!("Bad height in {value}.")))?;
    if w <= 0.0 || h <= 0.0 {
        return Err(invalid_input(format!("Size {value} must be positive.")));
    }
    Ok((w, h))
}

fn env_u64(name: &str) -> Result<Option<u64>> {
    match std::env::var(name) {
        Ok(v) => v
            .trim()
            .parse()
            .map(Some)
            .map_err(|_| invalid_input(format!("{name} must be a whole number, got {v:?}."))),
        Err(_) => Ok(None),
    }
}

fn env_bool(name: &str) -> Result<Option<bool>> {
    match std::env::var(name) {
        Ok(v) => match v.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(Some(true)),
            "0" | "false" | "no" | "off" => Ok(Some(false)),
            other => Err(invalid_input(format!("{name} must be a boolean, got {other:?}."))),
        },
        Err(_) => Ok(None),
    }
}

impl From<toml::de::Error> for ToolError {
    fn from(value: toml::de::Error) -> Self {
        invalid_input(format!("Invalid TOML: {value}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_size() {
        assert_eq!(parse_size("640x480").unwrap(), (640.0, 480.0));
        assert!(parse_size("640").is_err());
        assert!(parse_size("0x10").is_err());
    }

    #[test]
    fn defaults_are_the_documented_ones() {
        let config = Config::default();
        assert_eq!(config.max_pixels, 100_000_000);
        assert_eq!(config.render_timeout_ms, 30_000);
        assert!(!config.remote_input);
        assert!(!config.remote_svg_references);
        assert!(config.allowed_output_dirs.is_empty());
    }
}
