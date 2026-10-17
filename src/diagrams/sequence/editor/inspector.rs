//! Properties panel for a sequence diagram and its selected element.

use std::cell::{Cell, OnceCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;

use super::canvas::{Canvas, Selection};
use crate::canvas::combo::{index_of, string_list};
use crate::diagrams::sequence::{
    ArrowHead, BlockKind, Item, LineStyle, NotePlacement, ParticipantKind, Sequence,
};
use crate::shared::labels;

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/dev/brunopaz/Reefgraph/ui/sequence-inspector.ui")]
    pub struct Inspector {
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub title_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub autonumber_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub participant_label_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub participant_id_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub participant_kind_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub delete_participant_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub message_text_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub message_from_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub message_to_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub message_line_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub message_head_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub message_activate_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub message_deactivate_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub delete_message_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub note_text_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub note_placement_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub note_participant_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub note_span_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub delete_note_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub block_kind_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub block_label_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub block_branch_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub add_branch_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub delete_block_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub branch_label_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub delete_branch_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub activation_participant_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub activation_start_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub delete_activation_button: TemplateChild<gtk::Button>,
        pub canvas: OnceCell<Canvas>,
        /// Set while widgets are being filled from the model, so their change
        /// handlers do not write the same values back.
        pub updating: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Inspector {
        const NAME: &'static str = "ReefgraphSequenceInspector";
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
        imp.participant_kind_row.set_model(Some(&string_list(
            ParticipantKind::ALL.map(labels::participant_kind),
        )));
        imp.message_line_row
            .set_model(Some(&string_list(LineStyle::ALL.map(labels::line_style))));
        imp.message_head_row
            .set_model(Some(&string_list(ArrowHead::ALL.map(labels::arrow_head))));
        imp.note_placement_row.set_model(Some(&string_list(
            NotePlacement::ALL.map(labels::note_placement),
        )));
        imp.block_kind_row
            .set_model(Some(&string_list(BlockKind::ALL.map(labels::block_kind))));

        // Diagram
        self.on_apply(&imp.title_row, |canvas, text| {
            canvas.edit(|seq| seq.title = (!text.is_empty()).then(|| text.to_string()));
        });
        self.on_switch(&imp.autonumber_row, |canvas, on| {
            canvas.edit(|seq| seq.autonumber = on);
        });

        // Participant
        self.on_apply(&imp.participant_label_row, |canvas, text| {
            if let Some(Selection::Participant(id)) = canvas.selection() {
                canvas.edit(|seq| {
                    if let Some(p) = seq.participant_mut(&id) {
                        p.label = if text.is_empty() {
                            id.clone()
                        } else {
                            text.to_string()
                        };
                    }
                });
            }
        });
        imp.participant_id_row.connect_apply(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| inspector.rename_participant(row)
        ));
        imp.participant_id_row
            .connect_changed(|row| row.remove_css_class("error"));
        self.on_selected(&imp.participant_kind_row, |canvas, i| {
            if let Some(Selection::Participant(id)) = canvas.selection() {
                canvas.edit(|seq| {
                    if let Some(p) = seq.participant_mut(&id) {
                        p.kind = ParticipantKind::ALL[i];
                    }
                });
            }
        });

        // Message
        self.on_apply(&imp.message_text_row, |canvas, text| {
            edit_item(canvas, |item, _| {
                if let Item::Message(m) = item {
                    m.text = text.to_string();
                }
            });
        });
        for (row, to) in [(&imp.message_from_row, false), (&imp.message_to_row, true)] {
            self.on_selected(row, move |canvas, i| {
                edit_item(canvas, |item, ids| {
                    if let (Item::Message(m), Some(id)) = (item, ids.get(i)) {
                        if to {
                            m.to = id.clone();
                        } else {
                            m.from = id.clone();
                        }
                    }
                });
            });
        }
        self.on_selected(&imp.message_line_row, |canvas, i| {
            edit_item(canvas, |item, _| {
                if let Item::Message(m) = item {
                    m.line = LineStyle::ALL[i];
                }
            });
        });
        self.on_selected(&imp.message_head_row, |canvas, i| {
            edit_item(canvas, |item, _| {
                if let Item::Message(m) = item {
                    m.head = ArrowHead::ALL[i];
                }
            });
        });
        // Mermaid allows one activation shorthand per message, so the two
        // switches exclude each other.
        self.on_switch(&imp.message_activate_row, |canvas, on| {
            edit_item(canvas, |item, _| {
                if let Item::Message(m) = item {
                    m.activate = on;
                    m.deactivate &= !on;
                }
            });
        });
        self.on_switch(&imp.message_deactivate_row, |canvas, on| {
            edit_item(canvas, |item, _| {
                if let Item::Message(m) = item {
                    m.deactivate = on;
                    m.activate &= !on;
                }
            });
        });

        // Note
        self.on_apply(&imp.note_text_row, |canvas, text| {
            edit_item(canvas, |item, _| {
                if let Item::Note(n) = item {
                    n.text = text.to_string();
                }
            });
        });
        self.on_selected(&imp.note_placement_row, |canvas, i| {
            edit_item(canvas, |item, _| {
                if let Item::Note(n) = item {
                    n.placement = NotePlacement::ALL[i];
                    if n.placement != NotePlacement::Over {
                        n.participants.truncate(1);
                    }
                }
            });
        });
        self.on_selected(&imp.note_participant_row, |canvas, i| {
            edit_item(canvas, |item, ids| {
                if let (Item::Note(n), Some(id)) = (item, ids.get(i)) {
                    n.participants[0] = id.clone();
                }
            });
        });
        // Index 0 of the span row is "None".
        self.on_selected(&imp.note_span_row, |canvas, i| {
            edit_item(canvas, |item, ids| {
                if let Item::Note(n) = item {
                    n.participants.truncate(1);
                    if let Some(id) = i.checked_sub(1).and_then(|i| ids.get(i)) {
                        n.participants.push(id.clone());
                        n.placement = NotePlacement::Over;
                    }
                }
            });
        });

        // Block
        self.on_selected(&imp.block_kind_row, |canvas, i| {
            if let Some(Selection::Item(index)) = canvas.selection() {
                let changed = canvas.try_edit(|seq| {
                    if let Some(Item::BlockStart { kind, .. }) = seq.items.get_mut(index) {
                        *kind = BlockKind::ALL[i];
                    }
                    seq.validate()
                });
                if changed.is_err() {
                    // Kind does not allow the block's branches; show the old value again.
                    canvas.error_bell();
                    canvas.emit_by_name::<()>("selection-changed", &[]);
                }
            }
        });
        self.on_apply(&imp.block_label_row, |canvas, text| {
            edit_item(canvas, |item, _| {
                if let Item::BlockStart { label, .. } = item {
                    *label = text.to_string();
                }
            });
        });
        imp.add_branch_button.connect_clicked(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |_| {
                let Some(canvas) = inspector.canvas() else {
                    return;
                };
                if let Some(Selection::Item(index)) = canvas.selection() {
                    canvas.edit(|seq| {
                        if let Some(end) = seq.block_markers(index).and_then(|m| m.last().copied())
                        {
                            seq.items.insert(
                                end,
                                Item::BlockBranch {
                                    label: String::new(),
                                },
                            );
                        }
                    });
                }
            }
        ));

        // Branch
        self.on_apply(&imp.branch_label_row, |canvas, text| {
            edit_item(canvas, |item, _| {
                if let Item::BlockBranch { label } = item {
                    *label = text.to_string();
                }
            });
        });

        // Activation
        self.on_selected(&imp.activation_participant_row, |canvas, i| {
            edit_item(canvas, |item, ids| {
                if let (Item::Activate(p) | Item::Deactivate(p), Some(id)) = (item, ids.get(i)) {
                    *p = id.clone();
                }
            });
        });
        self.on_switch(&imp.activation_start_row, |canvas, on| {
            edit_item(canvas, |item, _| {
                if let Item::Activate(p) | Item::Deactivate(p) = item {
                    let id = std::mem::take(p);
                    *item = if on {
                        Item::Activate(id)
                    } else {
                        Item::Deactivate(id)
                    };
                }
            });
        });

        for button in [
            &imp.delete_participant_button,
            &imp.delete_message_button,
            &imp.delete_note_button,
            &imp.delete_block_button,
            &imp.delete_branch_button,
            &imp.delete_activation_button,
        ] {
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

    fn on_switch(&self, row: &adw::SwitchRow, apply: impl Fn(&Canvas, bool) + 'static) {
        row.connect_active_notify(glib::clone!(
            #[weak(rename_to = inspector)]
            self,
            move |row| {
                if inspector.imp().updating.get() {
                    return;
                }
                if let Some(canvas) = inspector.canvas() {
                    apply(&canvas, row.is_active());
                }
            }
        ));
    }

    fn rename_participant(&self, row: &adw::EntryRow) {
        let Some(canvas) = self.canvas() else {
            return;
        };
        let Some(Selection::Participant(id)) = canvas.selection() else {
            return;
        };
        if let Err(err) = canvas.rename_participant(&id, row.text().trim()) {
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
        canvas.with_sequence(|seq| {
            let page = match &selection {
                Some(Selection::Participant(id)) => seq.participant(id).map(|p| {
                    imp.participant_label_row.set_text(&p.label);
                    imp.participant_id_row.set_text(&p.id);
                    imp.participant_id_row.remove_css_class("error");
                    imp.participant_kind_row
                        .set_selected(index_of(&ParticipantKind::ALL, &p.kind));
                    "participant"
                }),
                Some(Selection::Item(i)) => {
                    seq.items.get(*i).map(|item| self.show_item(seq, *i, item))
                }
                None => None,
            };
            let page = page.unwrap_or_else(|| {
                imp.title_row
                    .set_text(seq.title.as_deref().unwrap_or_default());
                imp.autonumber_row.set_active(seq.autonumber);
                "diagram"
            });
            imp.stack.set_visible_child_name(page);
        });
        imp.updating.set(false);
    }

    /// Fills the page for an item and returns its name.
    fn show_item(&self, seq: &Sequence, index: usize, item: &Item) -> &'static str {
        let imp = self.imp();
        let names = string_list(seq.participants.iter().map(|p| p.label.clone()));
        let position = |id: &str| seq.participant_index(id).unwrap_or(0) as u32;
        match item {
            Item::Message(m) => {
                imp.message_text_row.set_text(&m.text);
                imp.message_from_row.set_model(Some(&names));
                imp.message_from_row.set_selected(position(&m.from));
                imp.message_to_row.set_model(Some(&names));
                imp.message_to_row.set_selected(position(&m.to));
                imp.message_line_row
                    .set_selected(index_of(&LineStyle::ALL, &m.line));
                imp.message_head_row
                    .set_selected(index_of(&ArrowHead::ALL, &m.head));
                imp.message_activate_row.set_active(m.activate);
                imp.message_deactivate_row.set_active(m.deactivate);
                "message"
            }
            Item::Note(n) => {
                imp.note_text_row.set_text(&n.text);
                imp.note_placement_row
                    .set_selected(index_of(&NotePlacement::ALL, &n.placement));
                imp.note_participant_row.set_model(Some(&names));
                imp.note_participant_row
                    .set_selected(n.participants.first().map_or(0, |id| position(id)));
                let span = string_list(
                    std::iter::once(gettext("None"))
                        .chain(seq.participants.iter().map(|p| p.label.clone())),
                );
                imp.note_span_row.set_model(Some(&span));
                imp.note_span_row
                    .set_selected(n.participants.get(1).map_or(0, |id| position(id) + 1));
                imp.note_span_row
                    .set_sensitive(n.placement == NotePlacement::Over);
                "note"
            }
            Item::BlockStart { kind, label } => {
                imp.block_kind_row
                    .set_selected(index_of(&BlockKind::ALL, kind));
                imp.block_label_row.set_text(label);
                imp.block_branch_row
                    .set_visible(kind.branch_keyword().is_some());
                "block"
            }
            Item::BlockEnd => {
                // The end marker shows its block.
                match seq
                    .block_markers(index)
                    .and_then(|m| seq.items.get(m[0]).cloned())
                {
                    Some(start) => self.show_item(seq, index, &start),
                    None => "diagram",
                }
            }
            Item::BlockBranch { label } => {
                imp.branch_label_row.set_text(label);
                "branch"
            }
            Item::Activate(id) | Item::Deactivate(id) => {
                imp.activation_participant_row.set_model(Some(&names));
                imp.activation_participant_row.set_selected(position(id));
                imp.activation_start_row
                    .set_active(matches!(item, Item::Activate(_)));
                "activation"
            }
        }
    }
}

/// Edits the selected item, also passing the participant ids in column order.
fn edit_item(canvas: &Canvas, f: impl FnOnce(&mut Item, &[String])) {
    let Some(Selection::Item(index)) = canvas.selection() else {
        return;
    };
    canvas.edit(|seq| {
        let ids: Vec<String> = seq.participants.iter().map(|p| p.id.clone()).collect();
        // The end marker stands for its block.
        let index = match seq.items.get(index) {
            Some(Item::BlockEnd) => seq.block_markers(index).map_or(index, |m| m[0]),
            _ => index,
        };
        if let Some(item) = seq.items.get_mut(index) {
            f(item, &ids);
        }
    });
}
