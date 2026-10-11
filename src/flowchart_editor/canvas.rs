//! Interactive flowchart canvas: renders a [`Flowchart`] and edits it through
//! direct manipulation (move, connect, drop, inline label editing).

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::OnceLock;

use gtk::glib::subclass::Signal;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib};

use super::geometry;
use super::render;
use crate::drawing::{self, Colors, log_draw_error};
use crate::flowchart::model::RenameError;
use crate::flowchart::{self, Direction, Flowchart, Node, NodeShape, Point};
use crate::geometry::Rect;
use crate::history::History;
use crate::interaction::{InlineEditor, Navigable, install_navigation};
use crate::routing::{self, Outline};
use crate::viewport::Viewport;

/// Screen-space radius of connection handles and their hit area.
const PORT_RADIUS: f64 = 5.0;
const PORT_HIT_RADIUS: f64 = 10.0;
const EDGE_HIT_DISTANCE: f64 = 6.0;
const SNAP: f64 = 10.0;
/// Pointer travel before a press on an element starts moving it.
const DRAG_THRESHOLD: f64 = 4.0;

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

    #[derive(Debug, Default)]
    pub struct Canvas {
        pub chart: RefCell<Flowchart>,
        pub selection: RefCell<Option<Selection>>,
        pub hovered: RefCell<Option<String>>,
        pub viewport: Cell<Viewport>,
        pub pointer: Cell<(f64, f64)>,
        pub fit_pending: Cell<bool>,
        pub(super) drag: RefCell<Drag>,
        pub history: RefCell<History<Flowchart>>,
        pub editor: OnceCell<InlineEditor>,
        pub editing: RefCell<Option<Selection>>,
        /// Node copied with Ctrl+C, as it was when copied.
        pub clipboard: RefCell<Option<Node>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Canvas {
        const NAME: &'static str = "MermanFlowchartCanvas";
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
            install_navigation(&*obj);
            obj.setup_editor();
            adw::StyleManager::default().connect_dark_notify(glib::clone!(
                #[weak]
                obj,
                move |_| obj.queue_draw()
            ));
        }

        fn dispose(&self) {
            if let Some(editor) = self.editor.get() {
                editor.unparent();
            }
        }
    }

    impl WidgetImpl for Canvas {
        fn measure(&self, _orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (200, 600, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
            if let Some(editor) = self.editor.get() {
                editor.present();
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

impl Navigable for Canvas {
    fn viewport(&self) -> &Cell<Viewport> {
        &self.imp().viewport
    }

    fn pointer(&self) -> &Cell<(f64, f64)> {
        &self.imp().pointer
    }

    fn fit_pending(&self) -> &Cell<bool> {
        &self.imp().fit_pending
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
        let (w, h) = self.widget_size();
        let at = self.free_spot(self.viewport().get().to_world(w / 2.0, h / 2.0));
        self.add_node(shape, at);
        self.grab_focus();
    }

    /// `at`, or the first point diagonally down from it with no node on it.
    fn free_spot(&self, mut at: Point) -> Point {
        while self.with_chart(|c| {
            c.nodes.iter().any(|n| {
                n.pos
                    .is_some_and(|p| (p.x - at.x).abs() < 20.0 && (p.y - at.y).abs() < 20.0)
            })
        }) {
            at = Point::new(at.x + 30.0, at.y + 30.0);
        }
        at
    }

    pub fn copy_selection(&self) {
        let Some(Selection::Node(id)) = self.selection() else {
            return;
        };
        let node = self.with_chart(|c| c.node(&id).cloned());
        if node.is_some() {
            self.imp().clipboard.replace(node);
        }
    }

    /// Adds a copy of the copied node next to where it was copied from.
    pub fn paste(&self) {
        let Some(node) = self.imp().clipboard.borrow().clone() else {
            return;
        };
        let origin = node.pos.unwrap_or_else(|| {
            let (w, h) = self.widget_size();
            self.viewport().get().to_world(w / 2.0, h / 2.0)
        });
        let at = snap(self.free_spot(origin));
        let mut id = String::new();
        self.edit(|chart| id = chart.paste_node(&node, at));
        self.select(Some(Selection::Node(id)));
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

    pub fn fit_to_content(&self) {
        let bounds = self.with_chart(|chart| {
            chart
                .nodes
                .iter()
                .filter_map(|n| self.node_rect(n))
                .reduce(|a, b| a.union(&b))
        });
        self.fit_bounds(bounds);
    }

    // ----- Geometry and hit testing -----------------------------------------

    fn to_world(&self, x: f64, y: f64) -> Point {
        self.viewport().get().to_world(x, y)
    }

    fn zoom(&self) -> f64 {
        self.viewport().get().zoom
    }

    fn node_size(&self, node: &Node) -> (f64, f64) {
        geometry::shape_size(node.shape, drawing::text_size(self, &node.label))
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
        let tolerance = EDGE_HIT_DISTANCE / self.zoom();
        self.with_chart(|chart| {
            self.edge_paths(chart)
                .iter()
                .enumerate()
                .map(|(i, path)| (i, routing::distance_to_path(p, path)))
                .filter(|(_, d)| *d <= tolerance)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        })
    }

    /// Node whose connection handle is under `p`. Handles are shown on the
    /// hovered and the selected node.
    fn port_at(&self, p: Point) -> Option<String> {
        let imp = self.imp();
        let radius = PORT_HIT_RADIUS / self.zoom();
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
        let nodes: HashMap<&str, (Outline, Rect)> = chart
            .nodes
            .iter()
            .filter_map(|n| {
                Some((
                    n.id.as_str(),
                    (geometry::outline(n.shape), self.node_rect(n)?),
                ))
            })
            .collect();
        let edges: Vec<(&str, &str)> = chart
            .edges
            .iter()
            .map(|e| (e.from.as_str(), e.to.as_str()))
            .collect();
        routing::route_edges(&nodes, &edges)
    }

    // ----- Rendering --------------------------------------------------------

    fn draw(&self, snapshot: &gtk::Snapshot) {
        let imp = self.imp();
        let viewport = self.viewport().get();
        let zoom = viewport.zoom;
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
            .map(|(e, path)| {
                (
                    drawing::label_layout(self, &e.label),
                    routing::path_midpoint(path),
                )
            })
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

        let cr = drawing::world_context(snapshot, &viewport, self.widget_size());
        drawing::draw_grid(&cr, &viewport.visible(self.widget_size()), zoom, &colors);

        for (i, (edge, path)) in chart.edges.iter().zip(&paths).enumerate() {
            let selected = selection == Some(Selection::Edge(i));
            log_draw_error(render::draw_edge(&cr, edge, path, &colors, selected));
        }
        for (layout, at) in &edge_labels {
            let (w, h) = drawing::layout_size(layout);
            log_draw_error(drawing::draw_label_background(
                &cr,
                &Rect::centered(*at, (w + 8.0, h + 4.0)),
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
                    log_draw_error(drawing::draw_rubber_band(
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
                        log_draw_error(drawing::draw_ports(
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
            drawing::draw_text(snapshot, &viewport, layout, *at, &colors.fg);
        }
        for (node, rect) in &nodes {
            drawing::draw_text(
                snapshot,
                &viewport,
                &drawing::label_layout(self, &node.label),
                rect.center(),
                &colors.fg,
            );
        }
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
        let bindings: [(&str, CanvasAction); 10] = [
            ("Delete|BackSpace", Canvas::delete_selection),
            ("<Control>c", Canvas::copy_selection),
            ("<Control>v", Canvas::paste),
            ("<Control>z", Canvas::undo),
            ("<Control><Shift>z|<Control>y", Canvas::redo),
            ("Escape", |c| c.select(None)),
            ("<Control>plus|<Control>equal|<Control>KP_Add", |c| {
                c.zoom_in()
            }),
            ("<Control>minus|<Control>KP_Subtract", |c| c.zoom_out()),
            ("<Control>0", |c| c.reset_zoom()),
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
                origin: self.viewport().get().offset,
            }
        };
        imp.drag.replace(drag);
        self.queue_draw();
    }

    fn on_drag_update(&self, gesture: &gtk::GestureDrag, dx: f64, dy: f64) {
        let imp = self.imp();
        let zoom = self.zoom();
        match &mut *imp.drag.borrow_mut() {
            Drag::None => return,
            Drag::Pan { origin } => {
                let origin = *origin;
                self.update_viewport(|v| v.offset = (origin.0 + dx, origin.1 + dy));
            }
            Drag::Move {
                id, origin, moved, ..
            } => {
                // Ignore jitter so a plain click does not move or snap the element.
                if !*moved && dx.hypot(dy) < DRAG_THRESHOLD {
                    return;
                }
                let pos = snap(Point::new(origin.x + dx / zoom, origin.y + dy / zoom));
                if let Some(node) = imp.chart.borrow_mut().node_mut(id) {
                    *moved = true;
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

    // ----- Inline label editing ---------------------------------------------

    fn setup_editor(&self) {
        let commit = glib::clone!(
            #[weak(rename_to = canvas)]
            self,
            move |text: &str| canvas.commit_label_edit(text)
        );
        let closed = glib::clone!(
            #[weak(rename_to = canvas)]
            self,
            move || {
                canvas.imp().editing.take();
                canvas.grab_focus();
            }
        );
        let _ = self
            .imp()
            .editor
            .set(InlineEditor::new(self, commit, closed));
    }

    fn edit_selected_label(&self) {
        if let Some(selection) = self.selection() {
            self.start_label_edit(selection);
        }
    }

    fn start_label_edit(&self, target: Selection) {
        let found = self.with_chart(|chart| match &target {
            Selection::Node(id) => chart
                .node(id)
                .and_then(|n| Some((n.label.clone(), self.node_rect(n)?.center()))),
            Selection::Edge(i) => {
                let label = chart.edges.get(*i)?.label.clone();
                let path = self.edge_paths(chart).swap_remove(*i);
                Some((label, routing::path_midpoint(&path)))
            }
        });
        let (Some((label, anchor)), Some(editor)) = (found, self.imp().editor.get()) else {
            return;
        };
        self.select(Some(target.clone()));
        self.imp().editing.replace(Some(target));
        editor.open(self.viewport().get().to_screen(anchor), &label);
    }

    fn commit_label_edit(&self, text: &str) {
        let Some(target) = self.imp().editing.take() else {
            return;
        };
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
