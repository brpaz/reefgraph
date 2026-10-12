pub use crate::geometry::Point;
pub use crate::ids::RenameError;
pub use crate::layered::Direction;

use crate::ids;

/// Id of the start state, written `[*]` as the source of a transition. Not a
/// valid Mermaid id, so it cannot clash with a declared state.
pub const START_ID: &str = "[*]start";
/// Id of the end state, written `[*]` as the target of a transition.
pub const END_ID: &str = "[*]end";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StateKind {
    #[default]
    State,
    Start,
    End,
    /// `<<choice>>`
    Choice,
    /// `<<fork>>`
    Fork,
    /// `<<join>>`
    Join,
}

impl StateKind {
    /// Kinds a declared state can have; start and end are implied by `[*]`.
    pub const DECLARABLE: [StateKind; 4] = [
        StateKind::State,
        StateKind::Choice,
        StateKind::Fork,
        StateKind::Join,
    ];

    /// Stereotype after `state <id>`, for pseudo-states declared that way.
    pub fn stereotype(self) -> Option<&'static str> {
        match self {
            StateKind::Choice => Some("choice"),
            StateKind::Fork => Some("fork"),
            StateKind::Join => Some("join"),
            StateKind::State | StateKind::Start | StateKind::End => None,
        }
    }

    pub fn from_stereotype(text: &str) -> Option<Self> {
        Self::DECLARABLE
            .into_iter()
            .find(|k| k.stereotype() == Some(text))
    }

    /// Whether states of this kind are `[*]` rather than named states.
    pub fn is_terminal(self) -> bool {
        match self {
            StateKind::Start | StateKind::End => true,
            StateKind::State | StateKind::Choice | StateKind::Fork | StateKind::Join => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub id: String,
    /// Description shown instead of the id; lines are separate `id : text`
    /// statements.
    pub label: String,
    pub kind: StateKind,
    /// Center in canvas coordinates; `None` until laid out.
    pub pos: Option<Point>,
}

impl State {
    pub fn new(id: impl Into<String>, kind: StateKind) -> Self {
        let id = id.into();
        Self {
            label: id.clone(),
            id,
            kind,
            pos: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Transition {
    pub from: String,
    pub to: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StateDiagram {
    /// Declared as `stateDiagram-v2` rather than `stateDiagram`.
    pub v2: bool,
    pub direction: Option<Direction>,
    pub states: Vec<State>,
    pub transitions: Vec<Transition>,
    /// Lines before the declaration.
    pub preamble: Vec<String>,
    /// Statements kept verbatim (comments, notes, `classDef`, `style`…).
    pub passthrough: Vec<String>,
}

const RESERVED_IDS: &[&str] = &[
    "state",
    "direction",
    "note",
    "end",
    "classDef",
    "class",
    "style",
    "stateDiagram",
];

impl StateDiagram {
    pub fn is_valid_id(id: &str) -> bool {
        ids::is_valid_id(id, RESERVED_IDS)
    }

    pub fn state(&self, id: &str) -> Option<&State> {
        self.states.iter().find(|s| s.id == id)
    }

    pub fn state_mut(&mut self, id: &str) -> Option<&mut State> {
        self.states.iter_mut().find(|s| s.id == id)
    }

    /// Returns the state with `id`, creating it when missing. `[*]` ids get
    /// their start or end kind.
    pub fn ensure_state(&mut self, id: &str) -> &mut State {
        let idx = match self.states.iter().position(|s| s.id == id) {
            Some(idx) => idx,
            None => {
                let kind = match id {
                    START_ID => StateKind::Start,
                    END_ID => StateKind::End,
                    _ => StateKind::State,
                };
                self.insert_state(State::new(id, kind))
            }
        };
        &mut self.states[idx]
    }

    /// Adds a state, keeping the start state first and the end state last
    /// so the order does not depend on where `[*]` appears in the source.
    fn insert_state(&mut self, state: State) -> usize {
        let idx = match state.kind {
            StateKind::Start => 0,
            StateKind::End => self.states.len(),
            StateKind::State | StateKind::Choice | StateKind::Fork | StateKind::Join => self
                .states
                .iter()
                .position(|s| s.kind == StateKind::End)
                .unwrap_or(self.states.len()),
        };
        self.states.insert(idx, state);
        idx
    }

    /// Adds a state of `kind` at `pos` and returns its id. Named states are
    /// numbered (`State`, `State2`…); there is only one start and one end, so
    /// adding another returns `None`.
    pub fn add_state(&mut self, kind: StateKind, pos: Point) -> Option<String> {
        let id = match kind {
            StateKind::Start => START_ID.to_string(),
            StateKind::End => END_ID.to_string(),
            StateKind::State => self.free_id("State"),
            StateKind::Choice => self.free_id("choice"),
            StateKind::Fork => self.free_id("fork"),
            StateKind::Join => self.free_id("join"),
        };
        if self.state(&id).is_some() {
            return None;
        }
        self.insert_state(State {
            pos: Some(pos),
            ..State::new(id.clone(), kind)
        });
        Some(id)
    }

    fn free_id(&self, stem: &str) -> String {
        std::iter::once(stem.to_string())
            .chain((2..).map(|i| format!("{stem}{i}")))
            .find(|id| self.state(id).is_none())
            .expect("name sequence is infinite")
    }

    /// Adds a copy of `state` at `pos` under a numbered variant of its id,
    /// returning the new id. Start and end states are not copied.
    pub fn paste_state(&mut self, state: &State, pos: Point) -> Option<String> {
        if state.kind.is_terminal() {
            return None;
        }
        let id = ids::copy_id(&state.id, |id| self.state(id).is_some());
        self.insert_state(State {
            label: if state.label == state.id {
                id.clone()
            } else {
                state.label.clone()
            },
            id: id.clone(),
            pos: Some(pos),
            ..state.clone()
        });
        Some(id)
    }

    pub fn remove_state(&mut self, id: &str) {
        self.states.retain(|s| s.id != id);
        self.transitions.retain(|t| t.from != id && t.to != id);
    }

    pub fn rename_state(&mut self, old: &str, new: &str) -> Result<(), RenameError> {
        if old == new {
            return Ok(());
        }
        let terminal = self.state(old).is_some_and(|s| s.kind.is_terminal());
        if terminal || !Self::is_valid_id(new) {
            return Err(RenameError::Invalid);
        }
        if self.state(new).is_some() {
            return Err(RenameError::Taken);
        }
        if let Some(s) = self.state_mut(old) {
            if s.label == old {
                s.label = new.to_string();
            }
            s.id = new.to_string();
        }
        for t in &mut self.transitions {
            if t.from == old {
                t.from = new.to_string();
            }
            if t.to == old {
                t.to = new.to_string();
            }
        }
        Ok(())
    }

    /// Whether a transition may go from `from` to `to`: nothing leaves the
    /// end state or enters the start state, which `[*]` could not express.
    pub fn can_connect(&self, from: &str, to: &str) -> bool {
        from != END_ID && to != START_ID
    }

    /// Adds a transition and returns its index, or `None` when the ends
    /// cannot be connected.
    pub fn add_transition(&mut self, from: &str, to: &str) -> Option<usize> {
        if !self.can_connect(from, to) {
            return None;
        }
        self.transitions.push(Transition {
            from: from.into(),
            to: to.into(),
            label: String::new(),
        });
        Some(self.transitions.len() - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_numbered_states_and_one_start_and_end() {
        let mut d = StateDiagram::default();
        let p = Point::default();
        assert_eq!(d.add_state(StateKind::State, p).as_deref(), Some("State"));
        assert_eq!(d.add_state(StateKind::State, p).as_deref(), Some("State2"));
        assert_eq!(d.add_state(StateKind::Choice, p).as_deref(), Some("choice"));
        assert_eq!(d.add_state(StateKind::Start, p).as_deref(), Some(START_ID));
        assert_eq!(d.add_state(StateKind::Start, p), None);
    }

    #[test]
    fn rename_and_remove_states() {
        let mut d = StateDiagram::default();
        d.ensure_state("A");
        d.ensure_state(START_ID);
        d.add_transition(START_ID, "A");
        d.rename_state("A", "Idle").unwrap();
        assert_eq!(d.transitions[0].to, "Idle");
        assert_eq!(d.state("Idle").unwrap().label, "Idle");
        assert_eq!(d.rename_state(START_ID, "B"), Err(RenameError::Invalid));
        assert_eq!(d.rename_state("Idle", "end"), Err(RenameError::Invalid));
        d.remove_state("Idle");
        assert!(d.transitions.is_empty());
    }

    #[test]
    fn keeps_start_first_and_end_last() {
        let mut d = StateDiagram::default();
        d.ensure_state(END_ID);
        d.ensure_state("A");
        d.ensure_state(START_ID);
        d.add_state(StateKind::State, Point::default());
        let ids: Vec<_> = d.states.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, [START_ID, "A", "State", END_ID]);
    }

    #[test]
    fn rejects_transitions_into_start_or_out_of_end() {
        let mut d = StateDiagram::default();
        assert_eq!(d.add_transition("A", START_ID), None);
        assert_eq!(d.add_transition(END_ID, "A"), None);
        assert_eq!(d.add_transition(START_ID, END_ID), Some(0));
    }

    #[test]
    fn pastes_named_states_only() {
        let mut d = StateDiagram::default();
        let fork = State::new("fork", StateKind::Fork);
        assert_eq!(
            d.paste_state(&fork, Point::default()).as_deref(),
            Some("fork2")
        );
        assert_eq!(d.state("fork2").unwrap().kind, StateKind::Fork);
        let start = State::new(START_ID, StateKind::Start);
        assert_eq!(d.paste_state(&start, Point::default()), None);
    }
}
