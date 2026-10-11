//! Interactive diagram canvas: renders a [`Flowchart`] and edits it through
//! direct manipulation (move, connect, drop, inline label editing).

mod geometry;
mod history;
mod render;

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::OnceLock;

use gtk::glib::subclass::Signal;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, pango};

use self::history::History;
use self::render::Colors;
use crate::flowchart::model::RenameError;
use crate::flowchart::{self, Direction, Flowchart, Node, NodeShape, Point};

pub use self::geometry::{Rect, shape_size};
pub use self::render::shape_path;

const MIN_ZOOM: f64 = 0.2;
const MAX_ZOOM: f64 = 4.0;
const ZOOM_STEP: f64 = 1.2;
const SCROLL_STEP: f64 = 30.0;
const MAX_LABEL_WIDTH: i32 = 200;
/// Screen-space radius of connection handles and their hit area.
const PORT_RADIUS: f64 = 5.0;
const PORT_HIT_RADIUS: f64 = 10.0;
const EDGE_HIT_DISTANCE: f64 = 6.0;
const PARALLEL_EDGE_SPACING: f64 = 24.0;
/// Detour offsets tried, in order, to route an edge around nodes in its way.
const DETOUR_OFFSETS: [f64; 6] = [50.0, -50.0, 90.0, -90.0, 150.0, -150.0];
const SNAP: f64 = 10.0;
const FIT_MARGIN: f64 = 40.0;

/// Canvas operation bound to a keyboard shortcut.
type CanvasAction = fn(&Canvas);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Node(String),
    Edge(usize),
}

#[derive(Debug, Default)]
enum Drag {
    #[default]
    None,
    Pan {
        origin: (f64, f64),
    },
    Move {
        id: String,
        origin: Point,
        before: Box<Flowchart>,
        moved: bool,
    },
    Connect {
        from: String,
        cursor: Point,
    },
}

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct Canvas {
        pub chart: RefCell<Flowchart>,
        pub selection: RefCell<Option<Selection>>,
        pub hovered: RefCell<Option<String>>,
        pub zoom: Cell<f64>,
        pub offset: Cell<(f64, f64)>,
        pub pointer: Cell<(f64, f64)>,
        pub fit_pending: Cell<bool>,
        pub(super) drag: RefCell<Drag>,
        pub history: RefCell<History<Flowchart>>,
        pub editor: OnceCell<(gtk::Popover, gtk::Entry)>,
        pub editing: RefCell<Option<Selection>>,
    }

    impl Default for Canvas {
        fn default() -> Self {
            Self {
                chart: RefCell::default(),
                selection: RefCell::default(),
                hovered: RefCell::default(),
                zoom: Cell::new(1.0),
                offset: Cell::new((0.0, 0.0)),
                pointer: Cell::new((0.0, 0.0)),
                fit_pending: Cell::new(false),
                drag: RefCell::default(),
                history: RefCell::default(),
                editor: OnceCell::new(),
                editing: RefCell::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Canvas {
        const NAME: &'static str = "MermanCanvas";
        type Type = super::Canvas;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("merman-canvas");
            klass.set_accessible_role(gtk::AccessibleRole::Img);
        }
    }

    impl ObjectImpl for Canvas {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // The user edited the diagram.
                    Signal::builder("changed").build(),
                    Signal::builder("selection-changed").build(),
                    // Undo/redo availability may have changed.
                    Signal::builder("history-changed").build(),
                ]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_focusable(true);
            obj.set_overflow(gtk::Overflow::Hidden);
            obj.add_css_class("view");
            obj.setup_controllers();
            obj.setup_editor();
            adw::StyleManager::default().connect_dark_notify(glib::clone!(
                #[weak]
                obj,
                move |_| obj.queue_draw()
            ));
        }

        fn dispose(&self) {
            if let Some((popover, _)) = self.editor.get() {
                popover.unparent();
            }
        }
    }

    impl WidgetImpl for Canvas {
        fn measure(&self, _orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (200, 600, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
            if let Some((popover, _)) = self.editor.get() {
                popover.present();
            }
            if self.fit_pending.get() && width > 0 && height > 0 {
                self.fit_pending.set(false);
                self.obj().fit_to_content();
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.obj().draw(snapshot);
        }
    }
}

glib::wrapper! {
    pub struct Canvas(ObjectSubclass<imp::Canvas>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Canvas {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Canvas {
    // ----- Public API -------------------------------------------------------

    pub fn with_chart<R>(&self, f: impl FnOnce(&Flowchart) -> R) -> R {
        f(&self.imp().chart.borrow())
    }

    /// Replaces the diagram with one parsed from source text. Nodes without a
    /// stored position keep the one they currently have on the canvas.
    pub fn set_chart(&self, mut chart: Flowchart) {
        let imp = self.imp();
        {
            let current = imp.chart.borrow();
            for node in chart.nodes.iter_mut().filter(|n| n.pos.is_none()) {
                node.pos = current.node(&node.id).and_then(|n| n.pos);
            }
        }
        flowchart::layout::layout_missing(&mut chart, |n| self.node_size(n));
        if *imp.chart.borrow() == chart {
            return;
        }
        let before = imp.chart.replace(chart);
        imp.history.borrow_mut().record(before);
        self.after_replace();
    }

    /// Loads a new document, resetting undo history and the viewport.
    pub fn load_chart(&self, chart: Flowchart) {
        let imp = self.imp();
        imp.chart.replace(Flowchart::default());
        self.set_chart(chart);
        imp.history.borrow_mut().clear();
        self.emit_by_name::<()>("history-changed", &[]);
        self.fit_to_content();
    }

    /// Applies a user edit, recording it for undo and notifying listeners.
    pub fn edit(&self, f: impl FnOnce(&mut Flowchart)) {
        let _ = self.try_edit(|chart| {
            f(chart);
            Ok::<_, Infallible>(())
        });
    }

    pub fn try_edit<E>(&self, f: impl FnOnce(&mut Flowchart) -> Result<(), E>) -> Result<(), E> {
        let imp = self.imp();
        let mut chart = imp.chart.borrow().clone();
        f(&mut chart)?;
        if *imp.chart.borrow() != chart {
            let before = imp.chart.replace(chart);
            imp.history.borrow_mut().record(before);
            self.after_replace();
            self.emit_by_name::<()>("changed", &[]);
        }
        Ok(())
    }

    pub fn undo(&self) {
        self.step_history(true);
    }

    pub fn redo(&self) {
        self.step_history(false);
    }

    pub fn can_undo(&self) -> bool {
        self.imp().history.borrow().can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.imp().history.borrow().can_redo()
    }

    pub fn selection(&self) -> Option<Selection> {
        self.imp().selection.borrow().clone()
    }

    pub fn select(&self, selection: Option<Selection>) {
        if self.imp().selection.replace(selection.clone()) != selection {
            self.queue_draw();
            self.emit_by_name::<()>("selection-changed", &[]);
        }
    }

    pub fn delete_selection(&self) {
        let Some(selection) = self.selection() else {
            return;
        };
        self.edit(|chart| match &selection {
            Selection::Node(id) => chart.remove_node(id),
            Selection::Edge(i) if *i < chart.edges.len() => {
                chart.edges.remove(*i);
            }
            Selection::Edge(_) => {}
        });
        self.select(None);
    }

    pub fn add_node(&self, shape: NodeShape, at: Point) -> String {
        let mut id = String::new();
        self.edit(|chart| id = chart.add_node(shape, snap(at)));
        self.select(Some(Selection::Node(id.clone())));
        id
    }

    /// Adds a node in the middle of the viewport, stepping aside from nodes
    /// already sitting there.
    pub fn add_node_at_center(&self, shape: NodeShape) {
        let mut at = self.to_world(self.width() as f64 / 2.0, self.height() as f64 / 2.0);
        while self.with_chart(|c| {
            c.nodes.iter().any(|n| {
                n.pos
                    .is_some_and(|p| (p.x - at.x).abs() < 20.0 && (p.y - at.y).abs() < 20.0)
            })
        }) {
            at = Point::new(at.x + 30.0, at.y + 30.0);
        }
        self.add_node(shape, at);
        self.grab_focus();
    }

    /// Changes the flow direction and re-lays out the diagram to match it.
    pub fn set_direction(&self, direction: Direction) {
        self.edit(|chart| {
            chart.direction = direction;
            flowchart::layout::layout_all(chart, |n| self.node_size(n));
        });
        self.fit_to_content();
    }

    pub fn rename_node(&self, old: &str, new: &str) -> Result<(), RenameError> {
        // Select the new id up front so the selection stays valid after the edit.
        let imp = self.imp();
        let previous = imp
            .selection
            .replace(Some(Selection::Node(new.to_string())));
        let result = self.try_edit(|chart| chart.rename_node(old, new));
        if result.is_err() {
            imp.selection.replace(previous);
        }
        result
    }

    pub fn auto_layout(&self) {
        self.edit(|chart| flowchart::layout::layout_all(chart, |n| self.node_size(n)));
        self.fit_to_content();
    }

    pub fn zoom_in(&self) {
        self.zoom_around(self.viewport_center(), ZOOM_STEP);
    }

    pub fn zoom_out(&self) {
        self.zoom_around(self.viewport_center(), 1.0 / ZOOM_STEP);
    }

    pub fn reset_zoom(&self) {
        let zoom = self.imp().zoom.get();
        self.zoom_around(self.viewport_center(), 1.0 / zoom);
    }

    pub fn fit_to_content(&self) {
        let imp = self.imp();
        let (w, h) = (self.width() as f64, self.height() as f64);
        if w <= 0.0 || h <= 0.0 {
            imp.fit_pending.set(true);
            return;
        }
        let bounds = self.with_chart(|chart| {
            chart
                .nodes
                .iter()
                .filter_map(|n| self.node_rect(n))
                .reduce(|a, b| a.union(&b))
        });
        let Some(bounds) = bounds.map(|b| b.inflate(FIT_MARGIN)) else {
            imp.zoom.set(1.0);
            imp.offset.set((0.0, 0.0));
            self.queue_draw();
            return;
        };
        let zoom = (w / bounds.w).min(h / bounds.h).clamp(MIN_ZOOM, 1.0);
        let c = bounds.center();
        imp.zoom.set(zoom);
        imp.offset.set((w / 2.0 - c.x * zoom, h / 2.0 - c.y * zoom));
        self.queue_draw();
    }

    // ----- Coordinates and hit testing --------------------------------------

    fn to_world(&self, x: f64, y: f64) -> Point {
        let imp = self.imp();
        let zoom = imp.zoom.get();
        let (ox, oy) = imp.offset.get();
        Point::new((x - ox) / zoom, (y - oy) / zoom)
    }

    fn to_screen(&self, p: Point) -> (f64, f64) {
        let imp = self.imp();
        let zoom = imp.zoom.get();
        let (ox, oy) = imp.offset.get();
        (p.x * zoom + ox, p.y * zoom + oy)
    }

    fn viewport_center(&self) -> (f64, f64) {
        (self.width() as f64 / 2.0, self.height() as f64 / 2.0)
    }

    fn label_layout(&self, text: &str) -> pango::Layout {
        let layout = self.create_pango_layout(Some(text));
        layout.set_width(MAX_LABEL_WIDTH * pango::SCALE);
        layout.set_wrap(pango::WrapMode::WordChar);
        layout.set_alignment(pango::Alignment::Center);
        layout
    }

    fn text_size(&self, text: &str) -> (f64, f64) {
        let (_, logical) = self.label_layout(text).pixel_extents();
        (logical.width() as f64, logical.height() as f64)
    }

    fn node_size(&self, node: &Node) -> (f64, f64) {
        geometry::shape_size(node.shape, self.text_size(&node.label))
    }

    fn node_rect(&self, node: &Node) -> Option<Rect> {
        Some(Rect::centered(node.pos?, self.node_size(node)))
    }

    fn node_at(&self, p: Point) -> Option<String> {
        self.with_chart(|chart| {
            chart
                .nodes
                .iter()
                .rev()
                .find(|n| self.node_rect(n).is_some_and(|r| r.contains(p)))
                .map(|n| n.id.clone())
        })
    }

    fn edge_at(&self, p: Point) -> Option<usize> {
        let tolerance = EDGE_HIT_DISTANCE / self.imp().zoom.get();
        self.with_chart(|chart| {
            self.edge_paths(chart)
                .iter()
                .enumerate()
                .map(|(i, path)| (i, geometry::distance_to_path(p, path)))
                .filter(|(_, d)| *d <= tolerance)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        })
    }

    /// Node whose connection handle is under `p`. Handles are shown on the
    /// hovered and the selected node.
    fn port_at(&self, p: Point) -> Option<String> {
        let imp = self.imp();
        let radius = PORT_HIT_RADIUS / imp.zoom.get();
        let selected = match &*imp.selection.borrow() {
            Some(Selection::Node(id)) => Some(id.clone()),
            _ => None,
        };
        let candidates = [imp.hovered.borrow().clone(), selected];
        self.with_chart(|chart| {
            candidates.into_iter().flatten().find(|id| {
                chart
                    .node(id)
                    .and_then(|n| self.node_rect(n))
                    .is_some_and(|r| {
                        r.ports()
                            .iter()
                            .any(|q| (q.x - p.x).hypot(q.y - p.y) <= radius)
                    })
            })
        })
    }

    fn edge_paths(&self, chart: &Flowchart) -> Vec<Vec<Point>> {
        let rects: HashMap<&str, (NodeShape, Rect)> = chart
            .nodes
            .iter()
            .filter_map(|n| Some((n.id.as_str(), (n.shape, self.node_rect(n)?))))
            .collect();
        let pair_key = |from: &str, to: &str| {
            if from <= to {
                (from.to_string(), to.to_string())
            } else {
                (to.to_string(), from.to_string())
            }
        };
        let mut groups: HashMap<(String, String), Vec<usize>> = HashMap::new();
        for (i, e) in chart.edges.iter().enumerate() {
            groups.entry(pair_key(&e.from, &e.to)).or_default().push(i);
        }
        chart
            .edges
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let (Some((fs, fr)), Some((ts, tr))) =
                    (rects.get(e.from.as_str()), rects.get(e.to.as_str()))
                else {
                    return Vec::new();
                };
                if e.from == e.to {
                    return geometry::self_loop_path(*fs, fr);
                }
                let key = pair_key(&e.from, &e.to);
                let group = &groups[&key];
                let k = group.iter().position(|&j| j == i).unwrap_or(0) as f64;
                let mut bend = (k - (group.len() - 1) as f64 / 2.0) * PARALLEL_EDGE_SPACING;
                // Bend direction is relative to the edge direction; keep it
                // consistent within a group of edges going both ways.
                if e.from != key.0 {
                    bend = -bend;
                }
                let path = geometry::edge_path(*fs, fr, *ts, tr, bend);
                let obstacles: Vec<Rect> = rects
                    .iter()
                    .filter(|(id, _)| **id != e.from && **id != e.to)
                    .map(|(_, (_, r))| r.inflate(4.0))
                    .collect();
                if !geometry::path_crosses(&path, &obstacles) {
                    return path;
                }
                DETOUR_OFFSETS
                    .iter()
                    .map(|offset| geometry::detour_path(*fs, fr, *ts, tr, bend + offset))
                    .find(|p| !geometry::path_crosses(p, &obstacles))
                    .unwrap_or(path)
            })
            .collect()
    }

    // ----- Rendering --------------------------------------------------------

    /// Draws all geometry into a single cairo layer, then the labels on top.
    fn draw(&self, snapshot: &gtk::Snapshot) {
        let imp = self.imp();
        let (w, h) = (self.width() as f64, self.height() as f64);
        let zoom = imp.zoom.get();
        let (ox, oy) = imp.offset.get();
        let colors = Colors::new(self.color(), adw::StyleManager::default().is_dark());
        let chart = imp.chart.borrow();
        let selection = imp.selection.borrow().clone();
        let drag = imp.drag.borrow();

        let paths = self.edge_paths(&chart);
        let edge_labels: Vec<_> = chart
            .edges
            .iter()
            .zip(&paths)
            .filter(|(e, path)| !e.label.is_empty() && !path.is_empty())
            .map(|(e, path)| (self.label_layout(&e.label), geometry::path_midpoint(path)))
            .collect();
        let nodes: Vec<_> = chart
            .nodes
            .iter()
            .filter_map(|node| Some((node, self.node_rect(node)?)))
            .collect();
        let connect_target = match &*drag {
            Drag::Connect { cursor, .. } => self.node_at(*cursor),
            _ => None,
        };

        let cr = snapshot.append_cairo(&graphene::Rect::new(0.0, 0.0, w as f32, h as f32));
        cr.translate(ox, oy);
        cr.scale(zoom, zoom);

        let top_left = self.to_world(0.0, 0.0);
        let visible = Rect {
            x: top_left.x,
            y: top_left.y,
            w: w / zoom,
            h: h / zoom,
        };
        render::draw_grid(&cr, &visible, zoom, &colors);

        for (i, (edge, path)) in chart.edges.iter().zip(&paths).enumerate() {
            let selected = selection == Some(Selection::Edge(i));
            log_draw_error(render::draw_edge(&cr, edge, path, &colors, selected));
        }
        for (layout, at) in &edge_labels {
            let (_, logical) = layout.pixel_extents();
            let size = (logical.width() as f64 + 8.0, logical.height() as f64 + 4.0);
            log_draw_error(render::draw_label_background(
                &cr,
                &Rect::centered(*at, size),
                &colors,
            ));
        }

        for (node, rect) in &nodes {
            let highlighted = selection == Some(Selection::Node(node.id.clone()))
                || connect_target.as_deref() == Some(node.id.as_str());
            log_draw_error(render::draw_node(
                &cr,
                node.shape,
                rect,
                &colors,
                highlighted,
            ));
        }

        // Interaction overlay: connection handles and the edge being dragged.
        match &*drag {
            Drag::Connect { from, cursor } => {
                if let Some(rect) = chart.node(from).and_then(|n| self.node_rect(n)) {
                    log_draw_error(render::draw_rubber_band(
                        &cr,
                        rect.center(),
                        *cursor,
                        zoom,
                        &colors,
                    ));
                }
            }
            Drag::None => {
                let selected = match &selection {
                    Some(Selection::Node(id)) => Some(id.clone()),
                    _ => None,
                };
                for id in [imp.hovered.borrow().clone(), selected]
                    .into_iter()
                    .flatten()
                {
                    if let Some(rect) = chart.node(&id).and_then(|n| self.node_rect(n)) {
                        log_draw_error(render::draw_ports(
                            &cr,
                            &rect.ports(),
                            PORT_RADIUS / zoom,
                            &colors,
                        ));
                    }
                }
            }
            _ => {}
        }
        drop(cr);

        for (layout, at) in &edge_labels {
            self.draw_text(snapshot, layout, *at, &colors.fg);
        }
        for (node, rect) in &nodes {
            self.draw_text(
                snapshot,
                &self.label_layout(&node.label),
                rect.center(),
                &colors.fg,
            );
        }
    }

    fn draw_text(
        &self,
        snapshot: &gtk::Snapshot,
        layout: &pango::Layout,
        center: Point,
        color: &gdk::RGBA,
    ) {
        let zoom = self.imp().zoom.get() as f32;
        let (sx, sy) = self.to_screen(center);
        let (_, logical) = layout.pixel_extents();
        snapshot.save();
        snapshot.translate(&graphene::Point::new(sx as f32, sy as f32));
        snapshot.scale(zoom, zoom);
        snapshot.translate(&graphene::Point::new(
            -(logical.x() as f32 + logical.width() as f32 / 2.0),
            -(logical.y() as f32 + logical.height() as f32 / 2.0),
        ));
        snapshot.append_layout(layout, color);
        snapshot.restore();
    }

    // ----- Interaction ------------------------------------------------------

    fn setup_controllers(&self) {
        let drag = gtk::GestureDrag::builder()
            .button(gdk::BUTTON_PRIMARY)
            .build();
        drag.connect_drag_begin(glib::clone!(
            #[weak(rename_to = canvas)]
            self,
            move |gesture, x, y| canvas.on_drag_begin(gesture, x, y)
        ));
        drag.connect_drag_update(glib::clone!(
            #[weak(rename_to = canvas)]
            self,
            move |gesture, dx, dy| canvas.on_drag_update(gesture, dx, dy)
        ));
        drag.connect_drag_end(glib::clone!(
            #[weak(rename_to = canvas)]
            self,
            move |_, _, _| canvas.on_drag_end()
        ));
        self.add_controller(drag);

        let pan = gtk::GestureDrag::builder()
            .button(gdk::BUTTON_MIDDLE)
            .build();
        pan.connect_drag_begin(glib::clone!(
            #[weak(rename_to = canvas)]
            self,
            move |_, _, _| {
                let origin = canvas.imp().offset.get();
                canvas.imp().drag.replace(Drag::Pan { origin });
            }
        ));
        pan.connect_drag_update(glib::clone!(
            #[weak(rename_to = canvas)]
            self,
            move |gesture, dx, dy| canvas.on_drag_update(gesture, dx, dy)
        ));
        pan.connect_drag_end(glib::clone!(
            #[weak(rename_to = canvas)]
            self,
            move |_, _, _| canvas.on_drag_end()
        ));
        self.add_controller(pan);

        let click = gtk::GestureClick::builder()
            .button(gdk::BUTTON_PRIMARY)
            .build();
        click.connect_pressed(glib::clone!(
            #[weak(rename_to = canvas)]
            self,
            move |_, n_press, x, y| {
                if n_press == 2 {
                    canvas.on_double_click(x, y);
                }
            }
        ));
        self.add_controller(click);

        let motion = gtk::EventControllerMotion::new();
        motion.connect_motion(glib::clone!(
            #[weak(rename_to = canvas)]
            self,
            move |_, x, y| canvas.on_motion(x, y)
        ));
        motion.connect_leave(glib::clone!(
            #[weak(rename_to = canvas)]
            self,
            move |_| {
                if canvas.imp().hovered.take().is_some() {
                    canvas.queue_draw();
                }
            }
        ));
        self.add_controller(motion);

        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
        scroll.connect_scroll(glib::clone!(
            #[weak(rename_to = canvas)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |controller, dx, dy| {
                canvas.on_scroll(controller, dx, dy);
                glib::Propagation::Stop
            }
        ));
        self.add_controller(scroll);

        let drop = gtk::DropTarget::new(String::static_type(), gdk::DragAction::COPY);
        drop.connect_drop(glib::clone!(
            #[weak(rename_to = canvas)]
            self,
            #[upgrade_or]
            false,
            move |_, value, x, y| {
                let shape = value
                    .get::<String>()
                    .ok()
                    .and_then(|s| NodeShape::from_name(&s));
                let Some(shape) = shape else {
                    return false;
                };
                canvas.add_node(shape, canvas.to_world(x, y));
                canvas.grab_focus();
                true
            }
        ));
        self.add_controller(drop);

        let shortcuts = gtk::ShortcutController::new();
        let bindings: [(&str, CanvasAction); 8] = [
            ("Delete|BackSpace", Canvas::delete_selection),
            ("<Control>z", Canvas::undo),
            ("<Control><Shift>z|<Control>y", Canvas::redo),
            ("Escape", |c| c.select(None)),
            (
                "<Control>plus|<Control>equal|<Control>KP_Add",
                Canvas::zoom_in,
            ),
            ("<Control>minus|<Control>KP_Subtract", Canvas::zoom_out),
            ("<Control>0", Canvas::reset_zoom),
            ("F2", Canvas::edit_selected_label),
        ];
        for (trigger, handler) in bindings {
            let action = gtk::CallbackAction::new(move |widget, _| {
                if let Some(canvas) = widget.downcast_ref::<Canvas>() {
                    handler(canvas);
                }
                glib::Propagation::Stop
            });
            shortcuts.add_shortcut(gtk::Shortcut::new(
                gtk::ShortcutTrigger::parse_string(trigger),
                Some(action),
            ));
        }
        self.add_controller(shortcuts);
    }

    fn on_drag_begin(&self, gesture: &gtk::GestureDrag, x: f64, y: f64) {
        let imp = self.imp();
        self.grab_focus();
        let world = self.to_world(x, y);
        let shift = gesture
            .current_event_state()
            .contains(gdk::ModifierType::SHIFT_MASK);

        let connect_from = self
            .port_at(world)
            .or_else(|| shift.then(|| self.node_at(world)).flatten());
        let drag = if let Some(from) = connect_from {
            Drag::Connect {
                from,
                cursor: world,
            }
        } else if let Some(id) = self.node_at(world) {
            self.select(Some(Selection::Node(id.clone())));
            let origin = self
                .with_chart(|c| c.node(&id).and_then(|n| n.pos))
                .unwrap_or(world);
            Drag::Move {
                id,
                origin,
                before: Box::new(imp.chart.borrow().clone()),
                moved: false,
            }
        } else if let Some(i) = self.edge_at(world) {
            self.select(Some(Selection::Edge(i)));
            Drag::None
        } else {
            self.select(None);
            Drag::Pan {
                origin: imp.offset.get(),
            }
        };
        imp.drag.replace(drag);
        self.queue_draw();
    }

    fn on_drag_update(&self, gesture: &gtk::GestureDrag, dx: f64, dy: f64) {
        let imp = self.imp();
        let zoom = imp.zoom.get();
        match &mut *imp.drag.borrow_mut() {
            Drag::None => return,
            Drag::Pan { origin } => imp.offset.set((origin.0 + dx, origin.1 + dy)),
            Drag::Move {
                id, origin, moved, ..
            } => {
                let pos = snap(Point::new(origin.x + dx / zoom, origin.y + dy / zoom));
                if let Some(node) = imp.chart.borrow_mut().node_mut(id) {
                    *moved |= node.pos != Some(pos);
                    node.pos = Some(pos);
                }
            }
            Drag::Connect { cursor, .. } => {
                if let Some((sx, sy)) = gesture.start_point() {
                    *cursor = self.to_world(sx + dx, sy + dy);
                }
            }
        }
        self.queue_draw();
    }

    fn on_drag_end(&self) {
        let imp = self.imp();
        match imp.drag.take() {
            Drag::Move {
                before,
                moved: true,
                ..
            } => {
                imp.history.borrow_mut().record(*before);
                self.emit_by_name::<()>("history-changed", &[]);
                self.emit_by_name::<()>("changed", &[]);
            }
            Drag::Connect { from, cursor } => {
                if let Some(to) = self.node_at(cursor).filter(|to| *to != from) {
                    let mut index = 0;
                    self.edit(|chart| index = chart.add_edge(&from, &to));
                    self.select(Some(Selection::Edge(index)));
                }
            }
            _ => {}
        }
        self.queue_draw();
    }

    fn on_double_click(&self, x: f64, y: f64) {
        let world = self.to_world(x, y);
        if let Some(id) = self.node_at(world) {
            self.start_label_edit(Selection::Node(id));
        } else if let Some(i) = self.edge_at(world) {
            self.start_label_edit(Selection::Edge(i));
        } else {
            let id = self.add_node(NodeShape::Rectangle, world);
            self.start_label_edit(Selection::Node(id));
        }
    }

    fn on_motion(&self, x: f64, y: f64) {
        let imp = self.imp();
        imp.pointer.set((x, y));
        if !matches!(*imp.drag.borrow(), Drag::None) {
            return;
        }
        let world = self.to_world(x, y);
        let on_port = self.port_at(world);
        // Keep the hovered node while the pointer is on one of its handles,
        // which stick out of the node outline.
        let hovered = on_port.clone().or_else(|| self.node_at(world));
        let cursor = if on_port.is_some() {
            "crosshair"
        } else if hovered.is_some() {
            "move"
        } else {
            "default"
        };
        self.set_cursor_from_name(Some(cursor));
        if imp.hovered.replace(hovered.clone()) != hovered {
            self.queue_draw();
        }
    }

    fn on_scroll(&self, controller: &gtk::EventControllerScroll, dx: f64, dy: f64) {
        let imp = self.imp();
        if controller
            .current_event_state()
            .contains(gdk::ModifierType::CONTROL_MASK)
        {
            let factor = if dy < 0.0 { ZOOM_STEP } else { 1.0 / ZOOM_STEP };
            self.zoom_around(imp.pointer.get(), factor);
        } else {
            let (ox, oy) = imp.offset.get();
            imp.offset
                .set((ox - dx * SCROLL_STEP, oy - dy * SCROLL_STEP));
            self.queue_draw();
        }
    }

    fn zoom_around(&self, (x, y): (f64, f64), factor: f64) {
        let imp = self.imp();
        let anchor = self.to_world(x, y);
        let zoom = (imp.zoom.get() * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        imp.zoom.set(zoom);
        imp.offset.set((x - anchor.x * zoom, y - anchor.y * zoom));
        self.queue_draw();
    }

    // ----- Inline label editing ---------------------------------------------

    fn setup_editor(&self) {
        let entry = gtk::Entry::builder().width_chars(18).build();
        let popover = gtk::Popover::builder().child(&entry).build();
        popover.set_parent(self);
        entry.connect_activate(glib::clone!(
            #[weak(rename_to = canvas)]
            self,
            move |entry| {
                canvas.commit_label_edit(&entry.text());
            }
        ));
        popover.connect_closed(glib::clone!(
            #[weak(rename_to = canvas)]
            self,
            move |_| {
                canvas.imp().editing.take();
                canvas.grab_focus();
            }
        ));
        let _ = self.imp().editor.set((popover, entry));
    }

    fn edit_selected_label(&self) {
        if let Some(selection) = self.selection() {
            self.start_label_edit(selection);
        }
    }

    fn start_label_edit(&self, target: Selection) {
        let Some((popover, entry)) = self.imp().editor.get() else {
            return;
        };
        let found = self.with_chart(|chart| match &target {
            Selection::Node(id) => chart
                .node(id)
                .and_then(|n| Some((n.label.clone(), self.node_rect(n)?.center()))),
            Selection::Edge(i) => {
                let label = chart.edges.get(*i)?.label.clone();
                let path = self.edge_paths(chart).swap_remove(*i);
                Some((label, geometry::path_midpoint(&path)))
            }
        });
        let Some((label, anchor)) = found else {
            return;
        };
        self.select(Some(target.clone()));
        self.imp().editing.replace(Some(target));
        let (sx, sy) = self.to_screen(anchor);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(sx as i32, sy as i32, 1, 1)));
        entry.set_text(&label);
        popover.popup();
        entry.grab_focus();
        entry.select_region(0, -1);
    }

    fn commit_label_edit(&self, text: &str) {
        let imp = self.imp();
        if let Some(target) = imp.editing.take() {
            self.edit(|chart| match target {
                Selection::Node(id) => {
                    if let Some(node) = chart.node_mut(&id) {
                        node.label = text.to_string();
                    }
                }
                Selection::Edge(i) => {
                    if let Some(edge) = chart.edges.get_mut(i) {
                        edge.label = text.to_string();
                    }
                }
            });
        }
        if let Some((popover, _)) = imp.editor.get() {
            popover.popdown();
        }
    }

    // ----- Internal helpers -------------------------------------------------

    fn step_history(&self, undo: bool) {
        let imp = self.imp();
        let current = imp.chart.borrow().clone();
        let restored = {
            let mut history = imp.history.borrow_mut();
            if undo {
                history.undo(current)
            } else {
                history.redo(current)
            }
        };
        if let Some(chart) = restored {
            imp.chart.replace(chart);
            self.after_replace();
            self.emit_by_name::<()>("changed", &[]);
        }
    }

    /// Drops selection and hover state that no longer match the diagram.
    fn after_replace(&self) {
        let imp = self.imp();
        let valid = self.with_chart(|chart| match &*imp.selection.borrow() {
            Some(Selection::Node(id)) => chart.node(id).is_some(),
            Some(Selection::Edge(i)) => *i < chart.edges.len(),
            None => true,
        });
        if !valid {
            self.select(None);
        }
        imp.hovered.take();
        self.emit_by_name::<()>("history-changed", &[]);
        // Selection contents (labels, shapes) may have changed too.
        self.emit_by_name::<()>("selection-changed", &[]);
        self.queue_draw();
    }
}

fn snap(p: Point) -> Point {
    Point::new((p.x / SNAP).round() * SNAP, (p.y / SNAP).round() * SNAP)
}

fn log_draw_error(result: Result<(), gtk::cairo::Error>) {
    if let Err(err) = result {
        tracing::warn!(error = %err, "failed to draw diagram element");
    }
}
