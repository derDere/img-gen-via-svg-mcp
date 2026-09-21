//! `get_capabilities` — report what this build can actually do.
//!
//! A caller cannot avoid a gap it does not know about, so the catalogue, the
//! format lists and the font resolution are all readable rather than implied.

use crate::encode::format::Format;
use crate::error::Result;
use crate::mcp::result::ToolOutcome;
use crate::mcp::state::ServerState;
use crate::svg::fidelity::GAPS;
use serde_json::json;

/// The filter primitives the renderer implements.
pub const FILTER_PRIMITIVES: [&str; 17] = [
    "feBlend",
    "feColorMatrix",
    "feComponentTransfer",
    "feComposite",
    "feConvolveMatrix",
    "feDiffuseLighting",
    "feDisplacementMap",
    "feDropShadow",
    "feFlood",
    "feGaussianBlur",
    "feImage",
    "feMerge",
    "feMorphology",
    "feOffset",
    "feSpecularLighting",
    "feTile",
    "feTurbulence",
];

/// Runs the tool.
pub fn run(state: &ServerState) -> Result<ToolOutcome> {
    let encodable: Vec<&str> =
        Format::ALL.iter().filter(|f| f.is_available()).map(|f| f.as_str()).collect();

    let generic: serde_json::Map<String, serde_json::Value> = state
        .fonts
        .generic_families()
        .into_iter()
        .map(|(name, value)| (name.to_string(), json!(value)))
        .collect();

    let structured = json!({
        "server_version": crate::VERSION,
        "renderer": { "name": "resvg", "version": "0.48" },
        "encoder": { "name": "image", "version": "0.25" },
        "transports": ["stdio"],
        "platform": std::env::consts::OS,
        "architecture": std::env::consts::ARCH,
        "formats": {
            "encode": encodable,
            "encode_containers": containers(),
            "notes": {
                "webp": "lossless encoding only",
                "avif": if cfg!(feature = "avif") { "encoding only; this build has no AVIF decoder" } else { "not built into this binary" },
                "gif": "at most 256 colours, alpha reduced to fully transparent or fully opaque",
                "tiff": "written with the encoder's default compression",
                "ico": "each image at most 256×256",
            },
        },
        "svg_support": {
            "profile": "static SVG 1.1, partial SVG 2",
            "filter_primitives": FILTER_PRIMITIVES,
            "known_gaps": GAPS.iter().map(|gap| json!({
                "code": gap.code,
                "description": gap.description,
            })).collect::<Vec<_>>(),
        },
        "fonts": {
            "system_fonts_loaded": state.fonts.system_fonts_loaded(),
            "face_count": state.fonts.face_count(),
            "generic_families": generic,
            "families": state.fonts.families(),
        },
        "config": {
            "allowed_input_dirs": state.config.allowed_input_dirs.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "allowed_output_dirs": state.config.allowed_output_dirs.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "follow_symlinks": state.config.follow_symlinks,
            "remote_input": state.config.remote_input,
            "remote_svg_references": state.config.remote_svg_references,
            "max_pixels": state.config.max_pixels,
            "max_inline_bytes": state.config.max_inline_bytes,
            "render_timeout_ms": state.config.render_timeout_ms,
            "default_fit": "contain",
            "default_size": format!("{}x{}", state.config.default_size.0, state.config.default_size.1),
        },
    });

    let text = format!(
        "{} encodable formats, {} filter primitives, {} known gaps, {} font faces loaded.",
        encodable.len(),
        FILTER_PRIMITIVES.len(),
        GAPS.len(),
        state.fonts.face_count()
    );

    Ok(ToolOutcome::new(structured, text))
}

/// The multi-image containers `render_icon` can build in this build.
fn containers() -> Vec<&'static str> {
    let mut containers = vec!["ico", "png_set"];
    if cfg!(feature = "icns") {
        containers.push("icns");
    }
    containers
}
