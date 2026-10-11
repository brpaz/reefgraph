//! Cairo drawing of shapes and edges, in canvas (world) coordinates.

use std::f64::consts::PI;

use gtk::cairo;

use crate::drawing::{Colors, rounded_rect, set_color};
use crate::flowchart::{Edge, Head, NodeShape, Point, Stroke};
use crate::geometry::Rect;

const ARROW_LENGTH: f64 = 10.0;
const ARROW_HALF_WIDTH: f64 = 5.0;
const MARKER_RADIUS: f64 = 4.0;

pub fn draw_node(
    cr: &cairo::Context,
    shape: NodeShape,
    rect: &Rect,
    colors: &Colors,
    highlighted: bool,
) -> Result<(), cairo::Error> {
    shape_path(cr, shape, rect)?;
    set_color(cr, &colors.node_fill);
    cr.fill_preserve()?;
    set_color(
        cr,
        if highlighted {
            &colors.accent
        } else {
            &colors.fg
        },
    );
    cr.set_line_width(if highlighted { 2.5 } else { 1.5 });
    cr.stroke()?;

    // Inner details drawn on top of the outline.
    let Rect { x, y, w, h } = *rect;
    match shape {
        NodeShape::Subroutine => {
            for lx in [x + 8.0, x + w - 8.0] {
                cr.move_to(lx, y);
                cr.line_to(lx, y + h);
            }
            cr.stroke()?;
        }
        NodeShape::DoubleCircle => {
            ellipse(cr, rect.center(), w / 2.0 - 5.0, h / 2.0 - 5.0)?;
            cr.stroke()?;
        }
        NodeShape::Cylinder => {
            let ry = cylinder_ry(rect);
            elliptic_arc(
                cr,
                Point::new(x + w / 2.0, y + ry),
                w / 2.0,
                ry,
                0.0,
                PI,
                false,
            )?;
            cr.stroke()?;
        }
        _ => {}
    }
    Ok(())
}

fn cylinder_ry(rect: &Rect) -> f64 {
    (rect.h * 0.12).min(10.0)
}

pub fn shape_path(cr: &cairo::Context, shape: NodeShape, rect: &Rect) -> Result<(), cairo::Error> {
    let Rect { x, y, w, h } = *rect;
    cr.new_path();
    match shape {
        NodeShape::Rectangle | NodeShape::Subroutine => cr.rectangle(x, y, w, h),
        NodeShape::Rounded => rounded_rect(cr, rect, 8.0),
        NodeShape::Stadium => rounded_rect(cr, rect, h / 2.0),
        NodeShape::Circle | NodeShape::DoubleCircle => {
            ellipse(cr, rect.center(), w / 2.0, h / 2.0)?
        }
        NodeShape::Cylinder => {
            let ry = cylinder_ry(rect);
            let (cx, rx) = (x + w / 2.0, w / 2.0);
            cr.move_to(x, y + ry);
            cr.line_to(x, y + h - ry);
            elliptic_arc(cr, Point::new(cx, y + h - ry), rx, ry, PI, 0.0, true)?;
            cr.line_to(x + w, y + ry);
            elliptic_arc(cr, Point::new(cx, y + ry), rx, ry, 0.0, -PI, true)?;
            cr.close_path();
        }
        NodeShape::Rhombus => polygon(
            cr,
            &[
                (x + w / 2.0, y),
                (x + w, y + h / 2.0),
                (x + w / 2.0, y + h),
                (x, y + h / 2.0),
            ],
        ),
        NodeShape::Hexagon => {
            let i = h / 4.0;
            polygon(
                cr,
                &[
                    (x + i, y),
                    (x + w - i, y),
                    (x + w, y + h / 2.0),
                    (x + w - i, y + h),
                    (x + i, y + h),
                    (x, y + h / 2.0),
                ],
            )
        }
        NodeShape::Parallelogram => {
            let i = h / 3.0;
            polygon(
                cr,
                &[(x + i, y), (x + w, y), (x + w - i, y + h), (x, y + h)],
            )
        }
        NodeShape::ParallelogramAlt => {
            let i = h / 3.0;
            polygon(
                cr,
                &[(x, y), (x + w - i, y), (x + w, y + h), (x + i, y + h)],
            )
        }
        NodeShape::Trapezoid => {
            let i = h / 3.0;
            polygon(
                cr,
                &[(x + i, y), (x + w - i, y), (x + w, y + h), (x, y + h)],
            )
        }
        NodeShape::TrapezoidAlt => {
            let i = h / 3.0;
            polygon(
                cr,
                &[(x, y), (x + w, y), (x + w - i, y + h), (x + i, y + h)],
            )
        }
        NodeShape::Asymmetric => {
            let i = h / 3.0;
            polygon(
                cr,
                &[
                    (x, y),
                    (x + w, y),
                    (x + w, y + h),
                    (x, y + h),
                    (x + i, y + h / 2.0),
                ],
            )
        }
    }
    Ok(())
}

fn polygon(cr: &cairo::Context, points: &[(f64, f64)]) {
    for (i, &(px, py)) in points.iter().enumerate() {
        if i == 0 {
            cr.move_to(px, py);
        } else {
            cr.line_to(px, py);
        }
    }
    cr.close_path();
}

fn ellipse(cr: &cairo::Context, c: Point, rx: f64, ry: f64) -> Result<(), cairo::Error> {
    cr.new_sub_path();
    elliptic_arc(cr, c, rx, ry, 0.0, 2.0 * PI, false)?;
    cr.close_path();
    Ok(())
}

fn elliptic_arc(
    cr: &cairo::Context,
    c: Point,
    rx: f64,
    ry: f64,
    from: f64,
    to: f64,
    negative: bool,
) -> Result<(), cairo::Error> {
    cr.save()?;
    cr.translate(c.x, c.y);
    cr.scale(rx.max(1e-3), ry.max(1e-3));
    if negative {
        cr.arc_negative(0.0, 0.0, 1.0, from, to);
    } else {
        cr.arc(0.0, 0.0, 1.0, from, to);
    }
    cr.restore()
}

pub fn draw_edge(
    cr: &cairo::Context,
    edge: &Edge,
    path: &[Point],
    colors: &Colors,
    highlighted: bool,
) -> Result<(), cairo::Error> {
    let [first, .., last] = path else {
        return Ok(());
    };
    let color = if highlighted {
        &colors.accent
    } else {
        &colors.fg
    };
    set_color(cr, color);
    let width = match edge.stroke {
        Stroke::Thick => 3.5,
        _ => 1.5,
    } + if highlighted { 1.0 } else { 0.0 };
    cr.set_line_width(width);
    if edge.stroke == Stroke::Dotted {
        cr.set_dash(&[4.0, 4.0], 0.0);
    }

    // Shorten the line under arrow heads so it does not poke through the tip.
    let mut points = path.to_vec();
    if edge.start == Head::Arrow {
        points[0] = towards(*first, path[1], ARROW_LENGTH * 0.8);
    }
    if edge.end == Head::Arrow {
        let n = points.len();
        points[n - 1] = towards(*last, path[n - 2], ARROW_LENGTH * 0.8);
    }
    cr.move_to(points[0].x, points[0].y);
    for p in &points[1..] {
        cr.line_to(p.x, p.y);
    }
    cr.stroke()?;
    cr.set_dash(&[], 0.0);

    draw_head(cr, edge.start, *first, path[1], width)?;
    draw_head(cr, edge.end, *last, path[path.len() - 2], width)?;
    Ok(())
}

/// Point at `distance` from `from` in the direction of `to`.
fn towards(from: Point, to: Point, distance: f64) -> Point {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let len = dx.hypot(dy);
    if len == 0.0 {
        return from;
    }
    Point::new(from.x + dx / len * distance, from.y + dy / len * distance)
}

/// Draws a head whose tip is at `tip`, the edge arriving from `from`.
fn draw_head(
    cr: &cairo::Context,
    head: Head,
    tip: Point,
    from: Point,
    width: f64,
) -> Result<(), cairo::Error> {
    let (dx, dy) = (tip.x - from.x, tip.y - from.y);
    let len = dx.hypot(dy);
    if len == 0.0 {
        return Ok(());
    }
    let (ux, uy) = (dx / len, dy / len);
    match head {
        Head::None => {}
        Head::Arrow => {
            let base = Point::new(tip.x - ux * ARROW_LENGTH, tip.y - uy * ARROW_LENGTH);
            cr.move_to(tip.x, tip.y);
            cr.line_to(
                base.x - uy * ARROW_HALF_WIDTH,
                base.y + ux * ARROW_HALF_WIDTH,
            );
            cr.line_to(
                base.x + uy * ARROW_HALF_WIDTH,
                base.y - ux * ARROW_HALF_WIDTH,
            );
            cr.close_path();
            cr.fill()?;
        }
        Head::Circle => {
            cr.new_sub_path();
            cr.arc(
                tip.x - ux * MARKER_RADIUS,
                tip.y - uy * MARKER_RADIUS,
                MARKER_RADIUS,
                0.0,
                2.0 * PI,
            );
            cr.fill()?;
        }
        Head::Cross => {
            let c = Point::new(tip.x - ux * MARKER_RADIUS, tip.y - uy * MARKER_RADIUS);
            let s = MARKER_RADIUS;
            cr.set_line_width(width.max(2.0));
            cr.move_to(c.x - s, c.y - s);
            cr.line_to(c.x + s, c.y + s);
            cr.move_to(c.x - s, c.y + s);
            cr.line_to(c.x + s, c.y - s);
            cr.stroke()?;
        }
    }
    Ok(())
}

pub fn draw_ports(
    cr: &cairo::Context,
    ports: &[Point],
    radius: f64,
    colors: &Colors,
) -> Result<(), cairo::Error> {
    for p in ports {
        cr.new_sub_path();
        cr.arc(p.x, p.y, radius, 0.0, 2.0 * PI);
    }
    set_color(cr, &colors.node_fill);
    cr.fill_preserve()?;
    set_color(cr, &colors.accent);
    cr.set_line_width(radius / 3.0);
    cr.stroke()
}

pub fn draw_rubber_band(
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::gdk;

    fn painted_pixels(shape: NodeShape) -> usize {
        let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 120, 80).unwrap();
        {
            let cr = cairo::Context::new(&surface).unwrap();
            let colors = Colors::new(gdk::RGBA::BLACK, false);
            let rect = Rect::centered(Point::new(60.0, 40.0), (100.0, 60.0));
            draw_node(&cr, shape, &rect, &colors, false).unwrap();
        }
        surface
            .data()
            .unwrap()
            .chunks(4)
            .filter(|px| px[3] != 0)
            .count()
    }

    #[test]
    fn every_shape_paints_something() {
        for shape in NodeShape::ALL {
            assert!(painted_pixels(shape) > 500, "{shape:?} drew nothing");
        }
    }
}
