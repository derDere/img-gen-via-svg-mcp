//! An MCP server that renders SVG documents to raster images.
//!
//! The crate is split so that everything below the protocol layer can be used,
//! and tested, without an MCP client: [`svg`] reads and inspects documents,
//! [`render`] turns one into pixels at an exact size, [`encode`] writes those
//! pixels into a file format, and [`io`] holds the path and network policy.
//! Only [`mcp`] knows that any of this is reachable over a protocol.

pub mod config;
pub mod encode;
pub mod error;
pub mod io;
pub mod mcp;
pub mod pipeline;
pub mod render;
pub mod svg;
pub mod warning;

/// This server's version, as published to a connecting client.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
