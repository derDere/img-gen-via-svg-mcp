//! Normalisation and minification for `optimize_svg`.
//!
//! The two modes differ in what they are allowed to destroy, and the difference
//! is stated rather than hidden: normalisation resolves the document through the
//! renderer's own pipeline and produces something that renders identically but
//! is no longer the document its author wrote, while minification leaves the
//! structure alone.

use crate::error::{ErrorCode, Result, ToolError};
use usvg::{Indent, WriteOptions};

/// Rewrites a document through the renderer's simplification pipeline.
///
/// CSS, inheritance, `use` references, nested transforms and unit conversions
/// are all resolved, and everything the renderer ignores is dropped. The result
/// renders identically and is usually much smaller.
pub fn normalize(tree: &usvg::Tree, precision: u8, text_to_paths: bool) -> String {
    let options = WriteOptions {
        id_prefix: None,
        preserve_text: !text_to_paths,
        coordinates_precision: precision,
        transforms_precision: precision,
        use_single_quote: false,
        indent: Indent::None,
        attributes_indent: Indent::None,
    };
    tree.to_string(&options)
}

/// Shortens a document without changing its structure.
///
/// Comments and the whitespace between elements go; every element, attribute
/// and id survives, so the result stays as editable as the input.
pub fn minify(text: &str) -> Result<String> {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut index = 0;
    let mut inside_element = false;

    while index < bytes.len() {
        if bytes[index..].starts_with(b"<!--") {
            match find(bytes, index + 4, b"-->") {
                Some(end) => {
                    index = end + 3;
                    continue;
                }
                None => {
                    return Err(ToolError::new(
                        ErrorCode::ParseFailed,
                        "The document holds an unterminated comment.",
                    ));
                }
            }
        }

        let character = bytes[index] as char;
        match character {
            '<' => {
                inside_element = true;
                out.push('<');
            }
            '>' => {
                inside_element = false;
                out.push('>');
            }
            c if c.is_ascii_whitespace() => {
                // Whitespace between elements carries no meaning in an SVG and
                // is dropped; whitespace inside a tag is collapsed to one space.
                let ends_with_space = out.ends_with(' ');
                let after_open = out.ends_with('<');
                if inside_element && !ends_with_space && !after_open {
                    out.push(' ');
                } else if !inside_element && out.ends_with('>') {
                    // Between elements: drop it entirely.
                } else if !inside_element {
                    out.push(c);
                }
            }
            c => out.push(c),
        }
        index += 1;
    }

    Ok(out.trim().to_string())
}

/// Finds a byte pattern at or after `from`.
fn find(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    haystack[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minification_drops_comments_and_inter_element_whitespace() {
        let input = "<svg>\n  <!-- a note -->\n  <rect  x=\"1\"   y=\"2\"/>\n</svg>";
        let output = minify(input).unwrap();
        assert_eq!(output, r#"<svg><rect x="1" y="2"/></svg>"#);
    }

    #[test]
    fn minification_keeps_text_content() {
        let output = minify("<svg><text>hello world</text></svg>").unwrap();
        assert!(output.contains("hello world"));
    }

    #[test]
    fn an_unterminated_comment_is_refused() {
        assert!(minify("<svg><!-- oops </svg>").is_err());
    }
}
