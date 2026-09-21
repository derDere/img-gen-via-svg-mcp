//! `convert_image` — convert a raster image between formats, sizes and
//! representations.

use crate::encode::format;
use crate::encode::raster;
use crate::error::{ErrorCode, Result, ToolError, invalid_input};
use crate::io::{paths, raster_in};
use crate::mcp::params::{ConvertImageParams, ConvertReturnMode, ResampleFilter};
use crate::mcp::result::{InlineImage, ToolOutcome, warning_suffix, warnings_json};
use crate::mcp::state::ServerState;
use crate::pipeline;
use crate::render::canvas;
use crate::render::fit;
use crate::render::sizing::{self, SizeOrigin, SizeRequest, SourceSize};
use crate::warning::Warnings;
use base64::Engine;
use image::{DynamicImage, imageops::FilterType};
use serde_json::json;

/// Runs the tool.
pub fn run(state: &ServerState, params: ConvertImageParams) -> Result<ToolOutcome> {
    let return_mode = params.return_mode.unwrap_or_default();
    let writes_file = matches!(return_mode, ConvertReturnMode::File | ConvertReturnMode::Both);
    if writes_file && params.output_path.is_none() {
        return Err(invalid_input("output_path is required unless return_mode returns data."));
    }
    if !writes_file && params.output_path.is_some() {
        return Err(invalid_input(
            "The chosen return_mode writes no file, but output_path was given.",
        ));
    }

    let warnings = Warnings::new();
    let source = load_source(state, &params)?;
    let source_format = source.format;
    let image = source.image;

    let target_format = format::resolve(params.format, params.output_path.as_deref())?;
    let encode = params.encode();
    encode.validate()?;

    let transparent = params.transparent;
    if transparent == Some(true) && !target_format.has_alpha() {
        return Err(ToolError::new(
            ErrorCode::UnsupportedFormat,
            format!(
                "{} has no alpha channel, so transparent: true cannot be honoured.",
                target_format.as_str()
            ),
        ));
    }

    let source_size = SourceSize {
        width: image.width() as f32,
        height: image.height() as f32,
        origin: SizeOrigin::Attributes,
    };
    let canvas_size = sizing::resolve(
        source_size,
        SizeRequest { width: params.width, height: params.height, scale: params.scale },
        state.config.max_pixels,
    )?;

    let fit_strategy = params.fit.unwrap_or_default();
    let align = params.align.unwrap_or_default();
    let mapping =
        fit::map((source_size.width, source_size.height), canvas_size, fit_strategy, align)?;

    let background = canvas::parse_optional_color(params.background.as_ref())?;
    let padding_color = canvas::parse_optional_color(params.padding_color.as_ref())?;

    let resized = place(&image, &mapping, canvas_size, params.filter.unwrap_or_default());
    let mut composed =
        compose(resized, canvas_size, background, padding_color, mapping.padding, &mapping);

    if mapping.adjusted {
        warnings.warn_detail(
            "aspect_adjusted",
            format!(
                "The source aspect ratio differs from the requested one; fit {} was applied.",
                format!("{fit_strategy:?}").to_lowercase()
            ),
            json!({
                "strategy": format!("{fit_strategy:?}").to_lowercase(),
                "padding_px": {
                    "top": mapping.padding.top, "right": mapping.padding.right,
                    "bottom": mapping.padding.bottom, "left": mapping.padding.left
                },
            }),
        );
    }

    let must_flatten = !target_format.has_alpha() || transparent == Some(false);
    if must_flatten {
        let colour = background.map(canvas::opaque).unwrap_or_else(canvas::white);
        composed = flatten_image(&composed, colour);
        warnings.warn_detail(
            "alpha_flattened",
            format!(
                "{} cannot carry transparency, so the image was composited onto {}.",
                target_format.as_str(),
                canvas::describe(colour)
            ),
            json!({ "color": canvas::describe(colour) }),
        );
    }

    let bytes = raster::encode_image(&composed, target_format, &encode, &warnings)?;

    let written = if writes_file {
        let path = params.output_path.as_deref().expect("checked above");
        Some(pipeline::write_output(
            path,
            &bytes,
            params.overwrite.unwrap_or(true),
            params.create_dirs.unwrap_or(false),
            &state.config,
        )?)
    } else {
        None
    };

    let limit = params.max_inline_bytes.unwrap_or(state.config.max_inline_bytes);
    let wants_data = matches!(
        return_mode,
        ConvertReturnMode::Base64 | ConvertReturnMode::Both | ConvertReturnMode::Image
    );
    if wants_data && bytes.len() as u64 > limit {
        return Err(ToolError::new(
            ErrorCode::InlineTooLarge,
            format!("The image is {} bytes; max_inline_bytes is {limit}.", bytes.len()),
        )
        .with_detail(json!({ "bytes": bytes.len(), "max_inline_bytes": limit })));
    }

    let base64 = matches!(return_mode, ConvertReturnMode::Base64 | ConvertReturnMode::Both)
        .then(|| base64::engine::general_purpose::STANDARD.encode(&bytes));

    let collected = warnings.collect();
    let structured = json!({
        "output_path": written.as_ref().map(|p| p.display().to_string()),
        "format": target_format.as_str(),
        "width": composed.width(),
        "height": composed.height(),
        "bytes": bytes.len(),
        "source": {
            "format": source_format.map(|f| format!("{f:?}").to_lowercase()),
            "width": image.width(),
            "height": image.height(),
        },
        "base64": base64,
        "warnings": warnings_json(&collected),
    });

    let text = match &written {
        Some(path) => format!(
            "Converted to {}×{} {} at {} ({} bytes).{}",
            composed.width(),
            composed.height(),
            target_format.as_str(),
            path.display(),
            bytes.len(),
            warning_suffix(&collected)
        ),
        None => format!(
            "Converted to {}×{} {} ({} bytes).{}",
            composed.width(),
            composed.height(),
            target_format.as_str(),
            bytes.len(),
            warning_suffix(&collected)
        ),
    };

    let mut outcome = ToolOutcome::new(structured, text);
    if return_mode == ConvertReturnMode::Image {
        outcome = outcome.with_image(InlineImage { bytes, mime_type: target_format.mime_type() });
    }
    Ok(outcome)
}

/// Reads the source image from a path or from Base64.
fn load_source(
    state: &ServerState,
    params: &ConvertImageParams,
) -> Result<raster_in::DecodedRaster> {
    match (&params.input_path, &params.input_base64) {
        (Some(path), None) => {
            let path = paths::resolve(path, paths::Direction::Input, &state.config)?;
            raster_in::from_path(&path)
        }
        (None, Some(text)) => {
            raster_in::from_base64(text, params.input_format.map(|f| f.to_image_format()))
        }
        (Some(_), Some(_)) => {
            Err(invalid_input("Give either input_path or input_base64, not both."))
        }
        (None, None) => Err(invalid_input("Either input_path or input_base64 is required.")),
    }
}

/// Resamples the source into the rectangle the mapping puts it in.
fn place(
    image: &DynamicImage,
    mapping: &fit::Mapping,
    canvas: (u32, u32),
    filter: ResampleFilter,
) -> DynamicImage {
    let (_, _, width, height) = mapping.drawn;
    let width = width.max(1).min(canvas.0.saturating_mul(4).max(1));
    let height = height.max(1).min(canvas.1.saturating_mul(4).max(1));
    image.resize_exact(width, height, to_filter(filter))
}

/// Draws the resampled image onto a canvas of exactly the requested size.
fn compose(
    drawn: DynamicImage,
    canvas_size: (u32, u32),
    background: Option<tiny_skia::Color>,
    padding_color: Option<tiny_skia::Color>,
    padding: fit::Padding,
    mapping: &fit::Mapping,
) -> DynamicImage {
    let mut target = image::RgbaImage::new(canvas_size.0, canvas_size.1);

    if let Some(color) = background {
        let pixel = to_pixel(color);
        for p in target.pixels_mut() {
            *p = pixel;
        }
    }
    if let Some(color) = padding_color {
        let pixel = to_pixel(color);
        let top = padding.top.min(canvas_size.1);
        let bottom = canvas_size.1.saturating_sub(padding.bottom).max(top);
        let left = padding.left.min(canvas_size.0);
        let right = canvas_size.0.saturating_sub(padding.right).max(left);
        for y in 0..canvas_size.1 {
            for x in 0..canvas_size.0 {
                if y < top || y >= bottom || x < left || x >= right {
                    target.put_pixel(x, y, pixel);
                }
            }
        }
    }

    let source = drawn.to_rgba8();
    let (offset_x, offset_y, _, _) = mapping.drawn;
    image::imageops::overlay(&mut target, &source, offset_x, offset_y);
    DynamicImage::ImageRgba8(target)
}

/// Composites an image onto an opaque colour.
fn flatten_image(image: &DynamicImage, color: tiny_skia::Color) -> DynamicImage {
    let mut target = image::RgbaImage::from_pixel(image.width(), image.height(), to_pixel(color));
    image::imageops::overlay(&mut target, &image.to_rgba8(), 0, 0);
    DynamicImage::ImageRgba8(target)
}

fn to_pixel(color: tiny_skia::Color) -> image::Rgba<u8> {
    let c = color.to_color_u8();
    image::Rgba([c.red(), c.green(), c.blue(), c.alpha()])
}

fn to_filter(filter: ResampleFilter) -> FilterType {
    match filter {
        ResampleFilter::Nearest => FilterType::Nearest,
        ResampleFilter::Triangle => FilterType::Triangle,
        ResampleFilter::CatmullRom => FilterType::CatmullRom,
        ResampleFilter::Gaussian => FilterType::Gaussian,
        ResampleFilter::Lanczos3 => FilterType::Lanczos3,
    }
}
