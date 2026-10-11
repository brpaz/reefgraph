//! Class diagram editing UI: canvas, properties panel and palette items.

mod diagram;
mod inspector;
mod render;

pub use inspector::Inspector;

use gettextrs::gettext;

use crate::palette::PaletteItem;

/// Classes the palette offers, differing by stereotype.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassPreset {
    Class,
    Interface,
    Abstract,
    Enumeration,
}

impl ClassPreset {
    pub const ALL: [ClassPreset; 4] = [
        ClassPreset::Class,
        ClassPreset::Interface,
        ClassPreset::Abstract,
        ClassPreset::Enumeration,
    ];

    pub fn id(self) -> &'static str {
        match self {
            ClassPreset::Class => "class",
            ClassPreset::Interface => "interface",
            ClassPreset::Abstract => "abstract",
            ClassPreset::Enumeration => "enumeration",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    /// Stereotypes a new class of this kind starts with.
    pub fn annotations(self) -> Vec<String> {
        match self {
            ClassPreset::Class => Vec::new(),
            other => vec![other.id().to_string()],
        }
    }

    fn title(self) -> String {
        match self {
            ClassPreset::Class => gettext("Class"),
            ClassPreset::Interface => gettext("Interface"),
            ClassPreset::Abstract => gettext("Abstract Class"),
            ClassPreset::Enumeration => gettext("Enumeration"),
        }
    }
}

pub fn palette_items() -> Vec<PaletteItem> {
    ClassPreset::ALL
        .into_iter()
        .map(|preset| PaletteItem {
            id: preset.id(),
            title: preset.title(),
            preview: Box::new(move |cr, r| {
                cr.rectangle(r.x, r.y, r.w, r.h);
                for f in [0.35, 0.68] {
                    cr.move_to(r.x, r.y + r.h * f);
                    cr.line_to(r.x + r.w, r.y + r.h * f);
                }
                // Stereotyped kinds get a short line above the name.
                if preset != ClassPreset::Class {
                    let y = r.y + r.h * 0.12;
                    cr.move_to(r.x + r.w * 0.3, y);
                    cr.line_to(r.x + r.w * 0.7, y);
                }
            }),
            aspect: 1.2,
        })
        .collect()
}
