//! The canvas: background, padding and flattening.
//!
//! The canvas is exactly the requested size. It is filled before the document
//! is drawn onto it and, for a format that cannot carry transparency, composited
//! onto an opaque colour afterwards. Every step that changes what the caller
//! would otherwise have received is reported.

use crate::error::{Result, invalid_input};
use crate::render::fit::Padding;
use crate::warning::Warnings;
use serde_json::json;
use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};

/// Parses a CSS colour into a premultiplication-ready colour.
///
/// Accepts everything the SVG colour syntax does: hex in three, four, six and
/// eight digits, `rgb()`, `rgba()`, `hsl()`, `hsla()`, the SVG colour keywords
/// and `transparent`.
pub fn parse_color(text: &str) -> Result<Color> {
    let parsed: svgtypes::Color = text
        .trim()
        .parse()
        .map_err(|_| invalid_input(format!("{text:?} is not a colour this server understands.")))?;
    Ok(Color::from_rgba8(parsed.red, parsed.green, parsed.blue, parsed.alpha))
}

/// Parses an optional colour.
pub fn parse_optional_color(text: Option<&String>) -> Result<Option<Color>> {
    text.map(|t| parse_color(t)).transpose()
}

/// White, the flatten colour used when the caller named none.
pub fn white() -> Color {
    Color::from_rgba8(255, 255, 255, 255)
}

/// Creates the canvas and applies the background and padding fills.
///
/// Step order matters and is fixed: a transparent canvas, then `background`
/// over the whole of it, then `padding_color` over the padding alone.
pub fn prepare(
    width: u32,
    height: u32,
    background: Option<Color>,
    padding_color: Option<Color>,
    padding: Padding,
) -> Result<Pixmap> {
    let mut pixmap = Pixmap::new(width, height)
        .ok_or_else(|| invalid_input(format!("A {width}×{height} canvas cannot be allocated.")))?;

    if let Some(color) = background {
        pixmap.fill(color);
    }

    if let Some(color) = padding_color {
        for (x0, y0, x1, y1) in padding_regions(width, height, padding) {
            fill_region(&mut pixmap, x0, y0, x1, y1, color);
        }
    }

    Ok(pixmap)
}

/// The rectangles that make up the padding region, as half-open pixel ranges.
fn padding_regions(width: u32, height: u32, padding: Padding) -> Vec<(u32, u32, u32, u32)> {
    let top = padding.top.min(height);
    let bottom = height.saturating_sub(padding.bottom).max(top);
    let left = padding.left.min(width);
    let right = width.saturating_sub(padding.right).max(left);

    [
        (0, 0, width, top),
        (0, bottom, width, height),
        (0, top, left, bottom),
        (right, top, width, bottom),
    ]
    .into_iter()
    .filter(|(x0, y0, x1, y1)| x1 > x0 && y1 > y0)
    .collect()
}

/// Fills a half-open pixel range with a solid colour.
///
/// Written pixel by pixel rather than through a path fill, so that the region
/// is exactly the one computed and no edge rule can widen or narrow it.
fn fill_region(pixmap: &mut Pixmap, x0: u32, y0: u32, x1: u32, y1: u32, color: Color) {
    let premultiplied = color.premultiply().to_color_u8();
    let width = pixmap.width();
    let pixels = pixmap.pixels_mut();
    for y in y0..y1 {
        for x in x0..x1 {
            pixels[(y * width + x) as usize] = premultiplied;
        }
    }
}

/// Composites the canvas onto an opaque colour.
///
/// Used when the target format has no alpha channel, or when the caller asked
/// for an opaque result. Always raises `alpha_flattened`, naming the colour, so
/// that a caller who expected transparency learns it from the result.
pub fn flatten(pixmap: Pixmap, color: Color, reason: &str, warnings: &Warnings) -> Result<Pixmap> {
    let mut flattened = Pixmap::new(pixmap.width(), pixmap.height())
        .ok_or_else(|| invalid_input("The flattening canvas cannot be allocated."))?;
    flattened.fill(opaque(color));
    flattened.draw_pixmap(
        0,
        0,
        pixmap.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    warnings.warn_detail(
        "alpha_flattened",
        format!("{reason} The image was composited onto {}.", describe(color)),
        json!({ "color": describe(color), "reason": reason }),
    );
    Ok(flattened)
}

/// Drops the alpha of a colour, compositing it onto white first when it is
/// partly transparent.
pub fn opaque(color: Color) -> Color {
    let alpha = color.alpha();
    if alpha >= 1.0 {
        return color;
    }
    let blend = |channel: f32| channel * alpha + (1.0 - alpha);
    Color::from_rgba(blend(color.red()), blend(color.green()), blend(color.blue()), 1.0)
        .unwrap_or_else(white)
}

/// A hex form of a colour, for messages and results.
pub fn describe(color: Color) -> String {
    let c = color.to_color_u8();
    if c.alpha() == 255 {
        format!("#{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue())
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue(), c.alpha())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_svg_colour_syntax() {
        assert_eq!(describe(parse_color("#f00").unwrap()), "#ff0000");
        assert_eq!(describe(parse_color("red").unwrap()), "#ff0000");
        assert_eq!(describe(parse_color("rgb(0,0,255)").unwrap()), "#0000ff");
        assert_eq!(describe(parse_color("#00ff0080").unwrap()), "#00ff0080");
        assert!(parse_color("not a colour").is_err());
    }

    #[test]
    fn a_transparent_canvas_stays_transparent() {
        let pixmap = prepare(4, 4, None, None, Padding::default()).unwrap();
        assert!(pixmap.pixels().iter().all(|p| p.alpha() == 0));
    }

    #[test]
    fn background_fills_everything_and_padding_colour_only_the_border() {
        let padding = Padding { top: 1, right: 0, bottom: 1, left: 0 };
        let pixmap = prepare(
            4,
            4,
            Some(parse_color("#ff0000").unwrap()),
            Some(parse_color("#0000ff").unwrap()),
            padding,
        )
        .unwrap();
        let at = |x: u32, y: u32| {
            let p = pixmap.pixel(x, y).unwrap();
            (p.red(), p.green(), p.blue())
        };
        assert_eq!(at(0, 0), (0, 0, 255), "top row is padding");
        assert_eq!(at(0, 3), (0, 0, 255), "bottom row is padding");
        assert_eq!(at(0, 1), (255, 0, 0), "the middle keeps the background");
    }

    #[test]
    fn flattening_makes_every_pixel_opaque() {
        let pixmap = prepare(2, 2, None, None, Padding::default()).unwrap();
        let warnings = Warnings::new();
        let flat = flatten(pixmap, white(), "JPEG has no alpha channel.", &warnings).unwrap();
        assert!(flat.pixels().iter().all(|p| p.alpha() == 255));
        assert!(warnings.contains("alpha_flattened"));
    }

    #[test]
    fn a_translucent_flatten_colour_is_composited_onto_white() {
        let half_red = parse_color("#ff000080").unwrap();
        let result = opaque(half_red).to_color_u8();
        assert_eq!(result.alpha(), 255);
        assert!(result.green() > 100 && result.green() < 160);
    }
}
