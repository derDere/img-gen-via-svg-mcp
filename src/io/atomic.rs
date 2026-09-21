//! Atomic file writing.
//!
//! Every output is written to a temporary file in the target directory and then
//! renamed into place, so a failure part-way through leaves no truncated image
//! and no half-written file behind.

use crate::error::{ErrorCode, Result, ToolError};
use serde_json::json;
use std::io::Write;
use std::path::Path;

/// Writes `data` to `path`.
///
/// `overwrite` decides what happens when the target exists; `create_dirs`
/// decides what happens when its parent does not.
pub fn write(path: &Path, data: &[u8], overwrite: bool, create_dirs: bool) -> Result<()> {
    if !overwrite && path.exists() {
        return Err(ToolError::new(
            ErrorCode::OutputExists,
            format!("{} exists and overwrite is false.", path.display()),
        )
        .with_detail(json!({ "path": path.display().to_string() }))
        .with_hint("Pass overwrite: true, or choose another output_path."));
    }

    let parent = path.parent().ok_or_else(|| {
        ToolError::new(
            ErrorCode::OutputUnwritable,
            format!("{} has no parent directory.", path.display()),
        )
    })?;

    if !parent.exists() {
        if !create_dirs {
            return Err(ToolError::new(
                ErrorCode::OutputUnwritable,
                format!("Directory {} does not exist.", parent.display()),
            )
            .with_detail(json!({ "directory": parent.display().to_string() }))
            .with_hint("Pass create_dirs: true, or create the directory first."));
        }
        std::fs::create_dir_all(parent).map_err(|e| unwritable(parent, e))?;
    }

    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("output"),
        std::process::id()
    ));

    let outcome = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(data)?;
        file.sync_all()
    })();

    if let Err(error) = outcome {
        let _ = std::fs::remove_file(&temporary);
        return Err(unwritable(path, error));
    }

    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(unwritable(path, error));
    }
    Ok(())
}

fn unwritable(path: &Path, error: std::io::Error) -> ToolError {
    ToolError::new(
        ErrorCode::OutputUnwritable,
        format!("Cannot write {}: {}", path.display(), error),
    )
    .with_detail(json!({ "path": path.display().to_string(), "os_error": error.to_string() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_an_existing_target_without_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.bin");
        write(&path, b"first", true, false).unwrap();
        let error = write(&path, b"second", false, false).unwrap_err();
        assert_eq!(error.code, ErrorCode::OutputExists);
        assert_eq!(std::fs::read(&path).unwrap(), b"first");
    }

    #[test]
    fn leaves_no_temporary_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.bin");
        write(&path, b"data", true, false).unwrap();
        let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().collect();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn refuses_a_missing_parent_without_create_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/deep/c.bin");
        let error = write(&path, b"data", true, false).unwrap_err();
        assert_eq!(error.code, ErrorCode::OutputUnwritable);
        assert!(!dir.path().join("nested").exists());
    }
}
