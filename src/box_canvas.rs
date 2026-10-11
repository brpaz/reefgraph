//! Canvas for diagrams of boxes with rows joined by links (class diagrams,
//! entity relationship diagrams). Boxes are moved by dragging, linked by
//! dragging from their handles and edited in place. What a box looks like
//! and what its rows mean is up to the [`BoxDiagram`] being edited.

use std::any::Any;
use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::OnceLock;

use gtk::glib::subclass::Signal;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{cairo, gdk, glib};

use crate::drawing::{self, Colors, Text, log_draw_error};
use crate::geometry::{Point, Rect};
use crate::history::History;
use crate::ids::RenameError;
use crate::interaction::{InlineEditor, Navigable, install_navigation};
use crate::routing::{self, Outline};
use crate::viewport::Viewport;

/// Screen-space radius of connection handles and their hit area.
const PORT_RADIUS: f64 = 5.0;
const PORT_HIT_RADIUS: f64 = 10.0;
const LINK_HIT_DISTANCE: f64 = 6.0;
/// Gap between a box and its "add row" button.
const ADD_BUTTON_GAP: f64 = 16.0;
const SNAP: f64 = 10.0;
/// Pointer travel before a press on a box starts moving it.
const DRAG_THRESHOLD: f64 = 4.0;

/// Measures label text, returning its width and height.
pub type Measure<'a> = &'a dyn Fn(&str) -> (f64, f64);

/// Text edited in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditTarget {
    /// The box's display name.
    Name(String),
    /// A row of a box; the index is defined by the diagram.
    Row(String, usize),
    Link(usize),
}

/// What the canvas is highlighting while drawing.
#[derive(Debug, Clone, Default)]
pub struct Highlight {
    pub boxes: Vec<String>,
    pub link: Option<usize>,
}

/// A diagram the box canvas can edit.
pub trait BoxDiagram: Debug {
    fn clone_diagram(&self) -> Box<dyn BoxDiagram>;
    fn same_as(&self, other: &dyn BoxDiagram) -> bool;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn box_ids(&self) -> Vec<String>;
    /// Positioned boxes and their outer rectangles, in drawing order.
    fn boxes(&self, measure: Measure) -> Vec<(&str, Rect)>;
    fn position(&self, id: &str) -> Option<Point>;
    fn set_position(&mut self, id: &str, pos: Point);
    /// Outer rectangle of a positioned box.
    fn box_rect(&self, id: &str, measure: Measure) -> Option<Rect>;
    /// Name shown in the box header: the label, or the id when there is none.
    fn label(&self, id: &str) -> Option<String>;
    fn set_label(&mut self, id: &str, label: String);
    /// Center of the box header, where its name editor points.
    fn header_center(&self, id: &str, measure: Measure) -> Option<Point>;
    /// Positions boxes that have none, or every box when `all` is set.
    fn layout(&mut self, all: bool, measure: Measure);
    /// Endpoints of every link, indexed like the links.
    fn links(&self) -> Vec<(String, String)>;

    /// Palette item added by double-clicking empty canvas.
    fn default_preset(&self) -> &'static str;
    /// Adds a box for a palette item, returning its id.
    fn add_box(&mut self, preset: &str, at: Point) -> Option<String>;
    fn remove_box(&mut self, id: &str);
    fn rename_box(&mut self, old: &str, new: &str) -> Result<(), RenameError>;
    fn add_link(&mut self, from: &str, to: &str) -> usize;
    fn remove_link(&mut self, index: usize);
    fn link_label(&self, index: usize) -> Option<String>;
    fn set_link_label(&mut self, index: usize, label: String);
    /// Row under `p`, if the point is on one of the box's rows.
    fn row_at(&self, id: &str, p: Point, measure: Measure) -> Option<usize>;
    /// Appends an empty row and returns its index.
    fn add_row(&mut self, id: &str) -> Option<usize>;
    /// Text of a row and where its in-place editor should point.
    fn row_text(&self, id: &str, row: usize, measure: Measure) -> Option<(String, Point)>;
    /// Applies edited row text.
    fn set_row_text(&mut self, id: &str, row: usize, text: &str) -> Result<(), String>;
    /// Draws boxes and links, returning their labels to draw on top.
    fn draw(
        &self,
        cr: &cairo::Context,
        paths: &[Vec<Point>],
        highlight: &Highlight,
        colors: &Colors,
        measure: Measure,
    ) -> Vec<Text>;
}

/// `same_as` for diagram types that implement `PartialEq`.
pub fn same<T: PartialEq + 'static>(diagram: &T, other: &dyn BoxDiagram) -> bool {
    other
        .as_any()
        .downcast_ref::<T>()
        .is_some_and(|other| other == diagram)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Box(String),
    Link(usize),
}

/// Canvas operation bound to a keyboard shortcut.
type CanvasAction = fn(&BoxCanvas);

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
        before: Box<dyn BoxDiagram>,
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
    pub struct BoxCanvas {
        pub diagram: RefCell<Option<Box<dyn BoxDiagram>>>,
        pub selection: RefCell<Option<Selection>>,
        pub hovered: RefCell<Option<String>>,
        pub viewport: Cell<Viewport>,
        pub pointer: Cell<(f64, f64)>,
        pub fit_pending: Cell<bool>,
        pub(super) drag: RefCell<Drag>,
        pub history: RefCell<History<Box<dyn BoxDiagram>>>,
        pub editor: OnceCell<InlineEditor>,
        pub editing: RefCell<Option<EditTarget>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for BoxCanvas {
        const NAME: &'static str = "MermanBoxCanvas";
        type Type = super::BoxCanvas;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("merman-canvas");
            klass.set_accessible_role(gtk::AccessibleRole::Img);
        }
    }

    impl ObjectImpl for BoxCanvas {
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

    impl WidgetImpl for BoxCanvas {
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
    pub struct BoxCanvas(ObjectSubclass<imp::BoxCanvas>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for BoxCanvas {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Navigable for BoxCanvas {
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

impl BoxCanvas {
    // ----- Public API -------------------------------------------------------

    /// Runs `f` on the diagram if it is an `M`.
    pub fn with<M: 'static, R>(&self, f: impl FnOnce(&M) -> R) -> Option<R> {
        let diagram = self.imp().diagram.borrow();
        diagram.as_ref()?.as_any().downcast_ref::<M>().map(f)
    }

    /// Applies a user edit to the diagram if it is an `M`, recording it for
    /// undo and notifying listeners.
    pub fn edit<M: 'static>(&self, f: impl FnOnce(&mut M)) {
        let _ = self.try_edit::<M, ()>(|m| {
            f(m);
            Ok(())
        });
    }

    pub fn try_edit<M: 'static, E>(
        &self,
        f: impl FnOnce(&mut M) -> Result<(), E>,
    ) -> Result<(), E> {
        self.try_edit_dyn(|diagram| match diagram.as_any_mut().downcast_mut::<M>() {
            Some(m) => f(m),
            None => Ok(()),
        })
    }

    /// Applies an edit and lays the whole diagram out again, as one undo step.
    pub fn edit_and_relayout<M: 'static>(&self, f: impl FnOnce(&mut M)) {
        let measure = self.measure();
        let _ = self.try_edit_dyn(|diagram| {
            if let Some(m) = diagram.as_any_mut().downcast_mut::<M>() {
                f(m);
                diagram.layout(true, &measure);
            }
            Ok::<_, ()>(())
        });
        self.fit_to_content();
    }

    /// Replaces the diagram with one parsed from source text. Boxes without
    /// a stored position keep the one they currently have on the canvas.
    pub fn set_diagram(&self, mut diagram: Box<dyn BoxDiagram>) {
        let imp = self.imp();
        if let Some(current) = &*imp.diagram.borrow() {
            for id in diagram.box_ids() {
                if diagram.position(&id).is_none()
                    && let Some(pos) = current.position(&id)
                {
                    diagram.set_position(&id, pos);
                }
            }
        }
        diagram.layout(false, &self.measure());
        let unchanged = imp
            .diagram
            .borrow()
            .as_ref()
            .is_some_and(|current| current.same_as(&*diagram));
        if unchanged {
            return;
        }
        if let Some(before) = imp.diagram.replace(Some(diagram)) {
            imp.history.borrow_mut().record(before);
        }
        self.after_replace();
    }

    /// Loads a new document, resetting undo history and the viewport.
    pub fn load_diagram(&self, diagram: Box<dyn BoxDiagram>) {
        let imp = self.imp();
        imp.diagram.replace(None);
        self.set_diagram(diagram);
        imp.history.borrow_mut().clear();
        self.emit_by_name::<()>("history-changed", &[]);
        self.fit_to_content();
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
        self.select(None);
        let _ = self.try_edit_dyn(|diagram| {
            match &selection {
                Selection::Box(id) => diagram.remove_box(id),
                Selection::Link(i) => diagram.remove_link(*i),
            }
            Ok::<_, ()>(())
        });
    }

    /// Adds a box for a palette item at `at`, or in the middle of the
    /// viewport, stepping aside from boxes already there.
    pub fn add_box(&self, preset: &str, at: Option<Point>) -> bool {
        let mut at = at.unwrap_or_else(|| {
            let (w, h) = self.widget_size();
            let mut at = self.viewport().get().to_world(w / 2.0, h / 2.0);
            while self.box_near(at) {
                at = Point::new(at.x + 30.0, at.y + 30.0);
            }
            at
        });
        at = snap(at);
        let mut added = None;
        let _ = self.try_edit_dyn(|diagram| {
            added = diagram.add_box(preset, at);
            Ok::<_, ()>(())
        });
        let Some(id) = added else {
            return false;
        };
        self.select(Some(Selection::Box(id)));
        self.grab_focus();
        true
    }

    /// Adds a row to the box and starts editing it in place.
    pub fn add_row(&self, id: &str) {
        let mut row = None;
        let _ = self.try_edit_dyn(|diagram| {
            row = diagram.add_row(id);
            Ok::<_, ()>(())
        });
        if let Some(row) = row {
            self.start_edit(EditTarget::Row(id.to_string(), row));
        }
    }

    pub fn rename_box(&self, old: &str, new: &str) -> Result<(), RenameError> {
        // Select the new id up front so the selection stays valid after the edit.
        let imp = self.imp();
        let previous = imp.selection.replace(Some(Selection::Box(new.to_string())));
        let result = self.try_edit_dyn(|diagram| diagram.rename_box(old, new));
        if result.is_err() {
            imp.selection.replace(previous);
        }
        result
    }

    pub fn auto_layout(&self) {
        let measure = self.measure();
        let _ = self.try_edit_dyn(|diagram| {
            diagram.layout(true, &measure);
            Ok::<_, ()>(())
        });
        self.fit_to_content();
    }

    pub fn fit_to_content(&self) {
        let measure = self.measure();
        let bounds = self.with_dyn(|diagram| {
            diagram
                .boxes(&measure)
                .into_iter()
                .map(|(_, rect)| rect)
                .reduce(|a, b| a.union(&b))
        });
        self.fit_bounds(bounds.flatten());
    }

    // ----- Diagram access ---------------------------------------------------

    fn with_dyn<R>(&self, f: impl FnOnce(&dyn BoxDiagram) -> R) -> Option<R> {
        self.imp().diagram.borrow().as_deref().map(f)
    }

    fn try_edit_dyn<E>(
        &self,
        f: impl FnOnce(&mut dyn BoxDiagram) -> Result<(), E>,
    ) -> Result<(), E> {
        let imp = self.imp();
        let Some(mut diagram) = imp.diagram.borrow().as_ref().map(|d| d.clone_diagram()) else {
            return Ok(());
        };
        f(&mut *diagram)?;
        let changed = imp
            .diagram
            .borrow()
            .as_ref()
            .is_some_and(|current| !current.same_as(&*diagram));
        if changed {
            if let Some(before) = imp.diagram.replace(Some(diagram)) {
                imp.history.borrow_mut().record(before);
            }
            self.after_replace();
            self.emit_by_name::<()>("changed", &[]);
        }
        Ok(())
    }

    // ----- Geometry and hit testing -----------------------------------------

    fn measure(&self) -> impl Fn(&str) -> (f64, f64) + '_ {
        move |text| drawing::text_size(self, text)
    }

    fn to_world(&self, x: f64, y: f64) -> Point {
        self.viewport().get().to_world(x, y)
    }

    fn zoom(&self) -> f64 {
        self.viewport().get().zoom
    }

    fn box_rect(&self, id: &str) -> Option<Rect> {
        self.with_dyn(|d| d.box_rect(id, &self.measure())).flatten()
    }

    fn box_at(&self, p: Point) -> Option<String> {
        let measure = self.measure();
        self.with_dyn(|diagram| {
            diagram
                .boxes(&measure)
                .into_iter()
                .rev()
                .find(|(_, rect)| rect.contains(p))
                .map(|(id, _)| id.to_string())
        })
        .flatten()
    }

    fn box_near(&self, p: Point) -> bool {
        let measure = self.measure();
        self.with_dyn(|diagram| {
            diagram.boxes(&measure).iter().any(|(_, rect)| {
                let c = rect.center();
                (c.x - p.x).abs() < 20.0 && (c.y - p.y).abs() < 20.0
            })
        })
        .unwrap_or(false)
    }

    fn link_paths(&self, diagram: &dyn BoxDiagram) -> Vec<Vec<Point>> {
        let measure = self.measure();
        let nodes: HashMap<&str, (Outline, Rect)> = diagram
            .boxes(&measure)
            .into_iter()
            .map(|(id, rect)| (id, (Outline::Rect, rect)))
            .collect();
        let links = diagram.links();
        let edges: Vec<(&str, &str)> = links
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        routing::route_edges(&nodes, &edges)
    }

    fn link_at(&self, p: Point) -> Option<usize> {
        let tolerance = LINK_HIT_DISTANCE / self.zoom();
        self.with_dyn(|diagram| {
            self.link_paths(diagram)
                .iter()
                .enumerate()
                .map(|(i, path)| (i, routing::distance_to_path(p, path)))
                .filter(|(_, d)| *d <= tolerance)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        })
        .flatten()
    }

    fn selected_box(&self) -> Option<String> {
        match self.selection() {
            Some(Selection::Box(id)) => Some(id),
            _ => None,
        }
    }

    /// Box whose connection handle is under `p`. Handles are shown on the
    /// hovered and the selected box.
    fn port_at(&self, p: Point) -> Option<String> {
        let radius = PORT_HIT_RADIUS / self.zoom();
        [self.imp().hovered.borrow().clone(), self.selected_box()]
            .into_iter()
            .flatten()
            .find(|id| {
                self.box_rect(id).is_some_and(|r| {
                    r.ports()
                        .iter()
                        .any(|q| (q.x - p.x).hypot(q.y - p.y) <= radius)
                })
            })
    }

    /// Center of the "add row" button below the selected box, left of the
    /// bottom connection handle.
    fn add_button_center(&self) -> Option<Point> {
        let rect = self.box_rect(&self.selected_box()?)?;
        let gap = ADD_BUTTON_GAP / self.zoom();
        Some(Point::new(rect.x + gap, rect.y + rect.h + gap))
    }

    fn on_add_button(&self, p: Point) -> bool {
        self.add_button_center().is_some_and(|c| {
            (c.x - p.x).hypot(c.y - p.y) <= drawing::ADD_BUTTON_RADIUS / self.zoom()
        })
    }

    // ----- Rendering --------------------------------------------------------

    fn draw(&self, snapshot: &gtk::Snapshot) {
        let imp = self.imp();
        let viewport = self.viewport().get();
        let zoom = viewport.zoom;
        let colors = Colors::new(self.color(), adw::StyleManager::default().is_dark());
        let diagram_ref = imp.diagram.borrow();
        let Some(diagram) = diagram_ref.as_deref() else {
            return;
        };
        let drag = imp.drag.borrow();
        let measure = self.measure();

        let mut highlight = Highlight::default();
        match self.selection() {
            Some(Selection::Box(id)) => highlight.boxes.push(id),
            Some(Selection::Link(i)) => highlight.link = Some(i),
            None => {}
        }
        if let Drag::Connect { cursor, .. } = &*drag {
            highlight.boxes.extend(self.box_at(*cursor));
        }

        let cr = drawing::world_context(snapshot, &viewport, self.widget_size());
        drawing::draw_grid(&cr, &viewport.visible(self.widget_size()), zoom, &colors);
        let paths = self.link_paths(diagram);
        let texts = diagram.draw(&cr, &paths, &highlight, &colors, &measure);

        // Interaction overlay: handles, the link being dragged and the add-row button.
        match &*drag {
            Drag::Connect { from, cursor } => {
                if let Some(rect) = diagram.box_rect(from, &measure) {
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
                for id in [imp.hovered.borrow().clone(), self.selected_box()]
                    .into_iter()
                    .flatten()
                {
                    if let Some(rect) = diagram.box_rect(&id, &measure) {
                        log_draw_error(drawing::draw_ports(
                            &cr,
                            &rect.ports(),
                            PORT_RADIUS / zoom,
                            &colors,
                        ));
                    }
                }
                if let Some(center) = self.add_button_center() {
                    log_draw_error(drawing::draw_add_button(&cr, center, zoom, &colors));
                }
            }
            _ => {}
        }
        drop(cr);

        for text in &texts {
            text.draw(self, snapshot, &viewport);
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
            move |_, value, x, y| match value.get::<String>() {
                Ok(preset) => canvas.add_box(&preset, Some(canvas.to_world(x, y))),
                Err(_) => false,
            }
        ));
        self.add_controller(drop);

        let shortcuts = gtk::ShortcutController::new();
        let bindings: [(&str, CanvasAction); 8] = [
            ("Delete|BackSpace", BoxCanvas::delete_selection),
            ("<Control>z", BoxCanvas::undo),
            ("<Control><Shift>z|<Control>y", BoxCanvas::redo),
            ("Escape", |c| c.select(None)),
            ("<Control>plus|<Control>equal|<Control>KP_Add", |c| {
                c.zoom_in()
            }),
            ("<Control>minus|<Control>KP_Subtract", |c| c.zoom_out()),
            ("<Control>0", |c| c.reset_zoom()),
            ("F2", BoxCanvas::edit_selected),
        ];
        for (trigger, handler) in bindings {
            let action = gtk::CallbackAction::new(move |widget, _| {
                if let Some(canvas) = widget.downcast_ref::<BoxCanvas>() {
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
        if self.on_add_button(world) {
            if let Some(id) = self.selected_box() {
                self.add_row(&id);
            }
            return;
        }
        let shift = gesture
            .current_event_state()
            .contains(gdk::ModifierType::SHIFT_MASK);
        let connect_from = self
            .port_at(world)
            .or_else(|| shift.then(|| self.box_at(world)).flatten());
        let drag = if let Some(from) = connect_from {
            Drag::Connect {
                from,
                cursor: world,
            }
        } else if let Some(id) = self.box_at(world) {
            self.select(Some(Selection::Box(id.clone())));
            let snapshot = self.with_dyn(|d| (d.position(&id), d.clone_diagram()));
            match snapshot {
                Some((origin, before)) => Drag::Move {
                    id,
                    origin: origin.unwrap_or(world),
                    before,
                    moved: false,
                },
                None => Drag::None,
            }
        } else if let Some(i) = self.link_at(world) {
            self.select(Some(Selection::Link(i)));
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
                // Ignore jitter so a plain click does not move or snap the box.
                if !*moved && dx.hypot(dy) < DRAG_THRESHOLD {
                    return;
                }
                *moved = true;
                let pos = snap(Point::new(origin.x + dx / zoom, origin.y + dy / zoom));
                if let Some(diagram) = imp.diagram.borrow_mut().as_mut() {
                    diagram.set_position(id, pos);
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
                imp.history.borrow_mut().record(before);
                self.emit_by_name::<()>("history-changed", &[]);
                self.emit_by_name::<()>("changed", &[]);
            }
            // Links to the same box are made in the properties panel, so a
            // stray click on a handle does not create one.
            Drag::Connect { from, cursor } => {
                if let Some(to) = self.box_at(cursor).filter(|to| *to != from) {
                    let mut index = 0;
                    let _ = self.try_edit_dyn(|diagram| {
                        index = diagram.add_link(&from, &to);
                        Ok::<_, ()>(())
                    });
                    self.select(Some(Selection::Link(index)));
                }
            }
            _ => {}
        }
        self.queue_draw();
    }

    fn on_double_click(&self, x: f64, y: f64) {
        let world = self.to_world(x, y);
        if let Some(id) = self.box_at(world) {
            let row = self
                .with_dyn(|d| d.row_at(&id, world, &self.measure()))
                .flatten();
            match row {
                Some(row) => self.start_edit(EditTarget::Row(id, row)),
                None => self.start_edit(EditTarget::Name(id)),
            }
        } else if let Some(i) = self.link_at(world) {
            self.start_edit(EditTarget::Link(i));
        } else if !self.on_add_button(world)
            && let Some(preset) = self.with_dyn(|d| d.default_preset())
            && self.add_box(preset, Some(world))
            && let Some(id) = self.selected_box()
        {
            self.start_edit(EditTarget::Name(id));
        }
    }

    fn on_motion(&self, x: f64, y: f64) {
        let imp = self.imp();
        if !matches!(*imp.drag.borrow(), Drag::None) {
            return;
        }
        let world = self.to_world(x, y);
        let on_port = self.port_at(world);
        // Keep the hovered box while the pointer is on one of its handles,
        // which stick out of the outline.
        let hovered = on_port.clone().or_else(|| self.box_at(world));
        let cursor = if on_port.is_some() {
            "crosshair"
        } else if self.on_add_button(world) {
            "pointer"
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

    // ----- Inline editing ---------------------------------------------------

    fn setup_editor(&self) {
        let commit = glib::clone!(
            #[weak(rename_to = canvas)]
            self,
            move |text: &str| canvas.commit_edit(text)
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

    fn edit_selected(&self) {
        match self.selection() {
            Some(Selection::Box(id)) => self.start_edit(EditTarget::Name(id)),
            Some(Selection::Link(i)) => self.start_edit(EditTarget::Link(i)),
            None => {}
        }
    }

    pub fn start_edit(&self, target: EditTarget) {
        let measure = self.measure();
        let found = self
            .with_dyn(|diagram| match &target {
                EditTarget::Name(id) => {
                    Some((diagram.label(id)?, diagram.header_center(id, &measure)?))
                }
                EditTarget::Row(id, row) => diagram.row_text(id, *row, &measure),
                // Links are labelled at the middle of their routed path.
                EditTarget::Link(i) => {
                    let path = self.link_paths(diagram).get(*i)?.clone();
                    Some((diagram.link_label(*i)?, routing::path_midpoint(&path)))
                }
            })
            .flatten();
        let (Some((text, anchor)), Some(editor)) = (found, self.imp().editor.get()) else {
            return;
        };
        self.select(Some(match &target {
            EditTarget::Name(id) | EditTarget::Row(id, _) => Selection::Box(id.clone()),
            EditTarget::Link(i) => Selection::Link(*i),
        }));
        self.imp().editing.replace(Some(target));
        editor.open(self.viewport().get().to_screen(anchor), &text);
    }

    fn commit_edit(&self, text: &str) {
        let Some(target) = self.imp().editing.take() else {
            return;
        };
        let text = text.trim();
        let result = match &target {
            EditTarget::Name(id) => self.apply_name(id, text),
            EditTarget::Row(id, row) => {
                self.try_edit_dyn(|diagram| diagram.set_row_text(id, *row, text))
            }
            EditTarget::Link(i) => self.try_edit_dyn(|diagram| {
                diagram.set_link_label(*i, text.to_string());
                Ok(())
            }),
        };
        if let Err(err) = result {
            tracing::debug!(error = %err, "rejected inline edit");
            self.error_bell();
        }
    }

    /// Boxes without a separate label show their id, so editing the name
    /// renames them; otherwise it changes the label, falling back to the id
    /// when cleared.
    fn apply_name(&self, id: &str, text: &str) -> Result<(), String> {
        let label = self.with_dyn(|d| d.label(id)).flatten();
        if label.as_deref() == Some(id) {
            return self.rename_box(id, text).map_err(|e| e.to_string());
        }
        let label = if text.is_empty() { id } else { text }.to_string();
        self.try_edit_dyn(|diagram| {
            diagram.set_label(id, label);
            Ok(())
        })
    }

    // ----- Internal helpers -------------------------------------------------

    fn step_history(&self, undo: bool) {
        let imp = self.imp();
        let Some(current) = imp.diagram.borrow().as_ref().map(|d| d.clone_diagram()) else {
            return;
        };
        let restored = {
            let mut history = imp.history.borrow_mut();
            if undo {
                history.undo(current)
            } else {
                history.redo(current)
            }
        };
        if let Some(diagram) = restored {
            imp.diagram.replace(Some(diagram));
            self.after_replace();
            self.emit_by_name::<()>("changed", &[]);
        }
    }

    /// Drops selection and hover state that no longer match the diagram.
    fn after_replace(&self) {
        let imp = self.imp();
        let valid = self
            .with_dyn(|diagram| match &*imp.selection.borrow() {
                Some(Selection::Box(id)) => diagram.box_ids().contains(id),
                Some(Selection::Link(i)) => *i < diagram.links().len(),
                None => true,
            })
            .unwrap_or(false);
        if !valid {
            imp.selection.replace(None);
        }
        imp.hovered.take();
        self.emit_by_name::<()>("history-changed", &[]);
        self.emit_by_name::<()>("selection-changed", &[]);
        self.queue_draw();
    }
}

fn snap(p: Point) -> Point {
    Point::new((p.x / SNAP).round() * SNAP, (p.y / SNAP).round() * SNAP)
}
