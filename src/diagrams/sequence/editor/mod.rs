//! Sequence diagram editing UI: canvas, properties panel and palette items.

mod canvas;
mod inspector;
mod render;

pub use canvas::{Canvas, PaletteElement};
pub use inspector::Inspector;

use gtk::cairo;

use crate::app::palette::PaletteItem;
use crate::canvas::drawing::rounded_rect;
use crate::diagrams::sequence::ParticipantKind;
use crate::shared::geometry::Rect;
use crate::shared::labels;

pub fn palette_items() -> Vec<PaletteItem> {
    PaletteElement::ALL
        .into_iter()
        .map(|element| {
            let (title, aspect): (String, f64) = match element {
                PaletteElement::Participant(kind) => (labels::participant_kind(kind), 1.6),
                PaletteElement::Note => (gettextrs::gettext("Note"), 1.6),
                PaletteElement::Block(kind) => (labels::block_kind(kind), 1.6),
            };
            PaletteItem {
                id: element.id(),
                title,
                preview: Box::new(move |cr, rect| preview(cr, element, rect)),
                aspect,
            }
        })
        .collect()
}

fn preview(cr: &cairo::Context, element: PaletteElement, r: &Rect) {
    let c = r.center();
    match element {
        PaletteElement::Participant(ParticipantKind::Participant) => {
            rounded_rect(
                cr,
                &Rect {
                    y: r.y + r.h * 0.2,
                    h: r.h * 0.6,
                    ..*r
                },
                3.0,
            );
        }
        PaletteElement::Participant(ParticipantKind::Actor) => {
            let s = r.h / 44.0;
            cr.new_sub_path();
            cr.arc(c.x, r.y + 8.0 * s, 7.0 * s, 0.0, std::f64::consts::TAU);
            cr.move_to(c.x, r.y + 15.0 * s);
            cr.line_to(c.x, r.y + 30.0 * s);
            cr.move_to(c.x - 12.0 * s, r.y + 21.0 * s);
            cr.line_to(c.x + 12.0 * s, r.y + 21.0 * s);
            cr.move_to(c.x - 10.0 * s, r.y + r.h);
            cr.line_to(c.x, r.y + 30.0 * s);
            cr.line_to(c.x + 10.0 * s, r.y + r.h);
        }
        PaletteElement::Note => {
            let fold = r.h * 0.3;
            cr.move_to(r.x, r.y);
            cr.line_to(r.x + r.w - fold, r.y);
            cr.line_to(r.x + r.w, r.y + fold);
            cr.line_to(r.x + r.w, r.y + r.h);
            cr.line_to(r.x, r.y + r.h);
            cr.close_path();
            cr.move_to(r.x + r.w - fold, r.y);
            cr.line_to(r.x + r.w - fold, r.y + fold);
            cr.line_to(r.x + r.w, r.y + fold);
        }
        PaletteElement::Block(_) => {
            cr.rectangle(r.x, r.y, r.w, r.h);
            let (tw, th) = (r.w * 0.4, r.h * 0.3);
            cr.move_to(r.x + tw, r.y);
            cr.line_to(r.x + tw, r.y + th - 3.0);
            cr.line_to(r.x + tw - 3.0, r.y + th);
            cr.line_to(r.x, r.y + th);
        }
    }
}
