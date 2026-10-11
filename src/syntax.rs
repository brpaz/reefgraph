//! Parsing pieces shared by all Mermaid diagram types.

use std::fmt;

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
    fn splits_statements_outside_quotes() {
        assert_eq!(split_statements("a; b[\"x;y\"] ;"), ["a", "b[\"x;y\"]"]);
    }
}
