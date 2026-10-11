//! Cairo drawing of sequence diagram elements, in canvas coordinates.

use gtk::{cairo, gdk};

use crate::drawing::{Colors, arrow_head, circle, rounded_rect, set_color};
use crate::geometry::{Point, Rect};
use crate::sequence::layout::{ACTOR_FIGURE_HEIGHT, BLOCK_HEADER, SELF_LOOP_SIZE};
use crate::sequence::{ArrowHead, LineStyle, ParticipantKind};

const ARROW_SIZE: (f64, f64) = (10.0, 5.0);
pub const NUMBER_RADIUS: f64 = 9.0;
/// Width of the block keyword tab beyond its text.
pub const TAB_PADDING: f64 = 16.0;

fn outline(cr: &cairo::Context, colors: &Colors, highlighted: bool) -> Result<(), cairo::Error> {
    set_color(
        cr,
        if highlighted {
            &colors.accent
        } else {
            &colors.fg
        },
    );
    cr.set_line_width(if highlighted { 2.5 } else { 1.5 });
    cr.stroke()
}

pub fn draw_participant(
    cr: &cairo::Context,
    kind: ParticipantKind,
    rect: &Rect,
    colors: &Colors,
    highlighted: bool,
) -> Result<(), cairo::Error> {
    match kind {
        ParticipantKind::Participant => {
            rounded_rect(cr, rect, 4.0);
            set_color(cr, &colors.node_fill);
            cr.fill_preserve()?;
            outline(cr, colors, highlighted)
        }
        ParticipantKind::Actor => {
            let cx = rect.center().x;
            let y = rect.y;
            circle(cr, Point::new(cx, y + 9.0), 7.0);
            set_color(cr, &colors.node_fill);
            cr.fill_preserve()?;
            cr.move_to(cx, y + 16.0);
            cr.line_to(cx, y + 30.0);
            cr.move_to(cx - 12.0, y + 21.0);
            cr.line_to(cx + 12.0, y + 21.0);
            cr.move_to(cx - 10.0, y + ACTOR_FIGURE_HEIGHT - 2.0);
            cr.line_to(cx, y + 30.0);
            cr.line_to(cx + 10.0, y + ACTOR_FIGURE_HEIGHT - 2.0);
            outline(cr, colors, highlighted)
        }
    }
}

/// Where the participant's label is centered.
pub fn participant_label_center(kind: ParticipantKind, rect: &Rect) -> Point {
    match kind {
        ParticipantKind::Participant => rect.center(),
        ParticipantKind::Actor => Point::new(
            rect.center().x,
            rect.y + ACTOR_FIGURE_HEIGHT + (rect.h - ACTOR_FIGURE_HEIGHT) / 2.0,
        ),
    }
}

pub fn draw_lifeline(
    cr: &cairo::Context,
    x: f64,
    top: f64,
    bottom: f64,
    colors: &Colors,
    highlighted: bool,
) -> Result<(), cairo::Error> {
    let color = if highlighted {
        colors.accent
    } else {
        colors.fg.with_alpha(0.5)
    };
    set_color(cr, &color);
    cr.set_line_width(if highlighted { 2.0 } else { 1.0 });
    cr.set_dash(&[4.0, 4.0], 0.0);
    cr.move_to(x, top);
    cr.line_to(x, bottom);
    cr.stroke()?;
    cr.set_dash(&[], 0.0);
    Ok(())
}

pub fn draw_activation(
    cr: &cairo::Context,
    rect: &Rect,
    colors: &Colors,
) -> Result<(), cairo::Error> {
    cr.rectangle(rect.x, rect.y, rect.w, rect.h);
    set_color(cr, &colors.node_fill);
    cr.fill_preserve()?;
    set_color(cr, &colors.fg);
    cr.set_line_width(1.0);
    cr.stroke()
}

/// Draws a message arrow from `x1` to `x2` at height `y`.
#[allow(clippy::too_many_arguments)]
pub fn draw_message(
    cr: &cairo::Context,
    line: LineStyle,
    head: ArrowHead,
    (x1, x2, y): (f64, f64, f64),
    self_loop: bool,
    colors: &Colors,
    highlighted: bool,
) -> Result<(), cairo::Error> {
    let color = if highlighted {
        &colors.accent
    } else {
        &colors.fg
    };
    set_color(cr, color);
    cr.set_line_width(if highlighted { 2.5 } else { 1.5 });
    if line == LineStyle::Dotted {
        cr.set_dash(&[5.0, 4.0], 0.0);
    }
    // Path points; the arrow head sits on the last one.
    let points = if self_loop {
        let (w, h) = SELF_LOOP_SIZE;
        vec![
            Point::new(x1, y),
            Point::new(x1 + w, y),
            Point::new(x1 + w, y + h),
            Point::new(x1, y + h),
        ]
    } else {
        vec![Point::new(x1, y), Point::new(x2, y)]
    };
    let n = points.len();
    let (tip, from) = (points[n - 1], points[n - 2]);
    // Stop the line under a filled head so it does not poke through the tip.
    let line_end = if head == ArrowHead::Filled {
        let dir = (tip.x - from.x).signum();
        Point::new(tip.x - dir * ARROW_SIZE.0 * 0.8, tip.y)
    } else {
        tip
    };
    cr.move_to(points[0].x, points[0].y);
    for p in &points[1..n - 1] {
        cr.line_to(p.x, p.y);
    }
    cr.line_to(line_end.x, line_end.y);
    cr.stroke()?;
    cr.set_dash(&[], 0.0);

    let dir = (tip.x - from.x).signum();
    match head {
        ArrowHead::None => {}
        ArrowHead::Filled => {
            arrow_head(cr, tip, from, ARROW_SIZE.0, ARROW_SIZE.1);
            cr.fill()?;
        }
        ArrowHead::Open => {
            cr.move_to(tip.x - dir * ARROW_SIZE.0, tip.y - ARROW_SIZE.1);
            cr.line_to(tip.x, tip.y);
            cr.line_to(tip.x - dir * ARROW_SIZE.0, tip.y + ARROW_SIZE.1);
            cr.stroke()?;
        }
        ArrowHead::Cross => {
            let c = Point::new(tip.x - dir * 6.0, tip.y);
            let s = 5.0;
            cr.move_to(c.x - s, c.y - s);
            cr.line_to(c.x + s, c.y + s);
            cr.move_to(c.x - s, c.y + s);
            cr.line_to(c.x + s, c.y - s);
            cr.stroke()?;
        }
    }
    Ok(())
}

pub fn draw_number_badge(
    cr: &cairo::Context,
    at: Point,
    colors: &Colors,
) -> Result<(), cairo::Error> {
    circle(cr, at, NUMBER_RADIUS);
    set_color(cr, &colors.accent);
    cr.fill()
}

pub fn note_fill(dark: bool) -> gdk::RGBA {
    if dark {
        gdk::RGBA::new(0.36, 0.33, 0.15, 1.0)
    } else {
        gdk::RGBA::new(1.0, 0.96, 0.68, 1.0)
    }
}

pub fn draw_note(
    cr: &cairo::Context,
    rect: &Rect,
    fill: &gdk::RGBA,
    colors: &Colors,
    highlighted: bool,
) -> Result<(), cairo::Error> {
    cr.rectangle(rect.x, rect.y, rect.w, rect.h);
    set_color(cr, fill);
    cr.fill_preserve()?;
    outline(cr, colors, highlighted)
}

/// Background of a `rect` block, colored as in the source when possible.
pub fn draw_highlight_block(
    cr: &cairo::Context,
    rect: &Rect,
    color: Option<gdk::RGBA>,
    dark: bool,
    colors: &Colors,
    highlighted: bool,
) -> Result<(), cairo::Error> {
    cr.rectangle(rect.x, rect.y, rect.w, rect.h);
    // Source colors are usually picked for light backgrounds; tone them
    // down on dark ones so labels stay readable.
    let fill = match color {
        Some(c) if dark => c.with_alpha(c.alpha() * 0.35),
        Some(c) => c,
        None => colors.accent.with_alpha(0.12),
    };
    set_color(cr, &fill);
    if highlighted {
        cr.fill_preserve()?;
        outline(cr, colors, true)
    } else {
        cr.fill()
    }
}

/// Frame of a block with its keyword tab and branch separators.
pub fn draw_frame(
    cr: &cairo::Context,
    rect: &Rect,
    tab_width: f64,
    branches: &[f64],
    colors: &Colors,
    highlighted: bool,
) -> Result<(), cairo::Error> {
    let color = if highlighted {
        colors.accent
    } else {
        colors.fg.with_alpha(0.7)
    };
    set_color(cr, &color);
    cr.set_line_width(if highlighted { 2.0 } else { 1.0 });
    cr.rectangle(rect.x, rect.y, rect.w, rect.h);
    cr.stroke()?;

    let (x, y, h) = (rect.x, rect.y, BLOCK_HEADER - 4.0);
    cr.move_to(x, y);
    cr.line_to(x + tab_width, y);
    cr.line_to(x + tab_width, y + h - 6.0);
    cr.line_to(x + tab_width - 6.0, y + h);
    cr.line_to(x, y + h);
    cr.close_path();
    set_color(cr, &colors.node_fill);
    cr.fill_preserve()?;
    set_color(cr, &color);
    cr.stroke()?;

    cr.set_dash(&[5.0, 4.0], 0.0);
    for by in branches {
        cr.move_to(rect.x, *by);
        cr.line_to(rect.x + rect.w, *by);
    }
    cr.stroke()?;
    cr.set_dash(&[], 0.0);
    Ok(())
}

/// Small tick across the lifeline for a standalone `activate`/`deactivate`.
pub fn draw_activation_marker(
    cr: &cairo::Context,
    at: Point,
    colors: &Colors,
    highlighted: bool,
) -> Result<(), cairo::Error> {
    let color = if highlighted {
        colors.accent
    } else {
        colors.fg.with_alpha(0.35)
    };
    set_color(cr, &color);
    cr.set_line_width(1.0);
    cr.move_to(at.x - 12.0, at.y);
    cr.line_to(at.x + 12.0, at.y);
    cr.stroke()
}

/// Insertion indicator shown while dragging rows or participants.
pub fn draw_drop_line(
    cr: &cairo::Context,
    from: Point,
    to: Point,
    zoom: f64,
    colors: &Colors,
) -> Result<(), cairo::Error> {
    set_color(cr, &colors.accent);
    cr.set_line_width(3.0 / zoom);
    cr.move_to(from.x, from.y);
    cr.line_to(to.x, to.y);
    cr.stroke()
}

pub fn draw_pending_message(
    cr: &cairo::Context,
    from: Point,
    to: Point,
    zoom: f64,
    colors: &Colors,
) -> Result<(), cairo::Error> {
    set_color(cr, &colors.accent);
    cr.set_line_width(2.0 / zoom);
    cr.set_dash(&[6.0 / zoom, 4.0 / zoom], 0.0);
    cr.move_to(from.x, from.y);
    cr.line_to(to.x, to.y);
    cr.stroke()?;
    cr.set_dash(&[], 0.0);
    arrow_head(cr, to, from, ARROW_SIZE.0, ARROW_SIZE.1);
    cr.fill()
}
