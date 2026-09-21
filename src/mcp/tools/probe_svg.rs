//! `probe_svg` — inspect a document without rendering it.

use crate::error::Result;
use crate::mcp::params::ProbeSvgParams;
use crate::mcp::result::{ToolOutcome, warning_suffix, warnings_json};
use crate::mcp::state::ServerState;
use crate::pipeline;
use crate::svg::probe;
use crate::warning::Warnings;

/// Runs the tool.
pub fn run(state: &ServerState, params: ProbeSvgParams) -> Result<ToolOutcome> {
    let warnings = Warnings::new();
    let document = pipeline::prepare(&params.document(), &state.config, &state.fonts, &warnings)?;

    let mut structured = probe::report(&document, document.resources_dir.as_deref());
    let collected = warnings.collect();
    structured["warnings"] = warnings_json(&collected);

    let unsupported_count = structured["unsupported"].as_array().map(Vec::len).unwrap_or_default();
    let missing = document.missing_fonts.len();
    let verdict = if unsupported_count == 0 && missing == 0 {
        "It renders exactly as written.".to_string()
    } else {
        format!(
            "It has {unsupported_count} unsupported construct(s) and {missing} unavailable font famil{}.",
            if missing == 1 { "y" } else { "ies" }
        )
    };

    let text = format!(
        "{}×{} ({}). {verdict}{}",
        document.source_size.width,
        document.source_size.height,
        pipeline::origin_name(document.source_size.origin),
        warning_suffix(&collected)
    );

    Ok(ToolOutcome::new(structured, text))
}
