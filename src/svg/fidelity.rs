//! The fidelity contract.
//!
//! Everything this server will not reproduce is named here, detected per
//! document, and reported in the result. Rendering a feature wrong without
//! saying so is the worst failure this project can have, so the catalogue
//! below, the scan that feeds it and the strict modes that act on it are the
//! load-bearing parts of the whole server.

use crate::config::Config;
use crate::error::{ErrorCode, Result, ToolError};
use crate::svg::fonts;
use crate::svg::scan::ScanReport;
use crate::warning::Warnings;
use serde::Deserialize;
use serde_json::json;
use std::path::Path;

/// What happens when a document uses something that will not render.
#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OnUnsupported {
    /// Render, and report every finding as a warning. The default.
    #[default]
    Warn,
    /// Refuse as soon as one unsupported construct is found.
    Error,
}

/// What happens when a font family is unavailable.
#[derive(Debug, Clone, Copy, Default, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OnMissingFont {
    /// Substitute, and report it as a warning. The default.
    #[default]
    Warn,
    /// Refuse rather than render in a different typeface.
    Error,
}

/// One entry in the catalogue of known gaps.
pub struct Gap {
    /// The warning code this gap raises.
    pub code: &'static str,
    /// One sentence describing what happens.
    pub description: &'static str,
}

/// Every gap this server knows about.
///
/// A gap that renders wrong without appearing here is a defect of the highest
/// severity; the corpus-coverage test refuses to build without a test case per
/// entry.
pub const GAPS: [Gap; 12] = [
    Gap {
        code: "animation",
        description: "SMIL animation is not rendered; the element's initial state is drawn.",
    },
    Gap { code: "scripting", description: "script elements and event attributes are ignored." },
    Gap {
        code: "interactive",
        description: "a renders its children; view and cursor have no effect.",
    },
    Gap {
        code: "css_advanced",
        description: "Only a subset of CSS is resolved; @media, @supports, @layer and custom properties are not.",
    },
    Gap { code: "svg_tiny_1_2", description: "SVG Tiny 1.2 specific elements are not supported." },
    Gap { code: "foreign_object", description: "foreignObject content is not rendered." },
    Gap {
        code: "filter_unsupported",
        description: "An unresolvable filter drops its element, as the SVG specification requires.",
    },
    Gap {
        code: "font_missing",
        description: "Text whose font family is not in the database is not rendered at all; the renderer does not fall back to another family for it.",
    },
    Gap {
        code: "font_substituted",
        description: "Text that names no family of its own is drawn in the default family, and when that one is absent too, in an available one.",
    },
    Gap {
        code: "unresolved_reference",
        description: "A referenced file or URL that cannot be resolved drops its element.",
    },
    Gap {
        code: "filter_displacement_scale",
        description: "feDisplacementMap's scale is applied twice by this renderer, so a document asking for N is displaced by about N squared.",
    },
    Gap {
        code: "parser_diagnostic",
        description: "The parser reported something it could not resolve; the message is passed through verbatim.",
    },
];

/// The outcome of checking one document against the contract.
#[derive(Debug, Default)]
pub struct FidelityReport {
    /// Findings that make the render differ from the document.
    pub findings: Vec<serde_json::Value>,
    /// Font families the document asked for that the database cannot supply.
    pub missing_fonts: Vec<String>,
}

impl FidelityReport {
    /// Whether the document renders exactly as written.
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty() && self.missing_fonts.is_empty()
    }
}

/// Checks a document against the contract and records what it finds.
///
/// Returns an error instead of warnings when the caller asked for the strict
/// mode and something was found.
#[allow(clippy::too_many_arguments)]
pub fn check(
    scan: &ScanReport,
    diagnostics: &[String],
    database: &fontdb::Database,
    resources_dir: Option<&Path>,
    config: &Config,
    on_unsupported: OnUnsupported,
    on_missing_font: OnMissingFont,
    warnings: &Warnings,
) -> Result<FidelityReport> {
    let mut report = FidelityReport::default();

    for finding in &scan.findings {
        report.findings.push(json!({
            "code": finding.code,
            "element": finding.element,
            "count": finding.count,
            "message": finding.message,
        }));
        warnings.warn_detail(
            finding.code.clone(),
            format!("{} ({}×): {}", finding.element, finding.count, finding.message),
            json!({ "element": finding.element, "count": finding.count }),
        );
    }

    for reference in &scan.references {
        if let Some(problem) = unresolvable(&reference.href, resources_dir, config) {
            report.findings.push(json!({
                "code": "unresolved_reference",
                "element": reference.kind,
                "count": 1,
                "message": problem.clone(),
                "href": reference.href,
            }));
            warnings.warn_detail(
                "unresolved_reference",
                problem,
                json!({ "href": reference.href, "element": reference.kind }),
            );
        }
    }

    for family in &scan.font_families {
        if !fonts::has_family(database, family) {
            report.missing_fonts.push(family.clone());
        }
    }
    // The renderer queries the family list and draws nothing when none of it
    // matches: text in a missing family disappears rather than changing
    // typeface. Saying "substituted" here would be a comfortable lie.
    for family in &report.missing_fonts {
        warnings.warn_detail(
            "font_missing",
            format!(
                "Font family {family:?} is not in the font database, so the text using it is not rendered."
            ),
            json!({
                "requested": family,
                "consequence": "text_not_rendered",
                "remedy": "Supply the font through fonts.extra_dirs or fonts.extra_files, or change the document's font-family.",
            }),
        );
    }

    for message in diagnostics {
        let code = classify(message);
        // A font message the family check already reported would be a duplicate.
        if code == "font_missing" && !report.missing_fonts.is_empty() {
            continue;
        }
        report.findings.push(json!({
            "code": code,
            "element": "",
            "count": 1,
            "message": message,
        }));
        warnings.warn_detail(code, message.clone(), json!({ "source": "parser" }));
    }

    if on_missing_font == OnMissingFont::Error && !report.missing_fonts.is_empty() {
        return Err(ToolError::new(
            ErrorCode::FontMissing,
            format!(
                "{} font famil{} unavailable and on_missing_font is error.",
                report.missing_fonts.len(),
                if report.missing_fonts.len() == 1 { "y is" } else { "ies are" }
            ),
        )
        .with_detail(json!({ "missing_fonts": report.missing_fonts }))
        .with_hint("Supply the fonts through fonts.extra_dirs or fonts.extra_files."));
    }

    if on_unsupported == OnUnsupported::Error && !report.findings.is_empty() {
        return Err(ToolError::new(
            ErrorCode::UnsupportedFeature,
            format!(
                "The document uses {} construct(s) this renderer does not reproduce, and on_unsupported is error.",
                report.findings.len()
            ),
        )
        .with_detail(json!({ "findings": report.findings }))
        .with_hint("Set on_unsupported to warn to render anyway and receive the findings as warnings."));
    }

    Ok(report)
}

/// Says why a reference cannot be resolved, or nothing when it can.
fn unresolvable(href: &str, resources_dir: Option<&Path>, config: &Config) -> Option<String> {
    if href.starts_with("http://") || href.starts_with("https://") {
        return (!config.remote_svg_references).then(|| {
            format!(
                "Reference {href} points at a remote resource and remote references are disabled; the element is not drawn."
            )
        });
    }
    let candidate = match resources_dir {
        Some(dir) => dir.join(href),
        None => {
            return Some(format!(
                "Reference {href} is relative but no resources_dir is known, so it cannot be resolved; the element is not drawn."
            ));
        }
    };
    (!candidate.exists()).then(|| {
        format!(
            "Reference {href} does not exist under {}; the element is not drawn.",
            resources_dir.map(|d| d.display().to_string()).unwrap_or_default()
        )
    })
}

/// Classifies a renderer message into a catalogue code.
pub fn classify(message: &str) -> &'static str {
    let lower = message.to_lowercase();
    if lower.contains("fallback from") {
        "font_substituted"
    } else if lower.contains("font") {
        "font_missing"
    } else if lower.contains("filter") {
        "filter_unsupported"
    } else if lower.contains("image") || lower.contains("load") || lower.contains("resolve") {
        "unresolved_reference"
    } else {
        "parser_diagnostic"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svg::scan;

    fn empty_db() -> fontdb::Database {
        fontdb::Database::new()
    }

    #[test]
    fn every_catalogue_code_is_unique() {
        let mut codes: Vec<&str> = GAPS.iter().map(|g| g.code).collect();
        let before = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), before);
    }

    #[test]
    fn animation_becomes_a_warning_and_a_finding() {
        let report = scan::scan(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><rect><animate attributeName="x"/></rect></svg>"#,
        )
        .unwrap();
        let warnings = Warnings::new();
        let outcome = check(
            &report,
            &[],
            &empty_db(),
            None,
            &Config::default(),
            OnUnsupported::Warn,
            OnMissingFont::Warn,
            &warnings,
        )
        .unwrap();
        assert!(!outcome.is_clean());
        assert!(warnings.contains("animation"));
    }

    #[test]
    fn strict_mode_refuses_instead_of_rendering() {
        let report = scan::scan(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><rect><animate attributeName="x"/></rect></svg>"#,
        )
        .unwrap();
        let error = check(
            &report,
            &[],
            &empty_db(),
            None,
            &Config::default(),
            OnUnsupported::Error,
            OnMissingFont::Warn,
            &Warnings::new(),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::UnsupportedFeature);
    }

    #[test]
    fn a_missing_font_is_reported_and_can_refuse() {
        let report = scan::scan(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><text font-family="Nope Sans">x</text></svg>"#,
        )
        .unwrap();
        let warnings = Warnings::new();
        let outcome = check(
            &report,
            &[],
            &empty_db(),
            None,
            &Config::default(),
            OnUnsupported::Warn,
            OnMissingFont::Warn,
            &warnings,
        )
        .unwrap();
        assert_eq!(outcome.missing_fonts, vec!["Nope Sans".to_string()]);
        assert!(warnings.contains("font_missing"));

        let error = check(
            &report,
            &[],
            &empty_db(),
            None,
            &Config::default(),
            OnUnsupported::Warn,
            OnMissingFont::Error,
            &Warnings::new(),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::FontMissing);
    }
}
