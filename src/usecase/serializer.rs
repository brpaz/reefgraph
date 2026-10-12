use std::fmt::Write;

use super::model::{Element, ElementKind, Relation, RelationKind, UseCaseDiagram};
use super::parser::KEYWORD;
use crate::syntax::position_line;

const INDENT: &str = "    ";

/// Renders the diagram as Mermaid source: top-level declarations, system
/// boundaries with their members, relationships, untouched statements and
/// positions.
pub fn serialize(diagram: &UseCaseDiagram) -> String {
    let mut out = String::new();
    for line in &diagram.preamble {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(KEYWORD);
    out.push('\n');
    if let Some(direction) = diagram.direction {
        let _ = writeln!(out, "{INDENT}direction {}", direction.keyword());
    }
    // Members of a boundary that no longer exists are written at the top
    // level so they are not lost.
    let orphaned = |e: &&Element| {
        e.boundary
            .as_deref()
            .is_some_and(|b| diagram.boundary(b).is_none())
    };
    for e in diagram
        .members(None)
        .chain(diagram.elements.iter().filter(orphaned))
    {
        let _ = writeln!(out, "{INDENT}{}", element_line(e));
    }
    for b in &diagram.boundaries {
        let _ = write!(out, "{INDENT}systemBoundary {}", b.id);
        if !b.title.is_empty() {
            let _ = write!(out, "[{}]", b.title.0);
        }
        let _ = writeln!(out, "{}", b.extra);
        for e in diagram.members(Some(&b.id)) {
            let _ = writeln!(out, "{INDENT}{INDENT}{}", element_line(e));
        }
        let _ = writeln!(out, "{INDENT}end");
    }
    for r in &diagram.relations {
        let _ = writeln!(out, "{INDENT}{}", relation_line(r));
    }
    for line in &diagram.passthrough {
        let _ = writeln!(out, "{INDENT}{line}");
    }
    for e in &diagram.elements {
        if let Some(pos) = e.pos {
            let _ = writeln!(out, "{}", position_line(&e.id, pos));
        }
    }
    out
}

fn element_line(e: &Element) -> String {
    let label = &e.label.0;
    let head = match e.kind {
        ElementKind::Actor if label.is_empty() => format!("actor {}", e.id),
        ElementKind::Actor => format!("actor {}({label})", e.id),
        ElementKind::UseCase if label.is_empty() => e.id.clone(),
        ElementKind::UseCase => format!("{}({label})", e.id),
        // The brackets choose the shape, so they are written even without a
        // label of its own.
        ElementKind::UseCaseRect if label.is_empty() => format!("{0}[{0}]", e.id),
        ElementKind::UseCaseRect => format!("{}[{label}]", e.id),
    };
    format!("{head}{}", e.extra)
}

fn relation_line(r: &Relation) -> String {
    let dashes = "-".repeat(2 + r.extra_dashes);
    let labelled = r.kind.is_labelled() && !r.label.is_empty();
    let operator = match r.kind {
        RelationKind::Line if labelled => format!("-- {} {dashes}", r.label.0),
        RelationKind::Line => dashes,
        RelationKind::Arrow if labelled => format!("-- {} {dashes}>", r.label.0),
        RelationKind::Arrow => format!("{dashes}>"),
        RelationKind::ArrowBack => format!("<{dashes}"),
        RelationKind::CircleTo => "--o".into(),
        RelationKind::CircleFrom => "o--".into(),
        RelationKind::CrossTo => "--x".into(),
        RelationKind::CrossFrom => "x--".into(),
        RelationKind::Include => "..> : include".into(),
        RelationKind::Extend => "..> : extend".into(),
        RelationKind::Generalization => "--|>".into(),
    };
    let edge = r
        .edge_id
        .as_ref()
        .map(|id| format!("{id}@"))
        .unwrap_or_default();
    format!("{} {edge}{operator} {}", r.from, r.to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usecase::model::{Label, Point};
    use crate::usecase::parser::parse;

    const SAMPLE: &str = "usecase-beta
    direction LR
    actor Customer(\"Customer\")
    actor Staff(\"Order staff\")@{ type: hollow, business: true } <<Employee>>
    Track
    systemBoundary ordering[\"Ordering System\"]@{ type: package }:::system
        Browse(\"Browse products\")
        Checkout(\"Checkout\") <<Core>>:::critical
        Review[Review order]
        Archive[Archive]
    end
    systemBoundary Empty
    end
    Customer starts@-- \"places order\" ---> Checkout
    Customer --> Browse
    Checkout pays@..> : include Payment
    Staff --> Review
    Staff -- watches --- Track
    Track <-- Staff
    Track --o Review
    Track o-- Review
    Track --x Review
    Track x-- Review
    Review ..> : extend Checkout
    Staff --|> Customer
    note for Checkout \"`Validates the **cart**`\"
    classDef critical stroke:#c33,stroke-width:3px
    starts@{ animation: fast }
%% @merman pos Customer 40 100
";

    #[test]
    fn round_trips_sample_exactly() {
        let d = parse(SAMPLE).unwrap();
        // `Payment` is only an endpoint, so it is declared on the way out.
        let expected = SAMPLE.replace("    Track\n", "    Track\n    Payment\n");
        assert_eq!(serialize(&d), expected);
        assert_eq!(serialize(&parse(&expected).unwrap()), expected);
    }

    #[test]
    fn edited_diagrams_parse_back_unchanged() {
        let mut d = UseCaseDiagram::default();
        let actor = d.add_element(ElementKind::Actor, Point::new(1.0, 2.0));
        let rect = d.add_element(ElementKind::UseCaseRect, Point::new(3.0, 4.0));
        d.element_mut(&actor).unwrap().label = Label::from_text("Buyer (main)");
        let b = d.add_boundary();
        d.element_mut(&rect).unwrap().boundary = Some(b);
        for kind in RelationKind::ALL {
            let i = d.add_relation(&actor, &rect);
            d.relations[i].kind = kind;
            d.relations[i].label = Label::from_text("says \"hi\"");
        }
        let mut expected = d.clone();
        for r in &mut expected.relations {
            if !r.kind.is_labelled() {
                r.label = Label::default();
            }
        }
        assert_eq!(parse(&serialize(&d)).unwrap(), expected);
    }
}
