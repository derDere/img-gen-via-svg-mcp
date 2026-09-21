//! One module per tool. Each exposes a `run` function that takes the server
//! state and the tool's parameters and returns the outcome, so that every tool
//! is testable without a protocol client.

pub mod capabilities;
pub mod convert_image;
pub mod optimize_svg;
pub mod probe_svg;
pub mod render_icon;
pub mod render_svg;
pub mod render_svg_batch;
