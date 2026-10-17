//! Cairo drawing of states and transitions, in canvas coordinates.

use gtk::cairo;

use crate::canvas::drawing::{Colors, arrow_head, circle, rounded_rect, set_color};
use crate::diagrams::state::StateKind;
use crate::shared::geometry::{Point, Rect};

const ARROW: (f64, f64) = (10.0, 5.0);
const CORNER_RADIUS: f64 = 10.0;
/// Gap between the end state's ring and its filled center.
const END_RING_GAP: f64 = 4.0;

fn diamond(cr: &cairo::Context, r: &Rect) {
    let c = r.center();
    cr.move_to(c.x, r.y);
    cr.line_to(r.x + r.w, c.y);
    cr.line_to(c.x, r.y + r.h);
    cr.line_to(r.x, c.y);
    cr.close_path();
}

pub fn draw_state(
    cr: &cairo::Context,
    rect: &Rect,
    kind: StateKind,
    colors: &Colors,
    highlighted: bool,
) -> Result<(), cairo::Error> {
    let stroke = if highlighted {
        &colors.accent
    } else {
        &colors.fg
    };
    let line_width = if highlighted { 2.5 } else { 1.5 };
    let center = rect.center();
    let radius = rect.w.min(rect.h) / 2.0;
    cr.set_line_width(line_width);
    match kind {
        StateKind::State | StateKind::Choice => {
            if kind == StateKind::State {
                rounded_rect(cr, rect, CORNER_RADIUS);
            } else {
                diamond(cr, rect);
            }
            set_color(cr, &colors.node_fill);
            cr.fill_preserve()?;
            set_color(cr, stroke);
            cr.stroke()
        }
        StateKind::Start => {
            circle(cr, center, radius);
            set_color(cr, stroke);
            cr.fill()
        }
        StateKind::End => {
            circle(cr, center, radius);
            set_color(cr, &colors.node_fill);
            cr.fill_preserve()?;
            set_color(cr, stroke);
            cr.stroke()?;
            circle(cr, center, radius - END_RING_GAP);
            cr.fill()
        }
        StateKind::Fork | StateKind::Join => {
            rounded_rect(cr, rect, 2.0);
            set_color(cr, stroke);
            cr.fill()
        }
    }
}

pub fn draw_transition(
    cr: &cairo::Context,
    path: &[Point],
    colors: &Colors,
    highlighted: bool,
) -> Result<(), cairo::Error> {
    let n = path.len();
    if n < 2 {
        return Ok(());
    }
    let color = if highlighted {
        &colors.accent
    } else {
        &colors.fg
    };
    set_color(cr, color);
    cr.set_line_width(if highlighted { 2.5 } else { 1.5 });
    cr.move_to(path[0].x, path[0].y);
    for p in &path[1..] {
        cr.line_to(p.x, p.y);
    }
    cr.stroke()?;
    arrow_head(cr, path[n - 1], path[n - 2], ARROW.0, ARROW.1);
    cr.fill()
}

/// Outline of a palette preview; the palette strokes it.
pub fn preview_path(cr: &cairo::Context, kind: StateKind, r: &Rect) {
    let c = r.center();
    let side = r.w.min(r.h);
    match kind {
        StateKind::State => rounded_rect(cr, r, r.h * 0.3),
        StateKind::Start => circle(cr, c, side * 0.3),
        StateKind::End => {
            circle(cr, c, side * 0.35);
            circle(cr, c, side * 0.18);
        }
        StateKind::Choice => diamond(cr, &Rect::centered(c, (side * 0.8, side * 0.8))),
        StateKind::Fork | StateKind::Join => {
            cr.rectangle(r.x, c.y - r.h * 0.1, r.w, r.h * 0.2);
        }
    }
}
