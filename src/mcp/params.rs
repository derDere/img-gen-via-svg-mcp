//! The parameter structs the tool schemas are generated from.
//!
//! Each tool's struct lists exactly the parameters the specification gives it,
//! so the schema a client sees and the contract in `docs/SPEC.md` cannot drift
//! apart. Every field is optional except the ones the specification marks
//! required; defaults live in the pipeline, not in `serde`, so that "absent"
//! stays distinguishable from "set to the default".

use crate::encode::format::Format;
use crate::encode::options::EncodeOptions;
use crate::error::Result;
use crate::pipeline::{DocumentRequest, RenderRequest};
use crate::render::fit::{Align, Fit};
use crate::render::renderer::ExportArea;
use crate::svg::document::{ImageRendering, ShapeRendering, TextRendering};
use crate::svg::fidelity::{OnMissingFont, OnUnsupported};
use crate::svg::fonts::FontSettings;
use serde::Deserialize;
use std::path::PathBuf;

/// What the tool sends back.
#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReturnMode {
    /// Write the file and return its path. The default.
    #[default]
    File,
    /// Return the image inline and write nothing.
    Image,
    /// Write the file and return the image inline as well.
    Both,
}

impl ReturnMode {
    /// Whether a file is written.
    pub fn writes_file(self) -> bool {
        matches!(self, Self::File | Self::Both)
    }

    /// Whether image content is attached to the result.
    pub fn returns_image(self) -> bool {
        matches!(self, Self::Image | Self::Both)
    }
}

/// Parameters of `render_svg`.
#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
pub struct RenderSvgParams {
    /// Filesystem path to an `.svg` or `.svgz` file. One input source is required.
    pub svg_path: Option<PathBuf>,
    /// The SVG document itself, as a string.
    pub svg_source: Option<String>,
    /// An `http://` or `https://` URL the document is fetched from.
    pub svg_url: Option<String>,
    /// Base directory for relative references inside the document.
    pub resources_dir: Option<PathBuf>,

    /// Where the file is written. Required unless return_mode is `image`.
    pub output_path: Option<PathBuf>,
    /// Whether an existing target may be replaced. Defaults to true.
    pub overwrite: Option<bool>,
    /// Whether missing parent directories are created. Defaults to false.
    pub create_dirs: Option<bool>,
    /// Whether the result is a file, inline image content, or both.
    pub return_mode: Option<ReturnMode>,
    /// Upper bound on inline image content, in bytes.
    pub max_inline_bytes: Option<u64>,

    /// The output format. Derived from the output path's extension when absent.
    pub format: Option<Format>,
    /// Absolute target width in pixels.
    pub width: Option<u32>,
    /// Absolute target height in pixels.
    pub height: Option<u32>,
    /// Multiplier applied to the document's own size, as an alternative to width and height.
    pub scale: Option<f32>,
    /// What to do when the requested aspect ratio differs from the document's.
    pub fit: Option<Fit>,
    /// Where the drawing sits on the canvas when it does not fill it.
    pub align: Option<Align>,
    /// A CSS colour filling the whole canvas before the document is drawn.
    pub background: Option<String>,
    /// A CSS colour filling only the padding left by fit `contain`.
    pub padding_color: Option<String>,
    /// Forces an alpha channel on or off.
    pub transparent: Option<bool>,

    /// JPEG quality, 1 to 100. Defaults to 90.
    pub jpeg_quality: Option<u8>,
    /// PNG deflate effort: fast, default, best, none, or level:0 to level:9.
    pub png_compression: Option<String>,
    /// Whether a lossless optimisation pass runs over the encoded PNG.
    pub png_optimize: Option<bool>,
    /// Physical resolution recorded in the file, in dots per inch.
    pub density_metadata: Option<f32>,
    /// Unit resolution used while parsing, in dots per inch. Defaults to 96.
    pub dpi: Option<f32>,

    /// Font settings for this call.
    pub fonts: Option<FontSettings>,
    /// Whether an unavailable font family is a warning or a refusal.
    pub on_missing_font: Option<OnMissingFont>,
    /// Languages resolving the systemLanguage attribute. Defaults to ["en"].
    pub languages: Option<Vec<String>>,
    /// Default text rendering method.
    pub text_rendering: Option<TextRendering>,
    /// Default shape rendering method.
    pub shape_rendering: Option<ShapeRendering>,
    /// Default rendering method for embedded raster images.
    pub image_rendering: Option<ImageRendering>,
    /// A CSS stylesheet injected while resolving the document.
    pub stylesheet: Option<String>,

    /// Render only the element carrying this id.
    pub export_id: Option<String>,
    /// Which part of the document defines the canvas.
    pub export_area: Option<ExportArea>,
    /// Whether an unsupported construct is a warning or a refusal.
    pub on_unsupported: Option<OnUnsupported>,
}

impl RenderSvgParams {
    /// The document half of the request.
    pub fn document(&self) -> DocumentRequest {
        DocumentRequest {
            svg_path: self.svg_path.clone(),
            svg_source: self.svg_source.clone(),
            svg_url: self.svg_url.clone(),
            resources_dir: self.resources_dir.clone(),
            dpi: self.dpi,
            fonts: self.fonts.clone().unwrap_or_default(),
            languages: self.languages.clone(),
            stylesheet: self.stylesheet.clone(),
            shape_rendering: self.shape_rendering,
            text_rendering: self.text_rendering,
            image_rendering: self.image_rendering,
            on_unsupported: self.on_unsupported,
            on_missing_font: self.on_missing_font,
        }
    }

    /// The image half of the request.
    pub fn render(&self) -> RenderRequest {
        RenderRequest {
            width: self.width,
            height: self.height,
            scale: self.scale,
            fit: self.fit,
            align: self.align,
            background: self.background.clone(),
            padding_color: self.padding_color.clone(),
            transparent: self.transparent,
            format: self.format,
            encode: EncodeOptions {
                jpeg_quality: self.jpeg_quality,
                png_compression: self.png_compression.clone(),
                png_optimize: self.png_optimize.unwrap_or(false),
                density_metadata: self.density_metadata,
            },
            export_id: self.export_id.clone(),
            export_area: self.export_area,
        }
    }

    /// Checks that the output target makes sense for the chosen return mode.
    pub fn check_output(&self) -> Result<ReturnMode> {
        let mode = self.return_mode.unwrap_or_default();
        match (mode, &self.output_path) {
            (ReturnMode::Image, Some(path)) => Err(crate::error::invalid_input(format!(
                "return_mode is image, which writes no file, but output_path {} was given.",
                path.display()
            ))
            .with_hint("Use return_mode both to receive the image and write the file.")),
            (ReturnMode::File | ReturnMode::Both, None) => Err(crate::error::invalid_input(
                "output_path is required unless return_mode is image.",
            )),
            _ => Ok(mode),
        }
    }
}

/// One entry of a `render_svg_batch` call.
#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
pub struct BatchOutput {
    /// Where this entry is written. Required unless return_mode is `image`.
    pub output_path: Option<PathBuf>,
    /// Whether an existing target may be replaced.
    pub overwrite: Option<bool>,
    /// Whether missing parent directories are created.
    pub create_dirs: Option<bool>,
    /// Whether this entry returns a file, inline image content, or both.
    pub return_mode: Option<ReturnMode>,
    /// The output format for this entry.
    pub format: Option<Format>,
    /// Absolute target width in pixels.
    pub width: Option<u32>,
    /// Absolute target height in pixels.
    pub height: Option<u32>,
    /// Multiplier applied to the document's own size.
    pub scale: Option<f32>,
    /// What to do when the requested aspect ratio differs from the document's.
    pub fit: Option<Fit>,
    /// Where the drawing sits on the canvas.
    pub align: Option<Align>,
    /// A CSS colour filling the whole canvas.
    pub background: Option<String>,
    /// A CSS colour filling only the padding.
    pub padding_color: Option<String>,
    /// Forces an alpha channel on or off.
    pub transparent: Option<bool>,
    /// JPEG quality, 1 to 100.
    pub jpeg_quality: Option<u8>,
    /// PNG deflate effort.
    pub png_compression: Option<String>,
    /// Whether a lossless optimisation pass runs over the encoded PNG.
    pub png_optimize: Option<bool>,
    /// Physical resolution recorded in the file, in dots per inch.
    pub density_metadata: Option<f32>,
    /// Render only the element carrying this id.
    pub export_id: Option<String>,
    /// Which part of the document defines the canvas.
    pub export_area: Option<ExportArea>,
}

impl BatchOutput {
    /// Merges this entry over the call's defaults.
    pub fn merged(&self, defaults: &BatchOutput) -> BatchOutput {
        macro_rules! pick {
            ($field:ident) => {
                self.$field.clone().or_else(|| defaults.$field.clone())
            };
        }
        BatchOutput {
            output_path: pick!(output_path),
            overwrite: pick!(overwrite),
            create_dirs: pick!(create_dirs),
            return_mode: pick!(return_mode),
            format: pick!(format),
            width: pick!(width),
            height: pick!(height),
            scale: pick!(scale),
            fit: pick!(fit),
            align: pick!(align),
            background: pick!(background),
            padding_color: pick!(padding_color),
            transparent: pick!(transparent),
            jpeg_quality: pick!(jpeg_quality),
            png_compression: pick!(png_compression),
            png_optimize: pick!(png_optimize),
            density_metadata: pick!(density_metadata),
            export_id: pick!(export_id),
            export_area: pick!(export_area),
        }
    }

    /// The image half of the request for this entry.
    pub fn render(&self) -> RenderRequest {
        RenderRequest {
            width: self.width,
            height: self.height,
            scale: self.scale,
            fit: self.fit,
            align: self.align,
            background: self.background.clone(),
            padding_color: self.padding_color.clone(),
            transparent: self.transparent,
            format: self.format,
            encode: EncodeOptions {
                jpeg_quality: self.jpeg_quality,
                png_compression: self.png_compression.clone(),
                png_optimize: self.png_optimize.unwrap_or(false),
                density_metadata: self.density_metadata,
            },
            export_id: self.export_id.clone(),
            export_area: self.export_area,
        }
    }
}

/// Parameters of `render_svg_batch`.
#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
pub struct RenderSvgBatchParams {
    /// Filesystem path to an `.svg` or `.svgz` file. One input source is required.
    pub svg_path: Option<PathBuf>,
    /// The SVG document itself, as a string.
    pub svg_source: Option<String>,
    /// An `http://` or `https://` URL the document is fetched from.
    pub svg_url: Option<String>,
    /// Base directory for relative references inside the document.
    pub resources_dir: Option<PathBuf>,
    /// Unit resolution used while parsing, in dots per inch.
    pub dpi: Option<f32>,
    /// Font settings, which apply to the whole call.
    pub fonts: Option<FontSettings>,
    /// Whether an unavailable font family is a warning or a refusal.
    pub on_missing_font: Option<OnMissingFont>,
    /// Languages resolving the systemLanguage attribute.
    pub languages: Option<Vec<String>>,
    /// A CSS stylesheet injected while resolving the document.
    pub stylesheet: Option<String>,
    /// Whether an unsupported construct is a warning or a refusal.
    pub on_unsupported: Option<OnUnsupported>,
    /// Settings every entry inherits unless it overrides them.
    pub defaults: Option<BatchOutput>,
    /// The outputs to produce; at least one, at most 64.
    pub outputs: Vec<BatchOutput>,
    /// Whether the call stops at the first failing entry.
    pub stop_on_error: Option<bool>,
}

impl RenderSvgBatchParams {
    /// The document half of the request.
    pub fn document(&self) -> DocumentRequest {
        DocumentRequest {
            svg_path: self.svg_path.clone(),
            svg_source: self.svg_source.clone(),
            svg_url: self.svg_url.clone(),
            resources_dir: self.resources_dir.clone(),
            dpi: self.dpi,
            fonts: self.fonts.clone().unwrap_or_default(),
            languages: self.languages.clone(),
            stylesheet: self.stylesheet.clone(),
            shape_rendering: None,
            text_rendering: None,
            image_rendering: None,
            on_unsupported: self.on_unsupported,
            on_missing_font: self.on_missing_font,
        }
    }
}

/// The container a `render_icon` call produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IconContainer {
    /// A Windows `.ico` file holding every requested size.
    Ico,
    /// An Apple `.icns` file holding every requested size.
    Icns,
    /// A directory of PNG files, one per size.
    PngSet,
}

/// Parameters of `render_icon`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct RenderIconParams {
    /// Filesystem path to an `.svg` or `.svgz` file. One input source is required.
    pub svg_path: Option<PathBuf>,
    /// The SVG document itself, as a string.
    pub svg_source: Option<String>,
    /// An `http://` or `https://` URL the document is fetched from.
    pub svg_url: Option<String>,
    /// Base directory for relative references inside the document.
    pub resources_dir: Option<PathBuf>,
    /// The icon file, or the directory for a PNG set.
    pub output_path: PathBuf,
    /// Which container is produced. Derived from the output path's extension when absent.
    pub container: Option<IconContainer>,
    /// The edge lengths to render.
    pub sizes: Option<Vec<u32>>,
    /// Filename pattern for a PNG set; must contain `{size}`.
    pub png_name_pattern: Option<String>,
    /// What to do when the document is not square.
    pub fit: Option<Fit>,
    /// Where the drawing sits on each square canvas.
    pub align: Option<Align>,
    /// A CSS colour filling the whole canvas.
    pub background: Option<String>,
    /// A CSS colour filling only the padding.
    pub padding_color: Option<String>,
    /// Whether an existing target may be replaced.
    pub overwrite: Option<bool>,
    /// Whether missing parent directories are created.
    pub create_dirs: Option<bool>,
    /// PNG deflate effort.
    pub png_compression: Option<String>,
    /// Whether a lossless optimisation pass runs over each encoded PNG.
    pub png_optimize: Option<bool>,
    /// Unit resolution used while parsing, in dots per inch.
    pub dpi: Option<f32>,
    /// Font settings for this call.
    pub fonts: Option<FontSettings>,
    /// Whether an unavailable font family is a warning or a refusal.
    pub on_missing_font: Option<OnMissingFont>,
    /// Languages resolving the systemLanguage attribute.
    pub languages: Option<Vec<String>>,
    /// A CSS stylesheet injected while resolving the document.
    pub stylesheet: Option<String>,
    /// Whether an unsupported construct is a warning or a refusal.
    pub on_unsupported: Option<OnUnsupported>,
}

impl RenderIconParams {
    /// The document half of the request.
    pub fn document(&self) -> DocumentRequest {
        DocumentRequest {
            svg_path: self.svg_path.clone(),
            svg_source: self.svg_source.clone(),
            svg_url: self.svg_url.clone(),
            resources_dir: self.resources_dir.clone(),
            dpi: self.dpi,
            fonts: self.fonts.clone().unwrap_or_default(),
            languages: self.languages.clone(),
            stylesheet: self.stylesheet.clone(),
            shape_rendering: None,
            text_rendering: None,
            image_rendering: None,
            on_unsupported: self.on_unsupported,
            on_missing_font: self.on_missing_font,
        }
    }
}

/// Parameters of `probe_svg`.
#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
pub struct ProbeSvgParams {
    /// Filesystem path to an `.svg` or `.svgz` file. One input source is required.
    pub svg_path: Option<PathBuf>,
    /// The SVG document itself, as a string.
    pub svg_source: Option<String>,
    /// An `http://` or `https://` URL the document is fetched from.
    pub svg_url: Option<String>,
    /// Base directory for relative references inside the document.
    pub resources_dir: Option<PathBuf>,
    /// Unit resolution used while parsing, in dots per inch.
    pub dpi: Option<f32>,
    /// Font settings, which decide which families count as resolvable.
    pub fonts: Option<FontSettings>,
    /// Languages resolving the systemLanguage attribute.
    pub languages: Option<Vec<String>>,
}

impl ProbeSvgParams {
    /// The document half of the request. Probing never refuses a document for
    /// what it holds; reporting that is the whole point of the tool.
    pub fn document(&self) -> DocumentRequest {
        DocumentRequest {
            svg_path: self.svg_path.clone(),
            svg_source: self.svg_source.clone(),
            svg_url: self.svg_url.clone(),
            resources_dir: self.resources_dir.clone(),
            dpi: self.dpi,
            fonts: self.fonts.clone().unwrap_or_default(),
            languages: self.languages.clone(),
            stylesheet: None,
            shape_rendering: None,
            text_rendering: None,
            image_rendering: None,
            on_unsupported: Some(OnUnsupported::Warn),
            on_missing_font: Some(OnMissingFont::Warn),
        }
    }
}

/// How `optimize_svg` rewrites a document.
#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OptimizeMode {
    /// Resolve the document through the renderer's own simplification pipeline
    /// and write the result back out. The default.
    #[default]
    Normalize,
    /// Keep the document structure and only shorten it.
    Minify,
}

/// How `optimize_svg` returns its result.
#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OptimizeReturnMode {
    /// Write the file and return its path. The default.
    #[default]
    File,
    /// Return the optimised document as a string and write nothing.
    Source,
    /// Write the file and return the document as well.
    Both,
}

/// Parameters of `optimize_svg`.
#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
pub struct OptimizeSvgParams {
    /// Filesystem path to an `.svg` or `.svgz` file. One input source is required.
    pub svg_path: Option<PathBuf>,
    /// The SVG document itself, as a string.
    pub svg_source: Option<String>,
    /// An `http://` or `https://` URL the document is fetched from.
    pub svg_url: Option<String>,
    /// Base directory for relative references inside the document.
    pub resources_dir: Option<PathBuf>,
    /// Where the optimised document is written. Required unless return_mode is `source`.
    pub output_path: Option<PathBuf>,
    /// Whether an existing target may be replaced.
    pub overwrite: Option<bool>,
    /// Whether missing parent directories are created.
    pub create_dirs: Option<bool>,
    /// Whether the result is a file, a string, or both.
    pub return_mode: Option<OptimizeReturnMode>,
    /// Which rewriting mode is applied.
    pub mode: Option<OptimizeMode>,
    /// Decimal places kept for coordinates, 1 to 12. Defaults to 8.
    pub precision: Option<u8>,
    /// Whether text is converted to outlines, removing the font dependency.
    pub text_to_paths: Option<bool>,
    /// Unit resolution used while parsing, in dots per inch.
    pub dpi: Option<f32>,
    /// Font settings for this call.
    pub fonts: Option<FontSettings>,
    /// Languages resolving the systemLanguage attribute.
    pub languages: Option<Vec<String>>,
}

impl OptimizeSvgParams {
    /// The document half of the request.
    pub fn document(&self) -> DocumentRequest {
        DocumentRequest {
            svg_path: self.svg_path.clone(),
            svg_source: self.svg_source.clone(),
            svg_url: self.svg_url.clone(),
            resources_dir: self.resources_dir.clone(),
            dpi: self.dpi,
            fonts: self.fonts.clone().unwrap_or_default(),
            languages: self.languages.clone(),
            stylesheet: None,
            shape_rendering: None,
            text_rendering: None,
            image_rendering: None,
            on_unsupported: Some(OnUnsupported::Warn),
            on_missing_font: Some(OnMissingFont::Warn),
        }
    }
}

/// How `convert_image` returns its result.
#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConvertReturnMode {
    /// Write the file and return its path. The default.
    #[default]
    File,
    /// Return the encoded image as Base64 and write nothing.
    Base64,
    /// Write the file and return the Base64 as well.
    Both,
    /// Return inline MCP image content and write nothing.
    Image,
}

/// Resampling filter used when a raster is resized.
#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResampleFilter {
    /// Nearest neighbour.
    Nearest,
    /// Linear.
    Triangle,
    /// Cubic.
    CatmullRom,
    /// Gaussian.
    Gaussian,
    /// Lanczos with a window of three. The default.
    #[default]
    Lanczos3,
}

/// Parameters of `convert_image`.
#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
pub struct ConvertImageParams {
    /// Path to the source raster image. One input source is required.
    pub input_path: Option<PathBuf>,
    /// The source raster image as Base64; a `data:` prefix is accepted.
    pub input_base64: Option<String>,
    /// Overrides format detection for Base64 input.
    pub input_format: Option<Format>,
    /// Where the converted file is written. Required unless return_mode returns data.
    pub output_path: Option<PathBuf>,
    /// Whether the result is a file, Base64, both, or inline image content.
    pub return_mode: Option<ConvertReturnMode>,
    /// The output format. Derived from the output path's extension when absent.
    pub format: Option<Format>,
    /// Absolute target width in pixels.
    pub width: Option<u32>,
    /// Absolute target height in pixels.
    pub height: Option<u32>,
    /// Multiplier applied to the source size.
    pub scale: Option<f32>,
    /// What to do when the requested aspect ratio differs from the source's.
    pub fit: Option<Fit>,
    /// Where the image sits on the canvas when it does not fill it.
    pub align: Option<Align>,
    /// Resampling filter used when resizing.
    pub filter: Option<ResampleFilter>,
    /// A CSS colour filling the whole canvas.
    pub background: Option<String>,
    /// A CSS colour filling only the padding.
    pub padding_color: Option<String>,
    /// Forces an alpha channel on or off.
    pub transparent: Option<bool>,
    /// JPEG quality, 1 to 100.
    pub jpeg_quality: Option<u8>,
    /// PNG deflate effort.
    pub png_compression: Option<String>,
    /// Whether a lossless optimisation pass runs over the encoded PNG.
    pub png_optimize: Option<bool>,
    /// Physical resolution recorded in the file, in dots per inch.
    pub density_metadata: Option<f32>,
    /// Whether an existing target may be replaced.
    pub overwrite: Option<bool>,
    /// Whether missing parent directories are created.
    pub create_dirs: Option<bool>,
    /// Upper bound on inline image content, in bytes.
    pub max_inline_bytes: Option<u64>,
}

impl ConvertImageParams {
    /// The encoder settings for this call.
    pub fn encode(&self) -> EncodeOptions {
        EncodeOptions {
            jpeg_quality: self.jpeg_quality,
            png_compression: self.png_compression.clone(),
            png_optimize: self.png_optimize.unwrap_or(false),
            density_metadata: self.density_metadata,
        }
    }
}

/// `get_capabilities` takes no parameters.
#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
pub struct NoParams {}
