//! Error type and stable error codes shared by every tool.
//!
//! Tool failures are values, not panics: each one carries a machine-readable
//! code from [`ErrorCode`], one human sentence, an optional detail object and
//! an optional hint telling the caller what to do differently.

use serde_json::{Value, json};

/// Stable, machine-readable classification of a tool failure.
///
/// The string form is part of this server's public contract and is what a
/// caller matches on; the message text is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// A parameter is missing, contradictory or out of range.
    InvalidInput,
    /// The named input file does not exist.
    InputNotFound,
    /// The input exists but cannot be read.
    InputUnreadable,
    /// A path lies outside a configured allowlist.
    PathNotAllowed,
    /// Remote access was requested while it is disabled.
    RemoteDisabled,
    /// A permitted remote fetch failed.
    RemoteFailed,
    /// The document is not well-formed, or has no usable SVG root.
    ParseFailed,
    /// `export_id` names an element that is not in the document.
    ElementNotFound,
    /// The resolved drawing has zero area.
    EmptyRender,
    /// `fit` is `error` and the aspect ratios differ.
    AspectMismatch,
    /// A size or pixel-count limit was exceeded.
    SizeLimitExceeded,
    /// Strict fidelity was requested and the document uses an unsupported feature.
    UnsupportedFeature,
    /// Strict font handling was requested and a family is unavailable.
    FontMissing,
    /// The format cannot be encoded by this build, or cannot carry what was asked.
    UnsupportedFormat,
    /// The target exists and overwriting was refused.
    OutputExists,
    /// The target cannot be written.
    OutputUnwritable,
    /// The encoder rejected the image.
    EncodeFailed,
    /// An inline result exceeds the configured size limit.
    InlineTooLarge,
    /// A bug in this server.
    InternalError,
}

impl ErrorCode {
    /// The wire form of this code.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::InputNotFound => "input_not_found",
            Self::InputUnreadable => "input_unreadable",
            Self::PathNotAllowed => "path_not_allowed",
            Self::RemoteDisabled => "remote_disabled",
            Self::RemoteFailed => "remote_failed",
            Self::ParseFailed => "parse_failed",
            Self::ElementNotFound => "element_not_found",
            Self::EmptyRender => "empty_render",
            Self::AspectMismatch => "aspect_mismatch",
            Self::SizeLimitExceeded => "size_limit_exceeded",
            Self::UnsupportedFeature => "unsupported_feature",
            Self::FontMissing => "font_missing",
            Self::UnsupportedFormat => "unsupported_format",
            Self::OutputExists => "output_exists",
            Self::OutputUnwritable => "output_unwritable",
            Self::EncodeFailed => "encode_failed",
            Self::InlineTooLarge => "inline_too_large",
            Self::InternalError => "internal_error",
        }
    }
}

/// A tool failure, reported to the caller as a tool-level error result.
#[derive(Debug)]
pub struct ToolError {
    /// The stable classification.
    pub code: ErrorCode,
    /// One sentence for a human reader.
    pub message: String,
    /// The specifics, as a JSON object.
    pub detail: Option<Value>,
    /// What the caller can do differently, when there is an obvious answer.
    pub hint: Option<String>,
}

impl ToolError {
    /// Builds an error with a code and a message.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), detail: None, hint: None }
    }

    /// Attaches the detail object.
    #[must_use]
    pub fn with_detail(mut self, detail: Value) -> Self {
        self.detail = Some(detail);
        self
    }

    /// Attaches the hint.
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// The structured payload returned alongside the error text.
    pub fn payload(&self) -> Value {
        let mut error = serde_json::Map::new();
        error.insert("code".into(), json!(self.code.as_str()));
        error.insert("message".into(), json!(self.message));
        if let Some(detail) = &self.detail {
            error.insert("detail".into(), detail.clone());
        }
        if let Some(hint) = &self.hint {
            error.insert("hint".into(), json!(hint));
        }
        json!({ "error": Value::Object(error) })
    }

    /// The one-line text form shown by clients that render only text.
    pub fn text(&self) -> String {
        match &self.hint {
            Some(hint) => format!("[{}] {} {}", self.code.as_str(), self.message, hint),
            None => format!("[{}] {}", self.code.as_str(), self.message),
        }
    }
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text())
    }
}

impl std::error::Error for ToolError {}

/// Shorthand for a fallible tool operation.
pub type Result<T> = std::result::Result<T, ToolError>;

/// Builds an [`ErrorCode::InvalidInput`] error.
pub fn invalid_input(message: impl Into<String>) -> ToolError {
    ToolError::new(ErrorCode::InvalidInput, message)
}

/// Builds an [`ErrorCode::InternalError`] error.
pub fn internal(message: impl Into<String>) -> ToolError {
    ToolError::new(ErrorCode::InternalError, message)
}
