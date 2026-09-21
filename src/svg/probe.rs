//! Document inspection for `probe_svg`.
//!
//! The report answers one question before any pixels are produced: will this
//! document render exactly as written? An empty `unsupported` array together
//! with every requested font resolved means yes.

use crate::pipeline::PreparedDocument;
use crate::svg::fonts;
use serde_json::{Value, json};

/// Builds the inspection report for a prepared document.
pub fn report(document: &PreparedDocument, resources_dir: Option<&std::path::Path>) -> Value {
    let size = document.source_size;
    let aspect_ratio = if size.height > 0.0 { size.width / size.height } else { 0.0 };

    json!({
        "origin": document.origin,
        "size": { "width": size.width, "height": size.height },
        "size_origin": crate::pipeline::origin_name(size.origin),
        "view_box": crate::mcp::tools::render_svg::view_box_json(document),
        "preserve_aspect_ratio": document.scan.root_preserve_aspect_ratio,
        "aspect_ratio": aspect_ratio,
        "recommended_sizes": recommended_sizes(size.width, size.height),
        "content": content_inventory(document),
        "filter_primitives": document.scan.filter_primitives.iter().collect::<Vec<_>>(),
        "blend_modes": document.scan.blend_modes.iter().collect::<Vec<_>>(),
        "fonts_requested": fonts_requested(document),
        "external_references": external_references(document, resources_dir),
        "unsupported": unsupported(document),
    })
}

/// The intrinsic size and its common multiples.
fn recommended_sizes(width: f32, height: f32) -> Value {
    let base_w = width.ceil().max(1.0) as u32;
    let base_h = height.ceil().max(1.0) as u32;
    json!([
        { "width": base_w, "height": base_h, "label": "intrinsic" },
        { "width": base_w * 2, "height": base_h * 2, "label": "2x" },
        { "width": base_w * 3, "height": base_h * 3, "label": "3x" },
    ])
}

/// What kinds of content the document holds.
fn content_inventory(document: &PreparedDocument) -> Value {
    let scan = &document.scan;
    json!({
        "has_text": scan.count_of("text") + scan.count_of("tspan") + scan.count_of("textPath") > 0,
        "has_filters": !document.tree.filters().is_empty(),
        "has_masks": !document.tree.masks().is_empty(),
        "has_clip_paths": !document.tree.clip_paths().is_empty(),
        "has_patterns": !document.tree.patterns().is_empty(),
        "has_gradients": !document.tree.linear_gradients().is_empty()
            || !document.tree.radial_gradients().is_empty(),
        "has_raster_images": scan.count_of("image") > 0,
        "node_count": scan.node_count,
        "element_counts": scan.element_counts,
    })
}

/// Which font families the document asks for, and what each resolves to.
fn fonts_requested(document: &PreparedDocument) -> Value {
    let substitute = document.fontdb.family_name(&fontdb::Family::SansSerif).to_string();
    let families: Vec<Value> = document
        .scan
        .font_families
        .iter()
        .map(|family| {
            let resolved = fonts::has_family(&document.fontdb, family);
            json!({
                "family": family,
                "resolved": resolved,
                "resolved_to": if resolved { family.clone() } else { substitute.clone() },
            })
        })
        .collect();
    json!(families)
}

/// Which external resources the document points at, and whether they resolve.
fn external_references(
    document: &PreparedDocument,
    resources_dir: Option<&std::path::Path>,
) -> Value {
    let references: Vec<Value> = document
        .scan
        .references
        .iter()
        .map(|reference| {
            let remote =
                reference.href.starts_with("http://") || reference.href.starts_with("https://");
            let resolved_path =
                (!remote).then(|| resources_dir.map(|dir| dir.join(&reference.href))).flatten();
            let exists = resolved_path.as_ref().is_some_and(|p| p.exists());
            json!({
                "href": reference.href,
                "kind": reference.kind,
                "remote": remote,
                "resolved": exists,
                "path": resolved_path.map(|p| p.display().to_string()),
            })
        })
        .collect();
    json!(references)
}

/// What the render will leave out.
fn unsupported(document: &PreparedDocument) -> Value {
    let findings: Vec<Value> = document
        .scan
        .findings
        .iter()
        .map(|finding| {
            json!({
                "code": finding.code,
                "element": finding.element,
                "count": finding.count,
                "message": finding.message,
            })
        })
        .collect();
    json!(findings)
}
