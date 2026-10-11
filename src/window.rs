use std::cell::{Cell, RefCell};
use std::time::Duration;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};
use sourceview5::prelude::*;

use crate::application::MermanApplication;
use crate::config::{APP_ID, PROFILE};
use crate::diagram::{self, Diagram, DiagramKind};
use crate::flowchart::{self, NodeShape};
use crate::interaction::Navigable;
use crate::palette::Palette;
use crate::{erd, erd_editor, flowchart_editor, sequence, sequence_editor};

const SAMPLE_DIAGRAM: &str = "flowchart TD
    A[Start] --> B{Is it working?}
    B -->|Yes| C([Ship it])
    B -->|No| D[Debug]
    D --> B
";
/// Pause after the last keystroke before the source is re-parsed.
const PARSE_DELAY: Duration = Duration::from_millis(300);

mod imp {
    use super::*;

    #[derive(Debug, gtk::CompositeTemplate)]
    #[template(resource = "/dev/brunopaz/Merman/ui/window.ui")]
    pub struct MermanApplicationWindow {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub window_title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub palette: TemplateChild<Palette>,
        #[template_child]
        pub editor_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub flowchart_canvas: TemplateChild<flowchart_editor::Canvas>,
        #[template_child]
        pub sequence_canvas: TemplateChild<sequence_editor::Canvas>,
        #[template_child]
        pub erd_canvas: TemplateChild<erd_editor::Canvas>,
        #[template_child]
        pub inspector_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub flowchart_inspector: TemplateChild<flowchart_editor::Inspector>,
        #[template_child]
        pub sequence_inspector: TemplateChild<sequence_editor::Inspector>,
        #[template_child]
        pub erd_inspector: TemplateChild<erd_editor::Inspector>,
        #[template_child]
        pub error_banner: TemplateChild<adw::Banner>,
        #[template_child]
        pub source_scroller: TemplateChild<gtk::ScrolledWindow>,
        pub buffer: sourceview5::Buffer,
        pub settings: gio::Settings,
        pub kind: Cell<DiagramKind>,
        pub file: RefCell<Option<gio::File>>,
        pub dirty: Cell<bool>,
        /// Set while the buffer is rewritten from the canvas, so the change is
        /// not parsed back into the canvas.
        pub syncing: Cell<bool>,
        pub pending_parse: RefCell<Option<glib::SourceId>>,
    }

    impl Default for MermanApplicationWindow {
        fn default() -> Self {
            Self {
                toast_overlay: TemplateChild::default(),
                window_title: TemplateChild::default(),
                palette: TemplateChild::default(),
                editor_stack: TemplateChild::default(),
                flowchart_canvas: TemplateChild::default(),
                sequence_canvas: TemplateChild::default(),
                erd_canvas: TemplateChild::default(),
                inspector_stack: TemplateChild::default(),
                flowchart_inspector: TemplateChild::default(),
                sequence_inspector: TemplateChild::default(),
                erd_inspector: TemplateChild::default(),
                error_banner: TemplateChild::default(),
                source_scroller: TemplateChild::default(),
                buffer: sourceview5::Buffer::new(None),
                settings: gio::Settings::new(*APP_ID),
                kind: Cell::new(DiagramKind::Flowchart),
                file: RefCell::default(),
                dirty: Cell::new(false),
                syncing: Cell::new(false),
                pending_parse: RefCell::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MermanApplicationWindow {
        const NAME: &'static str = "MermanApplicationWindow";
        type Type = super::MermanApplicationWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            flowchart_editor::Canvas::ensure_type();
            flowchart_editor::Inspector::ensure_type();
            sequence_editor::Canvas::ensure_type();
            sequence_editor::Inspector::ensure_type();
            erd_editor::Canvas::ensure_type();
            erd_editor::Inspector::ensure_type();
            Palette::ensure_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MermanApplicationWindow {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            if *PROFILE == "Devel" {
                obj.add_css_class("devel");
            }
            obj.load_window_size();
            obj.setup_source_view();
            obj.setup_actions();
            obj.connect_components();
            obj.show_kind(DiagramKind::Flowchart);
            obj.load_text(SAMPLE_DIAGRAM, None);
        }
    }

    impl WidgetImpl for MermanApplicationWindow {}

    impl WindowImpl for MermanApplicationWindow {
        fn close_request(&self) -> glib::Propagation {
            let obj = self.obj();
            if let Err(err) = obj.save_window_size() {
                tracing::warn!(error = %err, "failed to save window state");
            }
            if !self.dirty.get() {
                return self.parent_close_request();
            }
            glib::spawn_future_local(glib::clone!(
                #[weak]
                obj,
                async move {
                    if obj.confirm_discard().await {
                        obj.imp().dirty.set(false);
                        obj.close();
                    }
                }
            ));
            glib::Propagation::Stop
        }
    }

    impl ApplicationWindowImpl for MermanApplicationWindow {}
    impl AdwApplicationWindowImpl for MermanApplicationWindow {}
}

glib::wrapper! {
    pub struct MermanApplicationWindow(ObjectSubclass<imp::MermanApplicationWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionMap, gio::ActionGroup,
                    gtk::Root, gtk::Native, gtk::ShortcutManager,
                    gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl MermanApplicationWindow {
    pub fn new(app: &MermanApplication) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    // ----- Setup ------------------------------------------------------------

    fn setup_source_view(&self) {
        let imp = self.imp();
        let view = sourceview5::View::builder()
            .buffer(&imp.buffer)
            .monospace(true)
            .show_line_numbers(true)
            .highlight_current_line(true)
            .auto_indent(true)
            .insert_spaces_instead_of_tabs(true)
            .tab_width(4)
            .top_margin(6)
            .bottom_margin(6)
            .build();
        imp.source_scroller.set_child(Some(&view));

        let style_manager = adw::StyleManager::default();
        self.update_source_scheme(style_manager.is_dark());
        style_manager.connect_dark_notify(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |manager| win.update_source_scheme(manager.is_dark())
        ));

        imp.buffer.connect_changed(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.on_source_changed()
        ));
    }

    fn update_source_scheme(&self, dark: bool) {
        let id = if dark { "Adwaita-dark" } else { "Adwaita" };
        let scheme = sourceview5::StyleSchemeManager::default().scheme(id);
        self.imp().buffer.set_style_scheme(scheme.as_ref());
    }

    fn setup_actions(&self) {
        let actions = [
            gio::ActionEntry::builder("new-flowchart")
                .activate(|win: &Self, _, _| win.spawn_new_document(DiagramKind::Flowchart))
                .build(),
            gio::ActionEntry::builder("new-sequence")
                .activate(|win: &Self, _, _| win.spawn_new_document(DiagramKind::Sequence))
                .build(),
            gio::ActionEntry::builder("new-erd")
                .activate(|win: &Self, _, _| win.spawn_new_document(DiagramKind::Erd))
                .build(),
            gio::ActionEntry::builder("open")
                .activate(|win: &Self, _, _| {
                    let win = win.clone();
                    glib::spawn_future_local(async move { win.open().await });
                })
                .build(),
            gio::ActionEntry::builder("save")
                .activate(|win: &Self, _, _| {
                    let win = win.clone();
                    glib::spawn_future_local(async move {
                        win.save().await;
                    });
                })
                .build(),
            gio::ActionEntry::builder("save-as")
                .activate(|win: &Self, _, _| {
                    let win = win.clone();
                    glib::spawn_future_local(async move {
                        win.save_as().await;
                    });
                })
                .build(),
            gio::ActionEntry::builder("undo")
                .activate(|win: &Self, _, _| win.undo())
                .build(),
            gio::ActionEntry::builder("redo")
                .activate(|win: &Self, _, _| win.redo())
                .build(),
            gio::ActionEntry::builder("zoom-in")
                .activate(|win: &Self, _, _| win.zoom(true))
                .build(),
            gio::ActionEntry::builder("zoom-out")
                .activate(|win: &Self, _, _| win.zoom(false))
                .build(),
            gio::ActionEntry::builder("zoom-fit")
                .activate(|win: &Self, _, _| win.fit_to_content())
                .build(),
        ];
        self.add_action_entries(actions);
    }

    fn connect_components(&self) {
        let imp = self.imp();
        imp.flowchart_inspector.set_canvas(&imp.flowchart_canvas);
        imp.sequence_inspector.set_canvas(&imp.sequence_canvas);
        imp.erd_inspector.set_canvas(&imp.erd_canvas);

        imp.palette.connect_local(
            "item-activated",
            false,
            glib::clone!(
                #[weak(rename_to = win)]
                self,
                #[upgrade_or]
                None,
                move |args| {
                    let id = args[1].get::<String>().ok()?;
                    win.add_palette_item(&id);
                    None
                }
            ),
        );

        let canvases: [&gtk::Widget; 3] = [
            imp.flowchart_canvas.upcast_ref(),
            imp.sequence_canvas.upcast_ref(),
            imp.erd_canvas.upcast_ref(),
        ];
        for canvas in canvases {
            canvas.connect_local(
                "changed",
                false,
                glib::clone!(
                    #[weak(rename_to = win)]
                    self,
                    #[upgrade_or]
                    None,
                    move |_| {
                        win.sync_source_from_canvas();
                        None
                    }
                ),
            );
            canvas.connect_local(
                "history-changed",
                false,
                glib::clone!(
                    #[weak(rename_to = win)]
                    self,
                    #[upgrade_or]
                    None,
                    move |_| {
                        win.update_history_actions();
                        None
                    }
                ),
            );
        }
    }

    // ----- Active editor ----------------------------------------------------

    /// Shows the canvas, inspector and palette for a diagram type.
    fn show_kind(&self, kind: DiagramKind) {
        let imp = self.imp();
        imp.kind.set(kind);
        let (page, items) = match kind {
            DiagramKind::Flowchart => ("flowchart", flowchart_editor::palette_items()),
            DiagramKind::Sequence => ("sequence", sequence_editor::palette_items()),
            DiagramKind::Erd => ("erd", erd_editor::palette_items()),
        };
        imp.editor_stack.set_visible_child_name(page);
        imp.inspector_stack.set_visible_child_name(page);
        imp.palette.set_items(items);
        self.update_history_actions();
    }

    fn zoom(&self, zoom_in: bool) {
        let imp = self.imp();
        match imp.kind.get() {
            DiagramKind::Flowchart => zoom_canvas(&*imp.flowchart_canvas, zoom_in),
            DiagramKind::Sequence => zoom_canvas(&*imp.sequence_canvas, zoom_in),
            DiagramKind::Erd => zoom_canvas(&*imp.erd_canvas, zoom_in),
        }
    }

    fn undo(&self) {
        let imp = self.imp();
        match imp.kind.get() {
            DiagramKind::Flowchart => imp.flowchart_canvas.undo(),
            DiagramKind::Sequence => imp.sequence_canvas.undo(),
            DiagramKind::Erd => imp.erd_canvas.undo(),
        }
    }

    fn redo(&self) {
        let imp = self.imp();
        match imp.kind.get() {
            DiagramKind::Flowchart => imp.flowchart_canvas.redo(),
            DiagramKind::Sequence => imp.sequence_canvas.redo(),
            DiagramKind::Erd => imp.erd_canvas.redo(),
        }
    }

    fn fit_to_content(&self) {
        let imp = self.imp();
        match imp.kind.get() {
            DiagramKind::Flowchart => imp.flowchart_canvas.fit_to_content(),
            DiagramKind::Sequence => imp.sequence_canvas.fit_to_content(),
            DiagramKind::Erd => imp.erd_canvas.fit_to_content(),
        }
    }

    fn add_palette_item(&self, id: &str) {
        let imp = self.imp();
        match imp.kind.get() {
            DiagramKind::Flowchart => {
                if let Some(shape) = NodeShape::from_name(id) {
                    imp.flowchart_canvas.add_node_at_center(shape);
                }
            }
            DiagramKind::Sequence => {
                if let Some(element) = sequence_editor::PaletteElement::from_id(id) {
                    imp.sequence_canvas.add_element(element, None);
                }
            }
            DiagramKind::Erd => {
                if let Some(preset) = erd_editor::EntityPreset::from_id(id) {
                    imp.erd_canvas.add_entity_at_center(preset.attributes());
                }
            }
        }
    }

    fn update_history_actions(&self) {
        let imp = self.imp();
        let (can_undo, can_redo) = match imp.kind.get() {
            DiagramKind::Flowchart => (
                imp.flowchart_canvas.can_undo(),
                imp.flowchart_canvas.can_redo(),
            ),
            DiagramKind::Sequence => (
                imp.sequence_canvas.can_undo(),
                imp.sequence_canvas.can_redo(),
            ),
            DiagramKind::Erd => (imp.erd_canvas.can_undo(), imp.erd_canvas.can_redo()),
        };
        for (name, enabled) in [("undo", can_undo), ("redo", can_redo)] {
            if let Some(action) = self.lookup_action(name).and_downcast::<gio::SimpleAction>() {
                action.set_enabled(enabled);
            }
        }
    }

    // ----- Source <-> canvas synchronisation --------------------------------

    fn on_source_changed(&self) {
        let imp = self.imp();
        if imp.syncing.get() {
            return;
        }
        self.set_dirty(true);
        self.cancel_pending_parse();
        let id = glib::timeout_add_local_once(
            PARSE_DELAY,
            glib::clone!(
                #[weak(rename_to = win)]
                self,
                move || {
                    win.imp().pending_parse.take();
                    win.apply_source();
                }
            ),
        );
        imp.pending_parse.replace(Some(id));
    }

    fn cancel_pending_parse(&self) {
        if let Some(id) = self.imp().pending_parse.take() {
            id.remove();
        }
    }

    fn source_text(&self) -> String {
        let buffer = &self.imp().buffer;
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string()
    }

    /// Parses the source text. Returns the diagram, or shows the error and
    /// returns `None`.
    fn parse_source(&self, text: &str) -> Option<Diagram> {
        let banner = &self.imp().error_banner;
        match diagram::parse(text) {
            Ok(diagram) => {
                banner.set_revealed(false);
                Some(diagram)
            }
            Err(err) => {
                banner.set_title(&err.to_string());
                banner.set_revealed(true);
                None
            }
        }
    }

    /// Applies edited source text to the canvas. A change of diagram type
    /// loads the diagram into the other editor.
    fn apply_source(&self) {
        let Some(diagram) = self.parse_source(&self.source_text()) else {
            return;
        };
        if diagram.kind() == self.imp().kind.get() {
            self.update_canvas(diagram);
        } else {
            self.load_diagram(diagram);
        }
    }

    fn update_canvas(&self, diagram: Diagram) {
        let imp = self.imp();
        match diagram {
            Diagram::Flowchart(chart) => imp.flowchart_canvas.set_chart(chart),
            Diagram::Sequence(seq) => imp.sequence_canvas.set_sequence(seq),
            Diagram::Erd(erd) => imp.erd_canvas.set_erd(erd),
        }
    }

    /// Shows the diagram in its editor with fresh undo history and viewport.
    fn load_diagram(&self, diagram: Diagram) {
        let imp = self.imp();
        self.show_kind(diagram.kind());
        match diagram {
            Diagram::Flowchart(chart) => imp.flowchart_canvas.load_chart(chart),
            Diagram::Sequence(seq) => imp.sequence_canvas.load_sequence(seq),
            Diagram::Erd(erd) => imp.erd_canvas.load_erd(erd),
        }
    }

    fn sync_source_from_canvas(&self) {
        let imp = self.imp();
        self.cancel_pending_parse();
        let text = match imp.kind.get() {
            DiagramKind::Flowchart => imp.flowchart_canvas.with_chart(flowchart::serialize),
            DiagramKind::Sequence => imp.sequence_canvas.with_sequence(sequence::serialize),
            DiagramKind::Erd => imp.erd_canvas.with_erd(erd::serialize),
        };
        if text == self.source_text() {
            return;
        }
        let cursor = imp.buffer.cursor_position();
        // Delete and insert rather than `set_text`, which would discard the
        // buffer's undo history.
        let buffer = &imp.buffer;
        imp.syncing.set(true);
        buffer.begin_user_action();
        buffer.delete(&mut buffer.start_iter(), &mut buffer.end_iter());
        buffer.insert(&mut buffer.start_iter(), &text);
        buffer.end_user_action();
        imp.syncing.set(false);
        imp.buffer.place_cursor(
            &imp.buffer
                .iter_at_offset(cursor.min(imp.buffer.char_count())),
        );
        imp.error_banner.set_revealed(false);
        self.set_dirty(true);
    }

    // ----- Documents --------------------------------------------------------

    fn load_text(&self, text: &str, file: Option<gio::File>) {
        let imp = self.imp();
        self.cancel_pending_parse();
        imp.syncing.set(true);
        imp.buffer.set_enable_undo(false);
        imp.buffer.set_text(text);
        imp.buffer.set_enable_undo(true);
        imp.buffer.place_cursor(&imp.buffer.start_iter());
        imp.syncing.set(false);

        match self.parse_source(text) {
            Some(diagram) => self.load_diagram(diagram),
            None => imp.flowchart_canvas.load_chart(Default::default()),
        }
        imp.file.replace(file);
        imp.dirty.set(false);
        self.update_title();
    }

    pub async fn load_file(&self, file: &gio::File) {
        let text = match file.load_contents_future().await {
            Ok((bytes, _)) => String::from_utf8(bytes.to_vec()).ok(),
            Err(err) => {
                tracing::error!(error = %err, "failed to read diagram");
                None
            }
        };
        match text {
            Some(text) => self.load_text(&text, Some(file.clone())),
            None => self.toast(&gettext("Could not open the file")),
        }
    }

    fn spawn_new_document(&self, kind: DiagramKind) {
        let win = self.clone();
        glib::spawn_future_local(async move {
            if win.confirm_discard().await {
                win.load_text(kind.template(), None);
            }
        });
    }

    async fn open(&self) {
        if !self.confirm_discard().await {
            return;
        }
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Open Diagram"))
            .modal(true)
            .filters(&file_filters())
            .build();
        match dialog.open_future(Some(self)).await {
            Ok(file) => self.load_file(&file).await,
            Err(err) => log_dialog_error(&err),
        }
    }

    /// Saves to the current file, asking for one first if needed. Returns
    /// whether the document was saved.
    async fn save(&self) -> bool {
        let file = self.imp().file.borrow().clone();
        match file {
            Some(file) => self.write_file(&file).await,
            None => self.save_as().await,
        }
    }

    async fn save_as(&self) -> bool {
        let initial_name = self
            .imp()
            .file
            .borrow()
            .as_ref()
            .and_then(|f| f.basename())
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| format!("{}.mmd", gettext("diagram")));
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Save Diagram"))
            .modal(true)
            .initial_name(initial_name)
            .filters(&file_filters())
            .build();
        match dialog.save_future(Some(self)).await {
            Ok(file) => self.write_file(&file).await,
            Err(err) => {
                log_dialog_error(&err);
                false
            }
        }
    }

    async fn write_file(&self, file: &gio::File) -> bool {
        let contents = self.source_text().into_bytes();
        let result = file
            .replace_contents_future(
                contents,
                None,
                false,
                gio::FileCreateFlags::REPLACE_DESTINATION,
            )
            .await;
        match result {
            Ok(_) => {
                self.imp().file.replace(Some(file.clone()));
                self.imp().dirty.set(false);
                self.update_title();
                true
            }
            Err((_, err)) => {
                tracing::error!(error = %err, "failed to save diagram");
                self.toast(&gettext("Could not save the file"));
                false
            }
        }
    }

    /// Asks what to do with unsaved changes. Returns whether the caller may
    /// discard the current document.
    async fn confirm_discard(&self) -> bool {
        if !self.imp().dirty.get() {
            return true;
        }
        let dialog = adw::AlertDialog::new(
            Some(&gettext("Save Changes?")),
            Some(&gettext(
                "The diagram has unsaved changes. Changes which are not saved will be permanently lost.",
            )),
        );
        dialog.add_responses(&[
            ("cancel", &gettext("_Cancel")),
            ("discard", &gettext("_Discard")),
            ("save", &gettext("_Save")),
        ]);
        dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));
        dialog.set_close_response("cancel");
        match dialog.choose_future(Some(self)).await.as_str() {
            "discard" => true,
            "save" => self.save().await,
            _ => false,
        }
    }

    fn set_dirty(&self, dirty: bool) {
        if self.imp().dirty.replace(dirty) != dirty {
            self.update_title();
        }
    }

    fn update_title(&self) {
        let imp = self.imp();
        let file = imp.file.borrow();
        let name = file
            .as_ref()
            .and_then(|f| f.basename())
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| gettext("Untitled Diagram"));
        let title = if imp.dirty.get() {
            format!("• {name}")
        } else {
            name
        };
        let folder = file
            .as_ref()
            .and_then(|f| f.parent())
            .and_then(|p| p.path())
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        imp.window_title.set_title(&title);
        imp.window_title.set_subtitle(&folder);
        self.set_title(Some(&title));
    }

    fn toast(&self, message: &str) {
        self.imp().toast_overlay.add_toast(adw::Toast::new(message));
    }

    // ----- Window state -----------------------------------------------------

    fn save_window_size(&self) -> Result<(), glib::BoolError> {
        let settings = &self.imp().settings;
        let (width, height) = self.default_size();
        settings.set_int("window-width", width)?;
        settings.set_int("window-height", height)?;
        settings.set_boolean("is-maximized", self.is_maximized())?;
        Ok(())
    }

    fn load_window_size(&self) {
        let settings = &self.imp().settings;
        self.set_default_size(settings.int("window-width"), settings.int("window-height"));
        if settings.boolean("is-maximized") {
            self.maximize();
        }
    }
}

fn zoom_canvas(canvas: &impl Navigable, zoom_in: bool) {
    if zoom_in {
        canvas.zoom_in();
    } else {
        canvas.zoom_out();
    }
}

fn file_filters() -> gio::ListStore {
    let mermaid = gtk::FileFilter::new();
    mermaid.set_name(Some(&gettext("Mermaid Diagrams")));
    mermaid.add_suffix("mmd");
    mermaid.add_suffix("mermaid");
    let all = gtk::FileFilter::new();
    all.set_name(Some(&gettext("All Files")));
    all.add_pattern("*");
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&mermaid);
    filters.append(&all);
    filters
}

fn log_dialog_error(err: &glib::Error) {
    if !err.matches(gtk::DialogError::Dismissed) && !err.matches(gtk::DialogError::Cancelled) {
        tracing::error!(error = %err, "file dialog failed");
    }
}
