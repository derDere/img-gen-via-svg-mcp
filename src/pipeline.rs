//! The shared rendering pipeline.
//!
//! Every rendering tool walks the same twelve steps, in the same order, so that
//! a batch entry, an icon size and a single render cannot drift apart in their
//! behaviour. The document is prepared once; each output is then rendered from
//! the prepared document.

use crate::config::Config;
use crate::encode::format::{self, Format};
use crate::encode::options::EncodeOptions;
use crate::encode::raster;
use crate::error::{ErrorCode, Result, ToolError};
use crate::io::paths;
use crate::render::canvas;
use crate::render::fit::{Align, Fit, Mapping, Padding};
use crate::render::renderer::{self, ExportArea};
use crate::render::sizing::{self, SizeOrigin, SizeRequest, SourceSize};
use crate::svg::document::{self, DocumentOptions, ImageRendering, ShapeRendering, TextRendering};
use crate::svg::fidelity::{self, OnMissingFont, OnUnsupported};
use crate::svg::fonts::{FontSettings, FontStore};
use crate::svg::scan::{self, ScanReport};
use crate::svg::source;
use crate::warning::Warnings;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tiny_skia::Pixmap;

/// Everything that decides how a document is read and resolved.
#[derive(Debug, Clone, Default)]
pub struct DocumentRequest {
    /// Path to the document.
    pub svg_path: Option<PathBuf>,
    /// The document itself.
    pub svg_source: Option<String>,
    /// A URL the document is fetched from.
    pub svg_url: Option<String>,
    /// Base directory for relative references.
    pub resources_dir: Option<PathBuf>,
    /// Unit resolution in dots per inch.
    pub dpi: Option<f32>,
    /// Font settings for this call.
    pub fonts: FontSettings,
    /// Languages resolving `systemLanguage`.
    pub languages: Option<Vec<String>>,
    /// A stylesheet injected while resolving.
    pub stylesheet: Option<String>,
    /// Default shape rendering method.
    pub shape_rendering: Option<ShapeRendering>,
    /// Default text rendering method.
    pub text_rendering: Option<TextRendering>,
    /// Default image rendering method.
    pub image_rendering: Option<ImageRendering>,
    /// What to do about unsupported constructs.
    pub on_unsupported: Option<OnUnsupported>,
    /// What to do about an unavailable font.
    pub on_missing_font: Option<OnMissingFont>,
}

/// A document read, parsed and checked against the fidelity contract.
pub struct PreparedDocument {
    /// The resolved tree.
    pub tree: usvg::Tree,
    /// What the raw XML holds.
    pub scan: ScanReport,
    /// The document's intrinsic size and where it came from.
    pub source_size: SourceSize,
    /// Where the document came from.
    pub origin: String,
    /// The base directory for its relative references.
    pub resources_dir: Option<PathBuf>,
    /// The font database this document was resolved with.
    pub fontdb: Arc<fontdb::Database>,
    /// Font families the document asked for that are unavailable.
    pub missing_fonts: Vec<String>,
}

/// Reads, parses and checks a document.
pub fn prepare(
    request: &DocumentRequest,
    config: &Config,
    fonts: &FontStore,
    warnings: &Warnings,
) -> Result<PreparedDocument> {
    let loaded = source::load(
        request.svg_path.as_deref(),
        request.svg_source.as_deref(),
        request.svg_url.as_deref(),
        request.resources_dir.as_deref(),
        config,
    )?;

    let scan = scan::scan(&loaded.text)?;
    let fontdb = fonts.for_call(&request.fonts)?;

    // A default family that is not in the database would make every text run
    // whose own family is missing disappear instead of being substituted, so it
    // is resolved here and the substitution is reported.
    let requested_family = fonts.default_family(&request.fonts);
    let default_family = crate::svg::fonts::substitute_for(&fontdb, &requested_family);
    if default_family != requested_family && scan.text_without_explicit_family {
        warnings.warn_detail(
            "font_substituted",
            format!(
                "The default font family {requested_family:?} is not available; {default_family:?} is used for text that names no family of its own."
            ),
            serde_json::json!({ "requested": requested_family, "used": default_family }),
        );
    }

    let options = DocumentOptions {
        dpi: request.dpi.unwrap_or(96.0),
        default_family,
        default_size: config.default_size,
        languages: request.languages.clone().unwrap_or_else(|| vec!["en".to_string()]),
        stylesheet: request.stylesheet.clone(),
        resources_dir: loaded.resources_dir.clone(),
        shape_rendering: request.shape_rendering.unwrap_or_default(),
        text_rendering: request.text_rendering.unwrap_or_default(),
        image_rendering: request.image_rendering.unwrap_or_default(),
        fontdb: Arc::clone(&fontdb),
    };

    if let Some(dpi) = request.dpi
        && !(1.0..=5000.0).contains(&dpi)
    {
        return Err(crate::error::invalid_input(format!(
            "dpi is {dpi} but has to be between 1 and 5000."
        )));
    }

    let parsed = document::parse(&loaded.text, &options)?;

    let fidelity = fidelity::check(
        &scan,
        &parsed.diagnostics,
        &fontdb,
        loaded.resources_dir.as_deref(),
        config,
        request.on_unsupported.unwrap_or_default(),
        request.on_missing_font.unwrap_or_default(),
        warnings,
    )?;

    let source_size = sizing::source_size(&parsed.tree, &scan, warnings);

    Ok(PreparedDocument {
        tree: parsed.tree,
        scan,
        source_size,
        origin: loaded.origin,
        resources_dir: loaded.resources_dir,
        fontdb,
        missing_fonts: fidelity.missing_fonts,
    })
}

/// Everything that decides how one output is produced.
#[derive(Debug, Clone, Default)]
pub struct RenderRequest {
    /// Absolute target width in pixels.
    pub width: Option<u32>,
    /// Absolute target height in pixels.
    pub height: Option<u32>,
    /// Multiplier applied to the intrinsic size.
    pub scale: Option<f32>,
    /// What to do about a mismatched aspect ratio.
    pub fit: Option<Fit>,
    /// Where the drawing sits when it does not fill the canvas.
    pub align: Option<Align>,
    /// Colour filling the whole canvas.
    pub background: Option<String>,
    /// Colour filling the padding alone.
    pub padding_color: Option<String>,
    /// Whether the result carries an alpha channel.
    pub transparent: Option<bool>,
    /// The output format.
    pub format: Option<Format>,
    /// Encoder settings.
    pub encode: EncodeOptions,
    /// The single element to render.
    pub export_id: Option<String>,
    /// Which part of the document defines the canvas.
    pub export_area: Option<ExportArea>,
}

/// A rendered canvas, before it is encoded.
pub struct RenderedCanvas {
    /// The pixels.
    pub pixmap: Pixmap,
    /// The format the canvas was prepared for.
    pub format: Format,
    /// Whether the encoded image will carry transparency.
    pub has_alpha: bool,
    /// The strategy that was applied.
    pub fit: Fit,
    /// Scale factors applied, per axis.
    pub scale: (f32, f32),
    /// Translation applied, per axis.
    pub offset: (f32, f32),
    /// Padding around the drawing.
    pub padding: Padding,
    /// The background colour as it was applied, if any.
    pub background: Option<String>,
    /// Milliseconds spent rasterising.
    pub render_ms: u128,
}

/// One produced image, before it is written anywhere.
pub struct RenderedImage {
    /// The encoded file bytes.
    pub bytes: Vec<u8>,
    /// The format they are in.
    pub format: Format,
    /// The canvas width.
    pub width: u32,
    /// The canvas height.
    pub height: u32,
    /// Whether the encoded image can carry transparency.
    pub has_alpha: bool,
    /// The strategy that was applied.
    pub fit: Fit,
    /// Scale factors applied, per axis.
    pub scale: (f32, f32),
    /// Translation applied, per axis.
    pub offset: (f32, f32),
    /// Padding around the drawing.
    pub padding: Padding,
    /// The background colour as it was applied, if any.
    pub background: Option<String>,
    /// Milliseconds spent rasterising.
    pub render_ms: u128,
    /// Milliseconds spent encoding.
    pub encode_ms: u128,
}

/// Renders the prepared document into one image, encoded and ready to write.
pub fn render_one(
    document: &PreparedDocument,
    request: &RenderRequest,
    output_path: Option<&Path>,
    config: &Config,
    warnings: &Warnings,
) -> Result<RenderedImage> {
    let canvas = render_canvas(document, request, output_path, config, warnings)?;
    let encode_started = std::time::Instant::now();
    let bytes = raster::encode_pixmap(&canvas.pixmap, canvas.format, &request.encode, warnings)?;
    let encode_ms = encode_started.elapsed().as_millis();

    Ok(RenderedImage {
        bytes,
        format: canvas.format,
        width: canvas.pixmap.width(),
        height: canvas.pixmap.height(),
        has_alpha: canvas.has_alpha,
        fit: canvas.fit,
        scale: canvas.scale,
        offset: canvas.offset,
        padding: canvas.padding,
        background: canvas.background,
        render_ms: canvas.render_ms,
        encode_ms,
    })
}

/// Renders the prepared document onto a canvas of exactly the requested size.
pub fn render_canvas(
    document: &PreparedDocument,
    request: &RenderRequest,
    output_path: Option<&Path>,
    config: &Config,
    warnings: &Warnings,
) -> Result<RenderedCanvas> {
    let format = format::resolve(request.format, output_path)?;
    request.encode.validate()?;

    let transparent = request.transparent;
    if transparent == Some(true) && !format.has_alpha() {
        return Err(ToolError::new(
            ErrorCode::UnsupportedFormat,
            format!(
                "{} has no alpha channel, so transparent: true cannot be honoured.",
                format.as_str()
            ),
        )
        .with_detail(json!({ "format": format.as_str() }))
        .with_hint("Choose a format with an alpha channel, such as png, or drop transparent."));
    }

    let subject = renderer::subject(
        &document.tree,
        request.export_id.as_deref(),
        request.export_area.unwrap_or_default(),
    )?;
    let source = SourceSize {
        width: subject.source_rect.2,
        height: subject.source_rect.3,
        origin: document.source_size.origin,
    };

    let canvas_size = sizing::resolve(
        source,
        SizeRequest { width: request.width, height: request.height, scale: request.scale },
        config.max_pixels,
    )?;

    let fit = request.fit.unwrap_or_default();
    let align = request.align.unwrap_or_default();
    let mapping: Mapping =
        crate::render::fit::map((source.width, source.height), canvas_size, fit, align)?;

    let background = canvas::parse_optional_color(request.background.as_ref())?;
    let padding_color = canvas::parse_optional_color(request.padding_color.as_ref())?;
    if padding_color.is_some() && mapping.padding.is_empty() {
        warnings.warn_detail(
            "option_ignored",
            "padding_color was given but the drawing fills the canvas, so there is no padding to fill.",
            json!({ "fit": format!("{fit:?}").to_lowercase() }),
        );
    }

    let started = std::time::Instant::now();
    let mut pixmap: Pixmap =
        canvas::prepare(canvas_size.0, canvas_size.1, background, padding_color, mapping.padding)?;
    // The renderer reports a dropped filter or an undecodable raster image only
    // through the log, and those happen here rather than during parsing, so the
    // capture has to wrap the render call as well.
    let (outcome, diagnostics) =
        crate::svg::diagnostics::capture(|| renderer::render(&subject, &mapping, &mut pixmap));
    outcome?;
    for message in diagnostics {
        warnings.warn_detail(
            crate::svg::fidelity::classify(&message),
            message.clone(),
            json!({ "source": "renderer" }),
        );
    }
    let render_ms = started.elapsed().as_millis();

    if mapping.adjusted {
        warnings.warn_detail(
            "aspect_adjusted",
            format!(
                "The document's aspect ratio {:.4} differs from the requested {:.4}; fit {} was applied.",
                source.width / source.height,
                canvas_size.0 as f32 / canvas_size.1 as f32,
                format!("{fit:?}").to_lowercase()
            ),
            json!({
                "strategy": format!("{fit:?}").to_lowercase(),
                "source_ratio": source.width / source.height,
                "requested_ratio": canvas_size.0 as f32 / canvas_size.1 as f32,
                "padding_px": {
                    "top": mapping.padding.top, "right": mapping.padding.right,
                    "bottom": mapping.padding.bottom, "left": mapping.padding.left
                },
                "padding_fill": padding_fill_description(padding_color, background),
            }),
        );
    }

    let must_flatten = !format.has_alpha() || transparent == Some(false);
    let pixmap = if must_flatten {
        let colour = background.map(canvas::opaque).unwrap_or_else(canvas::white);
        let reason = if !format.has_alpha() {
            format!("{} has no alpha channel.", format.as_str())
        } else {
            "transparent: false was requested.".to_string()
        };
        canvas::flatten(pixmap, colour, &reason, warnings)?
    } else {
        pixmap
    };

    Ok(RenderedCanvas {
        pixmap,
        format,
        has_alpha: format.has_alpha() && !must_flatten,
        fit,
        scale: mapping.scale,
        offset: mapping.offset,
        padding: mapping.padding,
        background: background.map(canvas::describe),
        render_ms,
    })
}

/// Describes what the padding ended up filled with, for the warning detail.
fn padding_fill_description(
    padding_color: Option<tiny_skia::Color>,
    background: Option<tiny_skia::Color>,
) -> String {
    match padding_color.or(background) {
        Some(color) => canvas::describe(color),
        None => "transparent".to_string(),
    }
}

/// Writes a produced image, applying the output path policy.
pub fn write_output(
    path: &Path,
    bytes: &[u8],
    overwrite: bool,
    create_dirs: bool,
    config: &Config,
) -> Result<PathBuf> {
    let resolved = paths::resolve(path, paths::Direction::Output, config)?;
    crate::io::atomic::write(&resolved, bytes, overwrite, create_dirs)?;
    Ok(resolved)
}

/// The source-size origin as it appears in a result.
pub fn origin_name(origin: SizeOrigin) -> &'static str {
    match origin {
        SizeOrigin::Attributes => "attributes",
        SizeOrigin::ViewBox => "view_box",
        SizeOrigin::DefaultSize => "default_size",
    }
}
