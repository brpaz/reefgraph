//! Properties panel for a class diagram and its selection.

use std::cell::{Cell, OnceCell, RefCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;

use super::canvas::{Canvas, Selection};
use crate::class_diagram::model::Direction;
use crate::class_diagram::{Class, Head, Member, Relation};
use crate::combo::{index_of, string_list};
use crate::labels;

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/dev/brunopaz/Merman/ui/class-inspector.ui")]
    pub struct Inspector {
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub direction_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub layout_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub class_name_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub class_label_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub class_generic_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub class_annotations_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub members_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub add_member_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub delete_class_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub relation_label_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub relation_from_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub relation_from_head_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub relation_from_cardinality_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub relation_to_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub relation_to_head_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub relation_to_cardinality_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub relation_dashed_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub reverse_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub delete_relation_button: TemplateChild<gtk::Button>,
        pub canvas: OnceCell<Canvas>,
        /// Rows currently shown in the members group, one per member.
        pub member_rows: RefCell<Vec<adw::EntryRow>>,
        /// Set while widgets are being filled from the model, so their change
        /// handlers do not write the same values back.
        pub updating: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Inspector {
        const NAME: &'static str = "MermanClassInspector";
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
        for row in [&imp.relation_from_head_row, &imp.relation_to_head_row] {
            row.set_model(Some(&string_list(Head::ALL.map(labels::class_head))));
        }

        self.on_selected(&imp.direction_row, |canvas, i| {
            canvas.set_direction(direction_choices()[i]);
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

        // Class
        imp.class_name_row.connect_apply(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| inspector.rename_class(row)
        ));
        imp.class_name_row
            .connect_changed(|row| row.remove_css_class("error"));
        self.on_apply(&imp.class_label_row, |canvas, text| {
            edit_class(canvas, |class| {
                class.label = if text.is_empty() {
                    class.id.clone()
                } else {
                    text.to_string()
                };
            });
        });
        self.on_apply(&imp.class_generic_row, |canvas, text| {
            edit_class(canvas, |class| class.generic = text.to_string());
        });
        self.on_apply(&imp.class_annotations_row, |canvas, text| {
            edit_class(canvas, |class| {
                class.annotations = text
                    .split(',')
                    .map(|a| {
                        a.trim()
                            .trim_start_matches("<<")
                            .trim_end_matches(">>")
                            .trim()
                    })
                    .filter(|a| !a.is_empty())
                    .map(String::from)
                    .collect();
            });
        });
        imp.add_member_button.connect_clicked(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |_| {
                let Some(canvas) = inspector.canvas() else {
                    return;
                };
                if let Some(Selection::Class(id)) = canvas.selection() {
                    canvas.add_member(&id);
                }
            }
        ));

        // Relation
        self.on_apply(&imp.relation_label_row, |canvas, text| {
            edit_relation(canvas, |r, _| r.label = text.to_string());
        });
        self.on_apply(&imp.relation_from_cardinality_row, |canvas, text| {
            edit_relation(canvas, |r, _| r.from_cardinality = text.to_string());
        });
        self.on_apply(&imp.relation_to_cardinality_row, |canvas, text| {
            edit_relation(canvas, |r, _| r.to_cardinality = text.to_string());
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
        for (row, to) in [
            (&imp.relation_from_head_row, false),
            (&imp.relation_to_head_row, true),
        ] {
            self.on_selected(row, move |canvas, i| {
                edit_relation(canvas, |r, _| {
                    if to {
                        r.to_head = Head::ALL[i];
                    } else {
                        r.from_head = Head::ALL[i];
                    }
                });
            });
        }
        imp.relation_dashed_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| {
                if inspector.imp().updating.get() {
                    return;
                }
                if let Some(canvas) = inspector.canvas() {
                    let dashed = row.is_active();
                    edit_relation(&canvas, |r, _| r.dashed = dashed);
                }
            }
        ));
        imp.reverse_button.connect_clicked(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |_| {
                if let Some(canvas) = inspector.canvas() {
                    edit_relation(&canvas, |r, _| {
                        std::mem::swap(&mut r.from, &mut r.to);
                        std::mem::swap(&mut r.from_head, &mut r.to_head);
                        std::mem::swap(&mut r.from_cardinality, &mut r.to_cardinality);
                    });
                }
            }
        ));

        for button in [&imp.delete_class_button, &imp.delete_relation_button] {
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

    fn rename_class(&self, row: &adw::EntryRow) {
        let Some(canvas) = self.canvas() else {
            return;
        };
        let Some(Selection::Class(id)) = canvas.selection() else {
            return;
        };
        if let Err(err) = canvas.rename_class(&id, row.text().trim()) {
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
        canvas.with_diagram(|diagram| {
            let page = match &selection {
                Some(Selection::Class(id)) => diagram.class(id).map(|class| {
                    imp.class_name_row.set_text(&class.id);
                    imp.class_name_row.remove_css_class("error");
                    imp.class_label_row.set_text(if class.label == class.id {
                        ""
                    } else {
                        &class.label
                    });
                    imp.class_generic_row.set_text(&class.generic);
                    imp.class_annotations_row
                        .set_text(&class.annotations.join(", "));
                    self.show_members(&canvas, class);
                    "class"
                }),
                Some(Selection::Relation(i)) => diagram.relations.get(*i).map(|r| {
                    let names = string_list(diagram.classes.iter().map(|c| c.label.clone()));
                    let position = |id: &str| {
                        diagram.classes.iter().position(|c| c.id == id).unwrap_or(0) as u32
                    };
                    imp.relation_label_row.set_text(&r.label);
                    imp.relation_from_row.set_model(Some(&names));
                    imp.relation_from_row.set_selected(position(&r.from));
                    imp.relation_to_row.set_model(Some(&names));
                    imp.relation_to_row.set_selected(position(&r.to));
                    imp.relation_from_head_row
                        .set_selected(index_of(&Head::ALL, &r.from_head));
                    imp.relation_to_head_row
                        .set_selected(index_of(&Head::ALL, &r.to_head));
                    imp.relation_from_cardinality_row
                        .set_text(&r.from_cardinality);
                    imp.relation_to_cardinality_row.set_text(&r.to_cardinality);
                    imp.relation_dashed_row.set_active(r.dashed);
                    "relation"
                }),
                None => None,
            };
            let page = page.unwrap_or_else(|| {
                imp.direction_row
                    .set_selected(index_of(&direction_choices(), &diagram.direction));
                "diagram"
            });
            imp.stack.set_visible_child_name(page);
        });
        imp.updating.set(false);
    }

    /// Rebuilds the member rows; each row edits one member's text.
    fn show_members(&self, canvas: &Canvas, class: &Class) {
        let imp = self.imp();
        for row in imp.member_rows.take() {
            imp.members_group.remove(&row);
        }
        let mut rows = Vec::with_capacity(class.members.len());
        for index in class.display_order() {
            let member = &class.members[index];
            let title = if member.is_method() {
                gettext("Method")
            } else {
                gettext("Attribute")
            };
            let row = adw::EntryRow::builder()
                .title(title)
                .text(&member.0)
                .show_apply_button(true)
                .build();
            let delete = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .tooltip_text(gettext("Delete Member"))
                .valign(gtk::Align::Center)
                .css_classes(["flat"])
                .build();
            row.add_suffix(&delete);
            let id = class.id.clone();
            row.connect_apply(glib::clone!(
                #[weak]
                canvas,
                #[strong]
                id,
                move |row| {
                    let text = row.text().trim().to_string();
                    canvas.edit(|diagram| {
                        if let Some(class) = diagram.class_mut(&id)
                            && index < class.members.len()
                            && !text.is_empty()
                        {
                            class.members[index] = Member(text);
                        }
                    });
                }
            ));
            delete.connect_clicked(glib::clone!(
                #[weak]
                canvas,
                move |_| {
                    canvas.edit(|diagram| {
                        if let Some(class) = diagram.class_mut(&id)
                            && index < class.members.len()
                        {
                            class.members.remove(index);
                        }
                    });
                }
            ));
            imp.members_group.add(&row);
            rows.push(row);
        }
        imp.member_rows.replace(rows);
    }
}

fn edit_class(canvas: &Canvas, f: impl FnOnce(&mut Class)) {
    if let Some(Selection::Class(id)) = canvas.selection() {
        canvas.edit(|diagram| {
            if let Some(class) = diagram.class_mut(&id) {
                f(class);
            }
        });
    }
}

/// Edits the selected relation, also passing the class ids in order.
fn edit_relation(canvas: &Canvas, f: impl FnOnce(&mut Relation, &[String])) {
    let Some(Selection::Relation(index)) = canvas.selection() else {
        return;
    };
    canvas.edit(|diagram| {
        let ids: Vec<String> = diagram.classes.iter().map(|c| c.id.clone()).collect();
        if let Some(r) = diagram.relations.get_mut(index) {
            f(r, &ids);
        }
    });
}
