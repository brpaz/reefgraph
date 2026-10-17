//! Mindmap editing UI: box canvas integration, properties panel and palette
//! items.

mod diagram;
mod inspector;
mod render;

pub use inspector::Inspector;

use crate::app::palette::PaletteItem;
use crate::diagrams::mindmap::Shape;
use crate::shared::labels;

/// Every way a node can be drawn: plain text, then each shape.
pub const SHAPES: [Option<Shape>; 7] = [
    None,
    Some(Shape::Square),
    Some(Shape::Rounded),
    Some(Shape::Circle),
    Some(Shape::Bang),
    Some(Shape::Cloud),
    Some(Shape::Hexagon),
];

pub fn preset_id(shape: Option<Shape>) -> &'static str {
    match shape {
        None => "text",
        Some(Shape::Square) => "square",
        Some(Shape::Rounded) => "rounded",
        Some(Shape::Circle) => "circle",
        Some(Shape::Bang) => "bang",
        Some(Shape::Cloud) => "cloud",
        Some(Shape::Hexagon) => "hexagon",
    }
}

/// The node shape of a palette item, if `id` is one.
pub fn preset_shape(id: &str) -> Option<Option<Shape>> {
    SHAPES.into_iter().find(|s| preset_id(*s) == id)
}

pub fn palette_items() -> Vec<PaletteItem> {
    SHAPES
        .into_iter()
        .map(|shape| PaletteItem {
            id: preset_id(shape),
            title: labels::mindmap_shape(shape),
            preview: Box::new(move |cr, r| render::shape_path(cr, shape, r)),
            aspect: if shape == Some(Shape::Circle) {
                1.0
            } else {
                1.6
            },
        })
        .collect()
}
