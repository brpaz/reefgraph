//! Diagram types the editor supports, detected from the source declaration.

use crate::syntax::{ParseError, split_source};
use crate::{class_diagram, erd, flowchart, sequence};

#[derive(Debug, Clone, PartialEq)]
pub enum Diagram {
    Flowchart(flowchart::Flowchart),
    Sequence(sequence::Sequence),
    Erd(erd::Erd),
    Class(class_diagram::ClassDiagram),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramKind {
    Flowchart,
    Sequence,
    Erd,
    Class,
}

impl DiagramKind {
    /// Every kind, in the order offered for new documents.
    pub const ALL: [DiagramKind; 4] = [
        DiagramKind::Flowchart,
        DiagramKind::Sequence,
        DiagramKind::Class,
        DiagramKind::Erd,
    ];

    /// Source of a new, empty diagram.
    pub fn template(self) -> &'static str {
        match self {
            DiagramKind::Flowchart => "flowchart TD\n",
            DiagramKind::Sequence => "sequenceDiagram\n",
            DiagramKind::Erd => "erDiagram\n",
            DiagramKind::Class => "classDiagram\n",
        }
    }

    /// Small example diagram, shown on first start and as a preview.
    pub fn sample(self) -> &'static str {
        match self {
            DiagramKind::Flowchart => {
                "flowchart TD
    A[Start] --> B{Is it working?}
    B -->|Yes| C([Ship it])
    B -->|No| D[Debug]
    D --> B
"
            }
            DiagramKind::Sequence => {
                "sequenceDiagram
    actor User
    participant App
    User->>App: Save
    App-->>User: Saved
"
            }
            DiagramKind::Erd => {
                "erDiagram
    CUSTOMER ||--o{ ORDER : places
    CUSTOMER {
        string name
    }
    ORDER {
        int id PK
    }
"
            }
            DiagramKind::Class => {
                "classDiagram
    class Animal {
        +String name
        +eat()
    }
    Animal <|-- Duck
"
            }
        }
    }
}

pub fn parse(src: &str) -> Result<Diagram, ParseError> {
    let source = split_source(src)?;
    match source.keyword() {
        "flowchart" | "graph" => flowchart::parse(src).map(Diagram::Flowchart),
        sequence::parser::KEYWORD => sequence::parse(src).map(Diagram::Sequence),
        erd::parser::KEYWORD => erd::parse(src).map(Diagram::Erd),
        class_diagram::parser::KEYWORD | class_diagram::parser::KEYWORD_V2 => {
            class_diagram::parse(src).map(Diagram::Class)
        }
        other => Err(ParseError::new(
            source.header.0,
            format!(
                "`{other}` diagrams are not supported; use `flowchart`, `sequenceDiagram`, `classDiagram` or `erDiagram`"
            ),
        )),
    }
}

impl Diagram {
    pub fn kind(&self) -> DiagramKind {
        match self {
            Diagram::Flowchart(_) => DiagramKind::Flowchart,
            Diagram::Sequence(_) => DiagramKind::Sequence,
            Diagram::Erd(_) => DiagramKind::Erd,
            Diagram::Class(_) => DiagramKind::Class,
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
    fn templates_and_samples_parse() {
        for kind in DiagramKind::ALL {
            assert_eq!(parse(kind.template()).unwrap().kind(), kind);
            assert_eq!(parse(kind.sample()).unwrap().kind(), kind);
        }
    }
}
