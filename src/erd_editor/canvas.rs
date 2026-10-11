//! Interactive entity relationship canvas: entities are moved by dragging,
//! related by dragging from their handles, and edited in place.

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::OnceLock;

use gtk::glib::subclass::Signal;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib};

use super::render;
use crate::drawing::{self, Colors, log_draw_error};
use crate::erd::layout::{self, EntityBox, attribute_cells};
use crate::erd::model::{Direction, RenameError};
use crate::erd::{Attribute, Entity, Erd};
use crate::geometry::{Point, Rect};
use crate::history::History;
use crate::interaction::{InlineEditor, Navigable, install_navigation};
use crate::routing::{self, Outline};
use crate::viewport::Viewport;

/// Screen-space radius of connection handles and their hit area.
const PORT_RADIUS: f64 = 5.0;
const PORT_HIT_RADIUS: f64 = 10.0;
const EDGE_HIT_DISTANCE: f64 = 6.0;
/// Gap between an entity and its "add attribute" button.
const ADD_BUTTON_GAP: f64 = 16.0;
const SNAP: f64 = 10.0;
/// Pointer travel before a press on an element starts moving it.
const DRAG_THRESHOLD: f64 = 4.0;

/// Canvas operation bound to a keyboard shortcut.
type CanvasAction = fn(&Canvas);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Entity(String),
    Relationship(usize),
}

/// Text being edited in place.
#[derive(Debug, Clone, PartialEq, Eq)]
enum EditTarget {
    /// The entity's alias, or its name when it has no alias.
    Name(String),
    Attribute(String, usize),
    Relationship(usize),
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
        before: Box<Erd>,
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
        pub erd: RefCell<Erd>,
        pub selection: RefCell<Option<Selection>>,
        pub hovered: RefCell<Option<String>>,
        pub viewport: Cell<Viewport>,
        pub pointer: Cell<(f64, f64)>,
        pub fit_pending: Cell<bool>,
        pub(super) drag: RefCell<Drag>,
        pub history: RefCell<History<Erd>>,
        pub editor: OnceCell<InlineEditor>,
        pub(super) editing: RefCell<Option<EditTarget>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Canvas {
        const NAME: &'static str = "MermanErdCanvas";
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

    pub fn with_erd<R>(&self, f: impl FnOnce(&Erd) -> R) -> R {
        f(&self.imp().erd.borrow())
    }

    /// Replaces the diagram with one parsed from source text. Entities without
    /// a stored position keep the one they currently have on the canvas.
    pub fn set_erd(&self, mut erd: Erd) {
        let imp = self.imp();
        {
            let current = imp.erd.borrow();
            for entity in erd.entities.iter_mut().filter(|e| e.pos.is_none()) {
                entity.pos = current.entity(&entity.id).and_then(|e| e.pos);
            }
        }
        layout::layout_missing(&mut erd, |e| self.entity_box(e).size);
        if *imp.erd.borrow() == erd {
            return;
        }
        let before = imp.erd.replace(erd);
        imp.history.borrow_mut().record(before);
        self.after_replace();
    }

    /// Loads a new document, resetting undo history and the viewport.
    pub fn load_erd(&self, erd: Erd) {
        let imp = self.imp();
        imp.erd.replace(Erd::default());
        self.set_erd(erd);
        imp.history.borrow_mut().clear();
        self.emit_by_name::<()>("history-changed", &[]);
        self.fit_to_content();
    }

    /// Applies a user edit, recording it for undo and notifying listeners.
    pub fn edit(&self, f: impl FnOnce(&mut Erd)) {
        let _ = self.try_edit(|erd| {
            f(erd);
            Ok::<_, Infallible>(())
        });
    }

    pub fn try_edit<E>(&self, f: impl FnOnce(&mut Erd) -> Result<(), E>) -> Result<(), E> {
        let imp = self.imp();
        let mut erd = imp.erd.borrow().clone();
        f(&mut erd)?;
        if *imp.erd.borrow() != erd {
            let before = imp.erd.replace(erd);
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
        self.edit(|erd| match &selection {
            Selection::Entity(id) => erd.remove_entity(id),
            Selection::Relationship(i) if *i < erd.relationships.len() => {
                erd.relationships.remove(*i);
            }
            Selection::Relationship(_) => {}
        });
    }

    pub fn add_entity(&self, attributes: Vec<Attribute>, at: Point) -> String {
        let mut id = String::new();
        self.edit(|erd| id = erd.add_entity(attributes, snap(at)));
        self.select(Some(Selection::Entity(id.clone())));
        self.grab_focus();
        id
    }

    /// Adds an entity in the middle of the viewport, stepping aside from
    /// entities already sitting there.
    pub fn add_entity_at_center(&self, attributes: Vec<Attribute>) {
        let (w, h) = self.widget_size();
        let mut at = self.viewport().get().to_world(w / 2.0, h / 2.0);
        while self.with_erd(|erd| {
            erd.entities.iter().any(|e| {
                e.pos
                    .is_some_and(|p| (p.x - at.x).abs() < 20.0 && (p.y - at.y).abs() < 20.0)
            })
        }) {
            at = Point::new(at.x + 30.0, at.y + 30.0);
        }
        self.add_entity(attributes, at);
    }

    /// Adds an attribute to the entity and starts editing it in place.
    pub fn add_attribute(&self, id: &str) {
        let mut row = None;
        self.edit(|erd| {
            if let Some(entity) = erd.entity_mut(id) {
                entity
                    .attributes
                    .push(Attribute::new("string", "attribute"));
                row = Some(entity.attributes.len() - 1);
            }
        });
        if let Some(row) = row {
            self.start_edit(EditTarget::Attribute(id.to_string(), row));
        }
    }

    pub fn rename_entity(&self, old: &str, new: &str) -> Result<(), RenameError> {
        // Select the new id up front so the selection stays valid after the edit.
        let imp = self.imp();
        let previous = imp
            .selection
            .replace(Some(Selection::Entity(new.to_string())));
        let result = self.try_edit(|erd| erd.rename_entity(old, new));
        if result.is_err() {
            imp.selection.replace(previous);
        }
        result
    }

    /// Changes the layout direction and re-lays out the diagram to match it.
    pub fn set_direction(&self, direction: Option<Direction>) {
        self.edit(|erd| {
            erd.direction = direction;
            layout::layout_all(erd, |e| self.entity_box(e).size);
        });
        self.fit_to_content();
    }

    pub fn auto_layout(&self) {
        self.edit(|erd| layout::layout_all(erd, |e| self.entity_box(e).size));
        self.fit_to_content();
    }

    pub fn fit_to_content(&self) {
        let bounds = self.with_erd(|erd| {
            erd.entities
                .iter()
                .filter_map(|e| self.entity_rect(e))
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

    fn entity_box(&self, entity: &Entity) -> EntityBox {
        layout::entity_box(entity, |text| drawing::text_size(self, text))
    }

    fn entity_rect(&self, entity: &Entity) -> Option<Rect> {
        Some(self.entity_box(entity).rect(entity.pos?))
    }

    fn entity_at(&self, p: Point) -> Option<String> {
        self.with_erd(|erd| {
            erd.entities
                .iter()
                .rev()
                .find(|e| self.entity_rect(e).is_some_and(|r| r.contains(p)))
                .map(|e| e.id.clone())
        })
    }

    /// Attribute row of entity `id` under `p`.
    fn row_at(&self, id: &str, p: Point) -> Option<usize> {
        self.with_erd(|erd| {
            let entity = erd.entity(id)?;
            let row = self.entity_box(entity).row_at(entity.pos?, p)?;
            (row < entity.attributes.len()).then_some(row)
        })
    }

    fn relationship_at(&self, p: Point) -> Option<usize> {
        let tolerance = EDGE_HIT_DISTANCE / self.zoom();
        self.with_erd(|erd| {
            self.relationship_paths(erd)
                .iter()
                .enumerate()
                .map(|(i, path)| (i, routing::distance_to_path(p, path)))
                .filter(|(_, d)| *d <= tolerance)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        })
    }

    fn selected_entity(&self) -> Option<String> {
        match self.selection() {
            Some(Selection::Entity(id)) => Some(id),
            _ => None,
        }
    }

    /// Entity whose connection handle is under `p`. Handles are shown on the
    /// hovered and the selected entity.
    fn port_at(&self, p: Point) -> Option<String> {
        let radius = PORT_HIT_RADIUS / self.zoom();
        let candidates = [self.imp().hovered.borrow().clone(), self.selected_entity()];
        self.with_erd(|erd| {
            candidates.into_iter().flatten().find(|id| {
                erd.entity(id)
                    .and_then(|e| self.entity_rect(e))
                    .is_some_and(|r| {
                        r.ports()
                            .iter()
                            .any(|q| (q.x - p.x).hypot(q.y - p.y) <= radius)
                    })
            })
        })
    }

    /// Center of the "add attribute" button below the selected entity.
    fn add_button_center(&self) -> Option<Point> {
        let id = self.selected_entity()?;
        let rect = self.with_erd(|erd| self.entity_rect(erd.entity(&id)?))?;
        // Left of center, clear of the bottom connection handle.
        Some(Point::new(
            rect.x + ADD_BUTTON_GAP / self.zoom(),
            rect.y + rect.h + ADD_BUTTON_GAP / self.zoom(),
        ))
    }

    fn on_add_button(&self, p: Point) -> bool {
        self.add_button_center().is_some_and(|c| {
            (c.x - p.x).hypot(c.y - p.y) <= render::ADD_BUTTON_RADIUS / self.zoom()
        })
    }

    fn relationship_paths(&self, erd: &Erd) -> Vec<Vec<Point>> {
        let nodes: HashMap<&str, (Outline, Rect)> = erd
            .entities
            .iter()
            .filter_map(|e| Some((e.id.as_str(), (Outline::Rect, self.entity_rect(e)?))))
            .collect();
        let edges: Vec<(&str, &str)> = erd
            .relationships
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
        let erd = imp.erd.borrow();
        let selection = imp.selection.borrow().clone();
        let drag = imp.drag.borrow();
        let mut texts: Vec<(String, Point, gdk::RGBA)> = Vec::new();

        let paths = self.relationship_paths(&erd);
        let entities: Vec<_> = erd
            .entities
            .iter()
            .filter_map(|e| {
                let pos = e.pos?;
                let layout = self.entity_box(e);
                Some((e, layout.rect(pos), layout))
            })
            .collect();
        let connect_target = match &*drag {
            Drag::Connect { cursor, .. } => self.entity_at(*cursor),
            _ => None,
        };

        let cr = drawing::world_context(snapshot, &viewport, self.widget_size());
        drawing::draw_grid(&cr, &viewport.visible(self.widget_size()), zoom, &colors);

        for (i, (r, path)) in erd.relationships.iter().zip(&paths).enumerate() {
            log_draw_error(render::draw_relationship(
                &cr,
                path,
                r.from_cardinality,
                r.to_cardinality,
                r.identifying,
                &colors,
                selection == Some(Selection::Relationship(i)),
            ));
            if !r.label.is_empty() && !path.is_empty() {
                let at = routing::path_midpoint(path);
                let (w, h) = drawing::text_size(self, &r.label);
                log_draw_error(drawing::draw_label_background(
                    &cr,
                    &Rect::centered(at, (w + 8.0, h + 4.0)),
                    &colors,
                ));
                texts.push((r.label.clone(), at, colors.fg));
            }
        }

        let dim = colors.fg.with_alpha(0.6);
        for (entity, rect, layout) in &entities {
            let highlighted = selection == Some(Selection::Entity(entity.id.clone()))
                || connect_target.as_deref() == Some(entity.id.as_str());
            log_draw_error(render::draw_entity(
                &cr,
                rect,
                layout,
                entity.attributes.len(),
                &colors,
                highlighted,
            ));
            texts.push((
                entity.label.clone(),
                Point::new(rect.center().x, rect.y + layout.header_height / 2.0),
                colors.fg,
            ));
            for row in 0..entity.attributes.len() {
                let y = layout
                    .row_rect(entity.pos.unwrap_or_default(), row)
                    .center()
                    .y;
                for (col, cell) in attribute_cells(entity, row).into_iter().enumerate() {
                    if cell.is_empty() {
                        continue;
                    }
                    let (w, _) = drawing::text_size(self, &cell);
                    let x = rect.x + layout.columns[col] + w / 2.0;
                    let color = if col == 3 { dim } else { colors.fg };
                    texts.push((cell, Point::new(x, y), color));
                }
            }
        }

        // Interaction overlay: handles, the relationship being dragged and the
        // add-attribute button.
        match &*drag {
            Drag::Connect { from, cursor } => {
                if let Some(rect) = erd.entity(from).and_then(|e| self.entity_rect(e)) {
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
                for id in [imp.hovered.borrow().clone(), self.selected_entity()]
                    .into_iter()
                    .flatten()
                {
                    if let Some(rect) = erd.entity(&id).and_then(|e| self.entity_rect(e)) {
                        log_draw_error(drawing::draw_ports(
                            &cr,
                            &rect.ports(),
                            PORT_RADIUS / zoom,
                            &colors,
                        ));
                    }
                }
                if let Some(center) = self.add_button_center() {
                    log_draw_error(render::draw_add_button(&cr, center, zoom, &colors));
                }
            }
            _ => {}
        }
        drop(cr);

        for (text, at, color) in &texts {
            drawing::draw_text(
                snapshot,
                &viewport,
                &drawing::label_layout(self, text),
                *at,
                color,
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
                let preset = value
                    .get::<String>()
                    .ok()
                    .and_then(|id| super::EntityPreset::from_id(&id));
                let Some(preset) = preset else {
                    return false;
                };
                canvas.add_entity(preset.attributes(), canvas.to_world(x, y));
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
            if let Some(id) = self.selected_entity() {
                self.add_attribute(&id);
            }
            return;
        }
        let shift = gesture
            .current_event_state()
            .contains(gdk::ModifierType::SHIFT_MASK);
        let connect_from = self
            .port_at(world)
            .or_else(|| shift.then(|| self.entity_at(world)).flatten());
        let drag = if let Some(from) = connect_from {
            Drag::Connect {
                from,
                cursor: world,
            }
        } else if let Some(id) = self.entity_at(world) {
            self.select(Some(Selection::Entity(id.clone())));
            let origin = self
                .with_erd(|erd| erd.entity(&id).and_then(|e| e.pos))
                .unwrap_or(world);
            Drag::Move {
                id,
                origin,
                before: Box::new(imp.erd.borrow().clone()),
                moved: false,
            }
        } else if let Some(i) = self.relationship_at(world) {
            self.select(Some(Selection::Relationship(i)));
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
                if let Some(entity) = imp.erd.borrow_mut().entity_mut(id) {
                    *moved = true;
                    entity.pos = Some(pos);
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
                if let Some(to) = self.entity_at(cursor).filter(|to| *to != from) {
                    let mut index = 0;
                    self.edit(|erd| index = erd.add_relationship(&from, &to));
                    self.select(Some(Selection::Relationship(index)));
                }
            }
            _ => {}
        }
        self.queue_draw();
    }

    fn on_double_click(&self, x: f64, y: f64) {
        let world = self.to_world(x, y);
        if let Some(id) = self.entity_at(world) {
            match self.row_at(&id, world) {
                Some(row) => self.start_edit(EditTarget::Attribute(id, row)),
                None => self.start_edit(EditTarget::Name(id)),
            }
        } else if let Some(i) = self.relationship_at(world) {
            self.start_edit(EditTarget::Relationship(i));
        } else if !self.on_add_button(world) {
            let id = self.add_entity(Vec::new(), world);
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
        // Keep the hovered entity while the pointer is on one of its handles,
        // which stick out of the outline.
        let hovered = on_port.clone().or_else(|| self.entity_at(world));
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
            Some(Selection::Entity(id)) => self.start_edit(EditTarget::Name(id)),
            Some(Selection::Relationship(i)) => self.start_edit(EditTarget::Relationship(i)),
            None => {}
        }
    }

    fn start_edit(&self, target: EditTarget) {
        let found = self.with_erd(|erd| match &target {
            EditTarget::Name(id) => {
                let entity = erd.entity(id)?;
                let layout = self.entity_box(entity);
                let rect = layout.rect(entity.pos?);
                Some((
                    entity.label.clone(),
                    Point::new(rect.center().x, rect.y + layout.header_height / 2.0),
                ))
            }
            EditTarget::Attribute(id, row) => {
                let entity = erd.entity(id)?;
                let text = entity.attributes.get(*row)?.to_line();
                let at = self.entity_box(entity).row_rect(entity.pos?, *row).center();
                Some((text, at))
            }
            EditTarget::Relationship(i) => {
                let label = erd.relationships.get(*i)?.label.clone();
                let path = self.relationship_paths(erd).swap_remove(*i);
                Some((label, routing::path_midpoint(&path)))
            }
        });
        let (Some((text, anchor)), Some(editor)) = (found, self.imp().editor.get()) else {
            return;
        };
        self.select(Some(match &target {
            EditTarget::Name(id) | EditTarget::Attribute(id, _) => Selection::Entity(id.clone()),
            EditTarget::Relationship(i) => Selection::Relationship(*i),
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
                // Entities without an alias show their name, so editing it renames.
                let has_alias =
                    self.with_erd(|erd| erd.entity(&id).is_some_and(|e| e.label != e.id));
                if has_alias {
                    self.edit(|erd| {
                        if let Some(e) = erd.entity_mut(&id) {
                            e.label = if text.is_empty() {
                                id.clone()
                            } else {
                                text.to_string()
                            };
                        }
                    });
                    Ok(())
                } else {
                    self.rename_entity(&id, text).map_err(|e| e.to_string())
                }
            }
            EditTarget::Attribute(id, row) => Attribute::parse(text).map(|attribute| {
                self.edit(|erd| {
                    if let Some(slot) = erd.entity_mut(&id).and_then(|e| e.attributes.get_mut(row))
                    {
                        *slot = attribute;
                    }
                });
            }),
            EditTarget::Relationship(i) => {
                self.edit(|erd| {
                    if let Some(r) = erd.relationships.get_mut(i) {
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
        let current = imp.erd.borrow().clone();
        let restored = {
            let mut history = imp.history.borrow_mut();
            if undo {
                history.undo(current)
            } else {
                history.redo(current)
            }
        };
        if let Some(erd) = restored {
            imp.erd.replace(erd);
            self.after_replace();
            self.emit_by_name::<()>("changed", &[]);
        }
    }

    /// Drops selection and hover state that no longer match the diagram.
    fn after_replace(&self) {
        let imp = self.imp();
        let valid = self.with_erd(|erd| match &*imp.selection.borrow() {
            Some(Selection::Entity(id)) => erd.entity(id).is_some(),
            Some(Selection::Relationship(i)) => *i < erd.relationships.len(),
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

fn snap(p: Point) -> Point {
    Point::new((p.x / SNAP).round() * SNAP, (p.y / SNAP).round() * SNAP)
}
