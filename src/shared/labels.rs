//! Translatable display names for diagram enums.

use gettextrs::gettext;

use crate::diagrams::DiagramKind;
use crate::diagrams::c4::{BoundaryKind, ElementKind, ElementShape, Level};
use crate::diagrams::class::Head as ClassHead;
use crate::diagrams::erd::Cardinality;
use crate::diagrams::flowchart::{Direction, Head, NodeShape, Stroke};
use crate::diagrams::mindmap::Shape as MindmapShape;
use crate::diagrams::sequence::{ArrowHead, BlockKind, LineStyle, NotePlacement, ParticipantKind};
use crate::diagrams::state::StateKind;
use crate::diagrams::usecase::{
    ElementKind as UseCaseElementKind, RelationKind as UseCaseRelationKind,
};

pub fn diagram_kind(kind: DiagramKind) -> String {
    gettext(match kind {
        DiagramKind::Flowchart => "Flowchart",
        DiagramKind::Sequence => "Sequence Diagram",
        DiagramKind::State => "State Diagram",
        DiagramKind::UseCase => "Use Case Diagram",
        DiagramKind::Mindmap => "Mindmap",
        DiagramKind::Class => "Class Diagram",
        DiagramKind::Erd => "Entity Relationship Diagram",
        DiagramKind::C4 => "C4 Diagram",
    })
}

pub fn diagram_kind_description(kind: DiagramKind) -> String {
    gettext(match kind {
        DiagramKind::Flowchart => "Processes and decisions as shapes joined by arrows",
        DiagramKind::Sequence => "Messages exchanged between participants over time",
        DiagramKind::State => "States of a system and the transitions between them",
        DiagramKind::UseCase => "Actors and the use cases they take part in",
        DiagramKind::Mindmap => "Ideas branching out from a central topic",
        DiagramKind::Class => "Classes, their members and how they relate",
        DiagramKind::Erd => "Database entities, attributes and relationships",
        DiagramKind::C4 => "Software architecture as systems, containers and components",
    })
}

pub fn state_kind(kind: StateKind) -> String {
    gettext(match kind {
        StateKind::State => "State",
        StateKind::Start => "Start",
        StateKind::End => "End",
        StateKind::Choice => "Choice",
        StateKind::Fork => "Fork",
        StateKind::Join => "Join",
    })
}

pub fn usecase_element_kind(kind: UseCaseElementKind) -> String {
    gettext(match kind {
        UseCaseElementKind::Actor => "Actor",
        UseCaseElementKind::UseCase => "Use Case",
        UseCaseElementKind::UseCaseRect => "Use Case (Rectangle)",
    })
}

pub fn usecase_relation_kind(kind: UseCaseRelationKind) -> String {
    gettext(match kind {
        UseCaseRelationKind::Arrow => "Association (Arrow)",
        UseCaseRelationKind::Line => "Association",
        UseCaseRelationKind::ArrowBack => "Association (Reverse Arrow)",
        UseCaseRelationKind::CircleTo => "Association (Circle)",
        UseCaseRelationKind::CircleFrom => "Association (Reverse Circle)",
        UseCaseRelationKind::CrossTo => "Association (Cross)",
        UseCaseRelationKind::CrossFrom => "Association (Reverse Cross)",
        UseCaseRelationKind::Include => "Include",
        UseCaseRelationKind::Extend => "Extend",
        UseCaseRelationKind::Generalization => "Generalization",
    })
}

/// Name of a mindmap node shape; `None` is plain text.
pub fn mindmap_shape(shape: Option<MindmapShape>) -> String {
    gettext(match shape {
        None => "Text",
        Some(MindmapShape::Square) => "Square",
        Some(MindmapShape::Rounded) => "Rounded",
        Some(MindmapShape::Circle) => "Circle",
        Some(MindmapShape::Bang) => "Bang",
        Some(MindmapShape::Cloud) => "Cloud",
        Some(MindmapShape::Hexagon) => "Hexagon",
    })
}

pub fn c4_level(level: Level) -> String {
    gettext(match level {
        Level::Context => "System Context",
        Level::Container => "Container",
        Level::Component => "Component",
    })
}

pub fn c4_element_kind(kind: ElementKind) -> String {
    gettext(match kind {
        ElementKind::Person => "Person",
        ElementKind::System => "Software System",
        ElementKind::Container => "Container",
        ElementKind::Component => "Component",
    })
}

pub fn c4_element_shape(shape: ElementShape) -> String {
    gettext(match shape {
        ElementShape::Box => "Box",
        ElementShape::Database => "Database",
        ElementShape::Queue => "Queue",
    })
}

pub fn c4_boundary_kind(kind: BoundaryKind) -> String {
    gettext(match kind {
        BoundaryKind::Generic => "Boundary",
        BoundaryKind::Enterprise => "Enterprise",
        BoundaryKind::System => "System",
        BoundaryKind::Container => "Container",
    })
}

pub fn shape(shape: NodeShape) -> String {
    gettext(match shape {
        NodeShape::Rectangle => "Rectangle",
        NodeShape::Rounded => "Rounded",
        NodeShape::Stadium => "Stadium",
        NodeShape::Subroutine => "Subroutine",
        NodeShape::Cylinder => "Database",
        NodeShape::Circle => "Circle",
        NodeShape::DoubleCircle => "Double Circle",
        NodeShape::Rhombus => "Decision",
        NodeShape::Hexagon => "Hexagon",
        NodeShape::Parallelogram => "Input / Output",
        NodeShape::ParallelogramAlt => "Output / Input",
        NodeShape::Trapezoid => "Trapezoid",
        NodeShape::TrapezoidAlt => "Inverted Trapezoid",
        NodeShape::Asymmetric => "Flag",
    })
}

pub fn direction(direction: Direction) -> String {
    gettext(match direction {
        Direction::TopBottom => "Top to Bottom",
        Direction::BottomTop => "Bottom to Top",
        Direction::LeftRight => "Left to Right",
        Direction::RightLeft => "Right to Left",
    })
}

pub fn stroke(stroke: Stroke) -> String {
    gettext(match stroke {
        Stroke::Normal => "Solid",
        Stroke::Dotted => "Dotted",
        Stroke::Thick => "Thick",
    })
}

pub fn head(head: Head) -> String {
    gettext(match head {
        Head::None => "None",
        Head::Arrow => "Arrow",
        Head::Circle => "Circle",
        Head::Cross => "Cross",
    })
}

pub fn participant_kind(kind: ParticipantKind) -> String {
    gettext(match kind {
        ParticipantKind::Participant => "Participant",
        ParticipantKind::Actor => "Actor",
    })
}

pub fn line_style(line: LineStyle) -> String {
    gettext(match line {
        LineStyle::Solid => "Solid",
        LineStyle::Dotted => "Dotted",
    })
}

pub fn arrow_head(head: ArrowHead) -> String {
    gettext(match head {
        ArrowHead::None => "None",
        ArrowHead::Filled => "Arrow",
        ArrowHead::Cross => "Cross",
        ArrowHead::Open => "Open Arrow (async)",
    })
}

pub fn note_placement(placement: NotePlacement) -> String {
    gettext(match placement {
        NotePlacement::LeftOf => "Left of",
        NotePlacement::RightOf => "Right of",
        NotePlacement::Over => "Over",
    })
}

pub fn block_kind(kind: BlockKind) -> String {
    gettext(match kind {
        BlockKind::Loop => "Loop",
        BlockKind::Alt => "Alternative",
        BlockKind::Opt => "Optional",
        BlockKind::Par => "Parallel",
        BlockKind::Critical => "Critical Region",
        BlockKind::Break => "Break",
        BlockKind::Rect => "Highlight",
    })
}

pub fn cardinality(cardinality: Cardinality) -> String {
    gettext(match cardinality {
        Cardinality::ZeroOrOne => "Zero or one",
        Cardinality::ExactlyOne => "Exactly one",
        Cardinality::ZeroOrMore => "Zero or more",
        Cardinality::OneOrMore => "One or more",
    })
}

pub fn class_head(head: ClassHead) -> String {
    gettext(match head {
        ClassHead::None => "None",
        ClassHead::Inheritance => "Inheritance",
        ClassHead::Composition => "Composition",
        ClassHead::Aggregation => "Aggregation",
        ClassHead::Arrow => "Arrow",
        ClassHead::Lollipop => "Interface (lollipop)",
    })
}
