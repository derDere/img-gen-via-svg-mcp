//! The renderer call itself, including sub-element export.

use crate::error::{ErrorCode, Result, ToolError, invalid_input};
use crate::render::fit::Mapping;
use serde::Deserialize;
use serde_json::json;
use tiny_skia::Pixmap;

/// Which part of the document defines the canvas.
#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportArea {
    /// The document's own size. The default.
    #[default]
    Page,
    /// The bounding box of the exported element.
    Object,
    /// The tight bounding box of everything drawn.
    Drawing,
}

/// What is rendered, and the user-space rectangle that maps onto the canvas.
pub struct Subject<'a> {
    /// The parsed document.
    pub tree: &'a usvg::Tree,
    /// The single element to draw, when one was named.
    pub node: Option<&'a usvg::Node>,
    /// The user-space rectangle that fills the canvas: x, y, width, height.
    pub source_rect: (f32, f32, f32, f32),
}

/// Works out what to draw and which rectangle defines the canvas.
pub fn subject<'a>(
    tree: &'a usvg::Tree,
    export_id: Option<&str>,
    export_area: ExportArea,
) -> Result<Subject<'a>> {
    match export_id {
        Some(id) => {
            let node = tree.node_by_id(id).ok_or_else(|| {
                ToolError::new(
                    ErrorCode::ElementNotFound,
                    format!("No element with id {id:?} is in the document."),
                )
                .with_detail(json!({ "export_id": id }))
                .with_hint("Call probe_svg to see which ids the document defines.")
            })?;
            let bbox = node.abs_layer_bounding_box().ok_or_else(|| {
                ToolError::new(
                    ErrorCode::EmptyRender,
                    format!("Element {id:?} has zero size, so there is nothing to render."),
                )
            })?;
            let source_rect = match export_area {
                ExportArea::Page => {
                    let size = tree.size();
                    (0.0, 0.0, size.width(), size.height())
                }
                ExportArea::Object | ExportArea::Drawing => {
                    (bbox.x(), bbox.y(), bbox.width(), bbox.height())
                }
            };
            Ok(Subject { tree, node: Some(node), source_rect })
        }
        None => {
            let source_rect = match export_area {
                ExportArea::Page => {
                    let size = tree.size();
                    (0.0, 0.0, size.width(), size.height())
                }
                ExportArea::Drawing => {
                    let bbox = tree.root().abs_layer_bounding_box();
                    (bbox.x(), bbox.y(), bbox.width(), bbox.height())
                }
                ExportArea::Object => {
                    return Err(invalid_input(
                        "export_area object needs an export_id naming the element to bound.",
                    )
                    .with_hint("Pass export_id, or use export_area page or drawing."));
                }
            };
            Ok(Subject { tree, node: None, source_rect })
        }
    }
}

/// Draws the subject onto the canvas.
pub fn render(subject: &Subject<'_>, mapping: &Mapping, pixmap: &mut Pixmap) -> Result<()> {
    let (x, y, _, _) = subject.source_rect;

    match subject.node {
        // `render_node` places the element's own bounding box at the origin, so
        // the translation here undoes exactly as much of that as the chosen
        // export area calls for.
        Some(node) => {
            let bbox = node.abs_layer_bounding_box().ok_or_else(|| {
                ToolError::new(ErrorCode::EmptyRender, "The exported element has zero size.")
            })?;
            let transform = mapping.transform.pre_translate(bbox.x() - x, bbox.y() - y);
            resvg::render_node(node, transform, &mut pixmap.as_mut()).ok_or_else(|| {
                ToolError::new(ErrorCode::EmptyRender, "The exported element produced no pixels.")
            })?;
        }
        None => {
            let transform = mapping.transform.pre_translate(-x, -y);
            resvg::render(subject.tree, transform, &mut pixmap.as_mut());
        }
    }
    Ok(())
}
