//! Translatable display names for diagram enums.

use gettextrs::gettext;

use crate::erd::Cardinality;
use crate::flowchart::{Direction, Head, NodeShape, Stroke};
use crate::sequence::{ArrowHead, BlockKind, LineStyle, NotePlacement, ParticipantKind};

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
