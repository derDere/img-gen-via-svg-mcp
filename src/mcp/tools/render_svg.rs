//! `render_svg` — render one document to one raster image.

use crate::error::{ErrorCode, Result, ToolError};
use crate::mcp::params::{RenderSvgParams, ReturnMode};
use crate::mcp::result::{InlineImage, ToolOutcome, warning_suffix, warnings_json};
use crate::mcp::state::ServerState;
use crate::pipeline;
use crate::warning::Warnings;
use serde_json::{Value, json};

/// Runs the tool.
pub fn run(state: &ServerState, params: RenderSvgParams) -> Result<ToolOutcome> {
    let mode = params.check_output()?;
    let warnings = Warnings::new();

    let document = pipeline::prepare(&params.document(), &state.config, &state.fonts, &warnings)?;
    let rendered = pipeline::render_one(
        &document,
        &params.render(),
        params.output_path.as_deref(),
        &state.config,
        &warnings,
    )?;

    let written = if mode.writes_file() {
        let path = params.output_path.as_deref().expect("checked by check_output");
        Some(pipeline::write_output(
            path,
            &rendered.bytes,
            params.overwrite.unwrap_or(true),
            params.create_dirs.unwrap_or(false),
            &state.config,
        )?)
    } else {
        None
    };

    let limit = params.max_inline_bytes.unwrap_or(state.config.max_inline_bytes);
    let mut inline = None;
    if mode.returns_image() {
        let size = rendered.bytes.len() as u64;
        if size > limit {
            if mode == ReturnMode::Image {
                return Err(ToolError::new(
                    ErrorCode::InlineTooLarge,
                    format!("The image is {size} bytes; max_inline_bytes is {limit}."),
                )
                .with_detail(json!({ "bytes": size, "max_inline_bytes": limit }))
                .with_hint("Render smaller, or use return_mode file and read the written file."));
            }
            warnings.warn_detail(
                "inline_omitted",
                format!("The image is {size} bytes, above max_inline_bytes of {limit}, so it was written but not returned inline."),
                json!({ "bytes": size, "max_inline_bytes": limit }),
            );
        } else {
            inline = Some(InlineImage {
                bytes: rendered.bytes.clone(),
                mime_type: rendered.format.mime_type(),
            });
        }
    }

    let collected = warnings.collect();
    let structured = json!({
        "output_path": written.as_ref().map(|p| p.display().to_string()),
        "format": rendered.format.as_str(),
        "width": rendered.width,
        "height": rendered.height,
        "bytes": rendered.bytes.len(),
        "has_alpha": rendered.has_alpha,
        "source": source_json(&document),
        "fit": format!("{:?}", rendered.fit).to_lowercase(),
        "scale_applied": { "x": rendered.scale.0, "y": rendered.scale.1 },
        "offset_applied": { "x": rendered.offset.0, "y": rendered.offset.1 },
        "padding_px": {
            "top": rendered.padding.top, "right": rendered.padding.right,
            "bottom": rendered.padding.bottom, "left": rendered.padding.left
        },
        "background": rendered.background,
        "render_ms": rendered.render_ms,
        "encode_ms": rendered.encode_ms,
        "warnings": warnings_json(&collected),
        "inline_image": inline.as_ref().map(|i| json!({
            "mime_type": i.mime_type,
            "bytes": i.bytes.len(),
        })),
    });

    let text = match &written {
        Some(path) => format!(
            "Rendered {}×{} {} to {} ({} bytes).{}",
            rendered.width,
            rendered.height,
            rendered.format.as_str(),
            path.display(),
            rendered.bytes.len(),
            warning_suffix(&collected)
        ),
        None => format!(
            "Rendered {}×{} {} inline ({} bytes).{}",
            rendered.width,
            rendered.height,
            rendered.format.as_str(),
            rendered.bytes.len(),
            warning_suffix(&collected)
        ),
    };

    let mut outcome = ToolOutcome::new(structured, text);
    if let Some(image) = inline {
        outcome = outcome.with_image(image);
    }
    Ok(outcome)
}

/// The document's own size, as it appears in every rendering result.
pub fn source_json(document: &pipeline::PreparedDocument) -> Value {
    json!({
        "width": document.source_size.width,
        "height": document.source_size.height,
        "view_box": view_box_json(document),
        "size_origin": pipeline::origin_name(document.source_size.origin),
    })
}

/// The root `viewBox`, parsed into its four numbers.
pub fn view_box_json(document: &pipeline::PreparedDocument) -> Value {
    let Some(raw) = document.scan.root_view_box.as_deref() else {
        return Value::Null;
    };
    let numbers: Vec<f32> = raw
        .split([' ', ','])
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect();
    match numbers.as_slice() {
        [x, y, width, height] => json!({ "x": x, "y": y, "width": width, "height": height }),
        _ => Value::Null,
    }
}
