//! Parsing pieces shared by all Mermaid diagram types.

use std::fmt;

use crate::geometry::Point;

/// Marker of the comment lines that store manually placed node positions.
const POSITION_META: &str = "@merman pos";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based line number.
    pub line: usize,
    pub message: String,
}

impl ParseError {
    pub fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Diagram source split into its parts.
#[derive(Debug, PartialEq)]
pub struct Source<'a> {
    /// Front matter and comments before the declaration, kept verbatim.
    pub preamble: Vec<String>,
    /// Line number and trimmed text of the diagram declaration line.
    pub header: (usize, &'a str),
    /// Non-blank lines after the declaration, trimmed, with line numbers.
    pub body: Vec<(usize, &'a str)>,
}

impl Source<'_> {
    /// First word of the declaration, e.g. `flowchart` or `sequenceDiagram`.
    pub fn keyword(&self) -> &str {
        self.header
            .1
            .split(|c: char| c.is_whitespace() || c == ';')
            .next()
            .unwrap_or_default()
    }
}

pub fn split_source(src: &str) -> Result<Source<'_>, ParseError> {
    let mut preamble = Vec::new();
    let mut lines = src.lines().enumerate().map(|(i, l)| (i + 1, l));
    let mut header = None;
    let mut in_front_matter = false;

    for (line_no, raw) in lines.by_ref() {
        let line = raw.trim();
        if in_front_matter || (line_no == 1 && line == "---") {
            preamble.push(raw.to_string());
            in_front_matter = line_no == 1 || line != "---";
            continue;
        }
        if line.is_empty() {
            continue;
        }
        if line.starts_with("%%") {
            preamble.push(line.to_string());
            continue;
        }
        header = Some((line_no, line));
        break;
    }

    if in_front_matter {
        return Err(ParseError::new(
            1,
            "Front matter is missing its closing `---`",
        ));
    }
    let header = header.ok_or_else(|| ParseError::new(1, "Expected a diagram declaration"))?;
    let body = lines
        .map(|(n, l)| (n, l.trim()))
        .filter(|(_, l)| !l.is_empty())
        .collect();
    Ok(Source {
        preamble,
        header,
        body,
    })
}

/// Splits a line on `;` separators that are outside quoted strings.
pub fn split_statements(line: &str) -> Vec<&str> {
    let mut statements = Vec::new();
    let mut in_quote = false;
    let mut start = 0;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_quote = !in_quote,
            ';' if !in_quote => {
                statements.push(line[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    statements.push(line[start..].trim());
    statements.retain(|s| !s.is_empty());
    statements
}

/// Reads a `%% @merman pos <id> <x> <y>` comment, given the text after
/// `%%`. Anything else, including comments that only look similar, is `None`.
pub fn position_comment(comment: &str) -> Option<(String, Point)> {
    let meta = comment.trim_start().strip_prefix(POSITION_META)?;
    if !meta.starts_with(char::is_whitespace) {
        return None;
    }
    let mut parts = meta.split_whitespace();
    let id = parts.next()?.to_string();
    // `NaN` and `inf` parse as numbers but cannot be drawn.
    let x = parts.next()?.parse().ok().filter(|v: &f64| v.is_finite())?;
    let y = parts.next()?.parse().ok().filter(|v: &f64| v.is_finite())?;
    parts.next().is_none().then(|| (id, Point::new(x, y)))
}

/// Source without the comments this editor adds for itself, for use outside
/// of it. Other comments and the line endings are kept as they are.
pub fn strip_editor_comments(src: &str) -> String {
    src.split_inclusive('\n')
        .filter(|line| {
            line.trim()
                .strip_prefix("%%")
                .is_none_or(|comment| position_comment(comment).is_none())
        })
        .collect()
}

/// Entity codes Mermaid uses for characters the syntax reserves.
const ENTITIES: [(&str, &str); 7] = [
    ("#quot;", "\""),
    ("#39;", "'"),
    ("#40;", "("),
    ("#41;", ")"),
    ("#91;", "["),
    ("#93;", "]"),
    ("#96;", "`"),
];

/// Text shown for a label written as plain text, a `"string"` or a
/// "`markdown string`", with entity codes decoded.
pub fn label_text(raw: &str) -> String {
    let raw = raw.trim();
    let text = raw
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .map(|inner| {
            inner
                .strip_prefix('`')
                .and_then(|r| r.strip_suffix('`'))
                .unwrap_or(inner)
        })
        .unwrap_or(raw);
    ENTITIES
        .iter()
        .fold(text.to_string(), |t, (code, c)| t.replace(code, c))
}

/// Quoted label for text typed by the user, so any character is allowed.
pub fn quote_label(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "#quot;"))
}

/// Comment line storing a node position, rounded to whole units.
pub fn position_line(id: &str, pos: Point) -> String {
    format!(
        "%% {POSITION_META} {id} {} {}",
        pos.x.round(),
        pos.y.round()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_front_matter_comments_and_body() {
        let src = "---\ntitle: x\n---\n%% hi\n\nsequenceDiagram\n  A->>B: yo\n\n  %% c\n";
        let source = split_source(src).unwrap();
        assert_eq!(source.preamble, ["---", "title: x", "---", "%% hi"]);
        assert_eq!(source.header, (6, "sequenceDiagram"));
        assert_eq!(source.keyword(), "sequenceDiagram");
        assert_eq!(source.body, [(7, "A->>B: yo"), (9, "%% c")]);
    }

    #[test]
    fn keyword_stops_at_semicolon() {
        let source = split_source("graph TD;A-->B").unwrap();
        assert_eq!(source.keyword(), "graph");
    }

    #[test]
    fn reports_missing_declaration_and_front_matter_end() {
        assert!(split_source("  \n%% only a comment").is_err());
        assert!(split_source("---\ntitle: x\n").is_err());
    }

    #[test]
    fn reads_and_writes_position_comments() {
        let line = position_line("A", Point::new(10.4, -2.6));
        assert_eq!(line, "%% @merman pos A 10 -3");
        let comment = line.strip_prefix("%%").unwrap();
        assert_eq!(
            position_comment(comment),
            Some(("A".into(), Point::new(10.0, -3.0)))
        );
        assert_eq!(position_comment(" @merman pos A x"), None);
        assert_eq!(position_comment(" @merman pos A 1 2 3"), None);
        assert_eq!(position_comment(" @merman pos A NaN 2"), None);
        assert_eq!(position_comment(" @merman pos A 1 inf"), None);
        assert_eq!(position_comment(" @merman posX A 1 2"), None);
        assert_eq!(position_comment(" plain comment"), None);
    }

    #[test]
    fn strips_only_editor_comments() {
        let src = "flowchart TD\n    A --> B\n    %% keep me\n%% @merman pos A 1 2\n  %% @merman pos B 3 4\n%% @merman possible rename\n%% @merman pos C x\n";
        assert_eq!(
            strip_editor_comments(src),
            "flowchart TD\n    A --> B\n    %% keep me\n%% @merman possible rename\n%% @merman pos C x\n"
        );
        assert_eq!(strip_editor_comments("graph TD"), "graph TD");
    }

    #[test]
    fn stripping_keeps_line_endings() {
        let src = "graph TD\r\n%% @merman pos A 1 2\r\nA --> B\r\n";
        assert_eq!(strip_editor_comments(src), "graph TD\r\nA --> B\r\n");
    }

    #[test]
    fn decodes_and_quotes_labels() {
        assert_eq!(label_text("Sign in"), "Sign in");
        assert_eq!(label_text("\"Sign in\""), "Sign in");
        assert_eq!(label_text("\"`**Bold**`\""), "**Bold**");
        assert_eq!(label_text("\"Refund #40;partial#41;\""), "Refund (partial)");
        let quoted = quote_label("Say \"hi\"");
        assert_eq!(quoted, "\"Say #quot;hi#quot;\"");
        assert_eq!(label_text(&quoted), "Say \"hi\"");
    }

    #[test]
    fn splits_statements_outside_quotes() {
        assert_eq!(split_statements("a; b[\"x;y\"] ;"), ["a", "b[\"x;y\"]"]);
    }
}
