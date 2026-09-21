//! The pre-parse scan.
//!
//! `usvg` discards some constructs without saying anything — an `animate`
//! element simply is not in the resolved tree, and nothing in the parser's
//! output distinguishes a document that never had one. The scan therefore reads
//! the raw XML first and records what is there, so that the fidelity report can
//! name what the render will leave out.

use crate::error::{ErrorCode, Result, ToolError};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// One construct the renderer will not reproduce.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    /// The gap code from the catalogue.
    pub code: String,
    /// The element or selector the finding is about.
    pub element: String,
    /// How many times it occurs.
    pub count: usize,
    /// One sentence describing the consequence.
    pub message: String,
}

/// An external resource the document points at.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Reference {
    /// The href as written.
    pub href: String,
    /// What kind of element refers to it.
    pub kind: String,
}

/// Everything the scan found.
#[derive(Debug, Default, Clone)]
pub struct ScanReport {
    /// Constructs the renderer will not reproduce.
    pub findings: Vec<Finding>,
    /// Font families the document names.
    pub font_families: BTreeSet<String>,
    /// External references, excluding fragment-only ones.
    pub references: Vec<Reference>,
    /// Filter primitive element names in document order of first appearance.
    pub filter_primitives: BTreeSet<String>,
    /// Blend modes named by `mix-blend-mode`.
    pub blend_modes: BTreeSet<String>,
    /// Element name counts, used for the content inventory.
    pub element_counts: BTreeMap<String, usize>,
    /// Total number of elements.
    pub node_count: usize,
    /// The root `width` attribute, as written.
    pub root_width: Option<String>,
    /// The root `height` attribute, as written.
    pub root_height: Option<String>,
    /// The root `viewBox` attribute, as written.
    pub root_view_box: Option<String>,
    /// The root `preserveAspectRatio` attribute, as written.
    pub root_preserve_aspect_ratio: Option<String>,
    /// Whether any text element relies on the default font family because
    /// neither it nor an ancestor names one.
    pub text_without_explicit_family: bool,
}

impl ScanReport {
    /// How many elements of this name the document holds.
    pub fn count_of(&self, name: &str) -> usize {
        self.element_counts.get(name).copied().unwrap_or(0)
    }

    /// Whether any of the named elements is present.
    pub fn has_any(&self, names: &[&str]) -> bool {
        names.iter().any(|name| self.count_of(name) > 0)
    }
}

/// Elements whose `href` loads a resource. A hyperlink or a fragment reference
/// is not a resource, so neither is collected.
const RESOURCE_ELEMENTS: [&str; 3] = ["image", "feImage", "use"];

/// Elements that carry text and therefore need a font.
const TEXT_ELEMENTS: [&str; 3] = ["text", "tspan", "textPath"];

const ANIMATION_ELEMENTS: [&str; 5] =
    ["animate", "animateTransform", "animateMotion", "animateColor", "set"];
const TINY_ELEMENTS: [&str; 4] = ["textArea", "prefetch", "discard", "handler"];
const FILTER_PRIMITIVES: [&str; 17] = [
    "feBlend",
    "feColorMatrix",
    "feComponentTransfer",
    "feComposite",
    "feConvolveMatrix",
    "feDiffuseLighting",
    "feDisplacementMap",
    "feDropShadow",
    "feFlood",
    "feGaussianBlur",
    "feImage",
    "feMerge",
    "feMorphology",
    "feOffset",
    "feSpecularLighting",
    "feTile",
    "feTurbulence",
];

/// Reads the document as XML and reports what it holds.
pub fn scan(text: &str) -> Result<ScanReport> {
    let options = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let document = roxmltree::Document::parse_with_options(text, options).map_err(|e| {
        ToolError::new(ErrorCode::ParseFailed, format!("The document is not well-formed XML: {e}"))
            .with_detail(serde_json::json!({ "parser_message": e.to_string() }))
    })?;

    if document.root_element().tag_name().name() != "svg" {
        return Err(ToolError::new(
            ErrorCode::ParseFailed,
            format!(
                "The document root is <{}>, not <svg>.",
                document.root_element().tag_name().name()
            ),
        ));
    }

    let mut report = ScanReport::default();
    let mut stylesheets = String::new();

    let root = document.root_element();
    report.root_width = root.attribute("width").map(str::to_string);
    report.root_height = root.attribute("height").map(str::to_string);
    report.root_view_box = root.attribute("viewBox").map(str::to_string);
    report.root_preserve_aspect_ratio = root.attribute("preserveAspectRatio").map(str::to_string);

    for node in document.descendants().filter(roxmltree::Node::is_element) {
        let name = node.tag_name().name().to_string();
        report.node_count += 1;
        *report.element_counts.entry(name.clone()).or_default() += 1;

        if FILTER_PRIMITIVES.contains(&name.as_str()) {
            report.filter_primitives.insert(name.clone());
        }
        if name == "feDisplacementMap" {
            check_displacement_scale(&node, &mut report.findings);
        }
        if name == "style" {
            stylesheets.push_str(&node.text().unwrap_or_default().to_lowercase());
        }

        for attribute in node.attributes() {
            match attribute.name() {
                "font-family" => collect_families(attribute.value(), &mut report.font_families),
                "style" => {
                    let value = attribute.value();
                    if let Some(families) = css_value(value, "font-family") {
                        collect_families(&families, &mut report.font_families);
                    }
                    if let Some(mode) = css_value(value, "mix-blend-mode") {
                        report.blend_modes.insert(mode.trim().to_string());
                    }
                }
                "mix-blend-mode" => {
                    report.blend_modes.insert(attribute.value().trim().to_string());
                }
                "href" | "xlink:href" => {
                    let href = attribute.value().trim();
                    if RESOURCE_ELEMENTS.contains(&name.as_str())
                        && !href.is_empty()
                        && !href.starts_with('#')
                        && !href.starts_with("data:")
                    {
                        report
                            .references
                            .push(Reference { href: href.to_string(), kind: name.clone() });
                    }
                }
                other if other.starts_with("on") && other.len() > 2 => {
                    add(
                        &mut report.findings,
                        "scripting",
                        other,
                        "An event attribute has no effect; the render is static.",
                    );
                }
                _ => {}
            }
        }

        if TEXT_ELEMENTS.contains(&name.as_str())
            && node.text().is_some_and(|text| !text.trim().is_empty())
            && !names_a_font_family(&node)
        {
            report.text_without_explicit_family = true;
        }

        if ANIMATION_ELEMENTS.contains(&name.as_str()) {
            add(
                &mut report.findings,
                "animation",
                &name,
                "SMIL animation is not rendered; the element's initial state is drawn.",
            );
        } else if name == "script" {
            add(&mut report.findings, "scripting", &name, "Script elements are ignored.");
        } else if name == "foreignObject" {
            add(
                &mut report.findings,
                "foreign_object",
                &name,
                "foreignObject content is not rendered.",
            );
        } else if name == "view" || name == "cursor" {
            add(
                &mut report.findings,
                "interactive",
                &name,
                "The element has no effect on a static render.",
            );
        } else if TINY_ELEMENTS.contains(&name.as_str()) {
            add(
                &mut report.findings,
                "svg_tiny_1_2",
                &name,
                "SVG Tiny 1.2 specific elements are not supported.",
            );
        }
    }

    for (pattern, what) in [
        ("@media", "@media"),
        ("@supports", "@supports"),
        ("@layer", "@layer"),
        ("var(--", "custom property"),
    ] {
        if stylesheets.contains(pattern) {
            add(
                &mut report.findings,
                "css_advanced",
                what,
                "This CSS construct is not resolved; the declarations under it do not apply.",
            );
        }
    }

    report.findings.sort();
    report.references.sort();
    report.references.dedup();
    Ok(report)
}

/// Whether this element or an ancestor sets a font family.
fn names_a_font_family(node: &roxmltree::Node<'_, '_>) -> bool {
    std::iter::once(*node).chain(node.ancestors()).any(|ancestor| {
        ancestor.attribute("font-family").is_some()
            || ancestor
                .attribute("style")
                .is_some_and(|style| css_value(style, "font-family").is_some())
    })
}

/// Reports the renderer's quadratic handling of `feDisplacementMap`.
///
/// resvg 0.48 multiplies the displacement by the primitive's `scale` twice, so
/// a document asking for 38 is displaced by roughly 1444 pixels and its content
/// leaves the filter region entirely. The square root of the requested value
/// produces the displacement the document asks for, and the finding says so.
fn check_displacement_scale(node: &roxmltree::Node<'_, '_>, findings: &mut Vec<Finding>) {
    let Some(raw) = node.attribute("scale") else { return };
    let Ok(scale) = raw.trim().parse::<f32>() else { return };
    if scale.abs() <= 1.0 {
        return;
    }
    let effective = scale * scale;
    add(
        findings,
        "filter_displacement_scale",
        "feDisplacementMap",
        &format!(
            "This renderer applies the scale twice, so scale=\"{raw}\" displaces by about {effective:.0} pixels instead of {scale:.0}. Use scale=\"{:.3}\" to get the displacement this document asks for.",
            scale.abs().sqrt() * scale.signum()
        ),
    );
}

/// Records a finding, merging it with an identical earlier one.
fn add(findings: &mut Vec<Finding>, code: &str, element: &str, message: &str) {
    if let Some(existing) = findings.iter_mut().find(|f| f.code == code && f.element == element) {
        existing.count += 1;
        return;
    }
    findings.push(Finding {
        code: code.to_string(),
        element: element.to_string(),
        count: 1,
        message: message.to_string(),
    });
}

/// Splits a `font-family` list into its individual family names.
fn collect_families(value: &str, into: &mut BTreeSet<String>) {
    for family in value.split(',') {
        let family = family.trim().trim_matches(['"', '\'']).trim();
        if !family.is_empty() {
            into.insert(family.to_string());
        }
    }
}

/// Reads one declaration out of an inline `style` attribute.
fn css_value(style: &str, property: &str) -> Option<String> {
    style.split(';').find_map(|declaration| {
        let (name, value) = declaration.split_once(':')?;
        (name.trim().eq_ignore_ascii_case(property)).then(|| value.trim().to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_animation_and_counts_it() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
            <rect><animate attributeName="x" to="10"/><animate attributeName="y" to="5"/></rect>
        </svg>"#;
        let report = scan(svg).unwrap();
        let finding = report.findings.iter().find(|f| f.code == "animation").unwrap();
        assert_eq!(finding.element, "animate");
        assert_eq!(finding.count, 2);
    }

    #[test]
    fn collects_font_families_from_attributes_and_styles() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
            <text font-family="Inter, sans-serif">a</text>
            <text style="font-family: 'Comic Neue'">b</text>
        </svg>"#;
        let report = scan(svg).unwrap();
        assert!(report.font_families.contains("Inter"));
        assert!(report.font_families.contains("Comic Neue"));
    }

    #[test]
    fn records_filter_primitives_and_references() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
            <filter id="f"><feTurbulence/><feGaussianBlur/></filter>
            <image href="logo.png"/>
            <use xlink:href="#f"/>
        </svg>"##;
        let report = scan(svg).unwrap();
        assert!(report.filter_primitives.contains("feTurbulence"));
        assert_eq!(report.references.len(), 1);
        assert_eq!(report.references[0].href, "logo.png");
    }

    #[test]
    fn a_hyperlink_is_not_a_resource_reference() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
            <a xlink:href="https://example.org"><rect width="10" height="10"/></a>
            <image href="logo.png"/>
        </svg>"#;
        let report = scan(svg).unwrap();
        assert_eq!(report.references.len(), 1);
        assert_eq!(report.references[0].kind, "image");
    }

    #[test]
    fn notices_text_that_relies_on_the_default_family() {
        let named = scan(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><text font-family="Inter">a</text></svg>"#,
        )
        .unwrap();
        assert!(!named.text_without_explicit_family);

        let inherited = scan(
            r#"<svg xmlns="http://www.w3.org/2000/svg"><g font-family="Inter"><text>a</text></g></svg>"#,
        )
        .unwrap();
        assert!(!inherited.text_without_explicit_family);

        let bare = scan(r#"<svg xmlns="http://www.w3.org/2000/svg"><text>a</text></svg>"#).unwrap();
        assert!(bare.text_without_explicit_family);
    }

    #[test]
    fn rejects_a_non_svg_root() {
        let error = scan("<html></html>").unwrap_err();
        assert_eq!(error.code, ErrorCode::ParseFailed);
    }
}
