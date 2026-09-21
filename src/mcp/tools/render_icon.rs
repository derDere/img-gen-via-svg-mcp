//! `render_icon` — build a multi-resolution icon from one document.
//!
//! Every size is rasterised from the vector source at its own resolution. No
//! size is ever produced by downscaling a larger raster, which is the whole
//! reason for building an icon from an SVG.

use crate::encode::format::Format;
use crate::encode::ico;
use crate::encode::options::EncodeOptions;
use crate::encode::raster;
use crate::error::{Result, invalid_input};
use crate::mcp::params::{IconContainer, RenderIconParams};
use crate::mcp::result::{ToolOutcome, warning_suffix, warnings_json};
use crate::mcp::state::ServerState;
use crate::pipeline::{self, RenderRequest};
use crate::warning::Warnings;
use serde_json::json;
use std::path::PathBuf;

/// Sizes an `.ico` holds unless the caller names others.
pub const ICO_DEFAULT_SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];
/// Sizes an `.icns` holds unless the caller names others.
pub const ICNS_DEFAULT_SIZES: [u32; 7] = [16, 32, 64, 128, 256, 512, 1024];
/// Sizes a PNG set holds unless the caller names others.
pub const PNG_SET_DEFAULT_SIZES: [u32; 7] = [16, 32, 48, 64, 128, 256, 512];

/// Runs the tool.
pub fn run(state: &ServerState, params: RenderIconParams) -> Result<ToolOutcome> {
    let container = resolve_container(&params)?;
    let sizes = resolve_sizes(&params, container)?;
    let warnings = Warnings::new();

    let document = pipeline::prepare(&params.document(), &state.config, &state.fonts, &warnings)?;

    let encode = EncodeOptions {
        jpeg_quality: None,
        png_compression: params.png_compression.clone(),
        png_optimize: params.png_optimize.unwrap_or(false),
        density_metadata: None,
    };

    let mut images = Vec::with_capacity(sizes.len());
    for size in &sizes {
        let request = RenderRequest {
            width: Some(*size),
            height: Some(*size),
            fit: params.fit,
            align: params.align,
            background: params.background.clone(),
            padding_color: params.padding_color.clone(),
            format: Some(Format::Png),
            encode: encode.clone(),
            ..RenderRequest::default()
        };
        let canvas = pipeline::render_canvas(&document, &request, None, &state.config, &warnings)?;
        images.push(raster::pixmap_to_image(&canvas.pixmap));
    }

    let overwrite = params.overwrite.unwrap_or(true);
    let create_dirs = params.create_dirs.unwrap_or(false);

    let (files, entries, total_bytes) = match container {
        IconContainer::Ico => {
            let bytes = ico::encode(&images)?;
            let path = pipeline::write_output(
                &params.output_path,
                &bytes,
                overwrite,
                create_dirs,
                &state.config,
            )?;
            let entries: Vec<_> = sizes.iter().map(|s| json!({ "size": s })).collect();
            (vec![path.display().to_string()], entries, bytes.len())
        }
        IconContainer::Icns => {
            let bytes = encode_icns(&images)?;
            let path = pipeline::write_output(
                &params.output_path,
                &bytes,
                overwrite,
                create_dirs,
                &state.config,
            )?;
            let entries: Vec<_> = sizes.iter().map(|s| json!({ "size": s })).collect();
            (vec![path.display().to_string()], entries, bytes.len())
        }
        IconContainer::PngSet => {
            let pattern =
                params.png_name_pattern.clone().unwrap_or_else(|| "icon-{size}.png".to_string());
            if !pattern.contains("{size}") {
                return Err(invalid_input(format!(
                    "png_name_pattern {pattern:?} has to contain {{size}}."
                )));
            }
            let mut files = Vec::with_capacity(images.len());
            let mut entries = Vec::with_capacity(images.len());
            let mut total = 0usize;
            for (size, image) in sizes.iter().zip(&images) {
                let bytes = raster::encode_image(
                    &image::DynamicImage::ImageRgba8(image.clone()),
                    Format::Png,
                    &encode,
                    &warnings,
                )?;
                let name = pattern.replace("{size}", &size.to_string());
                let path: PathBuf = params.output_path.join(name);
                let written = pipeline::write_output(
                    &path,
                    &bytes,
                    overwrite,
                    // A PNG set writes into a directory, so creating it is part
                    // of what the caller asked for.
                    true,
                    &state.config,
                )?;
                total += bytes.len();
                entries.push(json!({ "size": size, "bytes": bytes.len() }));
                files.push(written.display().to_string());
            }
            (files, entries, total)
        }
    };

    let collected = warnings.collect();
    let structured = json!({
        "output_path": params.output_path.display().to_string(),
        "container": container_name(container),
        "sizes": sizes,
        "entries": entries,
        "files": files,
        "bytes": total_bytes,
        "source": crate::mcp::tools::render_svg::source_json(&document),
        "warnings": warnings_json(&collected),
    });

    let text = format!(
        "Built a {} icon with {} sizes ({}) at {} ({total_bytes} bytes).{}",
        container_name(container),
        sizes.len(),
        sizes.iter().map(u32::to_string).collect::<Vec<_>>().join(", "),
        params.output_path.display(),
        warning_suffix(&collected)
    );

    Ok(ToolOutcome::new(structured, text))
}

#[cfg(feature = "icns")]
fn encode_icns(images: &[image::RgbaImage]) -> Result<Vec<u8>> {
    crate::encode::icns::encode(images)
}

#[cfg(not(feature = "icns"))]
fn encode_icns(_images: &[image::RgbaImage]) -> Result<Vec<u8>> {
    Err(crate::error::ToolError::new(
        crate::error::ErrorCode::UnsupportedFormat,
        "This build cannot write ICNS files.",
    )
    .with_hint("Rebuild with the `icns` feature enabled, or use container ico or png_set."))
}

/// Decides which container to build.
fn resolve_container(params: &RenderIconParams) -> Result<IconContainer> {
    if let Some(container) = params.container {
        return Ok(container);
    }
    match params.output_path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase) {
        Some(extension) if extension == "ico" => Ok(IconContainer::Ico),
        Some(extension) if extension == "icns" => Ok(IconContainer::Icns),
        _ => Ok(IconContainer::Ico),
    }
}

/// Decides which sizes to render, and checks them against the container.
fn resolve_sizes(params: &RenderIconParams, container: IconContainer) -> Result<Vec<u32>> {
    let mut sizes = params.sizes.clone().unwrap_or_else(|| match container {
        IconContainer::Ico => ICO_DEFAULT_SIZES.to_vec(),
        IconContainer::Icns => ICNS_DEFAULT_SIZES.to_vec(),
        IconContainer::PngSet => PNG_SET_DEFAULT_SIZES.to_vec(),
    });
    sizes.sort_unstable();
    sizes.dedup();

    if sizes.is_empty() {
        return Err(invalid_input("sizes needs at least one entry."));
    }

    for size in &sizes {
        if *size == 0 {
            return Err(invalid_input("An icon size has to be at least 1 pixel."));
        }
        match container {
            IconContainer::Ico if *size > ico::MAX_EDGE => {
                return Err(invalid_input(format!(
                    "ICO holds images up to {} pixels; {size} was requested.",
                    ico::MAX_EDGE
                ))
                .with_hint("Drop the oversized entry, or use container png_set."));
            }
            IconContainer::Icns if !icns_supports(*size) => {
                return Err(invalid_input(format!(
                    "ICNS defines slots for 16, 32, 48, 64, 128, 256, 512 and 1024 pixels; {size} was requested."
                )));
            }
            _ => {}
        }
    }
    Ok(sizes)
}

#[cfg(feature = "icns")]
fn icns_supports(size: u32) -> bool {
    crate::encode::icns::SUPPORTED_SIZES.contains(&size)
}

#[cfg(not(feature = "icns"))]
fn icns_supports(size: u32) -> bool {
    [16u32, 32, 48, 64, 128, 256, 512, 1024].contains(&size)
}

/// The container's wire name.
fn container_name(container: IconContainer) -> &'static str {
    match container {
        IconContainer::Ico => "ico",
        IconContainer::Icns => "icns",
        IconContainer::PngSet => "png_set",
    }
}
