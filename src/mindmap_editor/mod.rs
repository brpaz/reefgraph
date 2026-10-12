//! Mindmap editing UI: box canvas integration, properties panel and palette
//! items.

mod diagram;
mod inspector;
mod render;

pub use inspector::Inspector;

use crate::labels;
use crate::mindmap::Shape;
use crate::palette::PaletteItem;

pub fn preset_id(shape: Shape) -> &'static str {
    match shape {
        Shape::Default => "default",
        Shape::Square => "square",
        Shape::Rounded => "rounded",
        Shape::Circle => "circle",
        Shape::Bang => "bang",
        Shape::Cloud => "cloud",
        Shape::Hexagon => "hexagon",
    }
}

pub fn preset_shape(id: &str) -> Option<Shape> {
    Shape::ALL.into_iter().find(|s| preset_id(*s) == id)
}

pub fn palette_items() -> Vec<PaletteItem> {
    Shape::ALL
        .into_iter()
        .map(|shape| PaletteItem {
            id: preset_id(shape),
            title: labels::mindmap_shape(shape),
            preview: Box::new(move |cr, r| render::shape_path(cr, shape, r)),
            aspect: if shape == Shape::Circle { 1.0 } else { 1.6 },
        })
        .collect()
}
