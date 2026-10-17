//! State geometry and auto layout for state diagrams.

use std::collections::HashMap;

use super::model::{Direction, State, StateDiagram, StateKind};
use crate::shared::geometry::Rect;
use crate::shared::layered::{self, Spacing};

pub const PADDING: (f64, f64) = (16.0, 10.0);
const MIN_SIZE: (f64, f64) = (80.0, 36.0);
pub const TERMINAL_SIZE: f64 = 20.0;
pub const CHOICE_SIZE: f64 = 30.0;
/// Length and thickness of fork and join bars.
pub const BAR_SIZE: (f64, f64) = (80.0, 8.0);
const SPACING: Spacing = Spacing {
    rank: 70.0,
    node: 50.0,
};

/// Outer size of a state. Fork and join bars lie across the flow.
pub fn state_size(
    state: &State,
    direction: Direction,
    text_size: impl Fn(&str) -> (f64, f64),
) -> (f64, f64) {
    match state.kind {
        StateKind::State => {
            let (w, h) = text_size(&state.label);
            (
                (w + 2.0 * PADDING.0).max(MIN_SIZE.0),
                (h + 2.0 * PADDING.1).max(MIN_SIZE.1),
            )
        }
        StateKind::Start | StateKind::End => (TERMINAL_SIZE, TERMINAL_SIZE),
        StateKind::Choice => (CHOICE_SIZE, CHOICE_SIZE),
        StateKind::Fork | StateKind::Join => match direction {
            Direction::TopBottom | Direction::BottomTop => BAR_SIZE,
            Direction::LeftRight | Direction::RightLeft => (BAR_SIZE.1, BAR_SIZE.0),
        },
    }
}

pub fn state_rect(
    state: &State,
    direction: Direction,
    text_size: impl Fn(&str) -> (f64, f64),
) -> Option<Rect> {
    Some(Rect::centered(
        state.pos?,
        state_size(state, direction, text_size),
    ))
}

/// Assigns a position to every state without one.
pub fn layout_missing(diagram: &mut StateDiagram, text_size: impl Fn(&str) -> (f64, f64)) {
    let direction = diagram.direction.unwrap_or_default();
    let index: HashMap<&str, usize> = diagram
        .states
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.as_str(), i))
        .collect();
    let edges: Vec<(usize, usize)> = diagram
        .transitions
        .iter()
        .filter_map(|t| Some((*index.get(t.from.as_str())?, *index.get(t.to.as_str())?)))
        .collect();
    let sizes: Vec<_> = diagram
        .states
        .iter()
        .map(|s| state_size(s, direction, &text_size))
        .collect();
    let mut positions: Vec<_> = diagram.states.iter().map(|s| s.pos).collect();
    layered::fill_missing(&mut positions, &sizes, &edges, direction, SPACING);
    for (state, pos) in diagram.states.iter_mut().zip(positions) {
        state.pos = pos;
    }
}

/// Discards every stored position and lays the whole diagram out again.
pub fn layout_all(diagram: &mut StateDiagram, text_size: impl Fn(&str) -> (f64, f64)) {
    for state in &mut diagram.states {
        state.pos = None;
    }
    layout_missing(diagram, text_size);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagrams::state::parser::parse;

    fn size(s: &str) -> (f64, f64) {
        (s.chars().count() as f64 * 8.0, 16.0)
    }

    #[test]
    fn sizes_depend_on_kind_and_direction() {
        let d = parse("stateDiagram-v2\n state f <<fork>>\n A : a long description").unwrap();
        let fork = d.state("f").unwrap();
        assert_eq!(state_size(fork, Direction::TopBottom, size), BAR_SIZE);
        assert_eq!(
            state_size(fork, Direction::LeftRight, size),
            (BAR_SIZE.1, BAR_SIZE.0)
        );
        let a = d.state("A").unwrap();
        assert_eq!(
            state_size(a, Direction::TopBottom, size).0,
            18.0 * 8.0 + 2.0 * PADDING.0
        );
    }

    #[test]
    fn lays_out_transitions_in_ranks() {
        let mut d = parse("stateDiagram-v2\n [*] --> A\n A --> B").unwrap();
        layout_missing(&mut d, size);
        let y: Vec<f64> = d.states.iter().map(|s| s.pos.unwrap().y).collect();
        assert!(y[0] < y[1] && y[1] < y[2]);
    }
}
