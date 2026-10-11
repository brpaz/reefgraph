//! Dialog for picking the type of a new diagram. Each type is shown as a
//! tile with a preview of a sample diagram drawn by its own canvas.

use adw::prelude::*;
use gettextrs::gettext;
use gtk::{gdk, glib};

use crate::box_canvas::BoxCanvas;
use crate::diagram::{self, Diagram, DiagramKind};
use crate::export::{self, ImageFormat};
use crate::{flowchart_editor, labels, sequence_editor};

const PREVIEW_SIZE: (i32, i32) = (200, 140);

/// Shows the dialog over `parent` and calls `on_pick` with the chosen type.
pub fn present(parent: &impl IsA<gtk::Widget>, on_pick: impl Fn(DiagramKind) + 'static) {
    let tiles = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .activate_on_single_click(true)
        .homogeneous(true)
        .min_children_per_line(1)
        .max_children_per_line(4)
        .row_spacing(12)
        .column_spacing(12)
        .margin_top(6)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .valign(gtk::Align::Start)
        .build();
    for kind in DiagramKind::ALL {
        tiles.append(&tile(kind));
    }

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(
        &gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .child(&tiles)
            .build(),
    ));
    let dialog = adw::Dialog::builder()
        .title(gettext("New Diagram"))
        .content_width(560)
        .child(&toolbar)
        .build();

    tiles.connect_child_activated(glib::clone!(
        #[weak]
        dialog,
        move |_, child| {
            let kind = usize::try_from(child.index())
                .ok()
                .and_then(|i| DiagramKind::ALL.get(i));
            if let Some(&kind) = kind {
                dialog.close();
                on_pick(kind);
            }
        }
    ));
    dialog.present(Some(parent));
}

fn tile(kind: DiagramKind) -> gtk::FlowBoxChild {
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
    image.set_paintable(preview(kind).as_ref());
    content.append(&image);
    content.append(
        &gtk::Label::builder()
            .label(labels::diagram_kind(kind))
            .css_classes(["heading"])
            .wrap(true)
            .justify(gtk::Justification::Center)
            .build(),
    );
    content.append(
        &gtk::Label::builder()
            .label(labels::diagram_kind_description(kind))
            .css_classes(["caption", "dim-label"])
            .wrap(true)
            .justify(gtk::Justification::Center)
            .max_width_chars(24)
            .build(),
    );
    gtk::FlowBoxChild::builder()
        .child(&content)
        .css_classes(["card", "activatable", "new-diagram-tile"])
        .build()
}

/// The sample diagram of `kind`, rendered as it would be exported.
fn preview(kind: DiagramKind) -> Option<gdk::Texture> {
    let diagram = diagram::parse(kind.sample())
        .inspect_err(|err| tracing::error!(error = %err.message, "invalid sample diagram"))
        .ok()?;
    let image = match diagram {
        Diagram::Flowchart(chart) => {
            let canvas = flowchart_editor::Canvas::default();
            canvas.set_chart(chart);
            export::render(&canvas, ImageFormat::Png)
        }
        Diagram::Sequence(seq) => {
            let canvas = sequence_editor::Canvas::default();
            canvas.set_sequence(seq);
            export::render(&canvas, ImageFormat::Png)
        }
        Diagram::Erd(erd) => {
            let canvas = BoxCanvas::default();
            canvas.set_diagram(Box::new(erd));
            export::render(&canvas, ImageFormat::Png)
        }
        Diagram::Class(class) => {
            let canvas = BoxCanvas::default();
            canvas.set_diagram(Box::new(class));
            export::render(&canvas, ImageFormat::Png)
        }
    }
    .inspect_err(|err| tracing::error!(error = %err, "failed to render diagram preview"))
    .ok()?;
    gdk::Texture::from_bytes(&glib::Bytes::from_owned(image))
        .inspect_err(|err| tracing::error!(error = %err, "failed to load diagram preview"))
        .ok()
}
