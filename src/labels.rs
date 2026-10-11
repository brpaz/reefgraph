//! Translatable display names for diagram enums.

use gettextrs::gettext;

use crate::flowchart::{Direction, Head, NodeShape, Stroke};

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
