//! Properties panel for an entity relationship diagram and its selection.

use std::cell::{Cell, OnceCell, RefCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;

use crate::box_canvas::{BoxCanvas as Canvas, Selection};
use crate::combo::{index_of, string_list};
use crate::erd::model::Direction;
use crate::erd::{Attribute, Cardinality, Erd, Relationship};
use crate::labels;

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/dev/brunopaz/Merman/ui/erd-inspector.ui")]
    pub struct Inspector {
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub direction_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub layout_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub entity_name_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub entity_label_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub attributes_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub add_attribute_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub delete_entity_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub relationship_label_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub relationship_from_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub relationship_from_cardinality_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub relationship_to_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub relationship_to_cardinality_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub relationship_identifying_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub reverse_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub delete_relationship_button: TemplateChild<gtk::Button>,
        pub canvas: OnceCell<Canvas>,
        /// Rows currently shown in the attributes group, one per attribute.
        pub attribute_rows: RefCell<Vec<adw::EntryRow>>,
        /// Set while widgets are being filled from the model, so their change
        /// handlers do not write the same values back.
        pub updating: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Inspector {
        const NAME: &'static str = "MermanErdInspector";
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
            .set_model(Some(&string_list(direction_choices().into_iter().map(
                |d| match d {
                    Some(d) => labels::direction(d),
                    None => gettext("Default"),
                },
            ))));
        for row in [
            &imp.relationship_from_cardinality_row,
            &imp.relationship_to_cardinality_row,
        ] {
            row.set_model(Some(&string_list(
                Cardinality::ALL.map(labels::cardinality),
            )));
        }

        self.on_selected(&imp.direction_row, |canvas, i| {
            let direction = direction_choices()[i];
            canvas.edit_and_relayout::<Erd>(|erd| erd.direction = direction);
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

        // Entity
        imp.entity_name_row.connect_apply(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| inspector.rename_entity(row)
        ));
        imp.entity_name_row
            .connect_changed(|row| row.remove_css_class("error"));
        imp.entity_label_row.connect_apply(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| {
                let Some(canvas) = inspector.canvas() else {
                    return;
                };
                if let Some(Selection::Box(id)) = canvas.selection() {
                    let text = row.text().trim().to_string();
                    canvas.edit::<Erd>(|erd| {
                        if let Some(e) = erd.entity_mut(&id) {
                            e.label = if text.is_empty() { id.clone() } else { text };
                        }
                    });
                }
            }
        ));
        imp.add_attribute_button.connect_clicked(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |_| {
                let Some(canvas) = inspector.canvas() else {
                    return;
                };
                if let Some(Selection::Box(id)) = canvas.selection() {
                    canvas.add_row(&id);
                }
            }
        ));

        // Relationship
        imp.relationship_label_row.connect_apply(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| {
                let text = row.text().trim().to_string();
                inspector.edit_relationship(|r, _| r.label = text);
            }
        ));
        for (row, to) in [
            (&imp.relationship_from_row, false),
            (&imp.relationship_to_row, true),
        ] {
            self.on_selected(row, move |canvas, i| {
                edit_relationship(canvas, |r, ids| {
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
        for (row, to) in [
            (&imp.relationship_from_cardinality_row, false),
            (&imp.relationship_to_cardinality_row, true),
        ] {
            self.on_selected(row, move |canvas, i| {
                edit_relationship(canvas, |r, _| {
                    let cardinality = Cardinality::ALL[i];
                    if to {
                        r.to_cardinality = cardinality;
                    } else {
                        r.from_cardinality = cardinality;
                    }
                });
            });
        }
        imp.relationship_identifying_row
            .connect_active_notify(glib::clone!(
                #[weak(rename_to = inspector)]
                self,
                move |row| {
                    if !inspector.imp().updating.get() {
                        let on = row.is_active();
                        inspector.edit_relationship(|r, _| r.identifying = on);
                    }
                }
            ));
        imp.reverse_button.connect_clicked(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |_| {
                inspector.edit_relationship(|r, _| {
                    std::mem::swap(&mut r.from, &mut r.to);
                    std::mem::swap(&mut r.from_cardinality, &mut r.to_cardinality);
                });
            }
        ));

        for button in [&imp.delete_entity_button, &imp.delete_relationship_button] {
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

    fn edit_relationship(&self, f: impl FnOnce(&mut Relationship, &[String])) {
        if let Some(canvas) = self.canvas() {
            edit_relationship(&canvas, f);
        }
    }

    fn rename_entity(&self, row: &adw::EntryRow) {
        let Some(canvas) = self.canvas() else {
            return;
        };
        let Some(Selection::Box(id)) = canvas.selection() else {
            return;
        };
        if let Err(err) = canvas.rename_box(&id, row.text().trim()) {
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
        canvas.with::<Erd, _>(|erd| {
            let page = match &selection {
                Some(Selection::Box(id)) => erd.entity(id).map(|entity| {
                    imp.entity_name_row.set_text(&entity.id);
                    imp.entity_name_row.remove_css_class("error");
                    imp.entity_label_row.set_text(if entity.label == entity.id {
                        ""
                    } else {
                        &entity.label
                    });
                    self.show_attributes(&canvas, &entity.id, &entity.attributes);
                    "entity"
                }),
                Some(Selection::Link(i)) => erd.relationships.get(*i).map(|r| {
                    let names = string_list(erd.entities.iter().map(|e| e.label.clone()));
                    let position =
                        |id: &str| erd.entities.iter().position(|e| e.id == id).unwrap_or(0) as u32;
                    imp.relationship_label_row.set_text(&r.label);
                    imp.relationship_from_row.set_model(Some(&names));
                    imp.relationship_from_row.set_selected(position(&r.from));
                    imp.relationship_to_row.set_model(Some(&names));
                    imp.relationship_to_row.set_selected(position(&r.to));
                    imp.relationship_from_cardinality_row
                        .set_selected(index_of(&Cardinality::ALL, &r.from_cardinality));
                    imp.relationship_to_cardinality_row
                        .set_selected(index_of(&Cardinality::ALL, &r.to_cardinality));
                    imp.relationship_identifying_row.set_active(r.identifying);
                    "relationship"
                }),
                None => None,
            };
            let page = page.unwrap_or_else(|| {
                imp.direction_row
                    .set_selected(index_of(&direction_choices(), &erd.direction));
                "diagram"
            });
            imp.stack.set_visible_child_name(page);
        });
        imp.updating.set(false);
    }

    /// Rebuilds the attribute rows; each row edits one attribute line.
    fn show_attributes(&self, canvas: &Canvas, id: &str, attributes: &[Attribute]) {
        let imp = self.imp();
        for row in imp.attribute_rows.take() {
            imp.attributes_group.remove(&row);
        }
        let mut rows = Vec::with_capacity(attributes.len());
        for (index, attribute) in attributes.iter().enumerate() {
            let row = adw::EntryRow::builder()
                .title(&attribute.name)
                // Titles come from the opened file.
                .use_markup(false)
                .text(attribute.to_line())
                .show_apply_button(true)
                .build();
            let delete = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .tooltip_text(gettext("Delete Attribute"))
                .valign(gtk::Align::Center)
                .css_classes(["flat"])
                .build();
            row.add_suffix(&delete);
            let id = id.to_string();
            row.connect_apply(glib::clone!(
                #[weak]
                canvas,
                #[strong]
                id,
                move |row| match Attribute::parse(row.text().trim()) {
                    Ok(attribute) => canvas.edit::<Erd>(|erd| {
                        if let Some(slot) = erd
                            .entity_mut(&id)
                            .and_then(|e| e.attributes.get_mut(index))
                        {
                            *slot = attribute;
                        }
                    }),
                    Err(err) => {
                        row.add_css_class("error");
                        row.set_tooltip_text(Some(&err));
                    }
                }
            ));
            row.connect_changed(|row| row.remove_css_class("error"));
            delete.connect_clicked(glib::clone!(
                #[weak]
                canvas,
                move |_| {
                    canvas.edit::<Erd>(|erd| {
                        if let Some(e) = erd.entity_mut(&id)
                            && index < e.attributes.len()
                        {
                            e.attributes.remove(index);
                        }
                    });
                }
            ));
            imp.attributes_group.add(&row);
            rows.push(row);
        }
        imp.attribute_rows.replace(rows);
    }
}

/// Edits the selected relationship, also passing the entity ids in order.
fn edit_relationship(canvas: &Canvas, f: impl FnOnce(&mut Relationship, &[String])) {
    let Some(Selection::Link(index)) = canvas.selection() else {
        return;
    };
    canvas.edit::<Erd>(|erd| {
        let ids: Vec<String> = erd.entities.iter().map(|e| e.id.clone()).collect();
        if let Some(r) = erd.relationships.get_mut(index) {
            f(r, &ids);
        }
    });
}
