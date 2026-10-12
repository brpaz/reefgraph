//! Properties panel for a C4 diagram, its boundaries and its selection.

use std::cell::{Cell, OnceCell, RefCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;

use crate::box_canvas::{BoxCanvas as Canvas, Selection};
use crate::c4::{
    Boundary, BoundaryKind, C4Diagram, Element, ElementKind, ElementShape, Level, Relation,
    RelationKind,
};
use crate::combo::{index_of, string_list};
use crate::labels;

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/dev/brunopaz/Merman/ui/c4-inspector.ui")]
    pub struct Inspector {
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub level_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub title_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub layout_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub boundaries_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub add_boundary_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub element_alias_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub element_label_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub element_kind_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub element_shape_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub element_external_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub element_technology_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub element_description_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub element_boundary_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub delete_element_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub relation_label_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub relation_technology_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub relation_from_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub relation_to_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub relation_bidirectional_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub reverse_button: TemplateChild<gtk::Button>,
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
        const NAME: &'static str = "MermanC4Inspector";
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

/// Boundary row choices of an element: none, then every boundary.
fn boundary_choices(d: &C4Diagram) -> Vec<Option<String>> {
    std::iter::once(None)
        .chain(d.boundaries.iter().map(|b| Some(b.id.clone())))
        .collect()
}

fn boundary_names(d: &C4Diagram, choices: &[Option<String>]) -> gtk::StringList {
    string_list(choices.iter().map(|c| match c {
        Some(id) => d.boundary(id).map(|b| b.label.clone()).unwrap_or_default(),
        None => gettext("None"),
    }))
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
        imp.level_row
            .set_model(Some(&string_list(Level::ALL.map(labels::c4_level))));
        imp.element_kind_row.set_model(Some(&string_list(
            ElementKind::ALL.map(labels::c4_element_kind),
        )));
        imp.element_shape_row.set_model(Some(&string_list(
            ElementShape::ALL.map(labels::c4_element_shape),
        )));

        // Diagram
        self.on_selected(&imp.level_row, |canvas, i| {
            canvas.edit::<C4Diagram>(|d| d.level = Level::ALL[i]);
        });
        imp.title_row.connect_apply(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| {
                let text = row.text().trim().to_string();
                if let Some(canvas) = inspector.canvas() {
                    canvas.edit::<C4Diagram>(|d| d.title = (!text.is_empty()).then_some(text));
                }
            }
        ));
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
                canvas.edit::<C4Diagram>(|d| id = d.add_boundary());
                inspector.imp().expanded.replace(Some(id));
                inspector.refresh();
            }
        ));

        // Element
        imp.element_alias_row.connect_apply(glib::clone!(
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
        imp.element_alias_row
            .connect_changed(|row| row.remove_css_class("error"));
        imp.element_label_row.connect_apply(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| {
                let text = row.text().trim().to_string();
                inspector.edit_element(|e| {
                    e.label = if text.is_empty() { e.id.clone() } else { text };
                });
            }
        ));
        self.on_selected(&imp.element_kind_row, |canvas, i| {
            edit_element(canvas, |e| {
                e.kind = ElementKind::ALL[i];
                if !e.kind.has_technology() {
                    e.technology.clear();
                }
                if !e.kind.has_shapes() {
                    e.shape = ElementShape::Box;
                }
            });
        });
        self.on_selected(&imp.element_shape_row, |canvas, i| {
            edit_element(canvas, |e| e.shape = ElementShape::ALL[i]);
        });
        imp.element_external_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| {
                if !inspector.imp().updating.get() {
                    let on = row.is_active();
                    inspector.edit_element(|e| e.external = on);
                }
            }
        ));
        imp.element_technology_row.connect_apply(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| {
                let text = row.text().trim().to_string();
                inspector.edit_element(|e| e.technology = text);
            }
        ));
        imp.element_description_row.connect_apply(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| {
                let text = row.text().trim().to_string();
                inspector.edit_element(|e| e.description = text);
            }
        ));
        self.on_selected(&imp.element_boundary_row, |canvas, i| {
            let Some(Selection::Box(id)) = canvas.selection() else {
                return;
            };
            canvas.edit::<C4Diagram>(|d| {
                let boundary = boundary_choices(d).swap_remove(i);
                if let Some(e) = d.element_mut(&id) {
                    e.boundary = boundary;
                }
            });
        });

        // Relation
        imp.relation_label_row.connect_apply(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| {
                let text = row.text().trim().to_string();
                inspector.edit_relation(|r, _| r.label = text);
            }
        ));
        imp.relation_technology_row.connect_apply(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| {
                let text = row.text().trim().to_string();
                inspector.edit_relation(|r, _| r.technology = text);
            }
        ));
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
        imp.relation_bidirectional_row
            .connect_active_notify(glib::clone!(
                #[weak(rename_to = inspector)]
                self,
                move |row| {
                    if !inspector.imp().updating.get() {
                        let on = row.is_active();
                        inspector.edit_relation(|r, _| {
                            r.kind = if on {
                                RelationKind::BiRel
                            } else {
                                RelationKind::Rel
                            };
                        });
                    }
                }
            ));
        imp.reverse_button.connect_clicked(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |_| inspector.edit_relation(|r, _| std::mem::swap(&mut r.from, &mut r.to))
        ));

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

    fn edit_element(&self, f: impl FnOnce(&mut Element)) {
        if let Some(canvas) = self.canvas() {
            edit_element(&canvas, f);
        }
    }

    fn edit_relation(&self, f: impl FnOnce(&mut Relation, &[String])) {
        if let Some(canvas) = self.canvas() {
            edit_relation(&canvas, f);
        }
    }

    fn refresh(&self) {
        let Some(canvas) = self.canvas() else {
            return;
        };
        let imp = self.imp();
        imp.updating.set(true);
        let selection = canvas.selection();
        canvas.with::<C4Diagram, _>(|d| {
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
                imp.level_row.set_selected(index_of(&Level::ALL, &d.level));
                imp.title_row
                    .set_text(d.title.as_deref().unwrap_or_default());
                self.show_boundaries(&canvas, d);
                "diagram"
            });
            imp.stack.set_visible_child_name(page);
        });
        imp.updating.set(false);
    }

    fn show_element(&self, d: &C4Diagram, e: &Element) {
        let imp = self.imp();
        imp.element_alias_row.set_text(&e.id);
        imp.element_alias_row.remove_css_class("error");
        imp.element_label_row.set_text(&e.label);
        imp.element_kind_row
            .set_selected(index_of(&ElementKind::ALL, &e.kind));
        imp.element_shape_row
            .set_selected(index_of(&ElementShape::ALL, &e.shape));
        imp.element_shape_row.set_visible(e.kind.has_shapes());
        imp.element_external_row.set_active(e.external);
        imp.element_technology_row.set_text(&e.technology);
        imp.element_technology_row
            .set_visible(e.kind.has_technology());
        imp.element_description_row.set_text(&e.description);
        let choices = boundary_choices(d);
        imp.element_boundary_row
            .set_model(Some(&boundary_names(d, &choices)));
        imp.element_boundary_row
            .set_selected(index_of(&choices, &e.boundary));
        imp.element_boundary_row
            .set_visible(!d.boundaries.is_empty());
    }

    fn show_relation(&self, d: &C4Diagram, r: &Relation) {
        let imp = self.imp();
        let names = string_list(d.elements.iter().map(|e| e.label.clone()));
        let position = |id: &str| d.elements.iter().position(|e| e.id == id).unwrap_or(0) as u32;
        imp.relation_label_row.set_text(&r.label);
        imp.relation_technology_row.set_text(&r.technology);
        imp.relation_from_row.set_model(Some(&names));
        imp.relation_from_row.set_selected(position(&r.from));
        imp.relation_to_row.set_model(Some(&names));
        imp.relation_to_row.set_selected(position(&r.to));
        imp.relation_bidirectional_row
            .set_active(r.kind == RelationKind::BiRel);
    }

    /// Rebuilds the boundary rows; each expands into the boundary's settings.
    fn show_boundaries(&self, canvas: &Canvas, d: &C4Diagram) {
        let imp = self.imp();
        for row in imp.boundary_rows.take() {
            imp.boundaries_group.remove(&row);
        }
        let expanded = imp.expanded.borrow().clone();
        let rows = d
            .boundaries
            .iter()
            .map(|b| {
                let row = self.boundary_row(canvas, d, b);
                row.set_expanded(expanded.as_deref() == Some(b.id.as_str()));
                imp.boundaries_group.add(&row);
                row
            })
            .collect();
        imp.boundary_rows.replace(rows);
    }

    fn boundary_row(&self, canvas: &Canvas, d: &C4Diagram, b: &Boundary) -> adw::ExpanderRow {
        let row = adw::ExpanderRow::builder()
            .title(&b.label)
            // Titles come from the opened file.
            .use_markup(false)
            .subtitle(labels::c4_boundary_kind(b.kind))
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
            .tooltip_text(gettext("Delete Boundary"))
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        delete.connect_clicked(glib::clone!(
            #[weak]
            canvas,
            #[strong]
            id,
            move |_| canvas.edit::<C4Diagram>(|d| d.remove_boundary(&id))
        ));
        row.add_suffix(&delete);

        let label = adw::EntryRow::builder()
            .title(gettext("Name"))
            .text(&b.label)
            .show_apply_button(true)
            .build();
        label.connect_apply(glib::clone!(
            #[weak]
            canvas,
            #[strong]
            id,
            move |row| {
                let text = row.text().trim().to_string();
                canvas.edit::<C4Diagram>(|d| {
                    if let Some(b) = d.boundary_mut(&id) {
                        b.label = if text.is_empty() { id.clone() } else { text };
                    }
                });
            }
        ));
        row.add_row(&label);

        let alias = adw::EntryRow::builder()
            .title(gettext("Alias"))
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
                canvas.edit::<C4Diagram>(|d| result = d.rename_boundary(&id, &new));
                if result.is_ok() && inspector.imp().expanded.borrow().as_deref() == Some(&id) {
                    inspector.imp().expanded.replace(Some(new));
                    inspector.refresh();
                }
                show_rename_result(row, result);
            }
        ));
        alias.connect_changed(|row| row.remove_css_class("error"));
        row.add_row(&alias);

        let kind = adw::ComboRow::builder()
            .title(gettext("Kind"))
            .model(&string_list(
                BoundaryKind::ALL.map(labels::c4_boundary_kind),
            ))
            .selected(index_of(&BoundaryKind::ALL, &b.kind))
            .build();
        kind.connect_selected_notify(glib::clone!(
            #[weak]
            canvas,
            #[strong]
            id,
            move |row| {
                let kind = BoundaryKind::ALL[row.selected() as usize];
                canvas.edit::<C4Diagram>(|d| {
                    if let Some(b) = d.boundary_mut(&id) {
                        b.kind = kind;
                    }
                });
            }
        ));
        row.add_row(&kind);

        // Any boundary except this one and those inside it can enclose it.
        let parents: Vec<Option<String>> = boundary_choices(d)
            .into_iter()
            .filter(|c| c.as_deref().is_none_or(|p| !encloses(d, &id, p)))
            .collect();
        let parent = adw::ComboRow::builder()
            .title(gettext("Inside"))
            .model(&boundary_names(d, &parents))
            .selected(index_of(&parents, &b.parent))
            .build();
        parent.connect_selected_notify(glib::clone!(
            #[weak]
            canvas,
            #[strong]
            id,
            move |row| {
                let parent = parents.get(row.selected() as usize).cloned().flatten();
                canvas.edit::<C4Diagram>(|d| {
                    if let Some(b) = d.boundary_mut(&id) {
                        b.parent = parent;
                    }
                });
            }
        ));
        parent.set_visible(d.boundaries.len() > 1);
        row.add_row(&parent);
        row
    }
}

/// Whether boundary `outer` is `inner` or one of the boundaries around it.
fn encloses(d: &C4Diagram, outer: &str, inner: &str) -> bool {
    let mut current = Some(inner);
    for _ in 0..=d.boundaries.len() {
        match current {
            Some(id) if id == outer => return true,
            Some(id) => current = d.boundary(id).and_then(|b| b.parent.as_deref()),
            None => return false,
        }
    }
    false
}

fn show_rename_result(row: &adw::EntryRow, result: Result<(), crate::ids::RenameError>) {
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
    canvas.edit::<C4Diagram>(|d| {
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
    canvas.edit::<C4Diagram>(|d| {
        let ids: Vec<String> = d.elements.iter().map(|e| e.id.clone()).collect();
        if let Some(r) = d.relations.get_mut(index) {
            f(r, &ids);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundaries_do_not_enclose_themselves_through_children() {
        let mut d = C4Diagram::default();
        let outer = d.add_boundary();
        let inner = d.add_boundary();
        d.boundary_mut(&inner).unwrap().parent = Some(outer.clone());
        assert!(encloses(&d, &outer, &inner));
        assert!(encloses(&d, &outer, &outer));
        assert!(!encloses(&d, &inner, &outer));
    }
}
