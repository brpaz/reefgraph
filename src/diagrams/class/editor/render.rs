//! Cairo drawing of class boxes and relations, in canvas coordinates.

use gtk::cairo;

use crate::canvas::drawing::{Colors, arrow_head, circle, rounded_rect, set_color};
use crate::diagrams::class::Head;
use crate::diagrams::class::layout::ClassBox;
use crate::shared::geometry::{Point, Rect};

const TRIANGLE: (f64, f64) = (14.0, 8.0);
const DIAMOND: (f64, f64) = (18.0, 6.0);
const ARROW: (f64, f64) = (10.0, 5.0);
const LOLLIPOP_RADIUS: f64 = 6.0;
/// Where cardinalities sit: along the relation from its end, and beside it.
const CARDINALITY_OFFSET: (f64, f64) = (20.0, 11.0);

pub fn draw_class(
    cr: &cairo::Context,
    rect: &Rect,
    layout: &ClassBox,
    colors: &Colors,
    highlighted: bool,
) -> Result<(), cairo::Error> {
    rounded_rect(cr, rect, 4.0);
    set_color(cr, &colors.node_fill);
    cr.fill_preserve()?;
    cr.save()?;
    cr.clip();
    set_color(cr, &colors.fg.with_alpha(0.08));
    cr.rectangle(rect.x, rect.y, rect.w, layout.header_height);
    cr.fill()?;
    cr.restore()?;

    set_color(cr, &colors.fg.with_alpha(0.5));
    cr.set_line_width(1.0);
    for y in [layout.header_height, layout.divider()] {
        cr.move_to(rect.x, rect.y + y);
        cr.line_to(rect.x + rect.w, rect.y + y);
    }
    cr.stroke()?;

    rounded_rect(cr, rect, 4.0);
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

/// Unit vector from `from` to `to`, if they differ.
fn direction(from: Point, to: Point) -> Option<(f64, f64)> {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let len = dx.hypot(dy);
    (len > 0.0).then(|| (dx / len, dy / len))
}

/// How far a head reaches back along the line, so the line stops at it.
fn head_length(head: Head) -> f64 {
    match head {
        Head::None | Head::Arrow => 0.0,
        Head::Inheritance => TRIANGLE.0,
        Head::Composition | Head::Aggregation => DIAMOND.0,
        Head::Lollipop => 2.0 * LOLLIPOP_RADIUS,
    }
}

pub fn draw_relation(
    cr: &cairo::Context,
    path: &[Point],
    from: Head,
    to: Head,
    dashed: bool,
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
    let mut points = path.to_vec();
    for (end, next, head) in [(0, 1, from), (n - 1, n - 2, to)] {
        if let Some((ux, uy)) = direction(path[end], path[next]) {
            let back = head_length(head);
            points[end] = Point::new(path[end].x + ux * back, path[end].y + uy * back);
        }
    }

    set_color(cr, color);
    cr.set_line_width(width);
    if dashed {
        cr.set_dash(&[6.0, 4.0], 0.0);
    }
    cr.move_to(points[0].x, points[0].y);
    for p in &points[1..] {
        cr.line_to(p.x, p.y);
    }
    cr.stroke()?;
    cr.set_dash(&[], 0.0);

    draw_head(cr, from, path[0], path[1], colors, color, width)?;
    draw_head(cr, to, path[n - 1], path[n - 2], colors, color, width)
}

/// Draws a head whose tip touches the class outline at `tip`, the line
/// continuing towards `toward`.
fn draw_head(
    cr: &cairo::Context,
    head: Head,
    tip: Point,
    toward: Point,
    colors: &Colors,
    color: &gtk::gdk::RGBA,
    width: f64,
) -> Result<(), cairo::Error> {
    let Some((ux, uy)) = direction(tip, toward) else {
        return Ok(());
    };
    let (nx, ny) = (-uy, ux);
    let p = |along: f64, across: f64| {
        Point::new(
            tip.x + ux * along + nx * across,
            tip.y + uy * along + ny * across,
        )
    };
    cr.set_line_width(width);
    let fill_and_stroke = |cr: &cairo::Context, fill: &gtk::gdk::RGBA| {
        set_color(cr, fill);
        cr.fill_preserve()?;
        set_color(cr, color);
        cr.stroke()
    };
    match head {
        Head::None => Ok(()),
        Head::Arrow => {
            arrow_head(cr, tip, p(ARROW.0, 0.0), ARROW.0, ARROW.1);
            set_color(cr, color);
            cr.fill()
        }
        Head::Inheritance => {
            let (a, b) = (p(TRIANGLE.0, -TRIANGLE.1), p(TRIANGLE.0, TRIANGLE.1));
            cr.move_to(tip.x, tip.y);
            cr.line_to(a.x, a.y);
            cr.line_to(b.x, b.y);
            cr.close_path();
            fill_and_stroke(cr, &colors.node_fill)
        }
        Head::Composition | Head::Aggregation => {
            let (side_a, back, side_b) = (
                p(DIAMOND.0 / 2.0, -DIAMOND.1),
                p(DIAMOND.0, 0.0),
                p(DIAMOND.0 / 2.0, DIAMOND.1),
            );
            cr.move_to(tip.x, tip.y);
            cr.line_to(side_a.x, side_a.y);
            cr.line_to(back.x, back.y);
            cr.line_to(side_b.x, side_b.y);
            cr.close_path();
            let fill = if head == Head::Composition {
                *color
            } else {
                colors.node_fill
            };
            fill_and_stroke(cr, &fill)
        }
        Head::Lollipop => {
            circle(cr, p(LOLLIPOP_RADIUS, 0.0), LOLLIPOP_RADIUS);
            fill_and_stroke(cr, &colors.node_fill)
        }
    }
}

/// Where the cardinality text for a relation end at `at` is centered.
pub fn cardinality_position(at: Point, toward: Point) -> Point {
    let (ux, uy) = direction(at, toward).unwrap_or((0.0, 1.0));
    let (along, across) = CARDINALITY_OFFSET;
    Point::new(
        at.x + ux * along - uy * across,
        at.y + uy * along + ux * across,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::gdk;

    #[test]
    fn every_head_paints_something() {
        let colors = Colors::new(gdk::RGBA::BLACK, false);
        for head in Head::ALL.into_iter().filter(|h| *h != Head::None) {
            let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 60, 30).unwrap();
            {
                let cr = cairo::Context::new(&surface).unwrap();
                draw_head(
                    &cr,
                    head,
                    Point::new(5.0, 15.0),
                    Point::new(55.0, 15.0),
                    &colors,
                    &colors.fg,
                    1.5,
                )
                .unwrap();
            }
            let painted = surface
                .data()
                .unwrap()
                .chunks(4)
                .filter(|px| px[3] != 0)
                .count();
            assert!(painted > 20, "{head:?} drew nothing");
        }
    }

    #[test]
    fn cardinality_sits_beside_the_line() {
        let at = cardinality_position(Point::new(0.0, 0.0), Point::new(100.0, 0.0));
        assert_eq!(at, Point::new(CARDINALITY_OFFSET.0, CARDINALITY_OFFSET.1));
    }
}
