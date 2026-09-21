//! Resolving the three input forms into document bytes.

use crate::config::Config;
use crate::error::{ErrorCode, Result, ToolError, invalid_input};
use crate::io::{paths, remote};
use std::io::Read;
use std::path::{Path, PathBuf};

/// A document loaded into memory, with the base directory its relative
/// references resolve against.
#[derive(Debug)]
pub struct LoadedSvg {
    /// The document text, decompressed when it arrived gzipped.
    pub text: String,
    /// The base directory for relative references, when one is known.
    pub resources_dir: Option<PathBuf>,
    /// Where the document came from, for messages and results.
    pub origin: String,
}

/// Loads a document from exactly one of the three input forms.
///
/// Zero or more than one is an error naming what was given, because a silent
/// precedence rule between them is the kind of surprise this server avoids.
pub fn load(
    svg_path: Option<&Path>,
    svg_source: Option<&str>,
    svg_url: Option<&str>,
    resources_dir: Option<&Path>,
    config: &Config,
) -> Result<LoadedSvg> {
    let given: Vec<&str> = [
        svg_path.map(|_| "svg_path"),
        svg_source.map(|_| "svg_source"),
        svg_url.map(|_| "svg_url"),
    ]
    .into_iter()
    .flatten()
    .collect();

    let resources_dir = match resources_dir {
        Some(dir) => Some(paths::resolve(dir, paths::Direction::Input, config)?),
        None => None,
    };

    match given.as_slice() {
        [] => {
            return Err(invalid_input(
                "Exactly one of svg_path, svg_source or svg_url is required; none was given.",
            ));
        }
        [_] => {}
        several => {
            return Err(invalid_input(format!(
                "Exactly one of svg_path, svg_source or svg_url is required; {} were given.",
                several.join(" and ")
            )));
        }
    }

    if let Some(path) = svg_path {
        let path = paths::resolve(path, paths::Direction::Input, config)?;
        let bytes = std::fs::read(&path).map_err(|e| {
            let code = if e.kind() == std::io::ErrorKind::NotFound {
                ErrorCode::InputNotFound
            } else {
                ErrorCode::InputUnreadable
            };
            ToolError::new(code, format!("Cannot read {}: {}", path.display(), e))
        })?;
        let text = decode(&bytes, &path.display().to_string())?;
        let resources_dir = resources_dir.or_else(|| path.parent().map(Path::to_path_buf));
        return Ok(LoadedSvg { text, resources_dir, origin: path.display().to_string() });
    }

    if let Some(source) = svg_source {
        return Ok(LoadedSvg {
            text: source.to_string(),
            resources_dir,
            origin: "svg_source".to_string(),
        });
    }

    let url = svg_url.expect("one input form is present");
    let bytes = remote::fetch(url, config)?;
    let text = decode(&bytes, url)?;
    Ok(LoadedSvg { text, resources_dir, origin: url.to_string() })
}

/// Turns document bytes into text, decompressing gzip and rejecting anything
/// that is not UTF-8.
fn decode(bytes: &[u8], origin: &str) -> Result<String> {
    let raw = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = flate2::read::GzDecoder::new(bytes);
        let mut out = Vec::new();
        decoder.read_to_end(&mut out).map_err(|e| {
            ToolError::new(
                ErrorCode::ParseFailed,
                format!("{origin} is gzip-compressed but cannot be decompressed: {e}"),
            )
        })?;
        out
    } else {
        bytes.to_vec()
    };

    String::from_utf8(raw).map_err(|_| {
        ToolError::new(ErrorCode::ParseFailed, format!("{origin} is not UTF-8 encoded."))
            .with_hint("SVG input has to be UTF-8; convert the file and try again.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_zero_and_several_sources() {
        let config = Config::default();
        assert!(load(None, None, None, None, &config).is_err());
        let error =
            load(Some(Path::new("/tmp/a.svg")), Some("<svg/>"), None, None, &config).unwrap_err();
        assert!(error.message.contains("svg_path and svg_source"));
    }

    #[test]
    fn takes_a_source_string_verbatim() {
        let config = Config::default();
        let loaded = load(None, Some("<svg/>"), None, None, &config).unwrap();
        assert_eq!(loaded.text, "<svg/>");
        assert_eq!(loaded.origin, "svg_source");
    }
}
