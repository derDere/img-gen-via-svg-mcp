//! The server type and its tool router.
//!
//! This is the only module that knows a protocol is involved. Each tool method
//! validates nothing of its own: it hands the parameters to the matching module
//! under [`crate::mcp::tools`], runs it off the async runtime because rendering
//! is CPU-bound, and bounds it with the operator's render timeout.

use crate::config::Config;
use crate::error::{ErrorCode, ToolError};
use crate::mcp::params::{
    ConvertImageParams, NoParams, OptimizeSvgParams, ProbeSvgParams, RenderIconParams,
    RenderSvgBatchParams, RenderSvgParams,
};
use crate::mcp::result::{self, ToolOutcome};
use crate::mcp::state::ServerState;
use crate::mcp::tools;
use rmcp::{
    ServerHandler,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

/// The MCP server.
#[derive(Clone)]
pub struct SvgServer {
    state: Arc<ServerState>,
    // Read by the `tool_handler` macro's generated dispatch.
    #[allow(dead_code)]
    tool_router: rmcp::handler::server::router::tool::ToolRouter<Self>,
}

impl SvgServer {
    /// Builds the server, loading the fonts named by the configuration.
    pub fn new(config: Config) -> Self {
        Self { state: ServerState::new(config), tool_router: Self::tool_router() }
    }

    /// The state, for tests that drive the tools without a transport.
    pub fn state(&self) -> &Arc<ServerState> {
        &self.state
    }

    /// Runs a tool body off the async runtime, bounded by the render timeout.
    ///
    /// `budget_units` scales the operator's per-render timeout for tools that
    /// produce several images in one call, so that a batch is not cut off by a
    /// budget meant for a single render.
    async fn run_tool<F>(&self, name: &'static str, budget_units: u32, body: F) -> CallToolResult
    where
        F: FnOnce(&ServerState) -> crate::error::Result<ToolOutcome> + Send + 'static,
    {
        let state = Arc::clone(&self.state);
        let budget = Duration::from_millis(
            state.config.render_timeout_ms.saturating_mul(budget_units.max(1) as u64),
        );

        let task = tokio::task::spawn_blocking(move || body(&state));
        match tokio::time::timeout(budget, task).await {
            Ok(Ok(Ok(outcome))) => result::success(outcome),
            Ok(Ok(Err(error))) => {
                tracing::warn!(tool = name, code = error.code.as_str(), "{}", error.message);
                result::failure(&error)
            }
            Ok(Err(join_error)) => {
                let error = ToolError::new(
                    ErrorCode::InternalError,
                    format!("The {name} tool panicked: {join_error}"),
                )
                .with_hint("Please report this with the input that caused it.");
                tracing::error!(tool = name, "{}", error.message);
                result::failure(&error)
            }
            Err(_) => {
                let error = ToolError::new(
                    ErrorCode::InternalError,
                    format!("{name} exceeded its time budget of {} ms.", budget.as_millis()),
                )
                .with_detail(json!({ "budget_ms": budget.as_millis() }))
                .with_hint("Render a smaller image, simplify the document's filters, or raise IMG_SVG_MCP_RENDER_TIMEOUT_MS.");
                tracing::warn!(tool = name, "{}", error.message);
                result::failure(&error)
            }
        }
    }
}

#[tool_router]
impl SvgServer {
    /// Render an SVG document to a raster image at an exact pixel size.
    #[tool(
        name = "render_svg",
        description = "Render an SVG file, source string or URL to a raster image at an absolute pixel size. Writes wherever output_path says. Formats: png, jpeg, bmp, gif, tiff, webp, ico, tga, qoi, pnm, farbfeld, openexr, hdr. Reports every deviation from the document as a warning."
    )]
    async fn render_svg(&self, Parameters(params): Parameters<RenderSvgParams>) -> CallToolResult {
        self.run_tool("render_svg", 1, move |state| tools::render_svg::run(state, params)).await
    }

    /// Render one SVG to several sizes and formats in a single call.
    #[tool(
        name = "render_svg_batch",
        description = "Render one SVG document to several sizes and formats in a single call. The document is parsed once, which is what makes an icon set cheap. Each entry reports its own result, and one failing entry does not stop the others unless stop_on_error is set."
    )]
    async fn render_svg_batch(
        &self,
        Parameters(params): Parameters<RenderSvgBatchParams>,
    ) -> CallToolResult {
        let units = params.outputs.len().clamp(1, 64) as u32;
        self.run_tool("render_svg_batch", units, move |state| {
            tools::render_svg_batch::run(state, params)
        })
        .await
    }

    /// Build a multi-resolution icon from an SVG.
    #[tool(
        name = "render_icon",
        description = "Build a multi-resolution icon from an SVG: a Windows .ico, an Apple .icns, or a directory of PNG files. Every size is rasterised from the vector source rather than downscaled from a larger raster."
    )]
    async fn render_icon(
        &self,
        Parameters(params): Parameters<RenderIconParams>,
    ) -> CallToolResult {
        let units = params.sizes.as_ref().map(Vec::len).unwrap_or(7).clamp(1, 32) as u32;
        self.run_tool("render_icon", units, move |state| tools::render_icon::run(state, params))
            .await
    }

    /// Inspect an SVG without rendering it.
    #[tool(
        name = "probe_svg",
        description = "Inspect an SVG without rendering it: its intrinsic size and viewBox, what it contains, which fonts it needs and whether they resolve, which external files it references, and anything this renderer will not reproduce. Call this before rendering when fidelity matters."
    )]
    async fn probe_svg(&self, Parameters(params): Parameters<ProbeSvgParams>) -> CallToolResult {
        self.run_tool("probe_svg", 1, move |state| tools::probe_svg::run(state, params)).await
    }

    /// Normalise or shrink an SVG document.
    #[tool(
        name = "optimize_svg",
        description = "Normalise or shrink an SVG document. Mode normalize resolves it through the renderer's own pipeline and produces a much smaller file that renders identically; mode minify only strips comments and whitespace and leaves the document editable."
    )]
    async fn optimize_svg(
        &self,
        Parameters(params): Parameters<OptimizeSvgParams>,
    ) -> CallToolResult {
        self.run_tool("optimize_svg", 1, move |state| tools::optimize_svg::run(state, params)).await
    }

    /// Convert a raster image between formats, sizes and representations.
    #[tool(
        name = "convert_image",
        description = "Convert a raster image between formats, resize it, and move it between a file and Base64. For vector input use render_svg, which rasterises at an exact size instead of resampling."
    )]
    async fn convert_image(
        &self,
        Parameters(params): Parameters<ConvertImageParams>,
    ) -> CallToolResult {
        self.run_tool("convert_image", 1, move |state| tools::convert_image::run(state, params))
            .await
    }

    /// Report what this build can do.
    #[tool(
        name = "get_capabilities",
        description = "Report what this build can actually do: the formats it encodes, the SVG filter primitives it implements, the known gaps it will report rather than render silently, the fonts it has loaded, and the operator's configuration."
    )]
    async fn get_capabilities(&self, Parameters(_): Parameters<NoParams>) -> CallToolResult {
        self.run_tool("get_capabilities", 1, tools::capabilities::run).await
    }
}

#[tool_handler]
impl ServerHandler for SvgServer {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
        info.server_info = rmcp::model::Implementation::new(env!("CARGO_PKG_NAME"), crate::VERSION)
            .with_title("SVG to raster images");
        info.instructions = Some(
            "Renders SVG to raster images at absolute pixel sizes, writing wherever you say. \
             Sizes are given in pixels as width and height, not as a scale factor. \
             When width and height do not match the document's aspect ratio, the drawing is \
             fitted inside the canvas and the remainder stays transparent; fit stretch, cover \
             or error change that. Every result carries a warnings array: a non-empty one means \
             the image may differ from the document, for instance because a font was substituted \
             or a feature is unsupported. Call probe_svg first when that matters, and \
             get_capabilities once to learn this build's limits."
                .to_string(),
        );
        info
    }
}

impl std::fmt::Debug for SvgServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SvgServer").finish_non_exhaustive()
    }
}
