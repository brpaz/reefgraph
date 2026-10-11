//! Interactive class diagram canvas: classes are moved by dragging, related
//! by dragging from their handles, and edited in place.

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::OnceLock;

use gtk::glib::subclass::Signal;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib};

use super::render;
use crate::class_diagram::layout::{self, ClassBox, annotation_text};
use crate::class_diagram::model::{Direction, RenameError};
use crate::class_diagram::{Class, ClassDiagram, Member};
use crate::drawing::{self, Colors, log_draw_error};
use crate::geometry::{Point, Rect};
use crate::history::History;
use crate::interaction::{InlineEditor, Navigable, install_navigation};
use crate::routing::{self, Outline};
use crate::viewport::Viewport;

/// Screen-space radius of connection handles and their hit area.
const PORT_RADIUS: f64 = 5.0;
const PORT_HIT_RADIUS: f64 = 10.0;
const EDGE_HIT_DISTANCE: f64 = 6.0;
/// Gap between a class and its "add attribute" button.
const ADD_BUTTON_GAP: f64 = 16.0;
const SNAP: f64 = 10.0;
/// Pointer travel before a press on an element starts moving it.
const DRAG_THRESHOLD: f64 = 4.0;

/// Canvas operation bound to a keyboard shortcut.
type CanvasAction = fn(&Canvas);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Class(String),
    Relation(usize),
}

/// Text being edited in place.
#[derive(Debug, Clone, PartialEq, Eq)]
enum EditTarget {
    /// The class's alias, or its name when it has no alias.
    Name(String),
    Member(String, usize),
    Relation(usize),
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
        before: Box<ClassDiagram>,
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
        pub diagram: RefCell<ClassDiagram>,
        pub selection: RefCell<Option<Selection>>,
        pub hovered: RefCell<Option<String>>,
        pub viewport: Cell<Viewport>,
        pub pointer: Cell<(f64, f64)>,
        pub fit_pending: Cell<bool>,
        pub(super) drag: RefCell<Drag>,
        pub history: RefCell<History<ClassDiagram>>,
        pub editor: OnceCell<InlineEditor>,
        pub(super) editing: RefCell<Option<EditTarget>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Canvas {
        const NAME: &'static str = "MermanClassCanvas";
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

    pub fn with_diagram<R>(&self, f: impl FnOnce(&ClassDiagram) -> R) -> R {
        f(&self.imp().diagram.borrow())
    }

    /// Replaces the diagram with one parsed from source text. Classes without
    /// a stored position keep the one they currently have on the canvas.
    pub fn set_diagram(&self, mut diagram: ClassDiagram) {
        let imp = self.imp();
        {
            let current = imp.diagram.borrow();
            for class in diagram.classes.iter_mut().filter(|e| e.pos.is_none()) {
                class.pos = current.class(&class.id).and_then(|e| e.pos);
            }
        }
        layout::layout_missing(&mut diagram, |e| self.class_box(e).size);
        if *imp.diagram.borrow() == diagram {
            return;
        }
        let before = imp.diagram.replace(diagram);
        imp.history.borrow_mut().record(before);
        self.after_replace();
    }

    /// Loads a new document, resetting undo history and the viewport.
    pub fn load_diagram(&self, diagram: ClassDiagram) {
        let imp = self.imp();
        imp.diagram.replace(ClassDiagram::default());
        self.set_diagram(diagram);
        imp.history.borrow_mut().clear();
        self.emit_by_name::<()>("history-changed", &[]);
        self.fit_to_content();
    }

    /// Applies a user edit, recording it for undo and notifying listeners.
    pub fn edit(&self, f: impl FnOnce(&mut ClassDiagram)) {
        let _ = self.try_edit(|diagram| {
            f(diagram);
            Ok::<_, Infallible>(())
        });
    }

    pub fn try_edit<E>(&self, f: impl FnOnce(&mut ClassDiagram) -> Result<(), E>) -> Result<(), E> {
        let imp = self.imp();
        let mut diagram = imp.diagram.borrow().clone();
        f(&mut diagram)?;
        if *imp.diagram.borrow() != diagram {
            let before = imp.diagram.replace(diagram);
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
        self.select(None);
        self.edit(|diagram| match &selection {
            Selection::Class(id) => diagram.remove_class(id),
            Selection::Relation(i) if *i < diagram.relations.len() => {
                diagram.relations.remove(*i);
            }
            Selection::Relation(_) => {}
        });
    }

    pub fn add_class(&self, annotations: Vec<String>, at: Point) -> String {
        let mut id = String::new();
        self.edit(|diagram| id = diagram.add_class(annotations, snap(at)));
        self.select(Some(Selection::Class(id.clone())));
        self.grab_focus();
        id
    }

    /// Adds a class in the middle of the viewport, stepping aside from
    /// classes already sitting there.
    pub fn add_class_at_center(&self, annotations: Vec<String>) {
        let (w, h) = self.widget_size();
        let mut at = self.viewport().get().to_world(w / 2.0, h / 2.0);
        while self.with_diagram(|diagram| {
            diagram.classes.iter().any(|e| {
                e.pos
                    .is_some_and(|p| (p.x - at.x).abs() < 20.0 && (p.y - at.y).abs() < 20.0)
            })
        }) {
            at = Point::new(at.x + 30.0, at.y + 30.0);
        }
        self.add_class(annotations, at);
    }

    /// Adds an attribute to the class and starts editing it in place.
    pub fn add_member(&self, id: &str) {
        let mut index = None;
        self.edit(|diagram| {
            if let Some(class) = diagram.class_mut(id) {
                class.members.push(Member("+field".into()));
                index = Some(class.members.len() - 1);
            }
        });
        if let Some(index) = index {
            self.start_edit(EditTarget::Member(id.to_string(), index));
        }
    }

    pub fn rename_class(&self, old: &str, new: &str) -> Result<(), RenameError> {
        // Select the new id up front so the selection stays valid after the edit.
        let imp = self.imp();
        let previous = imp
            .selection
            .replace(Some(Selection::Class(new.to_string())));
        let result = self.try_edit(|diagram| diagram.rename_class(old, new));
        if result.is_err() {
            imp.selection.replace(previous);
        }
        result
    }

    /// Changes the layout direction and re-lays out the diagram to match it.
    pub fn set_direction(&self, direction: Option<Direction>) {
        self.edit(|diagram| {
            diagram.direction = direction;
            layout::layout_all(diagram, |e| self.class_box(e).size);
        });
        self.fit_to_content();
    }

    pub fn auto_layout(&self) {
        self.edit(|diagram| layout::layout_all(diagram, |e| self.class_box(e).size));
        self.fit_to_content();
    }

    pub fn fit_to_content(&self) {
        let bounds = self.with_diagram(|diagram| {
            diagram
                .classes
                .iter()
                .filter_map(|e| self.class_rect(e))
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

    fn class_box(&self, class: &Class) -> ClassBox {
        layout::class_box(class, |text| drawing::text_size(self, text))
    }

    fn class_rect(&self, class: &Class) -> Option<Rect> {
        Some(self.class_box(class).rect(class.pos?))
    }

    fn class_at(&self, p: Point) -> Option<String> {
        self.with_diagram(|diagram| {
            diagram
                .classes
                .iter()
                .rev()
                .find(|e| self.class_rect(e).is_some_and(|r| r.contains(p)))
                .map(|e| e.id.clone())
        })
    }

    /// Index in `members` of the member of class `id` shown under `p`.
    fn member_at(&self, id: &str, p: Point) -> Option<usize> {
        self.with_diagram(|diagram| {
            let class = diagram.class(id)?;
            let row = self.class_box(class).row_at(class.pos?, p)?;
            class.display_order().get(row).copied()
        })
    }

    fn relation_at(&self, p: Point) -> Option<usize> {
        let tolerance = EDGE_HIT_DISTANCE / self.zoom();
        self.with_diagram(|diagram| {
            self.relation_paths(diagram)
                .iter()
                .enumerate()
                .map(|(i, path)| (i, routing::distance_to_path(p, path)))
                .filter(|(_, d)| *d <= tolerance)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        })
    }

    fn selected_class(&self) -> Option<String> {
        match self.selection() {
            Some(Selection::Class(id)) => Some(id),
            _ => None,
        }
    }

    /// Class whose connection handle is under `p`. Handles are shown on the
    /// hovered and the selected class.
    fn port_at(&self, p: Point) -> Option<String> {
        let radius = PORT_HIT_RADIUS / self.zoom();
        let candidates = [self.imp().hovered.borrow().clone(), self.selected_class()];
        self.with_diagram(|diagram| {
            candidates.into_iter().flatten().find(|id| {
                diagram
                    .class(id)
                    .and_then(|e| self.class_rect(e))
                    .is_some_and(|r| {
                        r.ports()
                            .iter()
                            .any(|q| (q.x - p.x).hypot(q.y - p.y) <= radius)
                    })
            })
        })
    }

    /// Center of the "add member" button below the selected class.
    fn add_button_center(&self) -> Option<Point> {
        let id = self.selected_class()?;
        let rect = self.with_diagram(|diagram| self.class_rect(diagram.class(&id)?))?;
        // Left of center, clear of the bottom connection handle.
        Some(Point::new(
            rect.x + ADD_BUTTON_GAP / self.zoom(),
            rect.y + rect.h + ADD_BUTTON_GAP / self.zoom(),
        ))
    }

    fn on_add_button(&self, p: Point) -> bool {
        self.add_button_center().is_some_and(|c| {
            (c.x - p.x).hypot(c.y - p.y) <= drawing::ADD_BUTTON_RADIUS / self.zoom()
        })
    }

    fn relation_paths(&self, diagram: &ClassDiagram) -> Vec<Vec<Point>> {
        let nodes: HashMap<&str, (Outline, Rect)> = diagram
            .classes
            .iter()
            .filter_map(|e| Some((e.id.as_str(), (Outline::Rect, self.class_rect(e)?))))
            .collect();
        let edges: Vec<(&str, &str)> = diagram
            .relations
            .iter()
            .map(|r| (r.from.as_str(), r.to.as_str()))
            .collect();
        routing::route_edges(&nodes, &edges)
    }

    // ----- Rendering --------------------------------------------------------

    fn draw(&self, snapshot: &gtk::Snapshot) {
        let imp = self.imp();
        let viewport = self.viewport().get();
        let zoom = viewport.zoom;
        let colors = Colors::new(self.color(), adw::StyleManager::default().is_dark());
        let diagram = imp.diagram.borrow();
        let selection = imp.selection.borrow().clone();
        let drag = imp.drag.borrow();
        let mut texts: Vec<Text> = Vec::new();

        let paths = self.relation_paths(&diagram);
        let classes: Vec<_> = diagram
            .classes
            .iter()
            .filter_map(|e| {
                let pos = e.pos?;
                let layout = self.class_box(e);
                Some((e, layout.rect(pos), layout))
            })
            .collect();
        let connect_target = match &*drag {
            Drag::Connect { cursor, .. } => self.class_at(*cursor),
            _ => None,
        };

        let cr = drawing::world_context(snapshot, &viewport, self.widget_size());
        drawing::draw_grid(&cr, &viewport.visible(self.widget_size()), zoom, &colors);

        for (i, (r, path)) in diagram.relations.iter().zip(&paths).enumerate() {
            log_draw_error(render::draw_relation(
                &cr,
                path,
                r.from_head,
                r.to_head,
                r.dashed,
                &colors,
                selection == Some(Selection::Relation(i)),
            ));
            if path.len() < 2 {
                continue;
            }
            if !r.label.is_empty() {
                let at = routing::path_midpoint(path);
                let (w, h) = drawing::text_size(self, &r.label);
                log_draw_error(drawing::draw_label_background(
                    &cr,
                    &Rect::centered(at, (w + 8.0, h + 4.0)),
                    &colors,
                ));
                texts.push(Text::plain(r.label.clone(), at, colors.fg));
            }
            let n = path.len();
            for (cardinality, at, toward) in [
                (&r.from_cardinality, path[0], path[1]),
                (&r.to_cardinality, path[n - 1], path[n - 2]),
            ] {
                if !cardinality.is_empty() {
                    texts.push(Text::plain(
                        cardinality.clone(),
                        render::cardinality_position(at, toward),
                        colors.fg,
                    ));
                }
            }
        }

        let dim = colors.fg.with_alpha(0.65);
        for (class, rect, layout) in &classes {
            let highlighted = selection == Some(Selection::Class(class.id.clone()))
                || connect_target.as_deref() == Some(class.id.as_str());
            log_draw_error(render::draw_class(&cr, rect, layout, &colors, highlighted));

            let mut y = rect.y + layout::HEADER_PADDING / 2.0;
            for annotation in &class.annotations {
                let text = annotation_text(annotation);
                let (_, h) = drawing::text_size(self, &text);
                texts.push(Text::plain(
                    text,
                    Point::new(rect.center().x, y + h / 2.0),
                    dim,
                ));
                y += h;
            }
            let title = class.title();
            let (_, h) = drawing::text_size(self, &title);
            texts.push(Text {
                bold: true,
                ..Text::plain(title, Point::new(rect.center().x, y + h / 2.0), colors.fg)
            });

            let center = class.pos.unwrap_or_default();
            for (row, index) in class.display_order().into_iter().enumerate() {
                let display = class.members[index].display();
                let (w, _) = drawing::text_size(self, &display.text);
                let row_center = layout.row_rect(center, row).center();
                texts.push(Text {
                    italic: display.is_abstract,
                    underline: display.is_static,
                    ..Text::plain(
                        display.text,
                        Point::new(rect.x + layout::PADDING + w / 2.0, row_center.y),
                        colors.fg,
                    )
                });
            }
        }

        // Interaction overlay: handles, the relation being dragged and the
        // add-member button.
        match &*drag {
            Drag::Connect { from, cursor } => {
                if let Some(rect) = diagram.class(from).and_then(|e| self.class_rect(e)) {
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
                for id in [imp.hovered.borrow().clone(), self.selected_class()]
                    .into_iter()
                    .flatten()
                {
                    if let Some(rect) = diagram.class(&id).and_then(|e| self.class_rect(e)) {
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
            let layout = drawing::label_layout(self, &text.text);
            drawing::apply_style(&layout, text.bold, text.italic, text.underline);
            drawing::draw_text(snapshot, &viewport, &layout, text.at, &text.color);
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
                let preset = value
                    .get::<String>()
                    .ok()
                    .and_then(|id| super::ClassPreset::from_id(&id));
                let Some(preset) = preset else {
                    return false;
                };
                canvas.add_class(preset.annotations(), canvas.to_world(x, y));
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
            ("<Control>plus|<Control>equal|<Control>KP_Add", |c| {
                c.zoom_in()
            }),
            ("<Control>minus|<Control>KP_Subtract", |c| c.zoom_out()),
            ("<Control>0", |c| c.reset_zoom()),
            ("F2", Canvas::edit_selected),
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
        if self.on_add_button(world) {
            if let Some(id) = self.selected_class() {
                self.add_member(&id);
            }
            return;
        }
        let shift = gesture
            .current_event_state()
            .contains(gdk::ModifierType::SHIFT_MASK);
        let connect_from = self
            .port_at(world)
            .or_else(|| shift.then(|| self.class_at(world)).flatten());
        let drag = if let Some(from) = connect_from {
            Drag::Connect {
                from,
                cursor: world,
            }
        } else if let Some(id) = self.class_at(world) {
            self.select(Some(Selection::Class(id.clone())));
            let origin = self
                .with_diagram(|diagram| diagram.class(&id).and_then(|e| e.pos))
                .unwrap_or(world);
            Drag::Move {
                id,
                origin,
                before: Box::new(imp.diagram.borrow().clone()),
                moved: false,
            }
        } else if let Some(i) = self.relation_at(world) {
            self.select(Some(Selection::Relation(i)));
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
                if let Some(class) = imp.diagram.borrow_mut().class_mut(id) {
                    *moved = true;
                    class.pos = Some(pos);
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
                if let Some(to) = self.class_at(cursor).filter(|to| *to != from) {
                    let mut index = 0;
                    self.edit(|diagram| index = diagram.add_relation(&from, &to));
                    self.select(Some(Selection::Relation(index)));
                }
            }
            _ => {}
        }
        self.queue_draw();
    }

    fn on_double_click(&self, x: f64, y: f64) {
        let world = self.to_world(x, y);
        if let Some(id) = self.class_at(world) {
            match self.member_at(&id, world) {
                Some(row) => self.start_edit(EditTarget::Member(id, row)),
                None => self.start_edit(EditTarget::Name(id)),
            }
        } else if let Some(i) = self.relation_at(world) {
            self.start_edit(EditTarget::Relation(i));
        } else if !self.on_add_button(world) {
            let id = self.add_class(Vec::new(), world);
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
        // Keep the hovered class while the pointer is on one of its handles,
        // which stick out of the outline.
        let hovered = on_port.clone().or_else(|| self.class_at(world));
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
            Some(Selection::Class(id)) => self.start_edit(EditTarget::Name(id)),
            Some(Selection::Relation(i)) => self.start_edit(EditTarget::Relation(i)),
            None => {}
        }
    }

    fn start_edit(&self, target: EditTarget) {
        let found = self.with_diagram(|diagram| match &target {
            EditTarget::Name(id) => {
                let class = diagram.class(id)?;
                let layout = self.class_box(class);
                let rect = layout.rect(class.pos?);
                Some((
                    class.label.clone(),
                    Point::new(rect.center().x, rect.y + layout.header_height / 2.0),
                ))
            }
            EditTarget::Member(id, index) => {
                let class = diagram.class(id)?;
                let text = class.members.get(*index)?.0.clone();
                let row = class.display_order().iter().position(|i| i == index)?;
                let at = self.class_box(class).row_rect(class.pos?, row).center();
                Some((text, at))
            }
            EditTarget::Relation(i) => {
                let label = diagram.relations.get(*i)?.label.clone();
                let path = self.relation_paths(diagram).swap_remove(*i);
                Some((label, routing::path_midpoint(&path)))
            }
        });
        let (Some((text, anchor)), Some(editor)) = (found, self.imp().editor.get()) else {
            return;
        };
        self.select(Some(match &target {
            EditTarget::Name(id) | EditTarget::Member(id, _) => Selection::Class(id.clone()),
            EditTarget::Relation(i) => Selection::Relation(*i),
        }));
        self.imp().editing.replace(Some(target));
        editor.open(self.viewport().get().to_screen(anchor), &text);
    }

    fn commit_edit(&self, text: &str) {
        let Some(target) = self.imp().editing.take() else {
            return;
        };
        let text = text.trim();
        let result = match target {
            EditTarget::Name(id) => {
                // Classes without a label show their name, so editing it renames.
                let has_alias = self
                    .with_diagram(|diagram| diagram.class(&id).is_some_and(|e| e.label != e.id));
                if has_alias {
                    self.edit(|diagram| {
                        if let Some(e) = diagram.class_mut(&id) {
                            e.label = if text.is_empty() {
                                id.clone()
                            } else {
                                text.to_string()
                            };
                        }
                    });
                    Ok(())
                } else {
                    self.rename_class(&id, text).map_err(|e| e.to_string())
                }
            }
            EditTarget::Member(id, index) => {
                // Clearing a member's text removes it.
                self.edit(|diagram| {
                    if let Some(class) = diagram.class_mut(&id)
                        && index < class.members.len()
                    {
                        if text.is_empty() {
                            class.members.remove(index);
                        } else {
                            class.members[index] = Member(text.to_string());
                        }
                    }
                });
                Ok(())
            }
            EditTarget::Relation(i) => {
                self.edit(|diagram| {
                    if let Some(r) = diagram.relations.get_mut(i) {
                        r.label = text.to_string();
                    }
                });
                Ok(())
            }
        };
        if let Err(err) = result {
            tracing::debug!(error = %err, "rejected inline edit");
            self.error_bell();
        }
    }

    // ----- Internal helpers -------------------------------------------------

    fn step_history(&self, undo: bool) {
        let imp = self.imp();
        let current = imp.diagram.borrow().clone();
        let restored = {
            let mut history = imp.history.borrow_mut();
            if undo {
                history.undo(current)
            } else {
                history.redo(current)
            }
        };
        if let Some(diagram) = restored {
            imp.diagram.replace(diagram);
            self.after_replace();
            self.emit_by_name::<()>("changed", &[]);
        }
    }

    /// Drops selection and hover state that no longer match the diagram.
    fn after_replace(&self) {
        let imp = self.imp();
        let valid = self.with_diagram(|diagram| match &*imp.selection.borrow() {
            Some(Selection::Class(id)) => diagram.class(id).is_some(),
            Some(Selection::Relation(i)) => *i < diagram.relations.len(),
            None => true,
        });
        if !valid {
            imp.selection.replace(None);
        }
        imp.hovered.take();
        self.emit_by_name::<()>("history-changed", &[]);
        self.emit_by_name::<()>("selection-changed", &[]);
        self.queue_draw();
    }
}

/// Label queued for drawing after the geometry.
struct Text {
    text: String,
    at: Point,
    color: gdk::RGBA,
    bold: bool,
    italic: bool,
    underline: bool,
}

impl Text {
    fn plain(text: String, at: Point, color: gdk::RGBA) -> Self {
        Self {
            text,
            at,
            color,
            bold: false,
            italic: false,
            underline: false,
        }
    }
}

fn snap(p: Point) -> Point {
    Point::new((p.x / SNAP).round() * SNAP, (p.y / SNAP).round() * SNAP)
}
