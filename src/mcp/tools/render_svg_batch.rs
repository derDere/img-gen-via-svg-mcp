//! `render_svg_batch` — render one document to several outputs in one call.

use crate::error::{Result, invalid_input};
use crate::mcp::params::{RenderSvgBatchParams, ReturnMode};
use crate::mcp::result::{InlineImage, ToolOutcome, warning_suffix, warnings_json};
use crate::mcp::state::ServerState;
use crate::mcp::tools::render_svg::source_json;
use crate::pipeline;
use crate::warning::Warnings;
use serde_json::json;

/// The most outputs one call may ask for.
pub const MAX_OUTPUTS: usize = 64;

/// Runs the tool.
pub fn run(state: &ServerState, params: RenderSvgBatchParams) -> Result<ToolOutcome> {
    if params.outputs.is_empty() {
        return Err(invalid_input("outputs needs at least one entry."));
    }
    if params.outputs.len() > MAX_OUTPUTS {
        return Err(invalid_input(format!(
            "outputs holds {} entries; at most {MAX_OUTPUTS} are accepted in one call.",
            params.outputs.len()
        ))
        .with_hint("Split the request into several calls."));
    }

    // The document is parsed once, which is what makes an icon set cheap, so
    // document-level warnings are raised once too rather than per entry.
    let document_warnings = Warnings::new();
    let document =
        pipeline::prepare(&params.document(), &state.config, &state.fonts, &document_warnings)?;

    let defaults = params.defaults.clone().unwrap_or_default();
    let stop_on_error = params.stop_on_error.unwrap_or(false);

    let mut results = Vec::with_capacity(params.outputs.len());
    let mut images = Vec::new();
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut completed_paths = Vec::new();

    for (index, entry) in params.outputs.iter().enumerate() {
        let merged = entry.merged(&defaults);
        let entry_warnings = Warnings::new();

        match render_entry(state, &document, &merged, &entry_warnings) {
            Ok((value, image)) => {
                succeeded += 1;
                if let Some(path) = value.get("output_path").and_then(|v| v.as_str()) {
                    completed_paths.push(path.to_string());
                }
                let mut value = value;
                value["index"] = json!(index);
                value["status"] = json!("ok");
                value["warnings"] = warnings_json(&entry_warnings.collect());
                results.push(value);
                if let Some(image) = image {
                    images.push(image);
                }
            }
            Err(error) => {
                failed += 1;
                results.push(json!({
                    "index": index,
                    "status": "error",
                    "output_path": merged.output_path.as_ref().map(|p| p.display().to_string()),
                    "error": error.payload()["error"],
                    "warnings": warnings_json(&entry_warnings.collect()),
                }));
                if stop_on_error {
                    return Err(error.with_detail(json!({
                        "index": index,
                        "completed": completed_paths,
                    })));
                }
            }
        }
    }

    let collected = document_warnings.collect();
    let structured = json!({
        "parsed_once": true,
        "source": source_json(&document),
        "succeeded": succeeded,
        "failed": failed,
        "results": results,
        "warnings": warnings_json(&collected),
    });

    let text = format!(
        "Rendered {succeeded} of {} outputs from one document, {failed} failed.{}",
        params.outputs.len(),
        warning_suffix(&collected)
    );

    let mut outcome = ToolOutcome::new(structured, text);
    for image in images {
        outcome = outcome.with_image(image);
    }
    Ok(outcome)
}

/// Renders and writes one entry.
fn render_entry(
    state: &ServerState,
    document: &pipeline::PreparedDocument,
    entry: &crate::mcp::params::BatchOutput,
    warnings: &Warnings,
) -> Result<(serde_json::Value, Option<InlineImage>)> {
    let mode = entry.return_mode.unwrap_or_default();
    if mode.writes_file() && entry.output_path.is_none() {
        return Err(invalid_input("output_path is required unless return_mode is image."));
    }
    if mode == ReturnMode::Image && entry.output_path.is_some() {
        return Err(invalid_input(
            "return_mode is image, which writes no file, but output_path was given.",
        ));
    }

    let rendered = pipeline::render_one(
        document,
        &entry.render(),
        entry.output_path.as_deref(),
        &state.config,
        warnings,
    )?;

    let written = if mode.writes_file() {
        let path = entry.output_path.as_deref().expect("checked above");
        Some(pipeline::write_output(
            path,
            &rendered.bytes,
            entry.overwrite.unwrap_or(true),
            entry.create_dirs.unwrap_or(false),
            &state.config,
        )?)
    } else {
        None
    };

    let image = mode.returns_image().then(|| InlineImage {
        bytes: rendered.bytes.clone(),
        mime_type: rendered.format.mime_type(),
    });

    Ok((
        json!({
            "output_path": written.as_ref().map(|p| p.display().to_string()),
            "format": rendered.format.as_str(),
            "width": rendered.width,
            "height": rendered.height,
            "bytes": rendered.bytes.len(),
            "has_alpha": rendered.has_alpha,
            "padding_px": {
                "top": rendered.padding.top, "right": rendered.padding.right,
                "bottom": rendered.padding.bottom, "left": rendered.padding.left
            },
        }),
        image,
    ))
}
