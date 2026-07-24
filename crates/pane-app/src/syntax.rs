//! Syntax highlighting via Tree-sitter, for a fixed set of languages. Parses
//! the whole file (only used for files under a size threshold — logs stay
//! plain) and returns colored byte ranges the renderer turns into spans.

use std::ops::Range;

use glyphon::Color;
use tree_sitter::Language;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

// Token palette (One-Dark-ish, tuned for the dark surface). Kept here since it
// is syntax-specific; the base UI theme lives in main.rs.
const KEYWORD: Color = Color::rgb(0xc0, 0x8a, 0xd8); // purple
const STRING: Color = Color::rgb(0x98, 0xc3, 0x79); // green
const COMMENT: Color = Color::rgb(0x6a, 0x70, 0x7c); // muted gray
const NUMBER: Color = Color::rgb(0xd1, 0x9a, 0x66); // orange (numbers/constants)
const FUNCTION: Color = Color::rgb(0x61, 0xaf, 0xef); // blue
const TYPE: Color = Color::rgb(0xe5, 0xc0, 0x7b); // yellow
const PROPERTY: Color = Color::rgb(0x56, 0xb6, 0xc2); // cyan (JSON keys, fields)
const PUNCT: Color = Color::rgb(0x8f, 0x95, 0xa0); // dim
const VARIABLE: Color = Color::rgb(0xe2, 0xe4, 0xe9); // default fg

/// Capture names we recognize. `configure` maps each query capture to the
/// longest matching prefix here, so listing base names covers their `.sub`
/// variants (e.g. `keyword.control` → `keyword`).
const HIGHLIGHT_NAMES: &[&str] = &[
    "keyword",
    "operator",
    "string",
    "comment",
    "number",
    "boolean",
    "constant",
    "function",
    "type",
    "constructor",
    "property",
    "variable",
    "punctuation",
    "tag",
    "attribute",
    "label",
    "escape",
];

fn color_for(name: &str) -> Color {
    match name.split('.').next().unwrap_or(name) {
        "keyword" | "tag" => KEYWORD,
        "string" | "escape" => STRING,
        "comment" => COMMENT,
        "number" | "boolean" | "constant" => NUMBER,
        "function" => FUNCTION,
        "type" | "constructor" | "attribute" | "label" => TYPE,
        "property" => PROPERTY,
        "punctuation" | "operator" => PUNCT,
        _ => VARIABLE,
    }
}

#[derive(Clone, Copy)]
pub enum Lang {
    Json,
    Rust,
    Toml,
    Markdown,
    Java,
}

impl Lang {
    /// Detects a supported language from the file extension.
    pub fn from_path(path: &str) -> Option<Lang> {
        let ext = path.rsplit('.').next()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "json" => Lang::Json,
            "rs" => Lang::Rust,
            "toml" => Lang::Toml,
            "md" | "markdown" => Lang::Markdown,
            "java" => Lang::Java,
            _ => return None,
        })
    }

    fn config(self) -> Option<HighlightConfiguration> {
        let (language, query) = match self {
            Lang::Json => (
                Language::new(tree_sitter_json::LANGUAGE),
                tree_sitter_json::HIGHLIGHTS_QUERY,
            ),
            Lang::Rust => (
                Language::new(tree_sitter_rust::LANGUAGE),
                tree_sitter_rust::HIGHLIGHTS_QUERY,
            ),
            Lang::Toml => (
                Language::new(tree_sitter_toml_ng::LANGUAGE),
                tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
            ),
            Lang::Markdown => (
                Language::new(tree_sitter_md::LANGUAGE),
                tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
            ),
            Lang::Java => (
                Language::new(tree_sitter_java::LANGUAGE),
                tree_sitter_java::HIGHLIGHTS_QUERY,
            ),
        };
        // Empty injection/locals queries: we don't nest languages in v1.
        let mut cfg = HighlightConfiguration::new(language, "src", query, "", "").ok()?;
        cfg.configure(HIGHLIGHT_NAMES);
        Some(cfg)
    }
}

/// Highlights `text`, returning sorted, non-overlapping colored byte ranges.
/// Ranges not covered here are drawn in the default foreground by the caller.
pub fn highlight(text: &str, lang: Lang) -> Vec<(Range<usize>, Color)> {
    let mut out = Vec::new();
    let Some(config) = lang.config() else {
        return out;
    };
    let mut hl = Highlighter::new();
    let events = match hl.highlight(&config, text.as_bytes(), None, |_| None) {
        Ok(e) => e,
        Err(_) => return out,
    };
    let mut stack: Vec<usize> = Vec::new();
    for event in events {
        match event {
            Ok(HighlightEvent::HighlightStart(h)) => stack.push(h.0),
            Ok(HighlightEvent::HighlightEnd) => {
                stack.pop();
            }
            Ok(HighlightEvent::Source { start, end }) => {
                if let Some(&idx) = stack.last() {
                    let color = color_for(HIGHLIGHT_NAMES[idx]);
                    // Merge with the previous range if contiguous and same color.
                    if let Some(last) = out.last_mut() {
                        if last.0.end == start && last.1 == color {
                            last.0.end = end;
                            continue;
                        }
                    }
                    out.push((start..end, color));
                }
            }
            Err(_) => break,
        }
    }
    out
}
