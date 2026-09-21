//! Process entry point: configuration, fonts, stdio transport, shutdown.

use img_gen_via_svg_mcp::config::Config;
use img_gen_via_svg_mcp::mcp::server::SvgServer;
use img_gen_via_svg_mcp::svg::diagnostics;
use rmcp::{ServiceExt, transport::stdio};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // stdout carries the MCP framing, so every line of logging goes to stderr.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("IMG_SVG_MCP_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_ansi(false)
        .init();

    // The renderer reports what it discards through the `log` crate; this
    // routes those messages into the call that caused them.
    diagnostics::install();

    let config = match Config::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Configuration error: {error}");
            std::process::exit(2);
        }
    };

    tracing::info!(
        version = img_gen_via_svg_mcp::VERSION,
        max_pixels = config.max_pixels,
        remote_input = config.remote_input,
        "img-gen-via-svg-mcp starting on stdio"
    );

    let service = SvgServer::new(config).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
