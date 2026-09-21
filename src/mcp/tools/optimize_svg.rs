//! `optimize_svg` — normalise or shrink a document.

use crate::error::{Result, invalid_input};
use crate::mcp::params::{OptimizeMode, OptimizeReturnMode, OptimizeSvgParams};
use crate::mcp::result::{ToolOutcome, warning_suffix, warnings_json};
use crate::mcp::state::ServerState;
use crate::pipeline;
use crate::svg::{optimize, source};
use crate::warning::Warnings;
use serde_json::json;

/// Runs the tool.
pub fn run(state: &ServerState, params: OptimizeSvgParams) -> Result<ToolOutcome> {
    let mode = params.mode.unwrap_or_default();
    let return_mode = params.return_mode.unwrap_or_default();
    let writes_file = matches!(return_mode, OptimizeReturnMode::File | OptimizeReturnMode::Both);
    let returns_source =
        matches!(return_mode, OptimizeReturnMode::Source | OptimizeReturnMode::Both);

    if writes_file && params.output_path.is_none() {
        return Err(invalid_input("output_path is required unless return_mode is source."));
    }
    if !writes_file && params.output_path.is_some() {
        return Err(invalid_input(
            "return_mode is source, which writes no file, but output_path was given.",
        )
        .with_hint("Use return_mode both to receive the document and write the file."));
    }

    let precision = params.precision.unwrap_or(8);
    if !(1..=12).contains(&precision) {
        return Err(invalid_input(format!(
            "precision is {precision} but has to be between 1 and 12."
        )));
    }

    let warnings = Warnings::new();

    let loaded = source::load(
        params.svg_path.as_deref(),
        params.svg_source.as_deref(),
        params.svg_url.as_deref(),
        params.resources_dir.as_deref(),
        &state.config,
    )?;
    let bytes_in = loaded.text.len();

    let optimised = match mode {
        OptimizeMode::Normalize => {
            let document =
                pipeline::prepare(&params.document(), &state.config, &state.fonts, &warnings)?;
            let text_to_paths = params.text_to_paths.unwrap_or(false);
            let out = optimize::normalize(&document.tree, precision, text_to_paths);
            warnings.warn_detail(
                "structure_lost",
                "Normalising resolved the document through the renderer's pipeline: grouping, ids and editing structure are not carried over, and the result renders identically rather than reading identically.",
                json!({ "mode": "normalize", "elements_in": document.scan.node_count }),
            );
            if text_to_paths {
                warnings.warn_detail(
                    "text_outlined",
                    "Text was converted to outlines, so the document no longer depends on a font and its text is no longer selectable.",
                    json!({ "text_to_paths": true }),
                );
            }
            out
        }
        OptimizeMode::Minify => {
            if params.text_to_paths.unwrap_or(false) {
                return Err(invalid_input(
                    "text_to_paths needs mode normalize; minify does not resolve text.",
                ));
            }
            optimize::minify(&loaded.text)?
        }
    };

    let bytes_out = optimised.len();
    let written = if writes_file {
        let path = params.output_path.as_deref().expect("checked above");
        Some(pipeline::write_output(
            path,
            optimised.as_bytes(),
            params.overwrite.unwrap_or(true),
            params.create_dirs.unwrap_or(false),
            &state.config,
        )?)
    } else {
        None
    };

    let collected = warnings.collect();
    let ratio = if bytes_in > 0 { bytes_out as f64 / bytes_in as f64 } else { 1.0 };
    let structured = json!({
        "output_path": written.as_ref().map(|p| p.display().to_string()),
        "mode": match mode { OptimizeMode::Normalize => "normalize", OptimizeMode::Minify => "minify" },
        "bytes_in": bytes_in,
        "bytes_out": bytes_out,
        "ratio": ratio,
        "source": returns_source.then_some(optimised),
        "warnings": warnings_json(&collected),
    });

    let text = format!(
        "Optimised {bytes_in} bytes to {bytes_out} ({:.1}% of the original).{}",
        ratio * 100.0,
        warning_suffix(&collected)
    );

    Ok(ToolOutcome::new(structured, text))
}
