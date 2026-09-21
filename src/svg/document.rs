//! Parsing a document into a resolved tree.

use crate::error::{ErrorCode, Result, ToolError};
use crate::svg::diagnostics;
use crate::svg::fonts::FontSettings;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;

/// How shapes are rasterised when the document leaves it to the renderer.
#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShapeRendering {
    /// Fastest, aliased.
    OptimizeSpeed,
    /// Aliased, but geometry is respected.
    CrispEdges,
    /// Antialiased and exact. The default.
    #[default]
    GeometricPrecision,
}

/// How text is rasterised when the document leaves it to the renderer.
#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextRendering {
    /// Fastest.
    OptimizeSpeed,
    /// Tuned for readability. The default.
    #[default]
    OptimizeLegibility,
    /// Exact glyph positions.
    GeometricPrecision,
}

/// How embedded raster images are scaled.
#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImageRendering {
    /// Smooth. The default.
    #[default]
    OptimizeQuality,
    /// Nearest neighbour.
    OptimizeSpeed,
}

/// Everything that influences how a document is parsed and resolved.
#[derive(Debug, Clone)]
pub struct DocumentOptions {
    /// Unit resolution, in dots per inch.
    pub dpi: f32,
    /// Family used when the document sets none.
    pub default_family: String,
    /// Size used when the document declares none.
    pub default_size: (f32, f32),
    /// Languages resolving `systemLanguage`.
    pub languages: Vec<String>,
    /// A stylesheet injected while resolving.
    pub stylesheet: Option<String>,
    /// Base directory for relative references.
    pub resources_dir: Option<PathBuf>,
    /// Default shape rendering method.
    pub shape_rendering: ShapeRendering,
    /// Default text rendering method.
    pub text_rendering: TextRendering,
    /// Default image rendering method.
    pub image_rendering: ImageRendering,
    /// The font database this call uses.
    pub fontdb: Arc<fontdb::Database>,
}

impl DocumentOptions {
    /// Builds the parser options this server hands to `usvg`.
    fn to_usvg(&self) -> usvg::Options<'static> {
        let mut options = usvg::Options {
            resources_dir: self.resources_dir.clone(),
            dpi: self.dpi,
            font_family: self.default_family.clone(),
            languages: self.languages.clone(),
            style_sheet: self.stylesheet.clone(),
            ..usvg::Options::default()
        };
        options.default_size = usvg::Size::from_wh(self.default_size.0, self.default_size.1)
            .unwrap_or(options.default_size);
        options.shape_rendering = match self.shape_rendering {
            ShapeRendering::OptimizeSpeed => usvg::ShapeRendering::OptimizeSpeed,
            ShapeRendering::CrispEdges => usvg::ShapeRendering::CrispEdges,
            ShapeRendering::GeometricPrecision => usvg::ShapeRendering::GeometricPrecision,
        };
        options.text_rendering = match self.text_rendering {
            TextRendering::OptimizeSpeed => usvg::TextRendering::OptimizeSpeed,
            TextRendering::OptimizeLegibility => usvg::TextRendering::OptimizeLegibility,
            TextRendering::GeometricPrecision => usvg::TextRendering::GeometricPrecision,
        };
        options.image_rendering = match self.image_rendering {
            ImageRendering::OptimizeQuality => usvg::ImageRendering::OptimizeQuality,
            ImageRendering::OptimizeSpeed => usvg::ImageRendering::OptimizeSpeed,
        };
        options.fontdb = Arc::clone(&self.fontdb);
        options
    }
}

/// A parsed document together with whatever the parser reported while doing it.
pub struct ParsedDocument {
    /// The resolved tree.
    pub tree: usvg::Tree,
    /// Messages the parser emitted, verbatim.
    pub diagnostics: Vec<String>,
}

/// Parses a document, capturing the parser's diagnostics for this call.
pub fn parse(text: &str, options: &DocumentOptions) -> Result<ParsedDocument> {
    let usvg_options = options.to_usvg();
    let (result, diagnostics) = diagnostics::capture(|| usvg::Tree::from_str(text, &usvg_options));
    let tree = result.map_err(|e| {
        ToolError::new(ErrorCode::ParseFailed, format!("The SVG cannot be parsed: {e}"))
            .with_detail(serde_json::json!({ "parser_message": e.to_string() }))
    })?;
    Ok(ParsedDocument { tree, diagnostics })
}

/// Per-call font settings resolved against the operator's configuration.
pub struct ResolvedFonts {
    /// The database for this call.
    pub database: Arc<fontdb::Database>,
    /// The default family for this call.
    pub default_family: String,
    /// The settings as the caller gave them.
    pub settings: FontSettings,
}
