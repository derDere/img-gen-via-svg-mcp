//! Capture of the parser's own diagnostics.
//!
//! `usvg` reports what it discards through the `log` crate. Left alone those
//! messages go to the operator's terminal and never reach the caller, which is
//! precisely the silent-failure mode this server exists to avoid. The logger
//! installed here routes messages into a per-thread buffer while a call is
//! running, so every diagnostic can be turned into a warning in that call's
//! result, and forwards everything else to stderr.

use std::cell::RefCell;

thread_local! {
    /// Messages captured on this thread, when capture is active.
    static BUFFER: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// The logger installed process-wide at start-up.
struct CapturingLogger;

impl log::Log for CapturingLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let message = record.args().to_string();
        let captured = BUFFER.with(|buffer| {
            let mut buffer = buffer.borrow_mut();
            match buffer.as_mut() {
                Some(messages) => {
                    messages.push(message.clone());
                    true
                }
                None => false,
            }
        });
        if !captured {
            eprintln!("[{}] {}", record.level(), message);
        }
    }

    fn flush(&self) {}
}

/// Installs the capturing logger. Calling this more than once is harmless.
pub fn install() {
    let _ = log::set_boxed_logger(Box::new(CapturingLogger));
    log::set_max_level(log::LevelFilter::Warn);
}

/// Runs `body` with diagnostic capture active on this thread and returns its
/// result together with everything the parser reported.
pub fn capture<T>(body: impl FnOnce() -> T) -> (T, Vec<String>) {
    BUFFER.with(|buffer| *buffer.borrow_mut() = Some(Vec::new()));
    let value = body();
    let messages = BUFFER.with(|buffer| buffer.borrow_mut().take()).unwrap_or_default();
    (value, messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_only_inside_the_scope() {
        install();
        let (_, messages) = capture(|| log::warn!("inside"));
        assert_eq!(messages, vec!["inside".to_string()]);
        let (_, empty) = capture(|| {});
        assert!(empty.is_empty());
    }
}
