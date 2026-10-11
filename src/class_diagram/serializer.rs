use std::fmt::Write;

use super::model::ClassDiagram;
use super::parser::{KEYWORD, KEYWORD_V2};
use crate::syntax::position_line;

const INDENT: &str = "    ";

/// Renders the diagram as Mermaid source: class declarations with their
/// members first, then relations, untouched statements and positions.
pub fn serialize(diagram: &ClassDiagram) -> String {
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
    for class in &diagram.classes {
        let _ = write!(out, "{INDENT}class {}", class.id);
        if !class.generic.is_empty() {
            let _ = write!(out, "~{}~", class.generic);
        }
        if class.label != class.id {
            let _ = write!(out, "[\"{}\"]", class.label);
        }
        if class.annotations.is_empty() && class.members.is_empty() {
            out.push('\n');
            continue;
        }
        out.push_str(" {\n");
        for annotation in &class.annotations {
            let _ = writeln!(out, "{INDENT}{INDENT}<<{annotation}>>");
        }
        for member in &class.members {
            let _ = writeln!(out, "{INDENT}{INDENT}{}", member.0);
        }
        let _ = writeln!(out, "{INDENT}}}");
    }
    for r in &diagram.relations {
        let _ = write!(out, "{INDENT}{}", r.from);
        if !r.from_cardinality.is_empty() {
            let _ = write!(out, " \"{}\"", r.from_cardinality);
        }
        let _ = write!(
            out,
            " {}{}{}",
            r.from_head.left_token(),
            if r.dashed { ".." } else { "--" },
            r.to_head.right_token()
        );
        if !r.to_cardinality.is_empty() {
            let _ = write!(out, " \"{}\"", r.to_cardinality);
        }
        let _ = write!(out, " {}", r.to);
        if !r.label.is_empty() {
            let _ = write!(out, " : {}", r.label);
        }
        out.push('\n');
    }
    for line in &diagram.passthrough {
        let _ = writeln!(out, "{INDENT}{line}");
    }
    for class in &diagram.classes {
        if let Some(pos) = class.pos {
            let _ = writeln!(out, "{}", position_line(&class.id, pos));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class_diagram::model::Head;
    use crate::class_diagram::parser::parse;

    const SAMPLE: &str = "classDiagram
    direction LR
    class Animal {
        <<abstract>>
        +int age
        +String gender
        +isMammal() bool
        +mate()*
    }
    class Duck[\"Rubber Duck\"] {
        +String beakColor
        +swim()
    }
    class Square~Shape~ {
        -List~int~ position
        +getPoints()$ List~int~
    }
    class Zebra
    Animal <|-- Duck
    Animal <|-- Zebra
    Customer \"1\" --> \"*\" Ticket : buys
    Engine *-- Car
    Pond o-- Duck : lives in
    Printer ..|> Device
    Service ..> Logger
    Widget --() Api
    note for Duck \"can fly\"
%% @merman pos Animal 200 60
";

    #[test]
    fn round_trips_sample_exactly() {
        let d = parse(SAMPLE).unwrap();
        // Classes only mentioned in relations are declared on the way out.
        let expected = SAMPLE.replace(
            "    class Zebra\n",
            "    class Zebra\n    class Customer\n    class Ticket\n    class Engine\n    class Car\n    class Pond\n    class Printer\n    class Device\n    class Service\n    class Logger\n    class Widget\n    class Api\n",
        );
        assert_eq!(serialize(&d), expected);
        assert_eq!(parse(&expected).unwrap(), d);
    }

    #[test]
    fn round_trips_every_head_pair() {
        let mut d = ClassDiagram::default();
        for from_head in Head::ALL {
            for to_head in Head::ALL {
                for dashed in [false, true] {
                    let i = d.add_relation("A", "B");
                    let r = &mut d.relations[i];
                    (r.from_head, r.to_head, r.dashed) = (from_head, to_head, dashed);
                }
            }
        }
        d.ensure_class("A");
        d.ensure_class("B");
        assert_eq!(parse(&serialize(&d)).unwrap().relations, d.relations);
    }
}
