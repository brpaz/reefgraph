//! Dialog for starting a new diagram: an empty diagram of each type, or a
//! diagram copied to the clipboard. Each choice is shown as a tile with a
//! preview drawn by its own canvas.

use std::rc::Rc;

use adw::prelude::*;
use gettextrs::gettext;
use gtk::{gdk, glib};

use crate::app::export::{self, ImageFormat};
use crate::canvas::box_canvas::BoxCanvas;
use crate::diagrams::{self, Diagram, DiagramKind, ForCanvas};
use crate::diagrams::{flowchart, sequence};
use crate::shared::{labels, markdown};

const PREVIEW_SIZE: (i32, i32) = (200, 140);

/// Shows the dialog over `parent` and calls `on_pick` with the source of the
/// chosen diagram.
pub fn present(parent: &impl IsA<gtk::Widget>, on_pick: impl Fn(&str) + 'static) {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(6)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();

    let empty = tile_box();
    for kind in DiagramKind::ALL {
        empty.append(&tile(
            &labels::diagram_kind(kind),
            Some(&labels::diagram_kind_description(kind)),
            diagrams::parse(kind.sample())
                .inspect_err(|err| tracing::error!(error = %err.message, "invalid sample diagram"))
                .ok()
                .and_then(preview),
        ));
    }
    let pasted = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&heading(&gettext("From Clipboard")));
    content.append(&pasted);
    content.append(&heading(&gettext("Empty Diagram")));
    content.append(&empty);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(
        &gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .child(&content)
            .build(),
    ));
    let dialog = adw::Dialog::builder()
        .title(gettext("New Diagram"))
        .content_width(560)
        .child(&toolbar)
        .build();

    let on_pick: Rc<dyn Fn(&str)> = Rc::new(on_pick);
    let templates = DiagramKind::ALL.map(|kind| kind.template().to_owned());
    connect_pick(&empty, &dialog, templates.to_vec(), on_pick.clone());

    let clipboard = parent.clipboard();
    glib::spawn_future_local(glib::clone!(
        #[weak]
        dialog,
        #[weak]
        pasted,
        async move {
            let text = clipboard
                .read_text_future()
                .await
                .inspect_err(|err| tracing::debug!(error = %err, "no text in the clipboard"))
                .ok()
                .flatten()
                .unwrap_or_default();
            show_pasted(&pasted, &dialog, &text, on_pick);
        }
    ));
    dialog.present(Some(parent));
}

/// Fills the clipboard section with a tile per diagram in `text` that parses.
fn show_pasted(container: &gtk::Box, dialog: &adw::Dialog, text: &str, on_pick: Rc<dyn Fn(&str)>) {
    let diagrams: Vec<(String, Diagram)> = markdown::pasted_diagrams(text)
        .into_iter()
        .filter_map(|source| diagrams::parse(&source).ok().map(|d| (source, d)))
        .collect();
    if diagrams.is_empty() {
        container.append(
            &gtk::Label::builder()
                .label(gettext("The clipboard has no supported Mermaid diagram"))
                .css_classes(["dim-label"])
                .xalign(0.0)
                .wrap(true)
                .build(),
        );
        return;
    }
    let tiles = tile_box();
    let mut sources = Vec::with_capacity(diagrams.len());
    for (source, diagram) in diagrams {
        tiles.append(&tile(
            &labels::diagram_kind(diagram.kind()),
            None,
            preview(diagram),
        ));
        sources.push(source);
    }
    connect_pick(&tiles, dialog, sources, on_pick);
    container.append(&tiles);
}

/// Closes the dialog and passes on the source of the activated tile.
fn connect_pick(
    tiles: &gtk::FlowBox,
    dialog: &adw::Dialog,
    sources: Vec<String>,
    on_pick: Rc<dyn Fn(&str)>,
) {
    tiles.connect_child_activated(glib::clone!(
        #[weak]
        dialog,
        move |_, child| {
            let source = usize::try_from(child.index())
                .ok()
                .and_then(|i| sources.get(i));
            if let Some(source) = source {
                dialog.close();
                on_pick(source);
            }
        }
    ));
}

fn heading(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .css_classes(["heading"])
        .xalign(0.0)
        .build()
}

fn tile_box() -> gtk::FlowBox {
    gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .activate_on_single_click(true)
        .homogeneous(true)
        .min_children_per_line(1)
        .max_children_per_line(4)
        .row_spacing(12)
        .column_spacing(12)
        .valign(gtk::Align::Start)
        .build()
}

fn tile(
    title: &str,
    description: Option<&str>,
    preview: Option<gdk::Texture>,
) -> gtk::FlowBoxChild {
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .build();
    // An image, unlike a picture, sizes to `pixel_size` rather than to the
    // preview texture, which is rendered at twice its displayed size.
    let image = gtk::Image::builder()
        .pixel_size(PREVIEW_SIZE.1)
        .width_request(PREVIEW_SIZE.0)
        .height_request(PREVIEW_SIZE.1)
        .build();
    image.set_paintable(preview.as_ref());
    content.append(&image);
    content.append(
        &gtk::Label::builder()
            .label(title)
            .css_classes(["heading"])
            .wrap(true)
            .justify(gtk::Justification::Center)
            .build(),
    );
    if let Some(description) = description {
        content.append(
            &gtk::Label::builder()
                .label(description)
                .css_classes(["caption", "dim-label"])
                .wrap(true)
                .justify(gtk::Justification::Center)
                .max_width_chars(24)
                .build(),
        );
    }
    gtk::FlowBoxChild::builder()
        .child(&content)
        .css_classes(["card", "activatable", "new-diagram-tile"])
        .build()
}

/// The diagram rendered as it would be exported.
fn preview(diagram: Diagram) -> Option<gdk::Texture> {
    let image = match diagram.for_canvas() {
        ForCanvas::Flowchart(chart) => {
            let canvas = flowchart::editor::Canvas::default();
            canvas.set_chart(chart);
            export::render(&canvas, ImageFormat::Png)
        }
        ForCanvas::Sequence(seq) => {
            let canvas = sequence::editor::Canvas::default();
            canvas.set_sequence(seq);
            export::render(&canvas, ImageFormat::Png)
        }
        ForCanvas::Boxes(boxes) => {
            let canvas = BoxCanvas::default();
            canvas.set_diagram(boxes);
            export::render(&canvas, ImageFormat::Png)
        }
    }
    .inspect_err(|err| tracing::error!(error = %err, "failed to render diagram preview"))
    .ok()?;
    gdk::Texture::from_bytes(&glib::Bytes::from_owned(image))
        .inspect_err(|err| tracing::error!(error = %err, "failed to load diagram preview"))
        .ok()
}
