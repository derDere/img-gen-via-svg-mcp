//! State shared by every tool call.

use crate::config::Config;
use crate::svg::fonts::FontStore;
use std::sync::Arc;

/// The operator's configuration and the font database, built once at start-up.
pub struct ServerState {
    /// What the operator configured.
    pub config: Config,
    /// The font database.
    pub fonts: FontStore,
}

impl ServerState {
    /// Builds the state from a configuration, loading the fonts.
    pub fn new(config: Config) -> Arc<Self> {
        let fonts = FontStore::new(&config);
        Arc::new(Self { config, fonts })
    }
}
