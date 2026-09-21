//! Assembly of tool results.
//!
//! Every tool returns the same shape: a structured JSON object matching its
//! section of the specification, a short line of text for clients that render
//! only text, and — when the caller asked for it — one image content block per
//! produced image.

use crate::error::ToolError;
use crate::warning::Warning;
use base64::Engine;
use rmcp::model::{CallToolResult, ContentBlock};
use serde_json::{Value, json};

/// An image attached to a result.
#[derive(Debug)]
pub struct InlineImage {
    /// The encoded file bytes.
    pub bytes: Vec<u8>,
    /// Its media type.
    pub mime_type: &'static str,
}

/// What a tool produced.
#[derive(Debug)]
pub struct ToolOutcome {
    /// The structured result.
    pub structured: Value,
    /// One line for a human reader.
    pub text: String,
    /// Images to attach, in the order they were produced.
    pub images: Vec<InlineImage>,
}

impl ToolOutcome {
    /// Builds an outcome without inline images.
    pub fn new(structured: Value, text: impl Into<String>) -> Self {
        Self { structured, text: text.into(), images: Vec::new() }
    }

    /// Attaches an image.
    #[must_use]
    pub fn with_image(mut self, image: InlineImage) -> Self {
        self.images.push(image);
        self
    }
}

/// Turns an outcome into the protocol's result type.
pub fn success(outcome: ToolOutcome) -> CallToolResult {
    let mut content = vec![ContentBlock::text(outcome.text)];
    for image in outcome.images {
        let data = base64::engine::general_purpose::STANDARD.encode(&image.bytes);
        content.push(ContentBlock::image(data, image.mime_type.to_string()));
    }
    let mut result = CallToolResult::success(content);
    result.structured_content = Some(outcome.structured);
    result
}

/// Turns a failure into the protocol's result type.
///
/// Tool failures are returned as tool-level errors rather than JSON-RPC errors,
/// because clients render the former to the user and hide the latter, and these
/// messages are written to be read.
pub fn failure(error: &ToolError) -> CallToolResult {
    let mut result = CallToolResult::error(vec![ContentBlock::text(error.text())]);
    result.structured_content = Some(error.payload());
    result
}

/// Renders the collected warnings into the result's `warnings` array.
pub fn warnings_json(warnings: &[Warning]) -> Value {
    json!(warnings)
}

/// A one-line summary of how many warnings a call raised.
pub fn warning_suffix(warnings: &[Warning]) -> String {
    match warnings.len() {
        0 => String::new(),
        1 => format!(" 1 warning: {}.", warnings[0].code),
        n => {
            let mut codes: Vec<&str> = warnings.iter().map(|w| w.code.as_str()).collect();
            codes.sort_unstable();
            codes.dedup();
            format!(" {n} warnings: {}.", codes.join(", "))
        }
    }
}
