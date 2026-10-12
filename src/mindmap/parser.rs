//! Parser for Mermaid mindmaps. The hierarchy comes from indentation: a
//! node's parent is the closest node above it that is indented less.

use super::model::{Mindmap, Node, Shape};
use crate::syntax::{ParseError, split_source};

pub const KEYWORD: &str = "mindmap";

pub fn parse(src: &str) -> Result<Mindmap, ParseError> {
    let source = split_source(src)?;
    let (header_line, header) = source.header;
    if header != KEYWORD {
        return Err(ParseError::new(
            header_line,
            format!("Expected `{KEYWORD}`, found `{header}`"),
        ));
    }
    let mut mindmap = Mindmap {
        preamble: source.preamble,
        ..Mindmap::default()
    };
    // Indentation and key of the nodes the next line may belong to.
    let mut open: Vec<(usize, String)> = Vec::new();

    // `split_source` trims lines, and indentation matters here.
    for (index, raw) in src.lines().enumerate().skip(header_line) {
        let line_no = index + 1;
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let error = |m: &str| ParseError::new(line_no, m);
        let is_extra = line.starts_with("::icon(") || line.starts_with(":::");
        if line.starts_with("%%") || is_extra {
            match mindmap.nodes.last_mut() {
                Some(node) => node.extras.push(line.to_string()),
                None if is_extra => return Err(error("Icons and classes need a node above them")),
                None => mindmap.leading.push(line.to_string()),
            }
            continue;
        }
        let indent = indent_width(raw);
        while open.last().is_some_and(|(i, _)| *i >= indent) {
            open.pop();
        }
        let parent = open.last().map(|(_, key)| key.clone());
        if parent.is_none() && !mindmap.nodes.is_empty() {
            return Err(error("A mindmap has one root; indent this node below it"));
        }
        let (id, shape, text) = parse_node(line).map_err(|m| error(&m))?;
        let key = format!("n{}", mindmap.nodes.len());
        mindmap.nodes.push(Node {
            key: key.clone(),
            id,
            text,
            shape,
            parent,
            extras: Vec::new(),
        });
        open.push((indent, key));
    }
    Ok(mindmap)
}

/// Columns of leading whitespace, with tabs advancing to the next multiple
/// of four so tab and space indented lines compare sensibly.
fn indent_width(line: &str) -> usize {
    line.chars()
        .take_while(|c| c.is_whitespace())
        .fold(0, |col, c| {
            if c == '\t' {
                (col / 4 + 1) * 4
            } else {
                col + 1
            }
        })
}

/// Splits a node line into id, shape and text. Quoted text, and text
/// without an id-like prefix and shape delimiters, is a plain node.
fn parse_node(line: &str) -> Result<(String, Shape, String), String> {
    let start = match line.find(['[', '(', ')', '{']) {
        Some(start) if !line.starts_with('"') => start,
        _ => return Ok((String::new(), Shape::Default, line.to_string())),
    };
    let id = &line[..start];
    if id.contains(char::is_whitespace) {
        return Ok((String::new(), Shape::Default, line.to_string()));
    }
    let rest = &line[start..];
    // Longer delimiters first: `((` before `(`, `))` before `)`.
    let shapes = [
        Shape::Circle,
        Shape::Bang,
        Shape::Hexagon,
        Shape::Square,
        Shape::Rounded,
        Shape::Cloud,
    ];
    let shape = shapes
        .into_iter()
        .find(|s| rest.starts_with(s.delimiters().0))
        .ok_or_else(|| format!("Unexpected `{rest}`"))?;
    let (open, close) = shape.delimiters();
    let text = rest[open.len()..].strip_suffix(close).ok_or_else(|| {
        format!("Node text is missing its closing `{close}` (text spanning several lines is not supported yet)")
    })?;
    Ok((id.to_string(), shape, text.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(m: &Mindmap) -> Vec<(String, Shape, String, Option<String>)> {
        m.nodes
            .iter()
            .map(|n| (n.id.clone(), n.shape, n.text.clone(), n.parent.clone()))
            .collect()
    }

    #[test]
    fn builds_the_tree_from_indentation() {
        let m = parse(
            "mindmap
  root((mindmap))
    Origins
      Long history
      ::icon(fa fa-book)
      Popularisation
    Research
        Deep
      Uneven",
        )
        .unwrap();
        let parents: Vec<_> = m.nodes.iter().map(|n| n.parent.as_deref()).collect();
        assert_eq!(
            parents,
            [
                None,
                Some("n0"),
                Some("n1"),
                Some("n1"),
                Some("n0"),
                Some("n4"),
                Some("n4")
            ]
        );
        assert_eq!(m.nodes[2].extras, ["::icon(fa fa-book)"]);
        assert_eq!(m.nodes[0].shape, Shape::Circle);
        assert_eq!(m.nodes[0].display(), "mindmap");
    }

    #[test]
    fn parses_every_shape() {
        let m = parse(
            "mindmap
    Root
        a[I am a square]
        b(I am rounded)
        c((I am a circle))
        d))I am a bang((
        e)I am a cloud(
        f{{I am a hexagon}}
        Plain text (with parentheses)
        g[\"`**Bold** text`\"]",
        )
        .unwrap();
        let s = summary(&m);
        assert_eq!(s[1].1, Shape::Square);
        assert_eq!(s[2].1, Shape::Rounded);
        assert_eq!(s[3].1, Shape::Circle);
        assert_eq!(s[4].1, Shape::Bang);
        assert_eq!(s[5].1, Shape::Cloud);
        assert_eq!(
            (s[6].0.as_str(), s[6].1, s[6].2.as_str()),
            ("f", Shape::Hexagon, "I am a hexagon")
        );
        assert_eq!(s[7].1, Shape::Default);
        assert_eq!(s[7].2, "Plain text (with parentheses)");
        assert_eq!(m.nodes[8].display(), "**Bold** text");
    }

    #[test]
    fn tabs_count_as_four_columns() {
        let m = parse("mindmap\n  Root\n\tChild\n      Grandchild").unwrap();
        let parents: Vec<_> = m.nodes.iter().map(|n| n.parent.as_deref()).collect();
        assert_eq!(parents, [None, Some("n0"), Some("n1")]);
    }

    #[test]
    fn quoted_plain_text_may_hold_delimiters() {
        let m = parse("mindmap\n  Root\n    \"f(x) {y}\"").unwrap();
        assert_eq!(m.nodes[1].shape, Shape::Default);
        assert_eq!(m.nodes[1].display(), "f(x) {y}");
    }

    #[test]
    fn keeps_comments_and_classes() {
        let m = parse("mindmap\n%% before\n  Root\n  :::urgent large\n  %% after").unwrap();
        assert_eq!(m.leading, ["%% before"]);
        assert_eq!(m.nodes[0].extras, [":::urgent large", "%% after"]);
    }

    #[test]
    fn reports_errors() {
        assert_eq!(parse("mindmap\n  A\n  B").unwrap_err().line, 3);
        assert!(parse("mindmap\n  id1[\"`**Root** with").is_err());
        assert!(parse("mindmap\n  ::icon(x)").is_err());
        assert!(parse("mindmaps").is_err());
        assert!(parse("mindmap\n").unwrap().nodes.is_empty());
    }
}
