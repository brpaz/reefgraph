use std::cell::{Cell, RefCell};
use std::time::Duration;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};
use sourceview5::prelude::*;

use crate::app::application::ReefgraphApplication;
use crate::app::config::{APP_ID, PROFILE};
use crate::app::export::{self, ExportError, ImageFormat};
use crate::app::new_diagram;
use crate::app::palette::Palette;
use crate::canvas::box_canvas::BoxCanvas;
use crate::canvas::interaction::Navigable;
use crate::diagrams::flowchart::{self, NodeShape};
use crate::diagrams::{self, Diagram, DiagramKind, ForCanvas};
use crate::diagrams::{c4, class, erd, mindmap, sequence, state, usecase};
use crate::shared::labels;
use crate::shared::markdown::{self, Embedding};

/// Pause after the last keystroke before the source is re-parsed.
const PARSE_DELAY: Duration = Duration::from_millis(300);

mod imp {
    use super::*;

    #[derive(Debug, gtk::CompositeTemplate)]
    #[template(resource = "/dev/brunopaz/Reefgraph/ui/window.ui")]
    pub struct ReefgraphApplicationWindow {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub window_title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub palette: TemplateChild<Palette>,
        #[template_child]
        pub editor_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub flowchart_canvas: TemplateChild<flowchart::editor::Canvas>,
        #[template_child]
        pub sequence_canvas: TemplateChild<sequence::editor::Canvas>,
        #[template_child]
        pub state_canvas: TemplateChild<BoxCanvas>,
        #[template_child]
        pub usecase_canvas: TemplateChild<BoxCanvas>,
        #[template_child]
        pub mindmap_canvas: TemplateChild<BoxCanvas>,
        #[template_child]
        pub erd_canvas: TemplateChild<BoxCanvas>,
        #[template_child]
        pub class_canvas: TemplateChild<BoxCanvas>,
        #[template_child]
        pub c4_canvas: TemplateChild<BoxCanvas>,
        #[template_child]
        pub inspector_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub flowchart_inspector: TemplateChild<flowchart::editor::Inspector>,
        #[template_child]
        pub sequence_inspector: TemplateChild<sequence::editor::Inspector>,
        #[template_child]
        pub state_inspector: TemplateChild<state::editor::Inspector>,
        #[template_child]
        pub usecase_inspector: TemplateChild<usecase::editor::Inspector>,
        #[template_child]
        pub mindmap_inspector: TemplateChild<mindmap::editor::Inspector>,
        #[template_child]
        pub erd_inspector: TemplateChild<erd::editor::Inspector>,
        #[template_child]
        pub class_inspector: TemplateChild<class::editor::Inspector>,
        #[template_child]
        pub c4_inspector: TemplateChild<c4::editor::Inspector>,
        #[template_child]
        pub error_banner: TemplateChild<adw::Banner>,
        #[template_child]
        pub source_scroller: TemplateChild<gtk::ScrolledWindow>,
        pub buffer: sourceview5::Buffer,
        pub settings: gio::Settings,
        pub kind: Cell<DiagramKind>,
        pub file: RefCell<Option<gio::File>>,
        /// Set when `file` is a Markdown document and the source is one of
        /// its Mermaid blocks.
        pub embedding: RefCell<Option<Embedding>>,
        pub dirty: Cell<bool>,
        /// Set while the buffer is rewritten from the canvas, so the change is
        /// not parsed back into the canvas.
        pub syncing: Cell<bool>,
        pub pending_parse: RefCell<Option<glib::SourceId>>,
    }

    impl Default for ReefgraphApplicationWindow {
        fn default() -> Self {
            Self {
                toast_overlay: TemplateChild::default(),
                window_title: TemplateChild::default(),
                palette: TemplateChild::default(),
                editor_stack: TemplateChild::default(),
                flowchart_canvas: TemplateChild::default(),
                sequence_canvas: TemplateChild::default(),
                state_canvas: TemplateChild::default(),
                usecase_canvas: TemplateChild::default(),
                mindmap_canvas: TemplateChild::default(),
                erd_canvas: TemplateChild::default(),
                class_canvas: TemplateChild::default(),
                c4_canvas: TemplateChild::default(),
                inspector_stack: TemplateChild::default(),
                flowchart_inspector: TemplateChild::default(),
                sequence_inspector: TemplateChild::default(),
                state_inspector: TemplateChild::default(),
                usecase_inspector: TemplateChild::default(),
                mindmap_inspector: TemplateChild::default(),
                erd_inspector: TemplateChild::default(),
                class_inspector: TemplateChild::default(),
                c4_inspector: TemplateChild::default(),
                error_banner: TemplateChild::default(),
                source_scroller: TemplateChild::default(),
                buffer: sourceview5::Buffer::new(None),
                settings: gio::Settings::new(*APP_ID),
                kind: Cell::new(DiagramKind::Flowchart),
                file: RefCell::default(),
                embedding: RefCell::default(),
                dirty: Cell::new(false),
                syncing: Cell::new(false),
                pending_parse: RefCell::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ReefgraphApplicationWindow {
        const NAME: &'static str = "ReefgraphApplicationWindow";
        type Type = super::ReefgraphApplicationWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            flowchart::editor::Canvas::ensure_type();
            flowchart::editor::Inspector::ensure_type();
            sequence::editor::Canvas::ensure_type();
            sequence::editor::Inspector::ensure_type();
            BoxCanvas::ensure_type();
            state::editor::Inspector::ensure_type();
            usecase::editor::Inspector::ensure_type();
            mindmap::editor::Inspector::ensure_type();
            erd::editor::Inspector::ensure_type();
            class::editor::Inspector::ensure_type();
            c4::editor::Inspector::ensure_type();
            Palette::ensure_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ReefgraphApplicationWindow {
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
            obj.load_text(DiagramKind::Flowchart.sample(), None);
        }
    }

    impl WidgetImpl for ReefgraphApplicationWindow {}

    impl WindowImpl for ReefgraphApplicationWindow {
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

    impl ApplicationWindowImpl for ReefgraphApplicationWindow {}
    impl AdwApplicationWindowImpl for ReefgraphApplicationWindow {}
}

glib::wrapper! {
    pub struct ReefgraphApplicationWindow(ObjectSubclass<imp::ReefgraphApplicationWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionMap, gio::ActionGroup,
                    gtk::Root, gtk::Native, gtk::ShortcutManager,
                    gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl ReefgraphApplicationWindow {
    pub fn new(app: &ReefgraphApplication) -> Self {
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

    fn show_shortcuts(&self) {
        let builder = gtk::Builder::from_resource("/dev/brunopaz/Reefgraph/ui/shortcuts.ui");
        let dialog: adw::ShortcutsDialog = builder
            .object("shortcuts_dialog")
            .expect("shortcuts_dialog is defined in shortcuts.blp");
        dialog.present(Some(self));
    }

    fn setup_actions(&self) {
        let actions = [
            gio::ActionEntry::builder("new-diagram")
                .activate(|win: &Self, _, _| {
                    new_diagram::present(
                        win,
                        glib::clone!(
                            #[weak]
                            win,
                            move |source| win.spawn_new_document(source)
                        ),
                    )
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
            gio::ActionEntry::builder("export-image")
                .activate(|win: &Self, _, _| {
                    let win = win.clone();
                    glib::spawn_future_local(async move { win.export_image().await });
                })
                .build(),
            gio::ActionEntry::builder("copy-source")
                .activate(|win: &Self, _, _| win.copy_source())
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
            gio::ActionEntry::builder("show-help-overlay")
                .activate(|win: &Self, _, _| win.show_shortcuts())
                .build(),
        ];
        self.add_action_entries(actions);
    }

    fn connect_components(&self) {
        let imp = self.imp();
        imp.flowchart_inspector.set_canvas(&imp.flowchart_canvas);
        imp.sequence_inspector.set_canvas(&imp.sequence_canvas);
        imp.state_inspector.set_canvas(&imp.state_canvas);
        imp.usecase_inspector.set_canvas(&imp.usecase_canvas);
        imp.mindmap_inspector.set_canvas(&imp.mindmap_canvas);
        imp.erd_inspector.set_canvas(&imp.erd_canvas);
        imp.class_inspector.set_canvas(&imp.class_canvas);
        imp.c4_inspector.set_canvas(&imp.c4_canvas);

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

        let canvases: [&gtk::Widget; 8] = [
            imp.flowchart_canvas.upcast_ref(),
            imp.sequence_canvas.upcast_ref(),
            imp.state_canvas.upcast_ref(),
            imp.usecase_canvas.upcast_ref(),
            imp.mindmap_canvas.upcast_ref(),
            imp.erd_canvas.upcast_ref(),
            imp.class_canvas.upcast_ref(),
            imp.c4_canvas.upcast_ref(),
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
            DiagramKind::Flowchart => ("flowchart", flowchart::editor::palette_items()),
            DiagramKind::Sequence => ("sequence", sequence::editor::palette_items()),
            DiagramKind::State => ("state", state::editor::palette_items()),
            DiagramKind::UseCase => ("usecase", usecase::editor::palette_items()),
            DiagramKind::Mindmap => ("mindmap", mindmap::editor::palette_items()),
            DiagramKind::Erd => ("erd", erd::editor::palette_items()),
            DiagramKind::Class => ("class", class::editor::palette_items()),
            DiagramKind::C4 => ("c4", c4::editor::palette_items()),
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
            kind => {
                if let Some(canvas) = self.box_canvas(kind) {
                    zoom_canvas(canvas, zoom_in);
                }
            }
        }
    }

    fn undo(&self) {
        let imp = self.imp();
        match imp.kind.get() {
            DiagramKind::Flowchart => imp.flowchart_canvas.undo(),
            DiagramKind::Sequence => imp.sequence_canvas.undo(),
            kind => {
                if let Some(canvas) = self.box_canvas(kind) {
                    canvas.undo();
                }
            }
        }
    }

    fn redo(&self) {
        let imp = self.imp();
        match imp.kind.get() {
            DiagramKind::Flowchart => imp.flowchart_canvas.redo(),
            DiagramKind::Sequence => imp.sequence_canvas.redo(),
            kind => {
                if let Some(canvas) = self.box_canvas(kind) {
                    canvas.redo();
                }
            }
        }
    }

    fn fit_to_content(&self) {
        let imp = self.imp();
        match imp.kind.get() {
            DiagramKind::Flowchart => imp.flowchart_canvas.fit_to_content(),
            DiagramKind::Sequence => imp.sequence_canvas.fit_to_content(),
            kind => {
                if let Some(canvas) = self.box_canvas(kind) {
                    canvas.fit_to_content();
                }
            }
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
                if let Some(element) = sequence::editor::PaletteElement::from_id(id) {
                    imp.sequence_canvas.add_element(element, None);
                }
            }
            kind => {
                if let Some(canvas) = self.box_canvas(kind) {
                    canvas.add_box(id, None);
                }
            }
        }
    }

    /// The box canvas editing diagrams of `kind`, if they use one.
    fn box_canvas(&self, kind: DiagramKind) -> Option<&BoxCanvas> {
        let imp = self.imp();
        let canvas = match kind {
            DiagramKind::Flowchart | DiagramKind::Sequence => return None,
            DiagramKind::State => &imp.state_canvas,
            DiagramKind::UseCase => &imp.usecase_canvas,
            DiagramKind::Mindmap => &imp.mindmap_canvas,
            DiagramKind::Erd => &imp.erd_canvas,
            DiagramKind::Class => &imp.class_canvas,
            DiagramKind::C4 => &imp.c4_canvas,
        };
        Some(canvas)
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
            kind => self
                .box_canvas(kind)
                .map_or((false, false), |c| (c.can_undo(), c.can_redo())),
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
        match diagrams::parse(text) {
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
        let kind = diagram.kind();
        match diagram.for_canvas() {
            ForCanvas::Flowchart(chart) => imp.flowchart_canvas.set_chart(chart),
            ForCanvas::Sequence(seq) => imp.sequence_canvas.set_sequence(seq),
            ForCanvas::Boxes(boxes) => {
                if let Some(canvas) = self.box_canvas(kind) {
                    canvas.set_diagram(boxes);
                }
            }
        }
    }

    /// Shows the diagram in its editor with fresh undo history and viewport.
    fn load_diagram(&self, diagram: Diagram) {
        let imp = self.imp();
        let kind = diagram.kind();
        self.show_kind(kind);
        match diagram.for_canvas() {
            ForCanvas::Flowchart(chart) => imp.flowchart_canvas.load_chart(chart),
            ForCanvas::Sequence(seq) => imp.sequence_canvas.load_sequence(seq),
            ForCanvas::Boxes(boxes) => {
                if let Some(canvas) = self.box_canvas(kind) {
                    canvas.load_diagram(boxes);
                }
            }
        }
    }

    fn sync_source_from_canvas(&self) {
        let imp = self.imp();
        self.cancel_pending_parse();
        let text = match imp.kind.get() {
            DiagramKind::Flowchart => imp.flowchart_canvas.with_chart(flowchart::serialize),
            DiagramKind::Sequence => imp.sequence_canvas.with_sequence(sequence::serialize),
            DiagramKind::State => imp.state_canvas.with(state::serialize).unwrap_or_default(),
            DiagramKind::UseCase => imp
                .usecase_canvas
                .with(usecase::serialize)
                .unwrap_or_default(),
            DiagramKind::Mindmap => imp
                .mindmap_canvas
                .with(mindmap::serialize)
                .unwrap_or_default(),
            DiagramKind::Erd => imp.erd_canvas.with(erd::serialize).unwrap_or_default(),
            DiagramKind::Class => imp.class_canvas.with(class::serialize).unwrap_or_default(),
            DiagramKind::C4 => imp.c4_canvas.with(c4::serialize).unwrap_or_default(),
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
        imp.embedding.replace(None);
        imp.dirty.set(false);
        self.update_title();
    }

    fn load_embedding(&self, embedding: Embedding, file: gio::File) {
        self.load_text(&embedding.source(), Some(file));
        self.imp().embedding.replace(Some(embedding));
    }

    pub async fn load_file(&self, file: &gio::File) {
        let text = match file.load_contents_future().await {
            Ok((bytes, _)) => String::from_utf8(bytes.to_vec()).ok(),
            Err(err) => {
                tracing::error!(error = %err, "failed to read diagram");
                None
            }
        };
        let Some(text) = text else {
            return self.toast(&gettext("Could not open the file"));
        };
        let is_markdown = file
            .basename()
            .is_some_and(|name| markdown::is_markdown_name(&name.to_string_lossy()));
        if is_markdown {
            self.open_markdown(text, file.clone()).await;
        } else {
            self.load_text(&text, Some(file.clone()));
        }
    }

    /// Opens a Mermaid block of a Markdown document, asking which one when
    /// there are several.
    async fn open_markdown(&self, text: String, file: gio::File) {
        let mut blocks = markdown::mermaid_blocks(&text);
        let index = match blocks.len() {
            0 => return self.toast(&gettext("The file has no Mermaid diagrams")),
            1 => 0,
            _ => match self.choose_block(&text, &blocks).await {
                Some(index) => index,
                None => return,
            },
        };
        let block = blocks.swap_remove(index);
        self.load_embedding(Embedding::new(text, block), file);
    }

    async fn choose_block(&self, text: &str, blocks: &[markdown::MermaidBlock]) -> Option<usize> {
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        for block in blocks {
            let source = block.source(text);
            let title = match diagrams::parse(&source) {
                Ok(diagram) => labels::diagram_kind(diagram.kind()),
                Err(_) => crate::shared::syntax::split_source(&source)
                    .map(|s| s.keyword().to_owned())
                    .unwrap_or_default(),
            };
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&title))
                .subtitle(gettext("Line {}").replace("{}", &block.line.to_string()))
                .activatable(true)
                .build();
            row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
            list.append(&row);
        }

        let dialog = adw::AlertDialog::new(
            Some(&gettext("Choose a Diagram")),
            Some(&gettext("The file has several Mermaid diagrams.")),
        );
        dialog.set_extra_child(Some(&list));
        dialog.add_response("cancel", &gettext("_Cancel"));
        dialog.set_close_response("cancel");

        let chosen = std::rc::Rc::new(Cell::new(None));
        list.connect_row_activated(glib::clone!(
            #[weak]
            dialog,
            #[strong]
            chosen,
            move |_, row| {
                chosen.set(usize::try_from(row.index()).ok());
                dialog.close();
            }
        ));
        dialog.choose_future(Some(self)).await;
        chosen.get()
    }

    fn spawn_new_document(&self, source: &str) {
        let win = self.clone();
        let source = source.to_owned();
        glib::spawn_future_local(async move {
            if win.confirm_discard().await {
                win.load_text(&source, None);
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
        let filters = file_filters();
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Save Diagram"))
            .modal(true)
            .initial_name(initial_name)
            .filters(&filters)
            .build();
        if self.imp().embedding.borrow().is_some() {
            dialog.set_default_filter(filters.item(MARKDOWN_FILTER).and_downcast_ref());
        }
        match dialog.save_future(Some(self)).await {
            Ok(file) => self.write_file(&file).await,
            Err(err) => {
                log_dialog_error(&err);
                false
            }
        }
    }

    /// Writes the source, or, to a Markdown file, the Markdown document with
    /// the source in place of the block being edited. Saving to any other
    /// file writes the source alone and leaves the Markdown document behind.
    async fn write_file(&self, file: &gio::File) -> bool {
        let imp = self.imp();
        let source = self.source_text();
        let to_markdown = file
            .basename()
            .is_some_and(|name| markdown::is_markdown_name(&name.to_string_lossy()));
        let contents = match imp.embedding.borrow_mut().as_mut() {
            Some(embedding) if to_markdown => embedding.replace(&source).to_owned(),
            _ => source,
        }
        .into_bytes();
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
                if !to_markdown {
                    imp.embedding.replace(None);
                }
                imp.file.replace(Some(file.clone()));
                imp.dirty.set(false);
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
    pub(crate) async fn confirm_discard(&self) -> bool {
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

    /// Copies the Mermaid source to the clipboard without the comments this
    /// editor adds for itself, ready to paste where Mermaid is rendered.
    fn copy_source(&self) {
        let text = crate::shared::syntax::strip_editor_comments(&self.source_text());
        self.clipboard().set_text(&text);
        self.toast(&gettext("Mermaid source copied"));
    }

    /// Asks for a file and exports the diagram there as SVG, or as PNG when
    /// the file name ends in `.png`.
    async fn export_image(&self) {
        let imp = self.imp();
        let stem = imp
            .file
            .borrow()
            .as_ref()
            .and_then(|f| f.path())
            .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| gettext("diagram"));
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Export Image"))
            .modal(true)
            .initial_name(format!("{stem}.svg"))
            .filters(&image_filters())
            .build();
        let file = match dialog.save_future(Some(self)).await {
            Ok(file) => file,
            Err(err) => return log_dialog_error(&err),
        };
        let name = file
            .basename()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let format = ImageFormat::from_file_name(&name);
        let image = match imp.kind.get() {
            DiagramKind::Flowchart => export::render(&*imp.flowchart_canvas, format),
            DiagramKind::Sequence => export::render(&*imp.sequence_canvas, format),
            kind => match self.box_canvas(kind) {
                Some(canvas) => export::render(canvas, format),
                None => Err(ExportError::Empty),
            },
        };
        let image = match image {
            Ok(image) => image,
            Err(ExportError::Empty) => return self.toast(&gettext("The diagram is empty")),
            Err(err) => {
                tracing::error!(error = %err, "failed to render image");
                return self.toast(&gettext("Could not export the image"));
            }
        };
        let result = file
            .replace_contents_future(
                image,
                None,
                false,
                gio::FileCreateFlags::REPLACE_DESTINATION,
            )
            .await;
        match result {
            Ok(_) => self.toast(&gettext("Image exported")),
            Err((_, err)) => {
                tracing::error!(error = %err, "failed to write image");
                self.toast(&gettext("Could not export the image"));
            }
        }
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

/// Position of the Markdown filter in `file_filters`.
const MARKDOWN_FILTER: u32 = 1;

fn file_filters() -> gio::ListStore {
    let mermaid = gtk::FileFilter::new();
    mermaid.set_name(Some(&gettext("Mermaid Diagrams")));
    mermaid.add_suffix("mmd");
    mermaid.add_suffix("mermaid");
    let markdown = gtk::FileFilter::new();
    markdown.set_name(Some(&gettext("Markdown Documents")));
    markdown.add_suffix("md");
    markdown.add_suffix("markdown");
    let all = gtk::FileFilter::new();
    all.set_name(Some(&gettext("All Files")));
    all.add_pattern("*");
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&mermaid);
    filters.append(&markdown);
    filters.append(&all);
    filters
}

fn image_filters() -> gio::ListStore {
    let svg = gtk::FileFilter::new();
    svg.set_name(Some(&gettext("SVG Images")));
    svg.add_suffix("svg");
    let png = gtk::FileFilter::new();
    png.set_name(Some(&gettext("PNG Images")));
    png.add_suffix("png");
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&svg);
    filters.append(&png);
    filters
}

fn log_dialog_error(err: &glib::Error) {
    if !err.matches(gtk::DialogError::Dismissed) && !err.matches(gtk::DialogError::Cancelled) {
        tracing::error!(error = %err, "file dialog failed");
    }
}
