//! The size model.
//!
//! An absolute pixel size is the primary way of asking for an output size, and
//! the rules below are applied in order, first match winning. Nothing is ever
//! silently clamped: a size beyond a limit is an error.

use crate::error::{ErrorCode, Result, ToolError, invalid_input};
use crate::svg::scan::ScanReport;
use crate::warning::Warnings;
use serde::Serialize;
use serde_json::json;

/// Where the document's intrinsic size came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeOrigin {
    /// The root `width` and `height` attributes.
    Attributes,
    /// The root `viewBox`.
    ViewBox,
    /// The configured fallback, because the document declares neither.
    DefaultSize,
}

/// The document's own size, and where it was read from.
#[derive(Debug, Clone, Copy)]
pub struct SourceSize {
    /// Width in user units.
    pub width: f32,
    /// Height in user units.
    pub height: f32,
    /// Which rule produced it.
    pub origin: SizeOrigin,
}

/// What the caller asked for.
#[derive(Debug, Clone, Copy, Default)]
pub struct SizeRequest {
    /// Absolute target width in pixels.
    pub width: Option<u32>,
    /// Absolute target height in pixels.
    pub height: Option<u32>,
    /// Multiplier applied to the intrinsic size.
    pub scale: Option<f32>,
}

/// Reads the intrinsic size off the resolved tree and classifies its origin.
pub fn source_size(tree: &usvg::Tree, scan: &ScanReport, warnings: &Warnings) -> SourceSize {
    let size = tree.size();
    let has_absolute_attributes = matches!(
        (&scan.root_width, &scan.root_height),
        (Some(w), Some(h)) if !w.trim().ends_with('%') && !h.trim().ends_with('%')
    );
    let origin = if has_absolute_attributes {
        SizeOrigin::Attributes
    } else if scan.root_view_box.is_some() {
        SizeOrigin::ViewBox
    } else {
        warnings.warn(
            "size_fallback",
            "The document declares neither an absolute size nor a viewBox; the configured default size is used.",
        );
        SizeOrigin::DefaultSize
    };
    SourceSize { width: size.width(), height: size.height(), origin }
}

/// Resolves the requested canvas size in pixels.
///
/// `max_pixels` bounds the product of the two dimensions.
pub fn resolve(source: SourceSize, request: SizeRequest, max_pixels: u64) -> Result<(u32, u32)> {
    if request.scale.is_some() && (request.width.is_some() || request.height.is_some()) {
        return Err(invalid_input(
            "scale cannot be combined with width or height; give either an absolute size or a factor.",
        )
        .with_hint("Drop scale and pass width and height in pixels, which is the primary form."));
    }

    let (width, height) = match (request.width, request.height, request.scale) {
        (Some(w), Some(h), _) => (w, h),
        (Some(w), None, _) => {
            let h = (w as f32 * source.height / source.width).ceil() as u32;
            (w, h.max(1))
        }
        (None, Some(h), _) => {
            let w = (h as f32 * source.width / source.height).ceil() as u32;
            (w.max(1), h)
        }
        (None, None, Some(scale)) => {
            if !(scale.is_finite() && scale > 0.0) {
                return Err(invalid_input(format!(
                    "scale must be a positive number, got {scale}."
                )));
            }
            (
                (source.width * scale).ceil().max(1.0) as u32,
                (source.height * scale).ceil().max(1.0) as u32,
            )
        }
        (None, None, None) => {
            (source.width.ceil().max(1.0) as u32, source.height.ceil().max(1.0) as u32)
        }
    };

    for (name, value) in [("width", width), ("height", height)] {
        if value == 0 {
            return Err(invalid_input(format!("{name} must be at least 1 pixel.")));
        }
        if value > 65535 {
            return Err(ToolError::new(
                ErrorCode::SizeLimitExceeded,
                format!("{name} is {value} pixels; the limit is 65535."),
            )
            .with_detail(json!({ name: value, "limit": 65535 })));
        }
    }

    let pixels = width as u64 * height as u64;
    if pixels > max_pixels {
        return Err(ToolError::new(
            ErrorCode::SizeLimitExceeded,
            format!("{width}×{height} is {pixels} pixels; the configured limit is {max_pixels}."),
        )
        .with_detail(
            json!({ "width": width, "height": height, "pixels": pixels, "max_pixels": max_pixels }),
        )
        .with_hint("Render smaller, or raise IMG_SVG_MCP_MAX_PIXELS."));
    }

    Ok((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> SourceSize {
        SourceSize { width: 100.0, height: 80.0, origin: SizeOrigin::Attributes }
    }

    #[test]
    fn an_absolute_size_is_taken_verbatim() {
        assert_eq!(
            resolve(
                source(),
                SizeRequest { width: Some(512), height: Some(512), scale: None },
                u64::MAX
            )
            .unwrap(),
            (512, 512)
        );
    }

    #[test]
    fn one_dimension_derives_the_other() {
        assert_eq!(
            resolve(
                source(),
                SizeRequest { width: Some(512), height: None, scale: None },
                u64::MAX
            )
            .unwrap(),
            (512, 410)
        );
        assert_eq!(
            resolve(
                source(),
                SizeRequest { width: None, height: Some(512), scale: None },
                u64::MAX
            )
            .unwrap(),
            (640, 512)
        );
    }

    #[test]
    fn a_scale_multiplies_the_intrinsic_size() {
        assert_eq!(
            resolve(
                source(),
                SizeRequest { width: None, height: None, scale: Some(2.0) },
                u64::MAX
            )
            .unwrap(),
            (200, 160)
        );
    }

    #[test]
    fn nothing_given_keeps_the_intrinsic_size() {
        assert_eq!(resolve(source(), SizeRequest::default(), u64::MAX).unwrap(), (100, 80));
    }

    #[test]
    fn scale_with_width_is_refused_rather_than_ranked() {
        let error = resolve(
            source(),
            SizeRequest { width: Some(10), height: None, scale: Some(2.0) },
            u64::MAX,
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn limits_produce_an_error_not_a_clamp() {
        let error = resolve(
            source(),
            SizeRequest { width: Some(70000), height: Some(10), scale: None },
            u64::MAX,
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::SizeLimitExceeded);

        let error = resolve(
            source(),
            SizeRequest { width: Some(20000), height: Some(20000), scale: None },
            100_000_000,
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::SizeLimitExceeded);
    }
}
