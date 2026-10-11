//! Diagram types the editor supports, detected from the source declaration.

use crate::syntax::{ParseError, split_source};
use crate::{flowchart, sequence};

#[derive(Debug, Clone, PartialEq)]
pub enum Diagram {
    Flowchart(flowchart::Flowchart),
    Sequence(sequence::Sequence),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramKind {
    Flowchart,
    Sequence,
}

impl DiagramKind {
    /// Source of a new, empty diagram.
    pub fn template(self) -> &'static str {
        match self {
            DiagramKind::Flowchart => "flowchart TD\n",
            DiagramKind::Sequence => "sequenceDiagram\n",
        }
    }
}

pub fn parse(src: &str) -> Result<Diagram, ParseError> {
    let source = split_source(src)?;
    match source.keyword() {
        "flowchart" | "graph" => flowchart::parse(src).map(Diagram::Flowchart),
        sequence::parser::KEYWORD => sequence::parse(src).map(Diagram::Sequence),
        other => Err(ParseError::new(
            source.header.0,
            format!("`{other}` diagrams are not supported; use `flowchart` or `sequenceDiagram`"),
        )),
    }
}

impl Diagram {
    pub fn kind(&self) -> DiagramKind {
        match self {
            Diagram::Flowchart(_) => DiagramKind::Flowchart,
            Diagram::Sequence(_) => DiagramKind::Sequence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatches_on_declaration() {
        assert_eq!(
            parse("graph LR\nA-->B").unwrap().kind(),
            DiagramKind::Flowchart
        );
        assert_eq!(
            parse("%% c\nsequenceDiagram\nA->>B: x").unwrap().kind(),
            DiagramKind::Sequence
        );
        let err = parse("\n\npie\n").unwrap_err();
        assert_eq!(err.line, 3);
        assert!(err.message.contains("pie"));
    }

    #[test]
    fn templates_parse() {
        for kind in [DiagramKind::Flowchart, DiagramKind::Sequence] {
            assert_eq!(parse(kind.template()).unwrap().kind(), kind);
        }
    }
}
