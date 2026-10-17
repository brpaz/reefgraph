//! Interaction pieces shared by the diagram canvases: viewport navigation and
//! in-place label editing.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gdk, glib};

use crate::canvas::viewport::Viewport;
use crate::shared::geometry::Rect;

const ZOOM_STEP: f64 = 1.2;
const SCROLL_STEP: f64 = 30.0;
const FIT_MARGIN: f64 = 40.0;

/// A canvas widget with a pannable, zoomable viewport.
pub trait Navigable: IsA<gtk::Widget> {
    fn viewport(&self) -> &Cell<Viewport>;
    /// Last pointer position in widget coordinates, used as the zoom anchor.
    fn pointer(&self) -> &Cell<(f64, f64)>;
    /// Set when a fit was requested before the widget had a size.
    fn fit_pending(&self) -> &Cell<bool>;

    fn widget_size(&self) -> (f64, f64) {
        (self.width() as f64, self.height() as f64)
    }

    fn update_viewport(&self, f: impl FnOnce(&mut Viewport)) {
        let mut viewport = self.viewport().get();
        f(&mut viewport);
        self.viewport().set(viewport);
        self.queue_draw();
    }

    fn zoom_in(&self) {
        let (w, h) = self.widget_size();
        self.update_viewport(|v| v.zoom_around((w / 2.0, h / 2.0), ZOOM_STEP));
    }

    fn zoom_out(&self) {
        let (w, h) = self.widget_size();
        self.update_viewport(|v| v.zoom_around((w / 2.0, h / 2.0), 1.0 / ZOOM_STEP));
    }

    fn reset_zoom(&self) {
        let (w, h) = self.widget_size();
        self.update_viewport(|v| v.zoom_around((w / 2.0, h / 2.0), 1.0 / v.zoom));
    }

    /// Fits `bounds` into view, or resets the view when there is no content.
    /// Deferred until the widget has been allocated a size.
    fn fit_bounds(&self, bounds: Option<Rect>) {
        let size = self.widget_size();
        if size.0 <= 0.0 || size.1 <= 0.0 {
            self.fit_pending().set(true);
            return;
        }
        self.update_viewport(|v| match bounds {
            Some(b) => v.fit(b.inflate(FIT_MARGIN), size),
            None => *v = Viewport::default(),
        });
    }
}

/// Adds scroll to pan, Ctrl+scroll to zoom and middle-button drag to pan.
pub fn install_navigation<W: Navigable>(widget: &W) {
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    let weak = widget.downgrade();
    scroll.connect_scroll(move |controller, dx, dy| {
        let Some(widget) = weak.upgrade() else {
            return glib::Propagation::Proceed;
        };
        let zoom = controller
            .current_event_state()
            .contains(gdk::ModifierType::CONTROL_MASK);
        let pointer = widget.pointer().get();
        widget.update_viewport(|v| {
            if zoom {
                v.zoom_around(pointer, if dy < 0.0 { ZOOM_STEP } else { 1.0 / ZOOM_STEP });
            } else {
                v.pan(-dx * SCROLL_STEP, -dy * SCROLL_STEP);
            }
        });
        glib::Propagation::Stop
    });
    widget.add_controller(scroll);

    let pan = gtk::GestureDrag::builder()
        .button(gdk::BUTTON_MIDDLE)
        .build();
    let origin = Rc::new(Cell::new((0.0, 0.0)));
    let weak = widget.downgrade();
    pan.connect_drag_begin(glib::clone!(
        #[strong]
        origin,
        move |_, _, _| {
            if let Some(widget) = weak.upgrade() {
                origin.set(widget.viewport().get().offset);
            }
        }
    ));
    let weak = widget.downgrade();
    pan.connect_drag_update(move |_, dx, dy| {
        if let Some(widget) = weak.upgrade() {
            let (ox, oy) = origin.get();
            widget.update_viewport(|v| v.offset = (ox + dx, oy + dy));
        }
    });
    widget.add_controller(pan);

    let motion = gtk::EventControllerMotion::new();
    let weak = widget.downgrade();
    motion.connect_motion(move |_, x, y| {
        if let Some(widget) = weak.upgrade() {
            widget.pointer().set((x, y));
        }
    });
    widget.add_controller(motion);
}

/// Popover with an entry for editing a label in place.
#[derive(Debug)]
pub struct InlineEditor {
    popover: gtk::Popover,
    entry: gtk::Entry,
}

impl InlineEditor {
    /// `on_commit` runs with the entered text when Enter is pressed;
    /// `on_closed` runs whenever the popover closes, committed or not.
    pub fn new(
        parent: &impl IsA<gtk::Widget>,
        on_commit: impl Fn(&str) + 'static,
        on_closed: impl Fn() + 'static,
    ) -> Self {
        let entry = gtk::Entry::builder().width_chars(18).build();
        let popover = gtk::Popover::builder().child(&entry).build();
        popover.set_parent(parent);
        entry.connect_activate(glib::clone!(
            #[weak]
            popover,
            move |entry| {
                on_commit(&entry.text());
                popover.popdown();
            }
        ));
        popover.connect_closed(move |_| on_closed());
        Self { popover, entry }
    }

    /// Opens the editor pointing at widget position `at`.
    pub fn open(&self, at: (f64, f64), text: &str) {
        self.popover
            .set_pointing_to(Some(&gdk::Rectangle::new(at.0 as i32, at.1 as i32, 1, 1)));
        self.entry.set_text(text);
        self.popover.popup();
        self.entry.grab_focus();
        self.entry.select_region(0, -1);
    }

    /// Must be called from the parent's `size_allocate`.
    pub fn present(&self) {
        self.popover.present();
    }

    /// Must be called from the parent's `dispose`.
    pub fn unparent(&self) {
        self.popover.unparent();
    }
}
