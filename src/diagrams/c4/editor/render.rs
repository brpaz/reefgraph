//! Cairo drawing of C4 elements, boundary frames and relations, in canvas
//! coordinates. Elements use the C4 model's usual colors in both themes.

use std::f64::consts::PI;

use gtk::{cairo, gdk};

use super::ElementPreset;
use crate::canvas::drawing::{Colors, arrow_head, circle, rgb, rounded_rect, set_color};
use crate::diagrams::c4::layout::{HEAD_RADIUS, HEAD_ROOM};
use crate::diagrams::c4::{Element, ElementKind, ElementShape};
use crate::shared::geometry::{Point, Rect};

/// Vertical radius of a database's ends, horizontal radius of a queue's.
const CAP_RADIUS: f64 = 8.0;
const ARROW_LENGTH: f64 = 10.0;
const ARROW_HALF_WIDTH: f64 = 4.5;

/// Fill and text color of an element.
pub fn element_colors(e: &Element) -> (gdk::RGBA, gdk::RGBA) {
    let fill = match (e.kind, e.external) {
        (ElementKind::Person, false) => rgb(0x08, 0x42, 0x7b),
        (ElementKind::Person, true) => rgb(0x68, 0x68, 0x68),
        (ElementKind::System, false) => rgb(0x11, 0x68, 0xbd),
        (ElementKind::System, true) => rgb(0x99, 0x99, 0x99),
        (ElementKind::Container, false) => rgb(0x43, 0x8d, 0xd5),
        (ElementKind::Container, true) => rgb(0xb3, 0xb3, 0xb3),
        (ElementKind::Component, false) => rgb(0x85, 0xbb, 0xf0),
        (ElementKind::Component, true) => rgb(0xcc, 0xcc, 0xcc),
    };
    let text = if e.kind == ElementKind::Component {
        rgb(0x1e, 0x1e, 0x1e)
    } else {
        gdk::RGBA::WHITE
    };
    (fill, text)
}

fn darker(c: &gdk::RGBA) -> gdk::RGBA {
    gdk::RGBA::new(c.red() * 0.75, c.green() * 0.75, c.blue() * 0.75, 1.0)
}

pub fn draw_element(
    cr: &cairo::Context,
    e: &Element,
    rect: &Rect,
    colors: &Colors,
    highlighted: bool,
) -> Result<(), cairo::Error> {
    let (fill, _) = element_colors(e);
    let body = Rect {
        y: rect.y
            + if e.kind == ElementKind::Person {
                HEAD_ROOM
            } else {
                0.0
            },
        h: rect.h
            - if e.kind == ElementKind::Person {
                HEAD_ROOM
            } else {
                0.0
            },
        ..*rect
    };
    let shape = if e.kind.has_shapes() {
        e.shape
    } else {
        ElementShape::Box
    };
    shape_path(
        cr,
        shape,
        &body,
        if e.kind == ElementKind::Person {
            14.0
        } else {
            6.0
        },
    );
    if e.kind == ElementKind::Person {
        circle(
            cr,
            Point::new(rect.center().x, rect.y + HEAD_RADIUS),
            HEAD_RADIUS,
        );
    }
    set_color(cr, &fill);
    cr.fill_preserve()?;
    set_color(
        cr,
        &if highlighted {
            colors.accent
        } else {
            darker(&fill)
        },
    );
    cr.set_line_width(if highlighted { 2.5 } else { 1.0 });
    cr.stroke()?;
    cap_lines(cr, shape, &body)?;
    Ok(())
}

/// Outline of a box, database (vertical cylinder) or queue (horizontal one).
fn shape_path(cr: &cairo::Context, shape: ElementShape, r: &Rect, radius: f64) {
    match shape {
        ElementShape::Box => rounded_rect(cr, r, radius),
        ElementShape::Database => {
            let ry = CAP_RADIUS.min(r.h / 4.0);
            cr.new_sub_path();
            ellipse_arc(
                cr,
                Point::new(r.center().x, r.y + ry),
                r.w / 2.0,
                ry,
                PI,
                2.0 * PI,
            );
            cr.line_to(r.x + r.w, r.y + r.h - ry);
            ellipse_arc(
                cr,
                Point::new(r.center().x, r.y + r.h - ry),
                r.w / 2.0,
                ry,
                0.0,
                PI,
            );
            cr.close_path();
        }
        ElementShape::Queue => {
            let rx = CAP_RADIUS.min(r.w / 4.0);
            cr.new_sub_path();
            ellipse_arc(
                cr,
                Point::new(r.x + r.w - rx, r.center().y),
                rx,
                r.h / 2.0,
                -PI / 2.0,
                PI / 2.0,
            );
            cr.line_to(r.x + rx, r.y + r.h);
            ellipse_arc(
                cr,
                Point::new(r.x + rx, r.center().y),
                rx,
                r.h / 2.0,
                PI / 2.0,
                3.0 * PI / 2.0,
            );
            cr.close_path();
        }
    }
}

/// The visible inner edge of a database's top or a queue's right end.
fn cap_lines(cr: &cairo::Context, shape: ElementShape, r: &Rect) -> Result<(), cairo::Error> {
    if shape == ElementShape::Box {
        return Ok(());
    }
    cap_path(cr, shape, r);
    cr.stroke()
}

fn cap_path(cr: &cairo::Context, shape: ElementShape, r: &Rect) {
    match shape {
        ElementShape::Box => {}
        ElementShape::Database => {
            let ry = CAP_RADIUS.min(r.h / 4.0);
            cr.new_sub_path();
            ellipse_arc(
                cr,
                Point::new(r.center().x, r.y + ry),
                r.w / 2.0,
                ry,
                0.0,
                PI,
            );
        }
        ElementShape::Queue => {
            let rx = CAP_RADIUS.min(r.w / 4.0);
            cr.new_sub_path();
            ellipse_arc(
                cr,
                Point::new(r.x + r.w - rx, r.center().y),
                rx,
                r.h / 2.0,
                PI / 2.0,
                3.0 * PI / 2.0,
            );
        }
    }
}

fn ellipse_arc(cr: &cairo::Context, c: Point, rx: f64, ry: f64, from: f64, to: f64) {
    let matrix = cr.matrix();
    cr.translate(c.x, c.y);
    cr.scale(rx, ry);
    cr.arc(0.0, 0.0, 1.0, from, to);
    cr.set_matrix(matrix);
}

pub fn draw_frame(cr: &cairo::Context, rect: &Rect, colors: &Colors) -> Result<(), cairo::Error> {
    rounded_rect(cr, rect, 6.0);
    set_color(cr, &colors.fg.with_alpha(0.03));
    cr.fill_preserve()?;
    set_color(cr, &colors.fg.with_alpha(0.6));
    cr.set_line_width(1.0);
    cr.set_dash(&[7.0, 4.0], 0.0);
    let result = cr.stroke();
    cr.set_dash(&[], 0.0);
    result
}

pub fn draw_relation(
    cr: &cairo::Context,
    path: &[Point],
    bidirectional: bool,
    colors: &Colors,
    highlighted: bool,
) -> Result<(), cairo::Error> {
    let n = path.len();
    if n < 2 {
        return Ok(());
    }
    let color = if highlighted {
        colors.accent
    } else {
        colors.fg.with_alpha(0.8)
    };
    set_color(cr, &color);
    cr.set_line_width(if highlighted { 2.5 } else { 1.5 });
    cr.set_dash(&[6.0, 4.0], 0.0);
    cr.move_to(path[0].x, path[0].y);
    for p in &path[1..] {
        cr.line_to(p.x, p.y);
    }
    cr.stroke()?;
    cr.set_dash(&[], 0.0);
    arrow_head(cr, path[n - 1], path[n - 2], ARROW_LENGTH, ARROW_HALF_WIDTH);
    if bidirectional {
        arrow_head(cr, path[0], path[1], ARROW_LENGTH, ARROW_HALF_WIDTH);
    }
    cr.fill()
}

/// Palette tile outline for a preset.
pub fn preview_path(cr: &cairo::Context, preset: ElementPreset, r: &Rect) {
    if preset.kind == ElementKind::Person {
        let head = r.h * 0.2;
        circle(cr, Point::new(r.center().x, r.y + head), head);
        let body = Rect {
            y: r.y + head * 1.6,
            h: r.h - head * 1.6,
            ..*r
        };
        rounded_rect(cr, &body, body.h * 0.3);
    } else {
        shape_path(cr, preset.shape, r, 4.0);
        cap_path(cr, preset.shape, r);
    }
}
