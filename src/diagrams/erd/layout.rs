//! Entity box geometry and auto layout for entity relationship diagrams.

use std::collections::HashMap;

use super::model::{Entity, Erd};
use crate::shared::geometry::{Point, Rect};
use crate::shared::layered::{self, Spacing};

/// Relationships carry cardinality markers and labels, so ranks sit further apart.
const SPACING: Spacing = Spacing {
    rank: 120.0,
    node: 60.0,
};

const PADDING: f64 = 10.0;
const COLUMN_GAP: f64 = 14.0;
const HEADER_PADDING: f64 = 14.0;
const ROW_PADDING: f64 = 8.0;
const MIN_WIDTH: f64 = 110.0;
const EMPTY_BODY: f64 = 12.0;

/// Table layout of an entity: a header with its name and one row per
/// attribute, split into type, name, keys and comment columns.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityBox {
    pub size: (f64, f64),
    pub header_height: f64,
    pub row_height: f64,
    /// Left edge of each column, relative to the box.
    pub columns: [f64; 4],
}

impl EntityBox {
    pub fn rect(&self, center: Point) -> Rect {
        Rect::centered(center, self.size)
    }

    /// Attribute row under `p`, for a box centered at `center`.
    pub fn row_at(&self, center: Point, p: Point) -> Option<usize> {
        let rect = self.rect(center);
        let y = p.y - rect.y - self.header_height;
        if !rect.contains(p) || y < 0.0 {
            return None;
        }
        Some((y / self.row_height) as usize)
    }

    /// Rectangle of attribute row `row`, for a box centered at `center`.
    pub fn row_rect(&self, center: Point, row: usize) -> Rect {
        let rect = self.rect(center);
        Rect {
            x: rect.x,
            y: rect.y + self.header_height + row as f64 * self.row_height,
            w: rect.w,
            h: self.row_height,
        }
    }
}

/// Cell texts of an attribute row, in column order.
pub fn attribute_cells(entity: &Entity, row: usize) -> [String; 4] {
    let a = &entity.attributes[row];
    let keys: Vec<_> = a.keys.iter().map(|k| k.keyword()).collect();
    [
        a.ty.clone(),
        a.name.clone(),
        keys.join(", "),
        a.comment.clone(),
    ]
}

pub fn entity_box(entity: &Entity, text_size: impl Fn(&str) -> (f64, f64)) -> EntityBox {
    let (label_w, label_h) = text_size(&entity.label);
    let line_h = text_size("Ag").1;
    let mut widths = [0.0_f64; 4];
    for row in 0..entity.attributes.len() {
        for (w, cell) in widths.iter_mut().zip(attribute_cells(entity, row)) {
            *w = w.max(text_size(&cell).0);
        }
    }
    let mut columns = [PADDING; 4];
    for i in 1..4 {
        let gap = if widths[i - 1] > 0.0 { COLUMN_GAP } else { 0.0 };
        columns[i] = columns[i - 1] + widths[i - 1] + gap;
    }
    let table_w = columns[3] + widths[3] + PADDING;
    let header_height = label_h + HEADER_PADDING;
    let row_height = line_h + ROW_PADDING;
    let body = if entity.attributes.is_empty() {
        EMPTY_BODY
    } else {
        row_height * entity.attributes.len() as f64
    };
    EntityBox {
        size: (
            table_w.max(label_w + 2.0 * PADDING).max(MIN_WIDTH),
            header_height + body,
        ),
        header_height,
        row_height,
        columns,
    }
}

/// Assigns a position to every entity without one.
pub fn layout_missing(erd: &mut Erd, size_of: impl Fn(&Entity) -> (f64, f64)) {
    let index: HashMap<&str, usize> = erd
        .entities
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.as_str(), i))
        .collect();
    let edges: Vec<(usize, usize)> = erd
        .relationships
        .iter()
        .filter_map(|r| Some((*index.get(r.from.as_str())?, *index.get(r.to.as_str())?)))
        .collect();
    let sizes: Vec<_> = erd.entities.iter().map(&size_of).collect();
    let mut positions: Vec<_> = erd.entities.iter().map(|e| e.pos).collect();
    layered::fill_missing(
        &mut positions,
        &sizes,
        &edges,
        erd.direction.unwrap_or_default(),
        SPACING,
    );
    for (entity, pos) in erd.entities.iter_mut().zip(positions) {
        entity.pos = pos;
    }
}

/// Discards every stored position and lays the whole diagram out again.
pub fn layout_all(erd: &mut Erd, size_of: impl Fn(&Entity) -> (f64, f64)) {
    for entity in &mut erd.entities {
        entity.pos = None;
    }
    layout_missing(erd, size_of);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagrams::erd::parser::parse;

    fn size(s: &str) -> (f64, f64) {
        (s.chars().count() as f64 * 8.0, 16.0)
    }

    #[test]
    fn columns_fit_the_widest_cell() {
        let erd =
            parse("erDiagram\n T {\n  int id PK\n  varchar(255) description \"long text\"\n }")
                .unwrap();
        let b = entity_box(&erd.entities[0], size);
        let type_w = 12.0 * 8.0;
        assert_eq!(b.columns[0], PADDING);
        assert_eq!(b.columns[1], PADDING + type_w + COLUMN_GAP);
        assert!(b.size.0 >= b.columns[3] + 9.0 * 8.0);
        assert_eq!(b.size.1, b.header_height + 2.0 * b.row_height);
    }

    #[test]
    fn finds_attribute_rows() {
        let erd = parse("erDiagram\n T {\n  int a\n  int b\n }").unwrap();
        let b = entity_box(&erd.entities[0], size);
        let c = Point::new(100.0, 100.0);
        let rect = b.rect(c);
        assert_eq!(b.row_at(c, Point::new(c.x, rect.y + 2.0)), None);
        assert_eq!(b.row_at(c, b.row_rect(c, 1).center()), Some(1));
        assert_eq!(b.row_at(c, Point::new(rect.x - 5.0, c.y)), None);
    }

    #[test]
    fn lays_out_related_entities_in_ranks() {
        let mut erd = parse("erDiagram\n A ||--o{ B : x").unwrap();
        layout_missing(&mut erd, |e| entity_box(e, size).size);
        let (a, b) = (erd.entities[0].pos.unwrap(), erd.entities[1].pos.unwrap());
        assert!(b.y > a.y);
    }
}
