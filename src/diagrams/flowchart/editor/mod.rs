//! Flowchart editing UI: canvas, properties panel and palette items.

mod canvas;
mod geometry;
mod inspector;
mod render;

pub use canvas::Canvas;
pub use inspector::Inspector;

use crate::app::palette::PaletteItem;
use crate::diagrams::flowchart::NodeShape;
use crate::shared::labels;

pub fn palette_items() -> Vec<PaletteItem> {
    NodeShape::ALL
        .into_iter()
        .map(|shape| {
            let (w, h) = geometry::shape_size(shape, (40.0, 16.0));
            PaletteItem {
                id: shape.name(),
                title: labels::shape(shape),
                preview: Box::new(move |cr, rect| {
                    let _ = render::shape_path(cr, shape, rect);
                }),
                aspect: w / h,
            }
        })
        .collect()
}
