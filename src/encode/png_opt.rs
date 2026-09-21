//! Lossless post-optimisation of an encoded PNG.

use crate::error::Result;
use crate::warning::Warnings;

/// Runs a lossless optimisation pass over encoded PNG bytes.
///
/// The decoded pixels are identical afterwards; only the file is smaller. A
/// pass that fails leaves the input untouched and reports why, because a
/// smaller file is never worth losing the image over.
#[cfg(feature = "png-optimize")]
pub fn optimise(data: Vec<u8>, warnings: &Warnings) -> Result<Vec<u8>> {
    let options = oxipng::Options::from_preset(2);
    match oxipng::optimize_from_memory(&data, &options) {
        Ok(optimised) if optimised.len() < data.len() => Ok(optimised),
        Ok(_) => Ok(data),
        Err(error) => {
            warnings.warn(
                "png_optimize_failed",
                format!("The PNG optimisation pass did not run: {error}. The unoptimised image was written."),
            );
            Ok(data)
        }
    }
}

/// Reports that this build cannot optimise, and returns the input untouched.
#[cfg(not(feature = "png-optimize"))]
pub fn optimise(data: Vec<u8>, warnings: &Warnings) -> Result<Vec<u8>> {
    warnings.warn(
        "png_optimize_failed",
        "This build has no PNG optimiser, so png_optimize had no effect.",
    );
    Ok(data)
}
