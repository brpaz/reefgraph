//! Class box geometry and auto layout for class diagrams.

use std::collections::HashMap;

use super::model::{Class, ClassDiagram};
use crate::geometry::{Point, Rect};
use crate::layered::{self, Spacing};

pub const PADDING: f64 = 10.0;
pub const HEADER_PADDING: f64 = 12.0;
const ROW_PADDING: f64 = 6.0;
const MIN_WIDTH: f64 = 120.0;
/// Height of a compartment without members.
const EMPTY_COMPARTMENT: f64 = 10.0;
/// Relations carry markers, cardinalities and labels, so ranks sit further apart.
const SPACING: Spacing = Spacing {
    rank: 100.0,
    node: 50.0,
};

/// Text shown for an annotation line in the header.
pub fn annotation_text(annotation: &str) -> String {
    format!("«{annotation}»")
}

/// Box of a class: a header (annotations and name), then an attribute
/// compartment and a method compartment.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassBox {
    pub size: (f64, f64),
    pub header_height: f64,
    pub row_height: f64,
    pub attributes: usize,
    pub methods: usize,
}

impl ClassBox {
    pub fn rect(&self, center: Point) -> Rect {
        Rect::centered(center, self.size)
    }

    fn attributes_height(&self) -> f64 {
        compartment_height(self.attributes, self.row_height)
    }

    /// Y of the line between the attribute and method compartments, relative
    /// to the top of the box.
    pub fn divider(&self) -> f64 {
        self.header_height + self.attributes_height()
    }

    /// Rectangle of the member shown at `row` (attributes first, then methods).
    pub fn row_rect(&self, center: Point, row: usize) -> Rect {
        let rect = self.rect(center);
        let offset = if row < self.attributes {
            self.header_height + row as f64 * self.row_height
        } else {
            self.divider() + (row - self.attributes) as f64 * self.row_height
        };
        Rect {
            x: rect.x,
            y: rect.y + offset,
            w: rect.w,
            h: self.row_height,
        }
    }

    /// Displayed member row under `p`, for a box centered at `center`.
    pub fn row_at(&self, center: Point, p: Point) -> Option<usize> {
        if !self.rect(center).contains(p) {
            return None;
        }
        (0..self.attributes + self.methods).find(|&row| self.row_rect(center, row).contains(p))
    }
}

fn compartment_height(rows: usize, row_height: f64) -> f64 {
    if rows == 0 {
        EMPTY_COMPARTMENT
    } else {
        rows as f64 * row_height
    }
}

pub fn class_box(class: &Class, text_size: impl Fn(&str) -> (f64, f64)) -> ClassBox {
    let (title_w, title_h) = text_size(&class.title());
    let line_h = text_size("Ag").1;
    let annotation_sizes: Vec<_> = class
        .annotations
        .iter()
        .map(|a| text_size(&annotation_text(a)))
        .collect();
    let member_width = class
        .members
        .iter()
        .map(|m| text_size(&m.display().text).0)
        .fold(0.0, f64::max);
    let width = annotation_sizes
        .iter()
        .map(|s| s.0)
        .fold(title_w.max(member_width), f64::max)
        + 2.0 * PADDING;
    let header_height =
        title_h + annotation_sizes.iter().map(|s| s.1).sum::<f64>() + HEADER_PADDING;
    let row_height = line_h + ROW_PADDING;
    let methods = class.members.iter().filter(|m| m.is_method()).count();
    let attributes = class.members.len() - methods;
    ClassBox {
        size: (
            width.max(MIN_WIDTH),
            header_height
                + compartment_height(attributes, row_height)
                + compartment_height(methods, row_height),
        ),
        header_height,
        row_height,
        attributes,
        methods,
    }
}

/// Assigns a position to every class without one.
pub fn layout_missing(diagram: &mut ClassDiagram, size_of: impl Fn(&Class) -> (f64, f64)) {
    let index: HashMap<&str, usize> = diagram
        .classes
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();
    let edges: Vec<(usize, usize)> = diagram
        .relations
        .iter()
        .filter_map(|r| Some((*index.get(r.from.as_str())?, *index.get(r.to.as_str())?)))
        .collect();
    let sizes: Vec<_> = diagram.classes.iter().map(&size_of).collect();
    let mut positions: Vec<_> = diagram.classes.iter().map(|c| c.pos).collect();
    layered::fill_missing(
        &mut positions,
        &sizes,
        &edges,
        diagram.direction.unwrap_or_default(),
        SPACING,
    );
    for (class, pos) in diagram.classes.iter_mut().zip(positions) {
        class.pos = pos;
    }
}

/// Discards every stored position and lays the whole diagram out again.
pub fn layout_all(diagram: &mut ClassDiagram, size_of: impl Fn(&Class) -> (f64, f64)) {
    for class in &mut diagram.classes {
        class.pos = None;
    }
    layout_missing(diagram, size_of);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class_diagram::parser::parse;

    fn size(s: &str) -> (f64, f64) {
        (s.chars().count() as f64 * 8.0, 16.0)
    }

    #[test]
    fn compartments_split_attributes_and_methods() {
        let d = parse("classDiagram\n class A {\n  <<interface>>\n  +int x\n  run()\n  stop()\n }")
            .unwrap();
        let b = class_box(&d.classes[0], size);
        assert_eq!((b.attributes, b.methods), (1, 2));
        assert_eq!(b.header_height, 16.0 * 2.0 + HEADER_PADDING);
        assert_eq!(b.size.1, b.header_height + 3.0 * b.row_height);
        let c = Point::new(0.0, 0.0);
        assert_eq!(b.row_rect(c, 1).y, b.rect(c).y + b.divider());
        assert_eq!(b.row_at(c, b.row_rect(c, 2).center()), Some(2));
        assert_eq!(b.row_at(c, Point::new(0.0, b.rect(c).y + 1.0)), None);
    }

    #[test]
    fn empty_classes_keep_both_compartments() {
        let d = parse("classDiagram\n class A").unwrap();
        let b = class_box(&d.classes[0], size);
        assert_eq!(b.size.1, b.header_height + 2.0 * EMPTY_COMPARTMENT);
        assert_eq!(b.size.0, MIN_WIDTH);
    }

    #[test]
    fn lays_out_inheritance_in_ranks() {
        let mut d = parse("classDiagram\n Animal <|-- Duck").unwrap();
        layout_missing(&mut d, |c| class_box(c, size).size);
        assert!(d.classes[1].pos.unwrap().y > d.classes[0].pos.unwrap().y);
    }
}
