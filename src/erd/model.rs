pub use crate::geometry::Point;
pub use crate::ids::RenameError;
pub use crate::layered::Direction;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cardinality {
    ZeroOrOne,
    #[default]
    ExactlyOne,
    ZeroOrMore,
    OneOrMore,
}

impl Cardinality {
    pub const ALL: [Cardinality; 4] = [
        Cardinality::ZeroOrOne,
        Cardinality::ExactlyOne,
        Cardinality::ZeroOrMore,
        Cardinality::OneOrMore,
    ];

    /// Marker on the left side of a relationship, e.g. `}o` in `}o--||`.
    pub fn left_token(self) -> &'static str {
        match self {
            Cardinality::ZeroOrOne => "|o",
            Cardinality::ExactlyOne => "||",
            Cardinality::ZeroOrMore => "}o",
            Cardinality::OneOrMore => "}|",
        }
    }

    /// Marker on the right side of a relationship, e.g. `o{` in `||--o{`.
    pub fn right_token(self) -> &'static str {
        match self {
            Cardinality::ZeroOrOne => "o|",
            Cardinality::ExactlyOne => "||",
            Cardinality::ZeroOrMore => "o{",
            Cardinality::OneOrMore => "|{",
        }
    }

    /// Word forms Mermaid accepts in place of the markers.
    pub fn aliases(self) -> &'static [&'static str] {
        match self {
            Cardinality::ZeroOrOne => &["zero or one", "one or zero"],
            Cardinality::ExactlyOne => &["only one", "1"],
            Cardinality::ZeroOrMore => &["zero or more", "zero or many", "many(0)", "0+"],
            Cardinality::OneOrMore => &["one or more", "one or many", "many(1)", "1+"],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Primary,
    Foreign,
    Unique,
}

impl Key {
    pub const ALL: [Key; 3] = [Key::Primary, Key::Foreign, Key::Unique];

    pub fn keyword(self) -> &'static str {
        match self {
            Key::Primary => "PK",
            Key::Foreign => "FK",
            Key::Unique => "UK",
        }
    }

    pub fn from_keyword(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.keyword() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Attribute {
    pub ty: String,
    pub name: String,
    pub keys: Vec<Key>,
    pub comment: String,
}

impl Attribute {
    pub fn new(ty: &str, name: &str) -> Self {
        Self {
            ty: ty.into(),
            name: name.into(),
            ..Self::default()
        }
    }

    /// Parses an attribute line: `type name [PK, FK] ["comment"]`.
    pub fn parse(line: &str) -> Result<Self, String> {
        let (main, comment) = match line.split_once('"') {
            Some((main, rest)) => {
                let (comment, after) = rest
                    .split_once('"')
                    .ok_or("Comment is missing its closing `\"`")?;
                if !after.trim().is_empty() {
                    return Err(format!("Unexpected `{}` after the comment", after.trim()));
                }
                (main, comment)
            }
            None => (line, ""),
        };
        let mut words = main.split_whitespace();
        let (Some(ty), Some(name)) = (words.next(), words.next()) else {
            return Err("Expected an attribute type followed by its name".into());
        };
        let keys = words
            .collect::<Vec<_>>()
            .join(" ")
            .split(',')
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(|k| {
                Key::from_keyword(k).ok_or_else(|| format!("Unknown key `{k}`; use PK, FK or UK"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            ty: ty.into(),
            name: name.into(),
            keys,
            comment: comment.into(),
        })
    }

    /// Mermaid source for the attribute, the inverse of [`Attribute::parse`].
    pub fn to_line(&self) -> String {
        let mut line = format!("{} {}", self.ty, self.name);
        if !self.keys.is_empty() {
            let keys: Vec<_> = self.keys.iter().map(|k| k.keyword()).collect();
            line.push(' ');
            line.push_str(&keys.join(", "));
        }
        if !self.comment.is_empty() {
            line.push_str(&format!(" \"{}\"", self.comment));
        }
        line
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub id: String,
    /// Display name, set with the `id[Label]` alias syntax.
    pub label: String,
    pub attributes: Vec<Attribute>,
    /// Center of the entity box in canvas coordinates; `None` until laid out.
    pub pos: Option<Point>,
}

impl Entity {
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            label: id.clone(),
            id,
            attributes: Vec::new(),
            pos: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Relationship {
    pub from: String,
    pub to: String,
    pub from_cardinality: Cardinality,
    pub to_cardinality: Cardinality,
    /// Identifying relationships are drawn solid (`--`), others dashed (`..`).
    pub identifying: bool,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Erd {
    /// Flow direction from the `direction` statement, if any.
    pub direction: Option<Direction>,
    pub entities: Vec<Entity>,
    pub relationships: Vec<Relationship>,
    /// Lines before the `erDiagram` declaration.
    pub preamble: Vec<String>,
    /// Statements kept verbatim (comments, `classDef`, `style`…).
    pub passthrough: Vec<String>,
}

/// Characters allowed in unquoted entity names.
pub fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

impl Erd {
    pub fn is_valid_id(id: &str) -> bool {
        !id.is_empty() && id.chars().all(is_name_char) && id != "erDiagram" && id != "direction"
    }

    pub fn entity(&self, id: &str) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id == id)
    }

    pub fn entity_mut(&mut self, id: &str) -> Option<&mut Entity> {
        self.entities.iter_mut().find(|e| e.id == id)
    }

    /// Returns the entity with `id`, creating it when missing.
    pub fn ensure_entity(&mut self, id: &str) -> &mut Entity {
        let idx = match self.entities.iter().position(|e| e.id == id) {
            Some(idx) => idx,
            None => {
                self.entities.push(Entity::new(id));
                self.entities.len() - 1
            }
        };
        &mut self.entities[idx]
    }

    /// Adds an entity named `ENTITY`, `ENTITY_2`, … at `pos`.
    pub fn add_entity(&mut self, attributes: Vec<Attribute>, pos: Point) -> String {
        let id = std::iter::once("ENTITY".to_string())
            .chain((2..).map(|i| format!("ENTITY_{i}")))
            .find(|id| self.entity(id).is_none())
            .expect("name sequence is infinite");
        self.entities.push(Entity {
            attributes,
            pos: Some(pos),
            ..Entity::new(id.clone())
        });
        id
    }

    pub fn remove_entity(&mut self, id: &str) {
        self.entities.retain(|e| e.id != id);
        self.relationships.retain(|r| r.from != id && r.to != id);
    }

    pub fn rename_entity(&mut self, old: &str, new: &str) -> Result<(), RenameError> {
        if old == new {
            return Ok(());
        }
        if !Self::is_valid_id(new) {
            return Err(RenameError::Invalid);
        }
        if self.entity(new).is_some() {
            return Err(RenameError::Taken);
        }
        if let Some(e) = self.entity_mut(old) {
            if e.label == old {
                e.label = new.to_string();
            }
            e.id = new.to_string();
        }
        for r in &mut self.relationships {
            if r.from == old {
                r.from = new.to_string();
            }
            if r.to == old {
                r.to = new.to_string();
            }
        }
        Ok(())
    }

    /// Adds a one-to-many identifying relationship and returns its index.
    pub fn add_relationship(&mut self, from: &str, to: &str) -> usize {
        self.relationships.push(Relationship {
            from: from.into(),
            to: to.into(),
            from_cardinality: Cardinality::ExactlyOne,
            to_cardinality: Cardinality::ZeroOrMore,
            identifying: true,
            label: "has".into(),
        });
        self.relationships.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_attribute_lines() {
        let a = Attribute::parse("string(99) firstName PK, FK \"Only 99 chars\"").unwrap();
        assert_eq!(a.ty, "string(99)");
        assert_eq!(a.name, "firstName");
        assert_eq!(a.keys, [Key::Primary, Key::Foreign]);
        assert_eq!(a.comment, "Only 99 chars");
        assert_eq!(a.to_line(), "string(99) firstName PK, FK \"Only 99 chars\"");

        let b = Attribute::parse("  int  age  ").unwrap();
        assert_eq!(b.to_line(), "int age");
        assert_eq!(
            Attribute::parse("string[] parts UK").unwrap().keys,
            [Key::Unique]
        );
    }

    #[test]
    fn rejects_bad_attribute_lines() {
        assert!(Attribute::parse("name").is_err());
        assert!(Attribute::parse("int id XK").is_err());
        assert!(Attribute::parse("int id \"open").is_err());
        assert!(Attribute::parse("int id \"c\" extra").is_err());
    }

    #[test]
    fn cardinality_tokens_mirror() {
        for c in Cardinality::ALL {
            let mirrored: String = c
                .left_token()
                .chars()
                .rev()
                .map(|ch| if ch == '}' { '{' } else { ch })
                .collect();
            assert_eq!(mirrored, c.right_token());
        }
    }

    #[test]
    fn add_rename_and_remove_entities() {
        let mut erd = Erd::default();
        let a = erd.add_entity(Vec::new(), Point::default());
        let b = erd.add_entity(Vec::new(), Point::default());
        assert_eq!((a.as_str(), b.as_str()), ("ENTITY", "ENTITY_2"));
        erd.add_relationship(&a, &b);
        erd.rename_entity(&a, "CUSTOMER").unwrap();
        assert_eq!(erd.relationships[0].from, "CUSTOMER");
        assert_eq!(erd.entities[0].label, "CUSTOMER");
        assert_eq!(erd.rename_entity(&b, "CUSTOMER"), Err(RenameError::Taken));
        assert_eq!(
            erd.rename_entity(&b, "has space"),
            Err(RenameError::Invalid)
        );
        erd.rename_entity(&b, "LINE-ITEM").unwrap();
        erd.remove_entity("CUSTOMER");
        assert!(erd.relationships.is_empty());
        assert_eq!(erd.entities.len(), 1);
    }
}
