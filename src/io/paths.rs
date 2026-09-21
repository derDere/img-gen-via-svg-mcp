//! Path policy.
//!
//! The caller chooses where files are read from and written to. An operator may
//! configure an allowlist; when a path falls outside it the call fails with a
//! named error. A path is never silently rewritten to a different one.

use crate::config::Config;
use crate::error::{ErrorCode, Result, ToolError};
use serde_json::json;
use std::path::{Component, Path, PathBuf};

/// Which side of the policy a path is being checked against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// A path the server reads from.
    Input,
    /// A path the server writes to.
    Output,
}

impl Direction {
    fn noun(self) -> &'static str {
        match self {
            Self::Input => "Input",
            Self::Output => "Output",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Input => "allowed_input_dirs",
            Self::Output => "allowed_output_dirs",
        }
    }
}

/// Resolves a caller-supplied path against the process's working directory and
/// checks it against the configured allowlist.
///
/// Returns the absolute path to use. The returned path is always the one the
/// caller asked for; the only alternative outcome is an error.
pub fn resolve(path: &Path, direction: Direction, config: &Config) -> Result<PathBuf> {
    let absolute = absolutise(path)?;
    let allowed = match direction {
        Direction::Input => &config.allowed_input_dirs,
        Direction::Output => &config.allowed_output_dirs,
    };
    if allowed.is_empty() {
        return Ok(absolute);
    }

    // An existing path is canonicalised so that a symlink cannot smuggle the
    // target out of an allowed directory; a path that does not exist yet is
    // checked in its lexically normalised form, with its nearest existing
    // ancestor canonicalised.
    let probe = if config.follow_symlinks { absolute.clone() } else { canonical_probe(&absolute) };

    let permitted = allowed.iter().any(|dir| {
        let dir = dir.canonicalize().unwrap_or_else(|_| dir.clone());
        probe.starts_with(&dir)
    });

    if permitted {
        Ok(absolute)
    } else {
        Err(ToolError::new(
            ErrorCode::PathNotAllowed,
            format!(
                "{} path {} is outside the configured allowlist.",
                direction.noun(),
                absolute.display()
            ),
        )
        .with_detail(json!({
            "path": absolute.display().to_string(),
            direction.key(): allowed.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        }))
        .with_hint(format!(
            "Choose a path under one of the allowed directories, or start the server without {}.",
            match direction {
                Direction::Input => "IMG_SVG_MCP_ALLOWED_INPUT_DIRS",
                Direction::Output => "IMG_SVG_MCP_ALLOWED_OUTPUT_DIRS",
            }
        )))
    }
}

/// Makes a path absolute against the process's working directory and removes
/// `.` and `..` components lexically.
pub fn absolutise(path: &Path) -> Result<PathBuf> {
    let base = if path.is_absolute() {
        path.to_path_buf()
    } else {
        let cwd = std::env::current_dir().map_err(|e| {
            ToolError::new(ErrorCode::InternalError, format!("Working directory unavailable: {e}"))
        })?;
        cwd.join(path)
    };
    Ok(normalise(&base))
}

/// Removes `.` and `..` components without touching the filesystem.
fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Canonicalises as much of the path as exists, so that a target that has not
/// been created yet can still be checked against the allowlist.
fn canonical_probe(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    let mut suffix = Vec::new();
    let mut cursor = path;
    while let Some(parent) = cursor.parent() {
        if let Some(name) = cursor.file_name() {
            suffix.push(name.to_os_string());
        }
        if let Ok(canonical) = parent.canonicalize() {
            let mut out = canonical;
            for name in suffix.iter().rev() {
                out.push(name);
            }
            return out;
        }
        cursor = parent;
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_dot_components() {
        assert_eq!(normalise(Path::new("/a/./b/../c")), PathBuf::from("/a/c"));
    }

    #[test]
    fn an_empty_allowlist_permits_everything() {
        let config = Config::default();
        let path = Path::new("/somewhere/else/out.png");
        assert_eq!(resolve(path, Direction::Output, &config).unwrap(), path);
    }

    #[test]
    fn an_allowlist_refuses_rather_than_redirects() {
        let config = Config {
            allowed_output_dirs: vec![PathBuf::from("/tmp/allowed")],
            ..Config::default()
        };
        let error = resolve(Path::new("/etc/logo.png"), Direction::Output, &config).unwrap_err();
        assert_eq!(error.code, ErrorCode::PathNotAllowed);
        assert!(error.message.contains("/etc/logo.png"));
    }
}
