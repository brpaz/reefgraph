//! Shape palette: click a tile to add a node, or drag it onto the canvas.

use std::sync::OnceLock;

use adw::subclass::prelude::*;
use gtk::glib::subclass::Signal;
use gtk::prelude::*;
use gtk::{gdk, glib};

use crate::canvas::{Rect, shape_path, shape_size};
use crate::flowchart::{NodeShape, Point};
use crate::labels;

const PREVIEW_SIZE: (i32, i32) = (56, 36);

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/dev/brunopaz/Merman/ui/palette.ui")]
    pub struct Palette {
        #[template_child]
        pub flowbox: TemplateChild<gtk::FlowBox>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Palette {
        const NAME: &'static str = "MermanPalette";
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
                    Signal::builder("shape-activated")
                        .param_types([String::static_type()])
                        .build(),
                ]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            for shape in NodeShape::ALL {
                self.flowbox.append(&obj.create_tile(shape));
            }
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
    fn create_tile(&self, shape: NodeShape) -> gtk::Button {
        let title = labels::shape(shape);
        let preview = gtk::DrawingArea::builder()
            .content_width(PREVIEW_SIZE.0)
            .content_height(PREVIEW_SIZE.1)
            .halign(gtk::Align::Center)
            .build();
        preview.set_draw_func(move |area, cr, w, h| draw_preview(area, cr, shape, w, h));

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
            move |_| palette.emit_by_name::<()>("shape-activated", &[&shape.name().to_string()])
        ));

        let source = gtk::DragSource::new();
        source.set_actions(gdk::DragAction::COPY);
        source.set_content(Some(&gdk::ContentProvider::for_value(
            &shape.name().to_string().to_value(),
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

fn draw_preview(
    area: &gtk::DrawingArea,
    cr: &gtk::cairo::Context,
    shape: NodeShape,
    w: i32,
    h: i32,
) {
    let (w, h) = (w as f64 - 4.0, h as f64 - 4.0);
    // Keep each shape's natural proportions within the preview box.
    let (sw, sh) = shape_size(shape, (40.0, 16.0));
    let scale = (w / sw).min(h / sh);
    let rect = Rect::centered(
        Point::new(w / 2.0 + 2.0, h / 2.0 + 2.0),
        (sw * scale, sh * scale),
    );
    let color = area.color();
    if shape_path(cr, shape, &rect).is_ok() {
        cr.set_source_rgba(
            color.red() as f64,
            color.green() as f64,
            color.blue() as f64,
            color.alpha() as f64,
        );
        cr.set_line_width(1.5);
        let _ = cr.stroke();
    }
}
