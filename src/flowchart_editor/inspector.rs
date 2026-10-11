//! Properties panel for the diagram, the selected node or the selected link.

use std::cell::{Cell, OnceCell};

use adw::subclass::prelude::*;
use gtk::glib;
use gtk::prelude::*;

use adw::prelude::*;

use super::canvas::{Canvas, Selection};
use crate::combo::{index_of, string_list};
use crate::flowchart::{Direction, Edge, Head, Node, NodeShape, Stroke};
use crate::labels;

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/dev/brunopaz/Merman/ui/flowchart-inspector.ui")]
    pub struct Inspector {
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub direction_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub layout_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub node_label_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub node_id_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub node_shape_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub delete_node_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub edge_label_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub edge_stroke_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub edge_start_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub edge_end_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub reverse_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub delete_edge_button: TemplateChild<gtk::Button>,
        pub canvas: OnceCell<Canvas>,
        /// Set while widgets are being filled from the model, so their change
        /// handlers do not write the same values back.
        pub updating: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Inspector {
        const NAME: &'static str = "MermanFlowchartInspector";
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

    fn setup_rows(&self) {
        let imp = self.imp();
        imp.direction_row
            .set_model(Some(&string_list(Direction::ALL.map(labels::direction))));
        imp.node_shape_row
            .set_model(Some(&string_list(NodeShape::ALL.map(labels::shape))));
        imp.edge_stroke_row
            .set_model(Some(&string_list(Stroke::ALL.map(labels::stroke))));
        for row in [&imp.edge_start_row, &imp.edge_end_row] {
            row.set_model(Some(&string_list(Head::ALL.map(labels::head))));
        }

        self.on_selected(&imp.direction_row, |canvas, i| {
            canvas.set_direction(Direction::ALL[i]);
        });
        imp.layout_button.connect_clicked(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |_| {
                if let Some(canvas) = inspector.canvas() {
                    canvas.auto_layout();
                }
            }
        ));

        imp.node_label_row.connect_apply(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| {
                let label = row.text().to_string();
                inspector.edit_node(|node| node.label = label);
            }
        ));
        imp.node_id_row.connect_apply(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| inspector.rename_node(row)
        ));
        imp.node_id_row
            .connect_changed(|row| row.remove_css_class("error"));
        self.on_selected(&imp.node_shape_row, |canvas, i| {
            edit_selected_node(canvas, |node| node.shape = NodeShape::ALL[i]);
        });

        imp.edge_label_row.connect_apply(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| {
                let label = row.text().to_string();
                inspector.edit_edge(|edge| edge.label = label);
            }
        ));
        self.on_selected(&imp.edge_stroke_row, |canvas, i| {
            edit_selected_edge(canvas, |edge| edge.stroke = Stroke::ALL[i]);
        });
        self.on_selected(&imp.edge_start_row, |canvas, i| {
            edit_selected_edge(canvas, |edge| edge.start = Head::ALL[i]);
        });
        self.on_selected(&imp.edge_end_row, |canvas, i| {
            edit_selected_edge(canvas, |edge| edge.end = Head::ALL[i]);
        });
        imp.reverse_button.connect_clicked(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |_| {
                inspector.edit_edge(|edge| {
                    std::mem::swap(&mut edge.from, &mut edge.to);
                    std::mem::swap(&mut edge.start, &mut edge.end);
                });
            }
        ));

        for button in [&imp.delete_node_button, &imp.delete_edge_button] {
            button.connect_clicked(glib::clone!(
                #[weak(rename_to = inspector)]
                self,
                move |_| {
                    if let Some(canvas) = inspector.canvas() {
                        canvas.delete_selection();
                    }
                }
            ));
        }
    }

    /// Runs `apply` with the selected index whenever the user changes `row`.
    fn on_selected(&self, row: &adw::ComboRow, apply: impl Fn(&Canvas, usize) + 'static) {
        row.connect_selected_notify(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| {
                if inspector.imp().updating.get() {
                    return;
                }
                if let Some(canvas) = inspector.canvas() {
                    apply(&canvas, row.selected() as usize);
                }
            }
        ));
    }

    fn edit_node(&self, f: impl FnOnce(&mut Node)) {
        if let Some(canvas) = self.canvas() {
            edit_selected_node(&canvas, f);
        }
    }

    fn edit_edge(&self, f: impl FnOnce(&mut Edge)) {
        if let Some(canvas) = self.canvas() {
            edit_selected_edge(&canvas, f);
        }
    }

    fn rename_node(&self, row: &adw::EntryRow) {
        let (Some(canvas), Some(Selection::Node(id))) =
            (self.canvas(), self.canvas().and_then(|c| c.selection()))
        else {
            return;
        };
        if let Err(err) = canvas.rename_node(&id, row.text().trim()) {
            row.add_css_class("error");
            row.set_tooltip_text(Some(&err.to_string()));
        } else {
            row.set_tooltip_text(None);
        }
    }

    fn refresh(&self) {
        let Some(canvas) = self.canvas() else {
            return;
        };
        let imp = self.imp();
        imp.updating.set(true);
        let selection = canvas.selection();
        canvas.with_chart(|chart| match selection {
            Some(Selection::Node(id)) => {
                if let Some(node) = chart.node(&id) {
                    imp.stack.set_visible_child_name("node");
                    imp.node_label_row.set_text(&node.label);
                    imp.node_id_row.set_text(&node.id);
                    imp.node_id_row.remove_css_class("error");
                    imp.node_shape_row
                        .set_selected(index_of(&NodeShape::ALL, &node.shape));
                }
            }
            Some(Selection::Edge(i)) => {
                if let Some(edge) = chart.edges.get(i) {
                    imp.stack.set_visible_child_name("edge");
                    imp.edge_label_row.set_text(&edge.label);
                    imp.edge_stroke_row
                        .set_selected(index_of(&Stroke::ALL, &edge.stroke));
                    imp.edge_start_row
                        .set_selected(index_of(&Head::ALL, &edge.start));
                    imp.edge_end_row
                        .set_selected(index_of(&Head::ALL, &edge.end));
                }
            }
            None => {
                imp.stack.set_visible_child_name("diagram");
                imp.direction_row
                    .set_selected(index_of(&Direction::ALL, &chart.direction));
            }
        });
        imp.updating.set(false);
    }
}

fn edit_selected_node(canvas: &Canvas, f: impl FnOnce(&mut Node)) {
    if let Some(Selection::Node(id)) = canvas.selection() {
        canvas.edit(|chart| {
            if let Some(node) = chart.node_mut(&id) {
                f(node);
            }
        });
    }
}

fn edit_selected_edge(canvas: &Canvas, f: impl FnOnce(&mut Edge)) {
    if let Some(Selection::Edge(i)) = canvas.selection() {
        canvas.edit(|chart| {
            if let Some(edge) = chart.edges.get_mut(i) {
                f(edge);
            }
        });
    }
}
