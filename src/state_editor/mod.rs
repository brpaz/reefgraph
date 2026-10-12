//! State diagram editing UI: box canvas integration, properties panel and
//! palette items.

mod diagram;
mod inspector;
mod render;

pub use inspector::Inspector;

use crate::labels;
use crate::palette::PaletteItem;
use crate::state::StateKind;

/// Palette order of the state kinds.
const PALETTE: [StateKind; 6] = [
    StateKind::State,
    StateKind::Start,
    StateKind::End,
    StateKind::Choice,
    StateKind::Fork,
    StateKind::Join,
];

pub fn preset_id(kind: StateKind) -> &'static str {
    match kind {
        StateKind::State => "state",
        StateKind::Start => "start",
        StateKind::End => "end",
        StateKind::Choice => "choice",
        StateKind::Fork => "fork",
        StateKind::Join => "join",
    }
}

pub fn preset_kind(id: &str) -> Option<StateKind> {
    PALETTE.into_iter().find(|k| preset_id(*k) == id)
}

pub fn palette_items() -> Vec<PaletteItem> {
    PALETTE
        .into_iter()
        .map(|kind| PaletteItem {
            id: preset_id(kind),
            title: labels::state_kind(kind),
            preview: Box::new(move |cr, r| render::preview_path(cr, kind, r)),
            aspect: match kind {
                StateKind::State | StateKind::Fork | StateKind::Join => 1.6,
                StateKind::Start | StateKind::End | StateKind::Choice => 1.0,
            },
        })
        .collect()
}
