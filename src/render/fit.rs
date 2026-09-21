//! Mapping a document onto a canvas of exactly the requested size.
//!
//! The canvas is always exactly the size the caller asked for. The strategy
//! decides what happens to a drawing whose aspect ratio differs from it, and
//! the result says which strategy took effect and what it cost.

use crate::error::{ErrorCode, Result, ToolError};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// What to do when the requested aspect ratio differs from the document's.
#[derive(
    Debug, Clone, Copy, Default, Deserialize, Serialize, schemars::JsonSchema, PartialEq, Eq,
)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// Scale uniformly to fit entirely inside the canvas; the rest is padding.
    /// The default: it neither distorts the drawing nor discards part of it.
    #[default]
    Contain,
    /// Scale each axis independently so the drawing fills the canvas.
    Stretch,
    /// Scale uniformly to cover the canvas; the overflow is cropped.
    Cover,
    /// Refuse a mismatched aspect ratio instead of resolving it.
    Error,
}

/// Where the drawing sits when it does not fill the canvas.
#[derive(
    Debug, Clone, Copy, Default, Deserialize, Serialize, schemars::JsonSchema, PartialEq, Eq,
)]
#[serde(rename_all = "kebab-case")]
pub enum Align {
    /// Centred on both axes. The default.
    #[default]
    Center,
    /// Centred horizontally, flush with the top.
    Top,
    /// Centred horizontally, flush with the bottom.
    Bottom,
    /// Flush with the left, centred vertically.
    Left,
    /// Flush with the right, centred vertically.
    Right,
    /// Flush with the top-left corner.
    TopLeft,
    /// Flush with the top-right corner.
    TopRight,
    /// Flush with the bottom-left corner.
    BottomLeft,
    /// Flush with the bottom-right corner.
    BottomRight,
}

impl Align {
    /// The horizontal and vertical fractions of the leftover space that go
    /// before the drawing, each in `0.0..=1.0`.
    fn factors(self) -> (f32, f32) {
        match self {
            Self::Center => (0.5, 0.5),
            Self::Top => (0.5, 0.0),
            Self::Bottom => (0.5, 1.0),
            Self::Left => (0.0, 0.5),
            Self::Right => (1.0, 0.5),
            Self::TopLeft => (0.0, 0.0),
            Self::TopRight => (1.0, 0.0),
            Self::BottomLeft => (0.0, 1.0),
            Self::BottomRight => (1.0, 1.0),
        }
    }
}

/// Padding in whole pixels on each side of the canvas.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct Padding {
    /// Pixels above the drawing.
    pub top: u32,
    /// Pixels to the right of the drawing.
    pub right: u32,
    /// Pixels below the drawing.
    pub bottom: u32,
    /// Pixels to the left of the drawing.
    pub left: u32,
}

impl Padding {
    /// Whether any side carries padding.
    pub fn is_empty(&self) -> bool {
        self.top == 0 && self.right == 0 && self.bottom == 0 && self.left == 0
    }
}

/// How the document is placed on the canvas.
#[derive(Debug, Clone, Copy)]
pub struct Mapping {
    /// The transform handed to the renderer.
    pub transform: tiny_skia::Transform,
    /// Scale factors actually applied, per axis.
    pub scale: (f32, f32),
    /// Translation actually applied, per axis.
    pub offset: (f32, f32),
    /// The drawing's rectangle on the canvas, in whole pixels.
    pub drawn: (i64, i64, u32, u32),
    /// The padding around it.
    pub padding: Padding,
    /// Whether the strategy had any effect at all.
    pub adjusted: bool,
}

/// Computes the mapping from a document of `source` size onto a `canvas`.
pub fn map(source: (f32, f32), canvas: (u32, u32), fit: Fit, align: Align) -> Result<Mapping> {
    let (source_width, source_height) = source;
    let (canvas_width, canvas_height) = (canvas.0 as f32, canvas.1 as f32);

    if source_width <= 0.0 || source_height <= 0.0 {
        return Err(ToolError::new(
            ErrorCode::EmptyRender,
            "The document has zero width or height, so there is nothing to render.",
        ));
    }

    let source_ratio = source_width / source_height;
    let canvas_ratio = canvas_width / canvas_height;
    let adjusted = (source_ratio - canvas_ratio).abs() > 1e-4;

    if fit == Fit::Error && adjusted {
        let fitted = (canvas_width / source_ratio).round() as u32;
        return Err(ToolError::new(
            ErrorCode::AspectMismatch,
            format!(
                "The requested {}×{} has aspect ratio {canvas_ratio:.4} but the document has {source_ratio:.4}, and fit is error.",
                canvas.0, canvas.1
            ),
        )
        .with_detail(json!({
            "source_ratio": source_ratio,
            "requested_ratio": canvas_ratio,
            "source": { "width": source_width, "height": source_height },
            "fitting_size": { "width": canvas.0, "height": fitted },
        }))
        .with_hint("Use fit contain, cover or stretch, or request a size with the document's aspect ratio."));
    }

    let (scale_x, scale_y) = match fit {
        Fit::Stretch => (canvas_width / source_width, canvas_height / source_height),
        Fit::Contain | Fit::Error => {
            let s = (canvas_width / source_width).min(canvas_height / source_height);
            (s, s)
        }
        Fit::Cover => {
            let s = (canvas_width / source_width).max(canvas_height / source_height);
            (s, s)
        }
    };

    let drawn_width = source_width * scale_x;
    let drawn_height = source_height * scale_y;
    let (fx, fy) = align.factors();
    let offset_x = (canvas_width - drawn_width) * fx;
    let offset_y = (canvas_height - drawn_height) * fy;

    let left = offset_x.round() as i64;
    let top = offset_y.round() as i64;
    let width = drawn_width.round().max(1.0) as u32;
    let height = drawn_height.round().max(1.0) as u32;

    let padding = Padding {
        left: left.max(0) as u32,
        top: top.max(0) as u32,
        right: (canvas.0 as i64 - (left + width as i64)).max(0) as u32,
        bottom: (canvas.1 as i64 - (top + height as i64)).max(0) as u32,
    };

    let transform =
        tiny_skia::Transform::from_translate(offset_x, offset_y).pre_scale(scale_x, scale_y);

    Ok(Mapping {
        transform,
        scale: (scale_x, scale_y),
        offset: (offset_x, offset_y),
        drawn: (left, top, width, height),
        padding,
        adjusted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: (f32, f32) = (100.0, 80.0);

    #[test]
    fn contain_letterboxes_and_keeps_the_canvas_exact() {
        let mapping = map(SOURCE, (512, 512), Fit::Contain, Align::Center).unwrap();
        assert_eq!(mapping.scale.0, mapping.scale.1);
        assert_eq!(mapping.drawn.2, 512);
        assert_eq!(mapping.drawn.3, 410);
        assert_eq!(mapping.padding.top, 51);
        assert_eq!(mapping.padding.bottom, 51);
        assert_eq!(mapping.padding.left, 0);
        assert!(mapping.adjusted);
    }

    #[test]
    fn alignment_moves_the_padding() {
        let mapping = map(SOURCE, (512, 512), Fit::Contain, Align::Top).unwrap();
        assert_eq!(mapping.padding.top, 0);
        assert_eq!(mapping.padding.bottom, 102);
    }

    #[test]
    fn stretch_fills_the_canvas_and_distorts() {
        let mapping = map(SOURCE, (512, 512), Fit::Stretch, Align::Center).unwrap();
        assert!(mapping.padding.is_empty());
        assert!((mapping.scale.0 - 5.12).abs() < 1e-4);
        assert!((mapping.scale.1 - 6.4).abs() < 1e-4);
    }

    #[test]
    fn cover_crops_and_leaves_no_padding() {
        let mapping = map(SOURCE, (512, 512), Fit::Cover, Align::Center).unwrap();
        assert!(mapping.padding.is_empty());
        assert_eq!(mapping.scale.0, mapping.scale.1);
        assert!(mapping.drawn.2 > 512);
    }

    #[test]
    fn error_refuses_a_mismatch_but_permits_a_match() {
        assert_eq!(
            map(SOURCE, (512, 512), Fit::Error, Align::Center).unwrap_err().code,
            ErrorCode::AspectMismatch
        );
        assert!(map(SOURCE, (200, 160), Fit::Error, Align::Center).is_ok());
    }

    #[test]
    fn a_matching_ratio_reports_no_adjustment() {
        let mapping = map(SOURCE, (200, 160), Fit::Contain, Align::Center).unwrap();
        assert!(!mapping.adjusted);
        assert!(mapping.padding.is_empty());
    }
}
