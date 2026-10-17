//! C4 diagram editing UI: box canvas integration, properties panel and
//! palette items.

mod diagram;
mod inspector;
mod render;

pub use inspector::Inspector;

use gettextrs::gettext;

use crate::app::palette::PaletteItem;
use crate::diagrams::c4::{ElementKind, ElementShape};

/// Elements the palette offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElementPreset {
    pub id: &'static str,
    pub kind: ElementKind,
    pub shape: ElementShape,
    pub external: bool,
}

impl ElementPreset {
    pub const ALL: [ElementPreset; 9] = [
        Self::new("person", ElementKind::Person, ElementShape::Box, false),
        Self::new("person-ext", ElementKind::Person, ElementShape::Box, true),
        Self::new("system", ElementKind::System, ElementShape::Box, false),
        Self::new("system-ext", ElementKind::System, ElementShape::Box, true),
        Self::new(
            "system-db",
            ElementKind::System,
            ElementShape::Database,
            false,
        ),
        Self::new(
            "container",
            ElementKind::Container,
            ElementShape::Box,
            false,
        ),
        Self::new(
            "container-db",
            ElementKind::Container,
            ElementShape::Database,
            false,
        ),
        Self::new(
            "container-queue",
            ElementKind::Container,
            ElementShape::Queue,
            false,
        ),
        Self::new(
            "component",
            ElementKind::Component,
            ElementShape::Box,
            false,
        ),
    ];

    const fn new(id: &'static str, kind: ElementKind, shape: ElementShape, external: bool) -> Self {
        Self {
            id,
            kind,
            shape,
            external,
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id == id)
    }

    fn title(self) -> String {
        gettext(match self.id {
            "person" => "Person",
            "person-ext" => "External Person",
            "system" => "System",
            "system-ext" => "External System",
            "system-db" => "System Database",
            "container" => "Container",
            "container-db" => "Container Database",
            "container-queue" => "Container Queue",
            _ => "Component",
        })
    }
}

pub fn palette_items() -> Vec<PaletteItem> {
    ElementPreset::ALL
        .into_iter()
        .map(|preset| PaletteItem {
            id: preset.id,
            title: preset.title(),
            preview: Box::new(move |cr, r| render::preview_path(cr, preset, r)),
            aspect: if preset.kind == ElementKind::Person {
                1.0
            } else {
                1.5
            },
        })
        .collect()
}
