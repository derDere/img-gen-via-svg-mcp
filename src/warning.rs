//! Fidelity and behaviour warnings collected during a call.
//!
//! A warning says that the produced image may not be what the document or the
//! caller asked for. Every deviation this server makes — a substituted font, a
//! dropped element, a flattened alpha channel, a letterboxed canvas — appears
//! here, because the alternative is a silently wrong image.

use serde::Serialize;
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// One deviation, reported to the caller.
#[derive(Debug, Clone, Serialize)]
pub struct Warning {
    /// Stable, machine-readable classification.
    pub code: String,
    /// One sentence for a human reader.
    pub message: String,
    /// The specifics, when there are any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
}

impl Warning {
    /// Builds a warning without a detail object.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into(), detail: None }
    }

    /// Builds a warning carrying a detail object.
    pub fn with_detail(code: impl Into<String>, message: impl Into<String>, detail: Value) -> Self {
        Self { code: code.into(), message: message.into(), detail: Some(detail) }
    }
}

/// Collects the warnings raised while one call runs.
///
/// The collector is cheap to clone and shared between the pipeline stages, so
/// that a stage deep in the renderer can report a deviation without threading a
/// return value back through every caller.
#[derive(Debug, Clone, Default)]
pub struct Warnings {
    inner: Arc<Mutex<Vec<Warning>>>,
}

impl Warnings {
    /// Creates an empty collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one warning.
    pub fn push(&self, warning: Warning) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.push(warning);
        }
    }

    /// Records a warning built from a code and a message.
    pub fn warn(&self, code: impl Into<String>, message: impl Into<String>) {
        self.push(Warning::new(code, message));
    }

    /// Records a warning built from a code, a message and a detail object.
    pub fn warn_detail(&self, code: impl Into<String>, message: impl Into<String>, detail: Value) {
        self.push(Warning::with_detail(code, message, detail));
    }

    /// Returns every warning collected so far, oldest first.
    pub fn collect(&self) -> Vec<Warning> {
        self.inner.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Returns true when nothing has been recorded.
    pub fn is_empty(&self) -> bool {
        self.inner.lock().map(|g| g.is_empty()).unwrap_or(true)
    }

    /// Returns true when a warning with this code has been recorded.
    pub fn contains(&self, code: &str) -> bool {
        self.inner.lock().map(|g| g.iter().any(|w| w.code == code)).unwrap_or(false)
    }
}
