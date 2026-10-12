//! Cairo drawing of mindmap nodes and branches, in canvas coordinates.

use std::f64::consts::PI;

use gtk::{cairo, gdk};

use crate::drawing::{Colors, circle, rgb, rounded_rect, set_color};
use crate::geometry::{Point, Rect};
use crate::mindmap::Shape;

/// Scallops around a cloud and spikes around a bang.
const CLOUD_BUMPS: usize = 12;
const BANG_SPIKES: usize = 16;

/// Color of the branch a node belongs to; each child of the root starts a
/// branch.
pub fn branch_color(branch: Option<usize>, colors: &Colors) -> gdk::RGBA {
    const PALETTE: [(u8, u8, u8); 6] = [
        (0x35, 0x84, 0xe4),
        (0x2e, 0xc2, 0x7e),
        (0xe6, 0x61, 0x00),
        (0x91, 0x41, 0xac),
        (0xe0, 0x1b, 0x24),
        (0x98, 0x6a, 0x44),
    ];
    match branch {
        Some(i) => {
            let (r, g, b) = PALETTE[i % PALETTE.len()];
            rgb(r, g, b)
        }
        None => colors.fg,
    }
}

/// Point on the ellipse inscribed in `r` at `angle`, scaled by `scale`.
fn on_ellipse(r: &Rect, angle: f64, scale: f64) -> Point {
    let c = r.center();
    Point::new(
        c.x + angle.cos() * r.w / 2.0 * scale,
        c.y + angle.sin() * r.h / 2.0 * scale,
    )
}

/// Outline of a node's shape; plain nodes are underlined instead.
pub fn shape_path(cr: &cairo::Context, shape: Shape, r: &Rect) {
    match shape {
        Shape::Default => {
            cr.move_to(r.x, r.y + r.h);
            cr.line_to(r.x + r.w, r.y + r.h);
        }
        Shape::Square => cr.rectangle(r.x, r.y, r.w, r.h),
        Shape::Rounded => rounded_rect(cr, r, r.h.min(24.0) / 2.5),
        Shape::Circle => circle(cr, r.center(), r.w.min(r.h) / 2.0),
        Shape::Hexagon => {
            let inset = r.h / 2.0;
            let c = r.center();
            cr.move_to(r.x, c.y);
            cr.line_to(r.x + inset, r.y);
            cr.line_to(r.x + r.w - inset, r.y);
            cr.line_to(r.x + r.w, c.y);
            cr.line_to(r.x + r.w - inset, r.y + r.h);
            cr.line_to(r.x + inset, r.y + r.h);
            cr.close_path();
        }
        Shape::Bang => {
            let n = 2 * BANG_SPIKES;
            for i in 0..n {
                let scale = if i % 2 == 0 { 1.0 } else { 0.75 };
                let p = on_ellipse(r, i as f64 * 2.0 * PI / n as f64, scale);
                if i == 0 {
                    cr.move_to(p.x, p.y);
                } else {
                    cr.line_to(p.x, p.y);
                }
            }
            cr.close_path();
        }
        Shape::Cloud => {
            // Half circles over each chord of an inner ellipse, bulging out.
            let points: Vec<Point> = (0..CLOUD_BUMPS)
                .map(|i| on_ellipse(r, i as f64 * 2.0 * PI / CLOUD_BUMPS as f64, 0.82))
                .collect();
            cr.new_sub_path();
            for (i, a) in points.iter().enumerate() {
                let b = points[(i + 1) % points.len()];
                let mid = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
                let radius = (b.x - a.x).hypot(b.y - a.y) / 2.0;
                let start = (a.y - mid.y).atan2(a.x - mid.x);
                cr.arc(mid.x, mid.y, radius, start, start + PI);
            }
            cr.close_path();
        }
    }
}

pub fn draw_node(
    cr: &cairo::Context,
    rect: &Rect,
    shape: Shape,
    color: &gdk::RGBA,
    colors: &Colors,
    highlighted: bool,
) -> Result<(), cairo::Error> {
    let stroke = if highlighted { &colors.accent } else { color };
    cr.set_line_width(if highlighted { 2.5 } else { 1.5 });
    shape_path(cr, shape, rect);
    if shape != Shape::Default {
        set_color(cr, &colors.node_fill);
        cr.fill_preserve()?;
        set_color(cr, &color.with_alpha(0.18));
        cr.fill_preserve()?;
    }
    set_color(cr, stroke);
    cr.stroke()
}

pub fn draw_branch(
    cr: &cairo::Context,
    path: &[Point],
    color: &gdk::RGBA,
    colors: &Colors,
    highlighted: bool,
) -> Result<(), cairo::Error> {
    let [first, .., last] = path else {
        return Ok(());
    };
    set_color(cr, if highlighted { &colors.accent } else { color });
    cr.set_line_width(if highlighted { 3.0 } else { 2.0 });
    // A horizontal S-curve, like the branches of a hand-drawn mindmap.
    let mid_x = (first.x + last.x) / 2.0;
    cr.move_to(first.x, first.y);
    cr.curve_to(mid_x, first.y, mid_x, last.y, last.x, last.y);
    cr.stroke()
}
