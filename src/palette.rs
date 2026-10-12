//! Palette of diagram elements: click a tile to add it, or drag it onto the canvas.
//! The items depend on the diagram type being edited.

use std::sync::OnceLock;

use adw::subclass::prelude::*;
use gtk::glib::subclass::Signal;
use gtk::prelude::*;
use gtk::{cairo, gdk, glib};

use crate::drawing::set_color;
use crate::geometry::{Point, Rect};

const PREVIEW_SIZE: (i32, i32) = (56, 36);

/// Builds a tile's preview outline inside the given rectangle.
pub type PreviewFn = Box<dyn Fn(&cairo::Context, &Rect)>;

/// One palette tile. `id` is the drag-and-drop payload and the value of
/// the `item-activated` signal.
pub struct PaletteItem {
    pub id: &'static str,
    pub title: String,
    /// Builds the preview outline inside the given rectangle; it is stroked
    /// afterwards.
    pub preview: PreviewFn,
    /// Preview width / height ratio.
    pub aspect: f64,
}

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/dev/brunopaz/Reefgraph/ui/palette.ui")]
    pub struct Palette {
        #[template_child]
        pub flowbox: TemplateChild<gtk::FlowBox>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Palette {
        const NAME: &'static str = "ReefgraphPalette";
        type Type = super::Palette;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Palette {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("item-activated")
                        .param_types([String::static_type()])
                        .build(),
                ]
            })
        }
    }

    impl WidgetImpl for Palette {}
    impl BinImpl for Palette {}
}

glib::wrapper! {
    pub struct Palette(ObjectSubclass<imp::Palette>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Palette {
    pub fn set_items(&self, items: Vec<PaletteItem>) {
        let flowbox = &self.imp().flowbox;
        flowbox.remove_all();
        for item in items {
            flowbox.append(&self.create_tile(item));
        }
    }

    fn create_tile(&self, item: PaletteItem) -> gtk::Button {
        let PaletteItem {
            id,
            title,
            preview: build_path,
            aspect,
        } = item;
        let preview = gtk::DrawingArea::builder()
            .content_width(PREVIEW_SIZE.0)
            .content_height(PREVIEW_SIZE.1)
            .halign(gtk::Align::Center)
            .build();
        preview.set_draw_func(move |area, cr, w, h| {
            let (w, h) = (w as f64 - 4.0, h as f64 - 4.0);
            let (pw, ph) = if w / h > aspect {
                (h * aspect, h)
            } else {
                (w, w / aspect)
            };
            let rect = Rect::centered(Point::new(w / 2.0 + 2.0, h / 2.0 + 2.0), (pw, ph));
            cr.new_path();
            build_path(cr, &rect);
            set_color(cr, &area.color());
            cr.set_line_width(1.5);
            let _ = cr.stroke();
        });

        let label = gtk::Label::builder()
            .label(&title)
            .wrap(true)
            .justify(gtk::Justification::Center)
            .max_width_chars(10)
            .css_classes(["caption"])
            .build();
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();
        content.append(&preview);
        content.append(&label);

        let button = gtk::Button::builder()
            .child(&content)
            .tooltip_text(title)
            .css_classes(["flat", "palette-tile"])
            .build();
        button.connect_clicked(glib::clone!(
            #[weak(rename_to = palette)]
            self,
            move |_| palette.emit_by_name::<()>("item-activated", &[&id.to_string()])
        ));

        let source = gtk::DragSource::new();
        source.set_actions(gdk::DragAction::COPY);
        source.set_content(Some(&gdk::ContentProvider::for_value(
            &id.to_string().to_value(),
        )));
        source.connect_drag_begin(glib::clone!(
            #[weak]
            preview,
            move |source, _| {
                let icon = gtk::WidgetPaintable::new(Some(&preview));
                source.set_icon(Some(&icon), PREVIEW_SIZE.0 / 2, PREVIEW_SIZE.1 / 2);
            }
        ));
        button.add_controller(source);
        button
    }
}
