//! Cairo drawing of entity boxes and relationships, in canvas coordinates.

use gtk::cairo;

use crate::canvas::drawing::{Colors, circle, rounded_rect, set_color};
use crate::diagrams::erd::Cardinality;
use crate::diagrams::erd::layout::EntityBox;
use crate::shared::geometry::{Point, Rect};

/// Half length of cardinality bars and crow's feet across the line.
const MARKER_SPREAD: f64 = 7.0;
const MARKER_CIRCLE: f64 = 5.0;

pub fn draw_entity(
    cr: &cairo::Context,
    rect: &Rect,
    layout: &EntityBox,
    rows: usize,
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
    set_color(cr, &colors.fg.with_alpha(0.035));
    for row in (1..rows).step_by(2) {
        let y = rect.y + layout.header_height + row as f64 * layout.row_height;
        cr.rectangle(rect.x, y, rect.w, layout.row_height);
    }
    cr.fill()?;
    cr.restore()?;

    set_color(cr, &colors.fg.with_alpha(0.5));
    cr.set_line_width(1.0);
    let y = rect.y + layout.header_height;
    cr.move_to(rect.x, y);
    cr.line_to(rect.x + rect.w, y);
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

pub fn draw_relationship(
    cr: &cairo::Context,
    path: &[Point],
    from: Cardinality,
    to: Cardinality,
    identifying: bool,
    colors: &Colors,
    highlighted: bool,
) -> Result<(), cairo::Error> {
    let n = path.len();
    if n < 2 {
        return Ok(());
    }
    set_color(
        cr,
        if highlighted {
            &colors.accent
        } else {
            &colors.fg
        },
    );
    cr.set_line_width(if highlighted { 2.5 } else { 1.5 });
    if !identifying {
        cr.set_dash(&[6.0, 4.0], 0.0);
    }
    cr.move_to(path[0].x, path[0].y);
    for p in &path[1..] {
        cr.line_to(p.x, p.y);
    }
    cr.stroke()?;
    cr.set_dash(&[], 0.0);
    draw_cardinality(cr, from, path[0], path[1], colors)?;
    draw_cardinality(cr, to, path[n - 1], path[n - 2], colors)
}

/// Draws crow's-foot notation where a relationship meets an entity at `at`,
/// the line continuing towards `toward`.
fn draw_cardinality(
    cr: &cairo::Context,
    cardinality: Cardinality,
    at: Point,
    toward: Point,
    colors: &Colors,
) -> Result<(), cairo::Error> {
    let (dx, dy) = (toward.x - at.x, toward.y - at.y);
    let len = dx.hypot(dy);
    if len == 0.0 {
        return Ok(());
    }
    let (ux, uy) = (dx / len, dy / len);
    let (nx, ny) = (-uy, ux);
    // Point `along` the line from `at`, shifted `across` it.
    let p = |along: f64, across: f64| {
        Point::new(
            at.x + ux * along + nx * across,
            at.y + uy * along + ny * across,
        )
    };
    let bar = |along: f64| {
        let (a, b) = (p(along, -MARKER_SPREAD), p(along, MARKER_SPREAD));
        cr.move_to(a.x, a.y);
        cr.line_to(b.x, b.y);
    };
    let crow = || {
        let tip = p(14.0, 0.0);
        for across in [-MARKER_SPREAD, 0.0, MARKER_SPREAD] {
            let end = p(0.0, across);
            cr.move_to(tip.x, tip.y);
            cr.line_to(end.x, end.y);
        }
    };
    let (many, optional) = match cardinality {
        Cardinality::ExactlyOne => {
            bar(8.0);
            bar(13.0);
            (false, false)
        }
        Cardinality::ZeroOrOne => {
            bar(8.0);
            (false, true)
        }
        Cardinality::OneOrMore => {
            crow();
            bar(18.0);
            (true, false)
        }
        Cardinality::ZeroOrMore => {
            crow();
            (true, true)
        }
    };
    cr.stroke()?;
    if optional {
        let c = p(if many { 21.0 } else { 18.0 }, 0.0);
        circle(cr, c, MARKER_CIRCLE);
        set_color(cr, &colors.node_fill);
        cr.fill_preserve()?;
        set_color(cr, &colors.fg);
        cr.stroke()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::gdk;

    #[test]
    fn every_cardinality_paints_something() {
        let colors = Colors::new(gdk::RGBA::BLACK, false);
        for c in Cardinality::ALL {
            let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 60, 30).unwrap();
            {
                let cr = cairo::Context::new(&surface).unwrap();
                draw_cardinality(
                    &cr,
                    c,
                    Point::new(5.0, 15.0),
                    Point::new(55.0, 15.0),
                    &colors,
                )
                .unwrap();
            }
            let painted = surface
                .data()
                .unwrap()
                .chunks(4)
                .filter(|px| px[3] != 0)
                .count();
            assert!(painted > 20, "{c:?} drew nothing");
        }
    }
}
