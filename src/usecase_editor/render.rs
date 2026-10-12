//! Cairo drawing of actors, use cases, boundary frames and relationships, in
//! canvas coordinates.

use std::f64::consts::PI;

use gtk::{cairo, gdk};

use crate::drawing::{Colors, arrow_head, circle, rounded_rect, set_color};
use crate::geometry::{Point, Rect};
use crate::usecase::layout::FIGURE_SIZE;
use crate::usecase::{ElementKind, RelationKind};

const ARROW: (f64, f64) = (10.0, 5.0);
const TRIANGLE: (f64, f64) = (14.0, 8.0);
const END_MARK: f64 = 5.0;

fn ellipse(cr: &cairo::Context, r: &Rect) {
    let c = r.center();
    cr.save().ok();
    cr.translate(c.x, c.y);
    cr.scale(r.w / 2.0, r.h / 2.0);
    cr.new_sub_path();
    cr.arc(0.0, 0.0, 1.0, 0.0, 2.0 * PI);
    cr.restore().ok();
}

/// Stick figure filling `r`.
fn figure(cr: &cairo::Context, r: &Rect) {
    let cx = r.center().x;
    let head = r.h * 0.17;
    let neck = r.y + 2.0 * head;
    let hip = r.y + r.h * 0.68;
    circle(cr, Point::new(cx, r.y + head), head);
    cr.move_to(cx, neck);
    cr.line_to(cx, hip);
    cr.move_to(r.x, neck + r.h * 0.12);
    cr.line_to(r.x + r.w, neck + r.h * 0.12);
    cr.move_to(r.x + r.w * 0.1, r.y + r.h);
    cr.line_to(cx, hip);
    cr.line_to(r.x + r.w * 0.9, r.y + r.h);
}

pub fn draw_element(
    cr: &cairo::Context,
    rect: &Rect,
    kind: ElementKind,
    colors: &Colors,
    highlighted: bool,
) -> Result<(), cairo::Error> {
    let stroke = if highlighted {
        &colors.accent
    } else {
        &colors.fg
    };
    cr.set_line_width(if highlighted { 2.5 } else { 1.5 });
    match kind {
        ElementKind::Actor => {
            let r = Rect {
                x: rect.center().x - FIGURE_SIZE.0 / 2.0,
                y: rect.y,
                w: FIGURE_SIZE.0,
                h: FIGURE_SIZE.1,
            };
            figure(cr, &r);
            set_color(cr, stroke);
            cr.stroke()
        }
        ElementKind::UseCase | ElementKind::UseCaseRect => {
            if kind == ElementKind::UseCase {
                ellipse(cr, rect);
            } else {
                rounded_rect(cr, rect, 4.0);
            }
            set_color(cr, &colors.node_fill);
            cr.fill_preserve()?;
            set_color(cr, stroke);
            cr.stroke()
        }
    }
}

/// System boundary frame behind its members.
pub fn draw_frame(cr: &cairo::Context, rect: &Rect, colors: &Colors) -> Result<(), cairo::Error> {
    rounded_rect(cr, rect, 6.0);
    set_color(cr, &colors.fg.with_alpha(0.04));
    cr.fill_preserve()?;
    set_color(cr, &colors.fg.with_alpha(0.55));
    cr.set_line_width(1.5);
    cr.stroke()
}

/// Unit vector from `from` to `to`, if they differ.
fn direction(from: Point, to: Point) -> Option<(f64, f64)> {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let len = dx.hypot(dy);
    (len > 0.0).then(|| (dx / len, dy / len))
}

/// Marker drawn at a relationship end.
#[derive(Clone, Copy, PartialEq)]
enum Mark {
    None,
    Arrow,
    OpenArrow,
    Circle,
    Cross,
    Triangle,
}

fn marks(kind: RelationKind) -> (Mark, Mark) {
    match kind {
        RelationKind::Line => (Mark::None, Mark::None),
        RelationKind::Arrow => (Mark::None, Mark::Arrow),
        RelationKind::ArrowBack => (Mark::Arrow, Mark::None),
        RelationKind::CircleTo => (Mark::None, Mark::Circle),
        RelationKind::CircleFrom => (Mark::Circle, Mark::None),
        RelationKind::CrossTo => (Mark::None, Mark::Cross),
        RelationKind::CrossFrom => (Mark::Cross, Mark::None),
        RelationKind::Include | RelationKind::Extend => (Mark::None, Mark::OpenArrow),
        RelationKind::Generalization => (Mark::None, Mark::Triangle),
    }
}

pub fn draw_relation(
    cr: &cairo::Context,
    path: &[Point],
    kind: RelationKind,
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
    let width = if highlighted { 2.5 } else { 1.5 };
    let (from_mark, to_mark) = marks(kind);
    let mut points = path.to_vec();
    if to_mark == Mark::Triangle
        && let Some((ux, uy)) = direction(path[n - 1], path[n - 2])
    {
        points[n - 1] = Point::new(
            path[n - 1].x + ux * TRIANGLE.0,
            path[n - 1].y + uy * TRIANGLE.0,
        );
    }
    set_color(cr, color);
    cr.set_line_width(width);
    if matches!(kind, RelationKind::Include | RelationKind::Extend) {
        cr.set_dash(&[6.0, 4.0], 0.0);
    }
    cr.move_to(points[0].x, points[0].y);
    for p in &points[1..] {
        cr.line_to(p.x, p.y);
    }
    cr.stroke()?;
    cr.set_dash(&[], 0.0);
    draw_mark(cr, from_mark, path[0], path[1], colors, color)?;
    draw_mark(cr, to_mark, path[n - 1], path[n - 2], colors, color)
}

/// Draws a marker whose tip touches the outline at `tip`, the line
/// continuing towards `toward`.
fn draw_mark(
    cr: &cairo::Context,
    mark: Mark,
    tip: Point,
    toward: Point,
    colors: &Colors,
    color: &gdk::RGBA,
) -> Result<(), cairo::Error> {
    let Some((ux, uy)) = direction(tip, toward) else {
        return Ok(());
    };
    let p = |along: f64, across: f64| {
        Point::new(
            tip.x + ux * along - uy * across,
            tip.y + uy * along + ux * across,
        )
    };
    match mark {
        Mark::None => Ok(()),
        Mark::Arrow => {
            arrow_head(cr, tip, p(ARROW.0, 0.0), ARROW.0, ARROW.1);
            set_color(cr, color);
            cr.fill()
        }
        Mark::OpenArrow => {
            let (a, b) = (p(ARROW.0, -ARROW.1), p(ARROW.0, ARROW.1));
            cr.move_to(a.x, a.y);
            cr.line_to(tip.x, tip.y);
            cr.line_to(b.x, b.y);
            set_color(cr, color);
            cr.stroke()
        }
        Mark::Circle => {
            circle(cr, p(END_MARK, 0.0), END_MARK);
            set_color(cr, &colors.node_fill);
            cr.fill_preserve()?;
            set_color(cr, color);
            cr.stroke()
        }
        Mark::Cross => {
            let c = p(END_MARK + 2.0, 0.0);
            let d = END_MARK * 0.8;
            cr.move_to(c.x - d, c.y - d);
            cr.line_to(c.x + d, c.y + d);
            cr.move_to(c.x - d, c.y + d);
            cr.line_to(c.x + d, c.y - d);
            set_color(cr, color);
            cr.stroke()
        }
        Mark::Triangle => {
            let (a, b) = (p(TRIANGLE.0, -TRIANGLE.1), p(TRIANGLE.0, TRIANGLE.1));
            cr.move_to(tip.x, tip.y);
            cr.line_to(a.x, a.y);
            cr.line_to(b.x, b.y);
            cr.close_path();
            set_color(cr, &colors.node_fill);
            cr.fill_preserve()?;
            set_color(cr, color);
            cr.stroke()
        }
    }
}

/// Outline of a palette preview; the palette strokes it.
pub fn preview_path(cr: &cairo::Context, kind: ElementKind, r: &Rect) {
    match kind {
        ElementKind::Actor => {
            let h = r.h;
            let w = h * FIGURE_SIZE.0 / FIGURE_SIZE.1;
            figure(cr, &Rect::centered(r.center(), (w, h)));
        }
        ElementKind::UseCase => ellipse(cr, r),
        ElementKind::UseCaseRect => rounded_rect(cr, r, 3.0),
    }
}
