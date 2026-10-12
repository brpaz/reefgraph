//! Properties panel for a use case diagram, its boundaries and its selection.

use std::cell::{Cell, OnceCell, RefCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;

use crate::box_canvas::{BoxCanvas as Canvas, Selection};
use crate::combo::{index_of, string_list};
use crate::ids::RenameError;
use crate::labels;
use crate::usecase::model::{Boundary, Direction};
use crate::usecase::{Element, ElementKind, Label, Relation, RelationKind, UseCaseDiagram};

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/dev/brunopaz/Reefgraph/ui/usecase-inspector.ui")]
    pub struct Inspector {
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub direction_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub layout_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub boundaries_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub add_boundary_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub element_id_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub element_label_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub element_kind_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub element_boundary_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub delete_element_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub relation_kind_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub relation_label_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub relation_from_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub relation_to_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub delete_relation_button: TemplateChild<gtk::Button>,
        pub canvas: OnceCell<Canvas>,
        /// Rows currently shown in the boundaries group, one per boundary.
        pub boundary_rows: RefCell<Vec<adw::ExpanderRow>>,
        /// Boundary whose row is expanded, kept across rebuilds.
        pub expanded: RefCell<Option<String>>,
        /// Set while widgets are being filled from the model, so their change
        /// handlers do not write the same values back.
        pub updating: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Inspector {
        const NAME: &'static str = "ReefgraphUseCaseInspector";
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

/// Direction row choices; the first entry leaves the direction unset.
fn direction_choices() -> Vec<Option<Direction>> {
    std::iter::once(None)
        .chain(Direction::ALL.into_iter().map(Some))
        .collect()
}

/// Boundary row choices of an element: none, then every boundary.
fn boundary_choices(d: &UseCaseDiagram) -> Vec<Option<String>> {
    std::iter::once(None)
        .chain(d.boundaries.iter().map(|b| Some(b.id.clone())))
        .collect()
}

impl Inspector {
    pub fn set_canvas(&self, canvas: &Canvas) {
        if self.imp().canvas.set(canvas.clone()).is_err() {
            return;
        }
        // Boundaries are listed on the diagram page, which also has to follow
        // edits made outside the panel.
        for signal in ["selection-changed", "changed"] {
            canvas.connect_local(
                signal,
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
        }
        self.refresh();
    }

    fn canvas(&self) -> Option<Canvas> {
        self.imp().canvas.get().cloned()
    }

    fn setup_rows(&self) {
        let imp = self.imp();
        imp.direction_row
            .set_model(Some(&string_list(direction_choices().into_iter().map(
                |d| match d {
                    Some(d) => labels::direction(d),
                    None => gettext("Default"),
                },
            ))));
        imp.element_kind_row.set_model(Some(&string_list(
            ElementKind::ALL.map(labels::usecase_element_kind),
        )));
        imp.relation_kind_row.set_model(Some(&string_list(
            RelationKind::ALL.map(labels::usecase_relation_kind),
        )));

        // Diagram
        self.on_selected(&imp.direction_row, |canvas, i| {
            let direction = direction_choices()[i];
            canvas.edit_and_relayout::<UseCaseDiagram>(|d| d.direction = direction);
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
        imp.add_boundary_button.connect_clicked(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |_| {
                let Some(canvas) = inspector.canvas() else {
                    return;
                };
                let mut id = String::new();
                canvas.edit::<UseCaseDiagram>(|d| id = d.add_boundary());
                inspector.imp().expanded.replace(Some(id));
                inspector.refresh();
            }
        ));

        // Element
        imp.element_id_row.connect_apply(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| {
                let Some(canvas) = inspector.canvas() else {
                    return;
                };
                let Some(Selection::Box(id)) = canvas.selection() else {
                    return;
                };
                show_rename_result(row, canvas.rename_box(&id, row.text().trim()));
            }
        ));
        imp.element_id_row
            .connect_changed(|row| row.remove_css_class("error"));
        self.on_apply(&imp.element_label_row, |canvas, text| {
            edit_element(canvas, |e| {
                e.label = if text == e.id {
                    Label::default()
                } else {
                    Label::from_text(text)
                };
            });
        });
        self.on_selected(&imp.element_kind_row, |canvas, i| {
            edit_element(canvas, |e| e.kind = ElementKind::ALL[i]);
        });
        self.on_selected(&imp.element_boundary_row, |canvas, i| {
            let Some(Selection::Box(id)) = canvas.selection() else {
                return;
            };
            canvas.edit::<UseCaseDiagram>(|d| {
                let boundary = boundary_choices(d).swap_remove(i);
                if let Some(e) = d.element_mut(&id) {
                    e.boundary = boundary;
                }
            });
        });

        // Relation
        self.on_selected(&imp.relation_kind_row, |canvas, i| {
            edit_relation(canvas, |r, _| r.kind = RelationKind::ALL[i]);
        });
        self.on_apply(&imp.relation_label_row, |canvas, text| {
            edit_relation(canvas, |r, _| r.label = Label::from_text(text));
        });
        for (row, to) in [
            (&imp.relation_from_row, false),
            (&imp.relation_to_row, true),
        ] {
            self.on_selected(row, move |canvas, i| {
                edit_relation(canvas, |r, ids| {
                    if let Some(id) = ids.get(i).cloned() {
                        if to {
                            r.to = id;
                        } else {
                            r.from = id;
                        }
                    }
                });
            });
        }

        for button in [&imp.delete_element_button, &imp.delete_relation_button] {
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

    /// Runs `apply` with the trimmed text when the row's apply button is used.
    fn on_apply(&self, row: &adw::EntryRow, apply: impl Fn(&Canvas, &str) + 'static) {
        row.connect_apply(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| {
                if let Some(canvas) = inspector.canvas() {
                    apply(&canvas, row.text().trim());
                }
            }
        ));
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

    fn refresh(&self) {
        let Some(canvas) = self.canvas() else {
            return;
        };
        let imp = self.imp();
        imp.updating.set(true);
        let selection = canvas.selection();
        canvas.with::<UseCaseDiagram, _>(|d| {
            let page = match &selection {
                Some(Selection::Box(id)) => d.element(id).map(|e| {
                    self.show_element(d, e);
                    "element"
                }),
                Some(Selection::Link(i)) => d.relations.get(*i).map(|r| {
                    self.show_relation(d, r);
                    "relation"
                }),
                None => None,
            };
            let page = page.unwrap_or_else(|| {
                imp.direction_row
                    .set_selected(index_of(&direction_choices(), &d.direction));
                self.show_boundaries(&canvas, d);
                "diagram"
            });
            imp.stack.set_visible_child_name(page);
        });
        imp.updating.set(false);
    }

    fn show_element(&self, d: &UseCaseDiagram, e: &Element) {
        let imp = self.imp();
        imp.element_id_row.set_text(&e.id);
        imp.element_id_row.remove_css_class("error");
        imp.element_label_row.set_text(&e.label.display(""));
        imp.element_kind_row
            .set_selected(index_of(&ElementKind::ALL, &e.kind));
        let choices = boundary_choices(d);
        imp.element_boundary_row
            .set_model(Some(&string_list(choices.iter().map(|c| match c {
                Some(id) => d.boundary(id).map(Boundary::name).unwrap_or_default(),
                None => gettext("None"),
            }))));
        imp.element_boundary_row
            .set_selected(index_of(&choices, &e.boundary));
        imp.element_boundary_row
            .set_visible(!d.boundaries.is_empty());
    }

    fn show_relation(&self, d: &UseCaseDiagram, r: &Relation) {
        let imp = self.imp();
        let names = string_list(d.elements.iter().map(Element::name));
        let position = |id: &str| d.elements.iter().position(|e| e.id == id).unwrap_or(0) as u32;
        imp.relation_kind_row
            .set_selected(index_of(&RelationKind::ALL, &r.kind));
        imp.relation_label_row.set_text(&r.label.display(""));
        imp.relation_label_row.set_visible(r.kind.is_labelled());
        imp.relation_from_row.set_model(Some(&names));
        imp.relation_from_row.set_selected(position(&r.from));
        imp.relation_to_row.set_model(Some(&names));
        imp.relation_to_row.set_selected(position(&r.to));
    }

    /// Rebuilds the boundary rows; each expands into the boundary's settings.
    fn show_boundaries(&self, canvas: &Canvas, d: &UseCaseDiagram) {
        let imp = self.imp();
        for row in imp.boundary_rows.take() {
            imp.boundaries_group.remove(&row);
        }
        let expanded = imp.expanded.borrow().clone();
        let rows = d
            .boundaries
            .iter()
            .map(|b| {
                let row = self.boundary_row(canvas, b);
                row.set_expanded(expanded.as_deref() == Some(b.id.as_str()));
                imp.boundaries_group.add(&row);
                row
            })
            .collect();
        imp.boundary_rows.replace(rows);
    }

    fn boundary_row(&self, canvas: &Canvas, b: &Boundary) -> adw::ExpanderRow {
        let row = adw::ExpanderRow::builder()
            .title(b.name())
            // Titles come from the opened file.
            .use_markup(false)
            .build();
        let id = b.id.clone();
        row.connect_expanded_notify(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            #[strong]
            id,
            move |row| {
                let mut expanded = inspector.imp().expanded.borrow_mut();
                if row.is_expanded() {
                    *expanded = Some(id.clone());
                } else if expanded.as_deref() == Some(id.as_str()) {
                    *expanded = None;
                }
            }
        ));

        let delete = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text(gettext("Delete System Boundary"))
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        delete.connect_clicked(glib::clone!(
            #[weak]
            canvas,
            #[strong]
            id,
            move |_| canvas.edit::<UseCaseDiagram>(|d| d.remove_boundary(&id))
        ));
        row.add_suffix(&delete);

        let title = adw::EntryRow::builder()
            .title(gettext("Title"))
            .text(b.title.display(""))
            .show_apply_button(true)
            .build();
        title.connect_apply(glib::clone!(
            #[weak]
            canvas,
            #[strong]
            id,
            move |row| {
                let text = row.text().trim().to_string();
                canvas.edit::<UseCaseDiagram>(|d| {
                    if let Some(b) = d.boundary_mut(&id) {
                        b.title = if text == id {
                            Label::default()
                        } else {
                            Label::from_text(&text)
                        };
                    }
                });
            }
        ));
        row.add_row(&title);

        let alias = adw::EntryRow::builder()
            .title(gettext("Id"))
            .text(&b.id)
            .show_apply_button(true)
            .build();
        alias.connect_apply(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            #[weak]
            canvas,
            #[strong]
            id,
            move |row| {
                let new = row.text().trim().to_string();
                let mut result = Ok(());
                canvas.edit::<UseCaseDiagram>(|d| result = d.rename_boundary(&id, &new));
                if result.is_ok() && inspector.imp().expanded.borrow().as_deref() == Some(&id) {
                    inspector.imp().expanded.replace(Some(new));
                    inspector.refresh();
                }
                show_rename_result(row, result);
            }
        ));
        alias.connect_changed(|row| row.remove_css_class("error"));
        row.add_row(&alias);
        row
    }
}

fn show_rename_result(row: &adw::EntryRow, result: Result<(), RenameError>) {
    match result {
        Ok(()) => row.set_tooltip_text(None),
        Err(err) => {
            row.add_css_class("error");
            row.set_tooltip_text(Some(&err.to_string()));
        }
    }
}

fn edit_element(canvas: &Canvas, f: impl FnOnce(&mut Element)) {
    let Some(Selection::Box(id)) = canvas.selection() else {
        return;
    };
    canvas.edit::<UseCaseDiagram>(|d| {
        if let Some(e) = d.element_mut(&id) {
            f(e);
        }
    });
}

/// Edits the selected relation, also passing the element ids in order.
fn edit_relation(canvas: &Canvas, f: impl FnOnce(&mut Relation, &[String])) {
    let Some(Selection::Link(index)) = canvas.selection() else {
        return;
    };
    canvas.edit::<UseCaseDiagram>(|d| {
        let ids: Vec<String> = d.elements.iter().map(|e| e.id.clone()).collect();
        if let Some(r) = d.relations.get_mut(index) {
            f(r, &ids);
        }
    });
}
