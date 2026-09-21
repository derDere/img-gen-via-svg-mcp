//! Encoding a rendered canvas into an image file.

pub mod format;
#[cfg(feature = "icns")]
pub mod icns;
pub mod ico;
pub mod metadata;
pub mod options;
pub mod png_opt;
pub mod raster;
