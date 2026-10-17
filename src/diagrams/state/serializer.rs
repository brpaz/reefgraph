use std::fmt::Write;

use super::model::{END_ID, START_ID, StateDiagram, StateKind};
use super::parser::{KEYWORD, KEYWORD_V2, TERMINAL};
use crate::shared::syntax::position_line;

const INDENT: &str = "    ";

/// Renders the diagram as Mermaid source: state declarations first, then
/// transitions, untouched statements and positions.
pub fn serialize(diagram: &StateDiagram) -> String {
    let mut out = String::new();
    for line in &diagram.preamble {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(if diagram.v2 { KEYWORD_V2 } else { KEYWORD });
    out.push('\n');
    if let Some(direction) = diagram.direction {
        let _ = writeln!(out, "{INDENT}direction {}", direction.keyword());
    }
    for state in &diagram.states {
        match state.kind {
            StateKind::Start | StateKind::End => {}
            StateKind::State if state.label != state.id => {
                for line in state.label.lines() {
                    let _ = writeln!(out, "{INDENT}{} : {line}", state.id);
                }
            }
            StateKind::State => {
                let _ = writeln!(out, "{INDENT}{}", state.id);
            }
            kind => {
                let stereotype = kind.stereotype().unwrap_or_default();
                let _ = writeln!(out, "{INDENT}state {} <<{stereotype}>>", state.id);
            }
        }
    }
    let end = |id: &str| match id {
        START_ID | END_ID => TERMINAL.to_string(),
        id => id.to_string(),
    };
    for t in &diagram.transitions {
        let _ = write!(out, "{INDENT}{} --> {}", end(&t.from), end(&t.to));
        if !t.label.is_empty() {
            let _ = write!(out, " : {}", t.label);
        }
        out.push('\n');
    }
    for line in &diagram.passthrough {
        let _ = writeln!(out, "{INDENT}{line}");
    }
    for state in &diagram.states {
        if let Some(pos) = state.pos {
            let _ = writeln!(out, "{}", position_line(&state.id, pos));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagrams::state::parser::parse;
    use crate::shared::geometry::Point;

    const SAMPLE: &str = "stateDiagram-v2
    direction LR
    Idle
    Busy : Working
    Busy : hard
    state check <<choice>>
    state split <<fork>>
    state merge <<join>>
    [*] --> Idle
    Idle --> Busy : start
    Busy --> check
    check --> [*] : done
    note right of Idle : waits
    classDef hot fill:#f00
%% @reefgraph pos Idle 100 40
%% @reefgraph pos [*]end 300 40
";

    #[test]
    fn round_trips_sample_exactly() {
        let d = parse(SAMPLE).unwrap();
        assert_eq!(serialize(&d), SAMPLE);
    }

    #[test]
    fn edited_diagrams_parse_back_unchanged() {
        let mut d = parse("stateDiagram-v2\n A --> B").unwrap();
        d.add_state(StateKind::End, Point::new(1.0, 2.0));
        d.add_state(StateKind::Start, Point::new(3.0, 4.0));
        d.add_state(StateKind::Join, Point::new(5.0, 6.0));
        d.add_transition(START_ID, "A");
        d.add_transition("B", END_ID);
        d.state_mut("A").unwrap().label = "First\nline two".into();
        // A first line that repeats the id is still a description.
        d.state_mut("B").unwrap().label = "B\nworking".into();
        assert_eq!(parse(&serialize(&d)).unwrap(), d);
    }
}
