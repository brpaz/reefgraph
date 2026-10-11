//! Interactive sequence diagram canvas. Participants are reordered by
//! dragging their boxes, rows by dragging them up or down, and messages are
//! created by dragging from one lifeline to another.

use std::cell::{Cell, OnceCell, RefCell};
use std::convert::Infallible;
use std::sync::OnceLock;

use gtk::glib::subclass::Signal;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib};

use super::render;
use crate::drawing::{self, Colors, log_draw_error};
use crate::export::{self, Exportable, Scene};
use crate::geometry::{Point, Rect};
use crate::history::History;
use crate::ids::RenameError;
use crate::interaction::{InlineEditor, Navigable, install_navigation};
use crate::sequence::layout::{self, Hit, Layout, RowShape};
use crate::sequence::{
    BlockKind, Item, Message, Note, NotePlacement, Participant, ParticipantKind, Sequence,
};
use crate::viewport::Viewport;

/// Distance a press must travel before it counts as a drag.
const DRAG_THRESHOLD: f64 = 5.0;
/// How far right of its own lifeline a message drag must end to become a
/// message to self.
const SELF_MESSAGE_REACH: f64 = 20.0;

/// Canvas operation bound to a keyboard shortcut.
type CanvasAction = fn(&Canvas);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Participant(String),
    Item(usize),
}

/// What Ctrl+C copied, as it was when copied.
#[derive(Debug, Clone)]
pub enum Copied {
    Participant(Participant),
    /// A message or note and the row it was copied from.
    Item(Item, usize),
}

#[derive(Debug, Default)]
enum Drag {
    #[default]
    None,
    Pan {
        origin: (f64, f64),
    },
    Participant {
        index: usize,
        start: Point,
        cursor: Point,
    },
    Row {
        index: usize,
        start: Point,
        cursor: Point,
    },
    Message {
        from: usize,
        start: Point,
        cursor: Point,
    },
}

/// Element a palette tile adds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PaletteElement {
    Participant(ParticipantKind),
    Note,
    Block(BlockKind),
}

impl PaletteElement {
    pub const ALL: [PaletteElement; 10] = [
        PaletteElement::Participant(ParticipantKind::Participant),
        PaletteElement::Participant(ParticipantKind::Actor),
        PaletteElement::Note,
        PaletteElement::Block(BlockKind::Loop),
        PaletteElement::Block(BlockKind::Alt),
        PaletteElement::Block(BlockKind::Opt),
        PaletteElement::Block(BlockKind::Par),
        PaletteElement::Block(BlockKind::Critical),
        PaletteElement::Block(BlockKind::Break),
        PaletteElement::Block(BlockKind::Rect),
    ];

    pub fn id(self) -> &'static str {
        match self {
            PaletteElement::Participant(kind) => kind.keyword(),
            PaletteElement::Note => "note",
            PaletteElement::Block(kind) => kind.keyword(),
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.id() == id)
    }
}

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct Canvas {
        pub seq: RefCell<Sequence>,
        pub selection: RefCell<Option<Selection>>,
        pub viewport: Cell<Viewport>,
        pub pointer: Cell<(f64, f64)>,
        pub fit_pending: Cell<bool>,
        pub(super) drag: RefCell<Drag>,
        pub history: RefCell<History<Sequence>>,
        pub editor: OnceCell<InlineEditor>,
        pub editing: RefCell<Option<Selection>>,
        pub clipboard: RefCell<Option<super::Copied>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Canvas {
        const NAME: &'static str = "MermanSequenceCanvas";
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

    pub fn with_sequence<R>(&self, f: impl FnOnce(&Sequence) -> R) -> R {
        f(&self.imp().seq.borrow())
    }

    /// Replaces the diagram with one parsed from source text.
    pub fn set_sequence(&self, seq: Sequence) {
        let imp = self.imp();
        if *imp.seq.borrow() == seq {
            return;
        }
        let before = imp.seq.replace(seq);
        imp.history.borrow_mut().record(before);
        self.after_replace();
    }

    /// Loads a new document, resetting undo history and the viewport.
    pub fn load_sequence(&self, seq: Sequence) {
        let imp = self.imp();
        imp.seq.replace(seq);
        imp.history.borrow_mut().clear();
        self.after_replace();
        self.fit_to_content();
    }

    /// Applies a user edit, recording it for undo and notifying listeners.
    pub fn edit(&self, f: impl FnOnce(&mut Sequence)) {
        let _ = self.try_edit(|seq| {
            f(seq);
            Ok::<_, Infallible>(())
        });
    }

    pub fn try_edit<E>(&self, f: impl FnOnce(&mut Sequence) -> Result<(), E>) -> Result<(), E> {
        let imp = self.imp();
        let mut seq = imp.seq.borrow().clone();
        f(&mut seq)?;
        if *imp.seq.borrow() != seq {
            let before = imp.seq.replace(seq);
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
        self.edit(|seq| match &selection {
            Selection::Participant(id) => seq.remove_participant(id),
            Selection::Item(i) => seq.remove_item(*i),
        });
    }

    /// Copies the selected participant, message or note. Activations and
    /// block markers only make sense next to the rows around them.
    pub fn copy_selection(&self) {
        let copied = self.with_sequence(|seq| match self.selection()? {
            Selection::Participant(id) => seq.participant(&id).cloned().map(Copied::Participant),
            Selection::Item(i) => match seq.items.get(i)? {
                item @ (Item::Message(_) | Item::Note(_)) => Some(Copied::Item(item.clone(), i)),
                _ => None,
            },
        });
        if copied.is_some() {
            self.imp().clipboard.replace(copied);
        }
    }

    /// Adds a copy of what was copied: a participant right of the one it was
    /// copied from, a row below the selected row or the one it was copied from.
    pub fn paste(&self) {
        let Some(copied) = self.imp().clipboard.borrow().clone() else {
            return;
        };
        match copied {
            Copied::Participant(participant) => {
                let index = self.with_sequence(|seq| {
                    seq.participants
                        .iter()
                        .position(|p| p.id == participant.id)
                        .map_or(seq.participants.len(), |i| i + 1)
                });
                let mut id = String::new();
                self.edit(|seq| id = seq.paste_participant(&participant, index));
                self.select(Some(Selection::Participant(id)));
            }
            Copied::Item(item, row) => {
                let (valid, index) = self.with_sequence(|seq| {
                    let valid = item
                        .participants()
                        .iter()
                        .all(|id| seq.participant(id).is_some());
                    let after = match self.selection() {
                        Some(Selection::Item(i)) => i,
                        _ => row,
                    };
                    (valid, (after + 1).min(seq.items.len()))
                });
                if !valid {
                    return;
                }
                self.edit(|seq| seq.items.insert(index, item));
                self.select(Some(Selection::Item(index)));
            }
        }
    }

    pub fn rename_participant(&self, old: &str, new: &str) -> Result<(), RenameError> {
        // Select the new id up front so the selection stays valid after the edit.
        let imp = self.imp();
        let previous = imp
            .selection
            .replace(Some(Selection::Participant(new.to_string())));
        let result = self.try_edit(|seq| seq.rename_participant(old, new));
        if result.is_err() {
            imp.selection.replace(previous);
        }
        result
    }

    /// Adds a palette element, at canvas point `at` when it was dropped.
    pub fn add_element(&self, element: PaletteElement, at: Option<Point>) {
        let layout = self.layout();
        match element {
            PaletteElement::Participant(kind) => {
                let mut id = String::new();
                let index = at.map(|p| layout.column_slot(p.x));
                self.edit(|seq| {
                    id = seq.add_participant(kind, index.unwrap_or(seq.participants.len()));
                });
                self.select(Some(Selection::Participant(id)));
            }
            PaletteElement::Note => {
                let column = at.and_then(|p| layout.nearest_column(p.x)).unwrap_or(0);
                let slot = at.map(|p| layout.row_slot(p.y));
                let mut index = 0;
                self.edit(|seq| {
                    if seq.participants.is_empty() {
                        seq.add_participant(ParticipantKind::Participant, 0);
                    }
                    let id = seq.participants[column.min(seq.participants.len() - 1)]
                        .id
                        .clone();
                    index = slot.unwrap_or(seq.items.len());
                    seq.items.insert(
                        index,
                        Item::Note(Note {
                            placement: NotePlacement::RightOf,
                            participants: vec![id],
                            text: "Note".into(),
                        }),
                    );
                });
                self.start_label_edit(Selection::Item(index));
            }
            PaletteElement::Block(kind) => {
                // Dropped on a message or note: wrap it. Otherwise add an empty block.
                let range = match at {
                    Some(p) => match layout.row_at(p.y) {
                        Some(i)
                            if self.with_sequence(|s| {
                                matches!(s.items[i], Item::Message(_) | Item::Note(_))
                            }) =>
                        {
                            i..i + 1
                        }
                        _ => {
                            let slot = layout.row_slot(p.y);
                            slot..slot
                        }
                    },
                    None => {
                        let end = self.with_sequence(|s| s.items.len());
                        end..end
                    }
                };
                let start = range.start;
                self.edit(|seq| seq.insert_block(kind, range));
                self.select(Some(Selection::Item(start)));
            }
        }
        self.grab_focus();
    }

    pub fn fit_to_content(&self) {
        self.fit_bounds(self.content_bounds());
    }

    // ----- Geometry ---------------------------------------------------------

    fn layout_of(&self, seq: &Sequence) -> Layout {
        layout::layout(seq, |text| drawing::text_size(self, text))
    }

    fn layout(&self) -> Layout {
        self.with_sequence(|seq| self.layout_of(seq))
    }

    fn to_world(&self, x: f64, y: f64) -> Point {
        self.viewport().get().to_world(x, y)
    }

    /// Point where the inline editor for a selection should open.
    fn anchor(&self, layout: &Layout, target: &Selection) -> Option<Point> {
        self.with_sequence(|seq| match target {
            Selection::Participant(id) => {
                let i = seq.participant_index(id)?;
                Some(layout.columns.get(i)?.head.center())
            }
            Selection::Item(i) => match &layout.rows.get(*i)?.shape {
                RowShape::Message { label, .. } => Some(*label),
                RowShape::Note(rect) => Some(rect.center()),
                RowShape::Marker { y } => {
                    let frame = layout.frames.iter().find(|f| {
                        f.start == *i || f.end == *i || f.branches.iter().any(|b| b.0 == *i)
                    })?;
                    Some(Point::new(frame.rect.x + 60.0, *y + 10.0))
                }
                RowShape::Activation { y, x } => Some(Point::new(*x, *y)),
            },
        })
    }

    // ----- Rendering --------------------------------------------------------

    fn draw(&self, snapshot: &gtk::Snapshot) {
        let scene = export::screen_scene(self, self.viewport().get());
        self.draw_scene(snapshot, &scene);
    }

    fn draw_drag_feedback(
        &self,
        cr: &gtk::cairo::Context,
        layout: &Layout,
        colors: &Colors,
        zoom: f64,
    ) {
        let result = match &*self.imp().drag.borrow() {
            Drag::Participant { cursor, start, .. } if moved(start, cursor) => {
                let slot = layout.column_slot(cursor.x);
                let x = match (
                    slot.checked_sub(1).and_then(|i| layout.columns.get(i)),
                    layout.columns.get(slot),
                ) {
                    (Some(a), Some(b)) => (a.center + b.center) / 2.0,
                    (Some(a), None) => a.head.x + a.head.w + 20.0,
                    (None, Some(b)) => b.head.x - 20.0,
                    (None, None) => cursor.x,
                };
                render::draw_drop_line(
                    cr,
                    Point::new(x, layout.lifeline_top - 40.0),
                    Point::new(x, layout.lifeline_bottom + 40.0),
                    zoom,
                    colors,
                )
            }
            Drag::Row { cursor, start, .. } if moved(start, cursor) => {
                let y = layout.slot_y(layout.row_slot(cursor.y));
                let (x0, x1) = (layout.bounds.x, layout.bounds.x + layout.bounds.w);
                render::draw_drop_line(cr, Point::new(x0, y), Point::new(x1, y), zoom, colors)
            }
            Drag::Message {
                from,
                start,
                cursor,
            } if moved(start, cursor) => {
                let x = layout.columns[*from].center;
                render::draw_pending_message(
                    cr,
                    Point::new(x, start.y),
                    Point::new(cursor.x, start.y),
                    zoom,
                    colors,
                )
            }
            _ => Ok(()),
        };
        log_draw_error(result);
    }

    // ----- Interaction ------------------------------------------------------

    fn setup_controllers(&self) {
        let drag = gtk::GestureDrag::builder()
            .button(gdk::BUTTON_PRIMARY)
            .build();
        drag.connect_drag_begin(glib::clone!(
            #[weak(rename_to = canvas)]
            self,
            move |_, x, y| canvas.on_drag_begin(x, y)
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
        self.add_controller(motion);

        let drop = gtk::DropTarget::new(String::static_type(), gdk::DragAction::COPY);
        drop.connect_drop(glib::clone!(
            #[weak(rename_to = canvas)]
            self,
            #[upgrade_or]
            false,
            move |_, value, x, y| {
                let element = value
                    .get::<String>()
                    .ok()
                    .and_then(|s| PaletteElement::from_id(&s));
                let Some(element) = element else {
                    return false;
                };
                canvas.add_element(element, Some(canvas.to_world(x, y)));
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

    fn on_drag_begin(&self, x: f64, y: f64) {
        self.grab_focus();
        let world = self.to_world(x, y);
        let layout = self.layout();
        let participant_id = |i: usize| self.with_sequence(|s| s.participants[i].id.clone());
        let drag = match layout.hit(world) {
            Some(Hit::Participant(index)) => {
                self.select(Some(Selection::Participant(participant_id(index))));
                Drag::Participant {
                    index,
                    start: world,
                    cursor: world,
                }
            }
            Some(Hit::Item(index)) => {
                self.select(Some(Selection::Item(index)));
                Drag::Row {
                    index,
                    start: world,
                    cursor: world,
                }
            }
            Some(Hit::Lifeline(from)) => Drag::Message {
                from,
                start: world,
                cursor: world,
            },
            None => {
                self.select(None);
                Drag::Pan {
                    origin: self.viewport().get().offset,
                }
            }
        };
        self.imp().drag.replace(drag);
        self.queue_draw();
    }

    fn on_drag_update(&self, gesture: &gtk::GestureDrag, dx: f64, dy: f64) {
        let cursor_at = || {
            let (sx, sy) = gesture.start_point().unwrap_or_default();
            self.to_world(sx + dx, sy + dy)
        };
        match &mut *self.imp().drag.borrow_mut() {
            Drag::None => return,
            Drag::Pan { origin } => {
                let origin = *origin;
                self.update_viewport(|v| v.offset = (origin.0 + dx, origin.1 + dy));
            }
            Drag::Participant { cursor, .. }
            | Drag::Row { cursor, .. }
            | Drag::Message { cursor, .. } => *cursor = cursor_at(),
        }
        self.queue_draw();
    }

    fn on_drag_end(&self) {
        let drag = self.imp().drag.take();
        let layout = self.layout();
        match drag {
            Drag::Participant {
                index,
                start,
                cursor,
            } if moved(&start, &cursor) => {
                let slot = layout.column_slot(cursor.x);
                let target = if slot > index { slot - 1 } else { slot };
                if target != index {
                    self.edit(|seq| seq.move_participant(index, target));
                }
            }
            Drag::Row {
                index,
                start,
                cursor,
            } if moved(&start, &cursor) => {
                let slot = layout.row_slot(cursor.y);
                if slot != index && slot != index + 1 {
                    let target = if slot > index { slot - 1 } else { slot };
                    // Selected first: the edit notifies listeners, which read it.
                    self.imp().selection.replace(Some(Selection::Item(target)));
                    if self.try_edit(|seq| seq.move_item(index, slot)).is_err() {
                        self.imp().selection.replace(Some(Selection::Item(index)));
                        self.error_bell();
                    }
                }
            }
            Drag::Message {
                from,
                start,
                cursor,
            } if moved(&start, &cursor) => {
                self.create_message(&layout, from, start, cursor);
            }
            _ => {}
        }
        self.queue_draw();
    }

    fn create_message(&self, layout: &Layout, from: usize, start: Point, cursor: Point) {
        let Some(to) = layout.nearest_column(cursor.x) else {
            return;
        };
        if to == from && cursor.x < layout.columns[from].center + SELF_MESSAGE_REACH {
            return;
        }
        let slot = layout.row_slot(start.y);
        self.edit(|seq| {
            let message = Message {
                from: seq.participants[from].id.clone(),
                to: seq.participants[to].id.clone(),
                text: "message".into(),
                ..Message::default()
            };
            seq.items.insert(slot, Item::Message(message));
        });
        self.start_label_edit(Selection::Item(slot));
    }

    fn on_double_click(&self, x: f64, y: f64) {
        let world = self.to_world(x, y);
        let layout = self.layout();
        match layout.hit(world) {
            Some(Hit::Participant(i)) => {
                let id = self.with_sequence(|s| s.participants[i].id.clone());
                self.start_label_edit(Selection::Participant(id));
            }
            Some(Hit::Item(i)) => self.start_label_edit(Selection::Item(i)),
            Some(Hit::Lifeline(_)) => {}
            None if world.y < layout.lifeline_top || layout.columns.is_empty() => {
                self.add_element(
                    PaletteElement::Participant(ParticipantKind::Participant),
                    Some(world),
                );
                if let Some(selection) = self.selection() {
                    self.start_label_edit(selection);
                }
            }
            None => {}
        }
    }

    fn on_motion(&self, x: f64, y: f64) {
        if !matches!(*self.imp().drag.borrow(), Drag::None) {
            return;
        }
        let cursor = match self.layout().hit(self.to_world(x, y)) {
            Some(Hit::Participant(_)) => "grab",
            Some(Hit::Item(_)) => "ns-resize",
            Some(Hit::Lifeline(_)) => "crosshair",
            None => "default",
        };
        self.set_cursor_from_name(Some(cursor));
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

    /// Text edited in place for a selection: participant labels, message and
    /// note text, block labels.
    fn label_of(&self, target: &Selection) -> Option<String> {
        self.with_sequence(|seq| match target {
            Selection::Participant(id) => Some(seq.participant(id)?.label.clone()),
            Selection::Item(i) => match seq.items.get(*i)? {
                Item::Message(m) => Some(m.text.clone()),
                Item::Note(n) => Some(n.text.clone()),
                Item::BlockStart { label, .. } | Item::BlockBranch { label } => Some(label.clone()),
                Item::BlockEnd => Some(block_label(seq, *i)),
                Item::Activate(_) | Item::Deactivate(_) => None,
            },
        })
    }

    fn start_label_edit(&self, target: Selection) {
        let layout = self.layout();
        let (Some(text), Some(anchor), Some(editor)) = (
            self.label_of(&target),
            self.anchor(&layout, &target),
            self.imp().editor.get(),
        ) else {
            return;
        };
        self.select(Some(target.clone()));
        self.imp().editing.replace(Some(target));
        editor.open(self.viewport().get().to_screen(anchor), &text);
    }

    fn commit_label_edit(&self, text: &str) {
        let Some(target) = self.imp().editing.take() else {
            return;
        };
        let text = text.trim().to_string();
        self.edit(|seq| match target {
            Selection::Participant(id) => {
                if let Some(p) = seq.participant_mut(&id) {
                    p.label = if text.is_empty() { id.clone() } else { text };
                }
            }
            Selection::Item(i) => {
                // The end marker edits the label of the block it closes.
                let i = match seq.items.get(i) {
                    Some(Item::BlockEnd) => seq.block_markers(i).map_or(i, |m| m[0]),
                    _ => i,
                };
                match seq.items.get_mut(i) {
                    Some(Item::Message(m)) => m.text = text,
                    Some(Item::Note(n)) => n.text = text,
                    Some(Item::BlockStart { label, .. } | Item::BlockBranch { label }) => {
                        *label = text;
                    }
                    _ => {}
                }
            }
        });
    }

    // ----- Internal helpers -------------------------------------------------

    fn step_history(&self, undo: bool) {
        let imp = self.imp();
        let current = imp.seq.borrow().clone();
        let restored = {
            let mut history = imp.history.borrow_mut();
            if undo {
                history.undo(current)
            } else {
                history.redo(current)
            }
        };
        if let Some(seq) = restored {
            imp.seq.replace(seq);
            self.after_replace();
            self.emit_by_name::<()>("changed", &[]);
        }
    }

    /// Drops a selection that no longer matches the diagram and notifies
    /// listeners.
    fn after_replace(&self) {
        let imp = self.imp();
        let valid = self.with_sequence(|seq| match &*imp.selection.borrow() {
            Some(Selection::Participant(id)) => seq.participant(id).is_some(),
            Some(Selection::Item(i)) => *i < seq.items.len(),
            None => true,
        });
        if !valid {
            imp.selection.replace(None);
        }
        self.emit_by_name::<()>("history-changed", &[]);
        self.emit_by_name::<()>("selection-changed", &[]);
        self.queue_draw();
    }
}

fn moved(start: &Point, cursor: &Point) -> bool {
    (cursor.x - start.x).hypot(cursor.y - start.y) >= DRAG_THRESHOLD
}

/// Label of the block marker at `index`; the end marker reports its block's label.
fn block_label(seq: &Sequence, index: usize) -> String {
    let index = match seq.items.get(index) {
        Some(Item::BlockEnd) => seq.block_markers(index).map_or(index, |m| m[0]),
        _ => index,
    };
    match seq.items.get(index) {
        Some(Item::BlockStart { label, .. } | Item::BlockBranch { label }) => label.clone(),
        _ => String::new(),
    }
}

impl Exportable for Canvas {
    fn content_bounds(&self) -> Option<Rect> {
        self.with_sequence(|seq| (!seq.participants.is_empty()).then(|| self.layout_of(seq).bounds))
    }

    fn draw_scene(&self, snapshot: &gtk::Snapshot, scene: &Scene) {
        let imp = self.imp();
        let viewport = scene.viewport;
        let dark = scene.dark;
        let colors = scene.colors;
        let seq = imp.seq.borrow();
        let layout = self.layout_of(&seq);
        let selection = imp.selection.borrow().clone().filter(|_| scene.interactive);
        let selected_item = match &selection {
            Some(Selection::Item(i)) => Some(*i),
            _ => None,
        };
        let selected_participant = match &selection {
            Some(Selection::Participant(id)) => seq.participant_index(id),
            _ => None,
        };
        let selected_block = selected_item.and_then(|i| seq.block_markers(i));
        let block_selected = |start: usize| selected_block.as_ref().is_some_and(|m| m[0] == start);
        let mut texts: Vec<(String, Point, gtk::gdk::RGBA)> = Vec::new();

        let cr = drawing::world_context(snapshot, &viewport, scene.size);
        if scene.interactive {
            drawing::draw_grid(&cr, &viewport.visible(scene.size), viewport.zoom, &colors);
        }

        if let (Some(title), Some(at)) = (&seq.title, layout.title) {
            texts.push((title.clone(), at, colors.fg));
        }

        // Highlight blocks sit behind everything else.
        for frame in layout.frames.iter().filter(|f| f.kind == BlockKind::Rect) {
            let label = block_label(&seq, frame.start);
            let color = gdk::RGBA::parse(label.trim()).ok();
            log_draw_error(render::draw_highlight_block(
                &cr,
                &frame.rect,
                color,
                dark,
                &colors,
                block_selected(frame.start),
            ));
        }

        for (i, column) in layout.columns.iter().enumerate() {
            log_draw_error(render::draw_lifeline(
                &cr,
                column.center,
                column.head.y + column.head.h,
                column.foot.y,
                &colors,
                selected_participant == Some(i),
            ));
        }
        for bar in &layout.activations {
            log_draw_error(render::draw_activation(&cr, &bar.rect, &colors));
        }

        for frame in layout.frames.iter().filter(|f| f.kind != BlockKind::Rect) {
            let keyword = frame.kind.keyword();
            let (kw, _) = drawing::text_size(self, keyword);
            let tab_width = kw + render::TAB_PADDING;
            let branch_ys: Vec<f64> = frame.branches.iter().map(|b| b.1).collect();
            log_draw_error(render::draw_frame(
                &cr,
                &frame.rect,
                tab_width,
                &branch_ys,
                &colors,
                block_selected(frame.start),
            ));
            let header_y = frame.rect.y + (layout::BLOCK_HEADER - 4.0) / 2.0;
            texts.push((
                keyword.to_string(),
                Point::new(frame.rect.x + tab_width / 2.0 - 3.0, header_y),
                colors.fg,
            ));
            let mut labels = vec![(block_label(&seq, frame.start), header_y, tab_width)];
            labels.extend(
                frame
                    .branches
                    .iter()
                    .map(|(i, y)| (block_label(&seq, *i), y + layout::BLOCK_HEADER / 2.0, 0.0)),
            );
            for (label, y, indent) in labels.into_iter().filter(|l| !l.0.is_empty()) {
                let text = format!("[{label}]");
                let (w, _) = drawing::text_size(self, &text);
                texts.push((
                    text,
                    Point::new(frame.rect.x + indent + 8.0 + w / 2.0, y),
                    colors.fg,
                ));
            }
        }

        let note_fill = render::note_fill(dark);
        for (i, (item, row)) in seq.items.iter().zip(&layout.rows).enumerate() {
            let highlighted = selected_item == Some(i);
            match (item, &row.shape) {
                (
                    Item::Message(m),
                    RowShape::Message {
                        y,
                        x1,
                        x2,
                        self_loop,
                        number,
                        label,
                    },
                ) => {
                    log_draw_error(render::draw_message(
                        &cr,
                        m.line,
                        m.head,
                        (*x1, *x2, *y),
                        *self_loop,
                        &colors,
                        highlighted,
                    ));
                    if let Some(n) = number {
                        let at = Point::new(*x1, *y);
                        log_draw_error(render::draw_number_badge(&cr, at, &colors));
                        texts.push((n.to_string(), at, gdk::RGBA::WHITE));
                    }
                    texts.push((m.text.clone(), *label, colors.fg));
                }
                (Item::Note(n), RowShape::Note(rect)) => {
                    log_draw_error(render::draw_note(
                        &cr,
                        rect,
                        &note_fill,
                        &colors,
                        highlighted,
                    ));
                    texts.push((n.text.clone(), rect.center(), colors.fg));
                }
                (Item::Activate(_) | Item::Deactivate(_), RowShape::Activation { y, x }) => {
                    log_draw_error(render::draw_activation_marker(
                        &cr,
                        Point::new(*x, *y),
                        &colors,
                        highlighted,
                    ));
                }
                _ => {}
            }
        }

        for (i, (p, column)) in seq.participants.iter().zip(&layout.columns).enumerate() {
            for rect in [&column.head, &column.foot] {
                log_draw_error(render::draw_participant(
                    &cr,
                    p.kind,
                    rect,
                    &colors,
                    selected_participant == Some(i),
                ));
                texts.push((
                    p.label.clone(),
                    render::participant_label_center(p.kind, rect),
                    colors.fg,
                ));
            }
        }

        if scene.interactive {
            self.draw_drag_feedback(&cr, &layout, &colors, viewport.zoom);
        }
        drop(cr);

        for (text, at, color) in texts.iter().filter(|t| !t.0.is_empty()) {
            let text_layout = drawing::label_layout(self, text);
            drawing::draw_text(snapshot, &viewport, &text_layout, *at, color);
        }
    }
}
