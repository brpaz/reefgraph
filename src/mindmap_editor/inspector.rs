//! Properties panel for a mindmap and its selected node.

use std::cell::{Cell, OnceCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

use crate::box_canvas::{BoxCanvas as Canvas, Selection};
use crate::combo::{index_of, string_list};
use crate::labels;
use crate::mindmap::{Mindmap, Shape};

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/dev/brunopaz/Merman/ui/mindmap-inspector.ui")]
    pub struct Inspector {
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub node_text_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub node_shape_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub add_child_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub delete_node_button: TemplateChild<gtk::Button>,
        pub canvas: OnceCell<Canvas>,
        /// Set while widgets are being filled from the model, so their change
        /// handlers do not write the same values back.
        pub updating: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Inspector {
        const NAME: &'static str = "MermanMindmapInspector";
        type Type = super::Inspector;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Inspector {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_rows();
        }
    }

    impl WidgetImpl for Inspector {}
    impl BinImpl for Inspector {}
}

glib::wrapper! {
    pub struct Inspector(ObjectSubclass<imp::Inspector>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Inspector {
    pub fn set_canvas(&self, canvas: &Canvas) {
        if self.imp().canvas.set(canvas.clone()).is_err() {
            return;
        }
        canvas.connect_local(
            "selection-changed",
            false,
            glib::clone!(
                #[weak(rename_to = inspector)]
                self,
                #[upgrade_or]
                None,
                move |_| {
                    inspector.refresh();
                    None
                }
            ),
        );
        self.refresh();
    }

    fn canvas(&self) -> Option<Canvas> {
        self.imp().canvas.get().cloned()
    }

    fn selected_node(&self) -> Option<(Canvas, String)> {
        let canvas = self.canvas()?;
        match canvas.selection() {
            Some(Selection::Box(key)) => Some((canvas, key)),
            _ => None,
        }
    }

    fn setup_rows(&self) {
        let imp = self.imp();
        imp.node_shape_row
            .set_model(Some(&string_list(Shape::ALL.map(labels::mindmap_shape))));

        imp.node_text_row.connect_apply(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| {
                let text = row.text().trim().to_string();
                if text.is_empty() {
                    return;
                }
                if let Some((canvas, key)) = inspector.selected_node() {
                    canvas.edit::<Mindmap>(|m| {
                        if let Some(node) = m.node_mut(&key) {
                            node.set_text(&text);
                        }
                    });
                }
            }
        ));
        imp.node_shape_row.connect_selected_notify(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| {
                if inspector.imp().updating.get() {
                    return;
                }
                let shape = Shape::ALL[row.selected() as usize];
                if let Some((canvas, key)) = inspector.selected_node() {
                    canvas.edit::<Mindmap>(|m| m.set_shape(&key, shape));
                }
            }
        ));
        imp.add_child_button.connect_clicked(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |_| {
                if let Some((canvas, key)) = inspector.selected_node() {
                    canvas.add_row(&key);
                }
            }
        ));
        imp.delete_node_button.connect_clicked(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |_| {
                if let Some(canvas) = inspector.canvas() {
                    canvas.delete_selection();
                }
            }
        ));
    }

    fn refresh(&self) {
        let imp = self.imp();
        imp.updating.set(true);
        let node = self.selected_node().and_then(|(canvas, key)| {
            canvas
                .with::<Mindmap, _>(|m| {
                    let n = m.node(&key)?;
                    // The root goes only with its children, which is refused.
                    let deletable = n.parent.is_some() || m.nodes.len() == 1;
                    Some((n.display(), n.shape, deletable))
                })
                .flatten()
        });
        match node {
            Some((text, shape, deletable)) => {
                imp.delete_node_button.set_sensitive(deletable);
                imp.node_text_row.set_text(&text);
                imp.node_shape_row
                    .set_selected(index_of(&Shape::ALL, &shape));
                imp.stack.set_visible_child_name("node");
            }
            None => imp.stack.set_visible_child_name("diagram"),
        }
        imp.updating.set(false);
    }
}
