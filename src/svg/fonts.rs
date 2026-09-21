//! The font database, and the detection of substituted fonts.
//!
//! The renderer carries no system text stack, so every glyph comes from a face
//! in this database. The database is built once at start-up and cloned, with
//! the caller's extra faces added, for any call that supplies them.

use crate::config::Config;
use crate::error::{ErrorCode, Result, ToolError};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;

/// Per-call font settings.
#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontSettings {
    /// Family used when the document sets no `font-family`.
    pub default_family: Option<String>,
    /// Resolution of the generic `serif` family.
    pub serif_family: Option<String>,
    /// Resolution of the generic `sans-serif` family.
    pub sans_serif_family: Option<String>,
    /// Resolution of the generic `cursive` family.
    pub cursive_family: Option<String>,
    /// Resolution of the generic `fantasy` family.
    pub fantasy_family: Option<String>,
    /// Resolution of the generic `monospace` family.
    pub monospace_family: Option<String>,
    /// Additional font directories for this call.
    #[serde(default)]
    pub extra_dirs: Vec<PathBuf>,
    /// Additional font files for this call.
    #[serde(default)]
    pub extra_files: Vec<PathBuf>,
    /// Whether the platform's system fonts are ignored for this call.
    pub skip_system_fonts: Option<bool>,
}

/// The font database built at start-up, plus the operator's family pins.
pub struct FontStore {
    system: Arc<fontdb::Database>,
    bare: Arc<fontdb::Database>,
    config: Config,
}

impl FontStore {
    /// Builds the start-up database from the operator's configuration.
    ///
    /// Two databases are kept: one with the platform's fonts loaded and one
    /// without, so that a call asking for `skip_system_fonts` gets a database
    /// that genuinely has none rather than a filtered view of one that does.
    pub fn new(config: &Config) -> Self {
        let mut bare = fontdb::Database::new();
        load_operator_fonts(&mut bare, config);
        apply_generic_families(&mut bare, config);

        let mut system = fontdb::Database::new();
        if !config.skip_system_fonts {
            system.load_system_fonts();
        }
        load_operator_fonts(&mut system, config);
        apply_generic_families(&mut system, config);

        Self { system: Arc::new(system), bare: Arc::new(bare), config: config.clone() }
    }

    /// Builds the database for one call.
    pub fn for_call(&self, settings: &FontSettings) -> Result<Arc<fontdb::Database>> {
        let skip = settings.skip_system_fonts.unwrap_or(self.config.skip_system_fonts);
        let base = if skip { &self.bare } else { &self.system };

        let nothing_extra = settings.extra_dirs.is_empty()
            && settings.extra_files.is_empty()
            && settings.serif_family.is_none()
            && settings.sans_serif_family.is_none()
            && settings.cursive_family.is_none()
            && settings.fantasy_family.is_none()
            && settings.monospace_family.is_none();
        if nothing_extra {
            return Ok(Arc::clone(base));
        }

        let mut database = (**base).clone();
        for dir in &settings.extra_dirs {
            if !dir.is_dir() {
                return Err(ToolError::new(
                    ErrorCode::InputNotFound,
                    format!("Font directory {} does not exist.", dir.display()),
                ));
            }
            database.load_fonts_dir(dir);
        }
        for file in &settings.extra_files {
            database.load_font_file(file).map_err(|e| {
                ToolError::new(
                    ErrorCode::InputUnreadable,
                    format!("Font file {} cannot be loaded: {}", file.display(), e),
                )
            })?;
        }
        if let Some(name) = &settings.serif_family {
            database.set_serif_family(name);
        }
        if let Some(name) = &settings.sans_serif_family {
            database.set_sans_serif_family(name);
        }
        if let Some(name) = &settings.cursive_family {
            database.set_cursive_family(name);
        }
        if let Some(name) = &settings.fantasy_family {
            database.set_fantasy_family(name);
        }
        if let Some(name) = &settings.monospace_family {
            database.set_monospace_family(name);
        }
        Ok(Arc::new(database))
    }

    /// The default family for a call, from the caller or the configuration.
    pub fn default_family(&self, settings: &FontSettings) -> String {
        settings.default_family.clone().unwrap_or_else(|| self.config.default_family.clone())
    }

    /// Whether the system fonts were loaded into the start-up database.
    pub fn system_fonts_loaded(&self) -> bool {
        !self.config.skip_system_fonts
    }

    /// The number of faces in the start-up database.
    pub fn face_count(&self) -> usize {
        self.system.len()
    }

    /// The families in the start-up database, sorted and deduplicated.
    pub fn families(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .system
            .faces()
            .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    /// How the generic families resolve in the start-up database.
    pub fn generic_families(&self) -> Vec<(&'static str, String)> {
        use fontdb::Family;
        vec![
            ("serif", self.system.family_name(&Family::Serif).to_string()),
            ("sans_serif", self.system.family_name(&Family::SansSerif).to_string()),
            ("cursive", self.system.family_name(&Family::Cursive).to_string()),
            ("fantasy", self.system.family_name(&Family::Fantasy).to_string()),
            ("monospace", self.system.family_name(&Family::Monospace).to_string()),
        ]
    }
}

fn load_operator_fonts(database: &mut fontdb::Database, config: &Config) {
    for dir in &config.font_dirs {
        database.load_fonts_dir(dir);
    }
    for file in &config.font_files {
        if let Err(error) = database.load_font_file(file) {
            eprintln!("[WARN] font file {} could not be loaded: {}", file.display(), error);
        }
    }
}

fn apply_generic_families(database: &mut fontdb::Database, config: &Config) {
    if let Some(name) = &config.serif_family {
        database.set_serif_family(name);
    }
    if let Some(name) = &config.sans_serif_family {
        database.set_sans_serif_family(name);
    }
    if let Some(name) = &config.cursive_family {
        database.set_cursive_family(name);
    }
    if let Some(name) = &config.fantasy_family {
        database.set_fantasy_family(name);
    }
    if let Some(name) = &config.monospace_family {
        database.set_monospace_family(name);
    }
}

/// The generic family names CSS defines, which never count as missing.
pub const GENERIC_FAMILIES: [&str; 6] =
    ["serif", "sans-serif", "cursive", "fantasy", "monospace", "system-ui"];

/// Whether the database holds a face for this family name.
pub fn has_family(database: &fontdb::Database, family: &str) -> bool {
    let wanted = family.trim().trim_matches(['"', '\'']).to_ascii_lowercase();
    if wanted.is_empty() || GENERIC_FAMILIES.contains(&wanted.as_str()) {
        return true;
    }
    database
        .faces()
        .any(|face| face.families.iter().any(|(name, _)| name.to_ascii_lowercase() == wanted))
}

/// What a family resolves to when it is not in the database.
pub fn substitute_for(database: &fontdb::Database, default_family: &str) -> String {
    if has_family(database, default_family) {
        default_family.to_string()
    } else {
        database.family_name(&fontdb::Family::SansSerif).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_families_are_never_missing() {
        let database = fontdb::Database::new();
        for generic in GENERIC_FAMILIES {
            assert!(has_family(&database, generic));
        }
        assert!(!has_family(&database, "Definitely Not Installed"));
    }

    #[test]
    fn quotes_around_a_family_name_are_ignored() {
        let database = fontdb::Database::new();
        assert!(has_family(&database, "'serif'"));
    }
}
