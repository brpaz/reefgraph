//! Entity relationship editing UI: canvas, properties panel and palette items.

mod diagram;
mod inspector;
mod render;

pub use inspector::Inspector;

use gettextrs::gettext;

use crate::app::palette::PaletteItem;
use crate::diagrams::erd::{Attribute, Key};

/// Entities the palette offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityPreset {
    Empty,
    WithId,
}

impl EntityPreset {
    pub const ALL: [EntityPreset; 2] = [EntityPreset::Empty, EntityPreset::WithId];

    pub fn id(self) -> &'static str {
        match self {
            EntityPreset::Empty => "entity",
            EntityPreset::WithId => "entity-with-id",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    pub fn attributes(self) -> Vec<Attribute> {
        match self {
            EntityPreset::Empty => Vec::new(),
            EntityPreset::WithId => vec![Attribute {
                keys: vec![Key::Primary],
                ..Attribute::new("int", "id")
            }],
        }
    }
}

pub fn palette_items() -> Vec<PaletteItem> {
    EntityPreset::ALL
        .into_iter()
        .map(|preset| PaletteItem {
            id: preset.id(),
            title: match preset {
                EntityPreset::Empty => gettext("Entity"),
                EntityPreset::WithId => gettext("Entity with Id"),
            },
            preview: Box::new(move |cr, r| {
                cr.rectangle(r.x, r.y, r.w, r.h);
                let header = r.y + r.h * 0.35;
                cr.move_to(r.x, header);
                cr.line_to(r.x + r.w, header);
                if preset == EntityPreset::WithId {
                    let row = header + (r.y + r.h - header) / 2.0;
                    cr.move_to(r.x + r.w * 0.15, row);
                    cr.line_to(r.x + r.w * 0.45, row);
                    cr.move_to(r.x + r.w * 0.55, row);
                    cr.line_to(r.x + r.w * 0.85, row);
                }
            }),
            aspect: 1.5,
        })
        .collect()
}
