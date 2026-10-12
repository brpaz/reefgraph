//! Use case diagram editing UI: box canvas integration, properties panel and
//! palette items.

mod diagram;
mod inspector;
mod render;

pub use inspector::Inspector;

use crate::labels;
use crate::palette::PaletteItem;
use crate::usecase::ElementKind;

pub fn preset_id(kind: ElementKind) -> &'static str {
    match kind {
        ElementKind::Actor => "actor",
        ElementKind::UseCase => "usecase",
        ElementKind::UseCaseRect => "usecase-rect",
    }
}

pub fn preset_kind(id: &str) -> Option<ElementKind> {
    ElementKind::ALL.into_iter().find(|k| preset_id(*k) == id)
}

pub fn palette_items() -> Vec<PaletteItem> {
    ElementKind::ALL
        .into_iter()
        .map(|kind| PaletteItem {
            id: preset_id(kind),
            title: labels::usecase_element_kind(kind),
            preview: Box::new(move |cr, r| render::preview_path(cr, kind, r)),
            aspect: match kind {
                ElementKind::Actor => 0.8,
                ElementKind::UseCase | ElementKind::UseCaseRect => 1.6,
            },
        })
        .collect()
}
