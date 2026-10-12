//! Properties panel for a state diagram and its selection.

use std::cell::{Cell, OnceCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;

use crate::box_canvas::{BoxCanvas as Canvas, Selection};
use crate::combo::{index_of, string_list};
use crate::labels;
use crate::state::model::Direction;
use crate::state::{State, StateDiagram, StateKind, Transition};

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/dev/brunopaz/Merman/ui/state-inspector.ui")]
    pub struct Inspector {
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub direction_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub layout_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub state_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub state_name_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub state_label_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub state_kind_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub delete_state_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub transition_label_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub transition_from_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub transition_to_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub delete_transition_button: TemplateChild<gtk::Button>,
        pub canvas: OnceCell<Canvas>,
        /// Set while widgets are being filled from the model, so their change
        /// handlers do not write the same values back.
        pub updating: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Inspector {
        const NAME: &'static str = "MermanStateInspector";
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

/// States a transition may start from (`to` false) or lead to (`to` true).
fn endpoint_choices(diagram: &StateDiagram, to: bool) -> Vec<&State> {
    let excluded = if to { StateKind::Start } else { StateKind::End };
    diagram
        .states
        .iter()
        .filter(|s| s.kind != excluded)
        .collect()
}

fn state_name(state: &State) -> String {
    if state.kind.is_terminal() {
        labels::state_kind(state.kind)
    } else {
        state.id.clone()
    }
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
        imp.state_kind_row.set_model(Some(&string_list(
            StateKind::DECLARABLE.map(labels::state_kind),
        )));

        self.on_selected(&imp.direction_row, |canvas, i| {
            let direction = direction_choices()[i];
            canvas.edit_and_relayout::<StateDiagram>(|d| d.direction = direction);
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

        // State
        imp.state_name_row.connect_apply(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| inspector.rename_state(row)
        ));
        imp.state_name_row
            .connect_changed(|row| row.remove_css_class("error"));
        self.on_apply(&imp.state_label_row, |canvas, text| {
            edit_state(canvas, |state| {
                state.label = if text.is_empty() {
                    state.id.clone()
                } else {
                    text.to_string()
                };
            });
        });
        self.on_selected(&imp.state_kind_row, |canvas, i| {
            edit_state(canvas, |state| state.kind = StateKind::DECLARABLE[i]);
        });

        // Transition
        self.on_apply(&imp.transition_label_row, |canvas, text| {
            edit_transition(canvas, |t, _| t.label = text.to_string());
        });
        for (row, to) in [
            (&imp.transition_from_row, false),
            (&imp.transition_to_row, true),
        ] {
            self.on_selected(row, move |canvas, i| {
                edit_transition(canvas, |t, diagram| {
                    let Some(state) = endpoint_choices(diagram, to).get(i).copied() else {
                        return;
                    };
                    if to {
                        t.to = state.id.clone();
                    } else {
                        t.from = state.id.clone();
                    }
                });
            });
        }

        for button in [&imp.delete_state_button, &imp.delete_transition_button] {
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

    fn rename_state(&self, row: &adw::EntryRow) {
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
        canvas.with::<StateDiagram, _>(|diagram| {
            let page = match &selection {
                Some(Selection::Box(id)) => diagram.state(id).map(|state| {
                    self.show_state(state);
                    "state"
                }),
                Some(Selection::Link(i)) => diagram.transitions.get(*i).map(|t| {
                    imp.transition_label_row.set_text(&t.label);
                    for (row, to, id) in [
                        (&imp.transition_from_row, false, &t.from),
                        (&imp.transition_to_row, true, &t.to),
                    ] {
                        let choices = endpoint_choices(diagram, to);
                        row.set_model(Some(&string_list(choices.iter().map(|s| state_name(s)))));
                        let position = choices.iter().position(|s| s.id == *id).unwrap_or(0);
                        row.set_selected(position as u32);
                    }
                    "transition"
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

    fn show_state(&self, state: &State) {
        let imp = self.imp();
        imp.state_group.set_title(&labels::state_kind(state.kind));
        let named = !state.kind.is_terminal();
        imp.state_name_row.set_visible(named);
        imp.state_kind_row.set_visible(named);
        imp.state_label_row
            .set_visible(state.kind == StateKind::State);
        imp.state_name_row.set_text(&state.id);
        imp.state_name_row.remove_css_class("error");
        imp.state_label_row.set_text(if state.label == state.id {
            ""
        } else {
            &state.label
        });
        if named {
            imp.state_kind_row
                .set_selected(index_of(&StateKind::DECLARABLE, &state.kind));
        }
    }
}

fn edit_state(canvas: &Canvas, f: impl FnOnce(&mut State)) {
    if let Some(Selection::Box(id)) = canvas.selection() {
        canvas.edit::<StateDiagram>(|diagram| {
            if let Some(state) = diagram.state_mut(&id) {
                f(state);
            }
        });
    }
}

/// Edits the selected transition; `f` also sees the diagram as it was before
/// the edit.
fn edit_transition(canvas: &Canvas, f: impl FnOnce(&mut Transition, &StateDiagram)) {
    let Some(Selection::Link(index)) = canvas.selection() else {
        return;
    };
    canvas.edit::<StateDiagram>(|diagram| {
        let before = diagram.clone();
        if let Some(t) = diagram.transitions.get_mut(index) {
            f(t, &before);
        }
    });
}
