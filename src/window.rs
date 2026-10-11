use std::cell::{Cell, RefCell};
use std::time::Duration;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};
use sourceview5::prelude::*;

use crate::application::MermanApplication;
use crate::canvas::Canvas;
use crate::config::{APP_ID, PROFILE};
use crate::flowchart::{self, Flowchart, NodeShape};
use crate::inspector::Inspector;
use crate::palette::Palette;

const SAMPLE_DIAGRAM: &str = "flowchart TD
    A[Start] --> B{Is it working?}
    B -->|Yes| C([Ship it])
    B -->|No| D[Debug]
    D --> B
";
const NEW_DIAGRAM: &str = "flowchart TD\n";
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
        pub canvas: TemplateChild<Canvas>,
        #[template_child]
        pub inspector: TemplateChild<Inspector>,
        #[template_child]
        pub error_banner: TemplateChild<adw::Banner>,
        #[template_child]
        pub source_scroller: TemplateChild<gtk::ScrolledWindow>,
        pub buffer: sourceview5::Buffer,
        pub settings: gio::Settings,
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
                canvas: TemplateChild::default(),
                inspector: TemplateChild::default(),
                error_banner: TemplateChild::default(),
                source_scroller: TemplateChild::default(),
                buffer: sourceview5::Buffer::new(None),
                settings: gio::Settings::new(*APP_ID),
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
            Canvas::ensure_type();
            Palette::ensure_type();
            Inspector::ensure_type();
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
            gio::ActionEntry::builder("new")
                .activate(|win: &Self, _, _| {
                    let win = win.clone();
                    glib::spawn_future_local(async move { win.new_document().await });
                })
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
                .activate(|win: &Self, _, _| win.imp().canvas.undo())
                .build(),
            gio::ActionEntry::builder("redo")
                .activate(|win: &Self, _, _| win.imp().canvas.redo())
                .build(),
            gio::ActionEntry::builder("zoom-in")
                .activate(|win: &Self, _, _| win.imp().canvas.zoom_in())
                .build(),
            gio::ActionEntry::builder("zoom-out")
                .activate(|win: &Self, _, _| win.imp().canvas.zoom_out())
                .build(),
            gio::ActionEntry::builder("zoom-fit")
                .activate(|win: &Self, _, _| win.imp().canvas.fit_to_content())
                .build(),
        ];
        self.add_action_entries(actions);
        self.update_history_actions();
    }

    fn connect_components(&self) {
        let imp = self.imp();
        imp.inspector.set_canvas(&imp.canvas);

        imp.palette.connect_local(
            "shape-activated",
            false,
            glib::clone!(
                #[weak(rename_to = win)]
                self,
                #[upgrade_or]
                None,
                move |args| {
                    let name = args[1].get::<String>().ok()?;
                    win.imp()
                        .canvas
                        .add_node_at_center(NodeShape::from_name(&name)?);
                    None
                }
            ),
        );
        imp.canvas.connect_local(
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
        imp.canvas.connect_local(
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

    fn update_history_actions(&self) {
        let canvas = &self.imp().canvas;
        for (name, enabled) in [("undo", canvas.can_undo()), ("redo", canvas.can_redo())] {
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

    /// Parses the source text. Returns the chart, or shows the error and
    /// returns `None`.
    fn parse_source(&self, text: &str) -> Option<Flowchart> {
        let banner = &self.imp().error_banner;
        match flowchart::parse(text) {
            Ok(chart) => {
                banner.set_revealed(false);
                Some(chart)
            }
            Err(err) => {
                banner.set_title(&err.to_string());
                banner.set_revealed(true);
                None
            }
        }
    }

    fn apply_source(&self) {
        if let Some(chart) = self.parse_source(&self.source_text()) {
            self.imp().canvas.set_chart(chart);
        }
    }

    fn sync_source_from_canvas(&self) {
        let imp = self.imp();
        self.cancel_pending_parse();
        let text = imp.canvas.with_chart(flowchart::serialize);
        if text == self.source_text() {
            return;
        }
        let cursor = imp.buffer.cursor_position();
        imp.syncing.set(true);
        imp.buffer.begin_user_action();
        imp.buffer.set_text(&text);
        imp.buffer.end_user_action();
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

        let chart = self.parse_source(text).unwrap_or_default();
        imp.canvas.load_chart(chart);
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

    async fn new_document(&self) {
        if self.confirm_discard().await {
            self.load_text(NEW_DIAGRAM, None);
        }
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
