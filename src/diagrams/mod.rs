//! Diagram types the editor supports, detected from the source declaration.

use crate::canvas::box_canvas::BoxDiagram;
use crate::shared::syntax::{ParseError, split_source};
pub mod c4;
pub mod class;
pub mod erd;
pub mod flowchart;
pub mod mindmap;
pub mod sequence;
pub mod state;
pub mod usecase;

#[derive(Debug, Clone, PartialEq)]
pub enum Diagram {
    Flowchart(flowchart::Flowchart),
    Sequence(sequence::Sequence),
    State(state::StateDiagram),
    UseCase(usecase::UseCaseDiagram),
    Erd(erd::Erd),
    Class(class::ClassDiagram),
    C4(c4::C4Diagram),
    Mindmap(mindmap::Mindmap),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramKind {
    Flowchart,
    Sequence,
    State,
    UseCase,
    Erd,
    Class,
    C4,
    Mindmap,
}

impl DiagramKind {
    /// Every kind, in the order offered for new documents.
    pub const ALL: [DiagramKind; 8] = [
        DiagramKind::Flowchart,
        DiagramKind::Sequence,
        DiagramKind::State,
        DiagramKind::UseCase,
        DiagramKind::Class,
        DiagramKind::Erd,
        DiagramKind::C4,
        DiagramKind::Mindmap,
    ];

    /// Source of a new, empty diagram.
    pub fn template(self) -> &'static str {
        match self {
            DiagramKind::Flowchart => "flowchart TD\n",
            DiagramKind::Sequence => "sequenceDiagram\n",
            DiagramKind::State => "stateDiagram-v2\n",
            DiagramKind::UseCase => "usecase-beta\n",
            DiagramKind::Mindmap => "mindmap\n",
            DiagramKind::Erd => "erDiagram\n",
            DiagramKind::Class => "classDiagram\n",
            DiagramKind::C4 => "C4Context\n",
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
            DiagramKind::State => {
                "stateDiagram-v2
    [*] --> Idle
    Idle --> Running : start
    Running --> Idle : stop
    Running --> [*] : done
"
            }
            DiagramKind::UseCase => {
                "usecase-beta
    direction LR
    actor Customer
    systemBoundary Shop[\"Online shop\"]
        Browse(\"Browse catalogue\")
        Checkout(\"Checkout\")
    end
    Customer --> Browse
    Customer --> Checkout
    Checkout ..> : include Browse
"
            }
            DiagramKind::Mindmap => {
                "mindmap
  root((Project))
    Goals
      Ship v1
    Team
      Design
      Code
    Risks
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
            DiagramKind::C4 => {
                "C4Context
    Person(customer, \"Customer\")
    System_Boundary(bank, \"Bank\") {
        System(banking, \"Online Banking\")
    }
    Rel(customer, banking, \"Uses\")
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
        state::parser::KEYWORD | state::parser::KEYWORD_V2 => state::parse(src).map(Diagram::State),
        usecase::parser::KEYWORD => usecase::parse(src).map(Diagram::UseCase),
        mindmap::parser::KEYWORD => mindmap::parse(src).map(Diagram::Mindmap),
        erd::parser::KEYWORD => erd::parse(src).map(Diagram::Erd),
        class::parser::KEYWORD | class::parser::KEYWORD_V2 => class::parse(src).map(Diagram::Class),
        keyword if c4::parser::is_keyword(keyword) => c4::parse(src).map(Diagram::C4),
        other => Err(ParseError::new(
            source.header.0,
            format!(
                "`{other}` diagrams are not supported; use `flowchart`, `sequenceDiagram`, `stateDiagram-v2`, `usecase-beta`, `classDiagram`, `erDiagram`, `C4Context` or `mindmap`"
            ),
        )),
    }
}

/// A diagram grouped by the kind of canvas that edits it.
pub enum ForCanvas {
    Flowchart(flowchart::Flowchart),
    Sequence(sequence::Sequence),
    Boxes(Box<dyn BoxDiagram>),
}

impl Diagram {
    pub fn for_canvas(self) -> ForCanvas {
        match self {
            Diagram::Flowchart(d) => ForCanvas::Flowchart(d),
            Diagram::Sequence(d) => ForCanvas::Sequence(d),
            Diagram::State(d) => ForCanvas::Boxes(Box::new(d)),
            Diagram::UseCase(d) => ForCanvas::Boxes(Box::new(d)),
            Diagram::Mindmap(d) => ForCanvas::Boxes(Box::new(d)),
            Diagram::Erd(d) => ForCanvas::Boxes(Box::new(d)),
            Diagram::Class(d) => ForCanvas::Boxes(Box::new(d)),
            Diagram::C4(d) => ForCanvas::Boxes(Box::new(d)),
        }
    }

    pub fn kind(&self) -> DiagramKind {
        match self {
            Diagram::Flowchart(_) => DiagramKind::Flowchart,
            Diagram::Sequence(_) => DiagramKind::Sequence,
            Diagram::State(_) => DiagramKind::State,
            Diagram::UseCase(_) => DiagramKind::UseCase,
            Diagram::Mindmap(_) => DiagramKind::Mindmap,
            Diagram::Erd(_) => DiagramKind::Erd,
            Diagram::Class(_) => DiagramKind::Class,
            Diagram::C4(_) => DiagramKind::C4,
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
