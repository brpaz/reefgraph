//! Drawing helpers shared by the diagram canvases.

use std::f64::consts::{FRAC_PI_2, PI};

use gtk::prelude::*;
use gtk::{cairo, gdk, graphene, pango};

use crate::geometry::{Point, Rect};
use crate::viewport::Viewport;

const GRID_SPACING: f64 = 20.0;
/// Width at which labels wrap, in canvas units.
const MAX_LABEL_WIDTH: i32 = 200;
/// Screen-space radius of the add-row button.
pub const ADD_BUTTON_RADIUS: f64 = 9.0;

#[derive(Debug, Clone, Copy)]
pub struct Colors {
    pub fg: gdk::RGBA,
    pub node_fill: gdk::RGBA,
    pub accent: gdk::RGBA,
    pub grid: gdk::RGBA,
}

impl Colors {
    pub fn new(fg: gdk::RGBA, dark: bool) -> Self {
        let (node_fill, accent) = if dark {
            (rgb(0x30, 0x30, 0x34), rgb(0x78, 0xae, 0xed))
        } else {
            (rgb(0xfa, 0xfa, 0xfb), rgb(0x1c, 0x71, 0xd8))
        };
        Self {
            fg,
            node_fill,
            accent,
            grid: fg.with_alpha(0.15),
        }
    }
}

pub fn rgb(r: u8, g: u8, b: u8) -> gdk::RGBA {
    gdk::RGBA::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
}

pub fn set_color(cr: &cairo::Context, c: &gdk::RGBA) {
    cr.set_source_rgba(
        c.red() as f64,
        c.green() as f64,
        c.blue() as f64,
        c.alpha() as f64,
    );
}

pub fn draw_grid(cr: &cairo::Context, visible: &Rect, zoom: f64, colors: &Colors) {
    if GRID_SPACING * zoom < 8.0 {
        return;
    }
    set_color(cr, &colors.grid);
    let radius = 1.0 / zoom;
    let mut x = (visible.x / GRID_SPACING).floor() * GRID_SPACING;
    while x <= visible.x + visible.w {
        let mut y = (visible.y / GRID_SPACING).floor() * GRID_SPACING;
        while y <= visible.y + visible.h {
            cr.rectangle(x - radius / 2.0, y - radius / 2.0, radius, radius);
            y += GRID_SPACING;
        }
        x += GRID_SPACING;
    }
    let _ = cr.fill();
}

pub fn rounded_rect(cr: &cairo::Context, rect: &Rect, radius: f64) {
    let Rect { x, y, w, h } = *rect;
    let r = radius.min(w / 2.0).min(h / 2.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, FRAC_PI_2, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * FRAC_PI_2);
    cr.close_path();
}

pub fn draw_label_background(
    cr: &cairo::Context,
    rect: &Rect,
    colors: &Colors,
) -> Result<(), cairo::Error> {
    rounded_rect(cr, rect, 4.0);
    set_color(cr, &colors.node_fill);
    cr.fill()
}

/// Centered, wrapping text layout for a label.
pub fn label_layout(widget: &impl IsA<gtk::Widget>, text: &str) -> pango::Layout {
    let layout = widget.create_pango_layout(Some(text));
    layout.set_width(MAX_LABEL_WIDTH * pango::SCALE);
    layout.set_wrap(pango::WrapMode::WordChar);
    layout.set_alignment(pango::Alignment::Center);
    layout
}

/// Label queued for drawing on top of a canvas's geometry.
#[derive(Debug, Clone)]
pub struct Text {
    pub text: String,
    pub at: Point,
    pub color: gdk::RGBA,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Text {
    pub fn plain(text: impl Into<String>, at: Point, color: gdk::RGBA) -> Self {
        Self {
            text: text.into(),
            at,
            color,
            bold: false,
            italic: false,
            underline: false,
        }
    }

    /// Appends the label, centered on its canvas point.
    pub fn draw(
        &self,
        widget: &impl IsA<gtk::Widget>,
        snapshot: &gtk::Snapshot,
        viewport: &Viewport,
    ) {
        if self.text.is_empty() {
            return;
        }
        let layout = label_layout(widget, &self.text);
        apply_style(&layout, self.bold, self.italic, self.underline);
        draw_text(snapshot, viewport, &layout, self.at, &self.color);
    }
}

/// Applies font styles to the whole text of `layout`.
pub fn apply_style(layout: &pango::Layout, bold: bool, italic: bool, underline: bool) {
    if !(bold || italic || underline) {
        return;
    }
    let attrs = pango::AttrList::new();
    if bold {
        attrs.insert(pango::AttrInt::new_weight(pango::Weight::Bold));
    }
    if italic {
        attrs.insert(pango::AttrInt::new_style(pango::Style::Italic));
    }
    if underline {
        attrs.insert(pango::AttrInt::new_underline(pango::Underline::Single));
    }
    layout.set_attributes(Some(&attrs));
}

pub fn layout_size(layout: &pango::Layout) -> (f64, f64) {
    let (_, logical) = layout.pixel_extents();
    (logical.width() as f64, logical.height() as f64)
}

pub fn text_size(widget: &impl IsA<gtk::Widget>, text: &str) -> (f64, f64) {
    layout_size(&label_layout(widget, text))
}

/// Appends `layout` centered on the canvas point `center`.
pub fn draw_text(
    snapshot: &gtk::Snapshot,
    viewport: &Viewport,
    layout: &pango::Layout,
    center: Point,
    color: &gdk::RGBA,
) {
    let (sx, sy) = viewport.to_screen(center);
    let (_, logical) = layout.pixel_extents();
    snapshot.save();
    snapshot.translate(&graphene::Point::new(sx as f32, sy as f32));
    snapshot.scale(viewport.zoom as f32, viewport.zoom as f32);
    snapshot.translate(&graphene::Point::new(
        -(logical.x() as f32 + logical.width() as f32 / 2.0),
        -(logical.y() as f32 + logical.height() as f32 / 2.0),
    ));
    snapshot.append_layout(layout, color);
    snapshot.restore();
}

/// Cairo context covering the whole widget, transformed to canvas coordinates.
/// Canvases draw all geometry into this single layer: several cairo nodes per
/// frame do not render on every GSK backend (Broadway drops all but the first).
pub fn world_context(
    snapshot: &gtk::Snapshot,
    viewport: &Viewport,
    (w, h): (f64, f64),
) -> cairo::Context {
    let cr = snapshot.append_cairo(&graphene::Rect::new(0.0, 0.0, w as f32, h as f32));
    cr.translate(viewport.offset.0, viewport.offset.1);
    cr.scale(viewport.zoom, viewport.zoom);
    cr
}

pub fn log_draw_error(result: Result<(), cairo::Error>) {
    if let Err(err) = result {
        tracing::warn!(error = %err, "failed to draw diagram element");
    }
}

/// Filled arrow head with its tip at `tip`, the line arriving from `from`.
pub fn arrow_head(cr: &cairo::Context, tip: Point, from: Point, length: f64, half_width: f64) {
    let (dx, dy) = (tip.x - from.x, tip.y - from.y);
    let len = dx.hypot(dy);
    if len == 0.0 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let base = Point::new(tip.x - ux * length, tip.y - uy * length);
    cr.move_to(tip.x, tip.y);
    cr.line_to(base.x - uy * half_width, base.y + ux * half_width);
    cr.line_to(base.x + uy * half_width, base.y - ux * half_width);
    cr.close_path();
}

pub fn circle(cr: &cairo::Context, c: Point, r: f64) {
    cr.new_sub_path();
    cr.arc(c.x, c.y, r, 0.0, 2.0 * PI);
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

/// Round "+" button shown under a selected box for adding a row to it.
pub fn draw_add_button(
    cr: &cairo::Context,
    center: Point,
    zoom: f64,
    colors: &Colors,
) -> Result<(), cairo::Error> {
    let r = ADD_BUTTON_RADIUS / zoom;
    circle(cr, center, r);
    set_color(cr, &colors.accent);
    cr.fill()?;
    set_color(cr, &colors.node_fill);
    cr.set_line_width(2.0 / zoom);
    let s = r * 0.5;
    cr.move_to(center.x - s, center.y);
    cr.line_to(center.x + s, center.y);
    cr.move_to(center.x, center.y - s);
    cr.line_to(center.x, center.y + s);
    cr.stroke()
}
