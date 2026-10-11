pub use crate::geometry::Point;
pub use crate::ids::RenameError;
pub use crate::layered::Direction;

use crate::ids;

/// End marker of a relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Head {
    #[default]
    None,
    /// Hollow triangle (`<|`, `|>`).
    Inheritance,
    /// Filled diamond (`*`).
    Composition,
    /// Hollow diamond (`o`).
    Aggregation,
    /// Arrow (`<`, `>`).
    Arrow,
    /// Provided interface (`()`).
    Lollipop,
}

impl Head {
    pub const ALL: [Head; 6] = [
        Head::None,
        Head::Inheritance,
        Head::Composition,
        Head::Aggregation,
        Head::Arrow,
        Head::Lollipop,
    ];

    /// Marker written before the line, e.g. `<|` in `<|--`.
    pub fn left_token(self) -> &'static str {
        match self {
            Head::None => "",
            Head::Inheritance => "<|",
            Head::Composition => "*",
            Head::Aggregation => "o",
            Head::Arrow => "<",
            Head::Lollipop => "()",
        }
    }

    /// Marker written after the line, e.g. `|>` in `..|>`.
    pub fn right_token(self) -> &'static str {
        match self {
            Head::None => "",
            Head::Inheritance => "|>",
            Head::Composition => "*",
            Head::Aggregation => "o",
            Head::Arrow => ">",
            Head::Lollipop => "()",
        }
    }
}

/// A class member as written in the source. Members containing `(` are
/// methods, the rest are attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member(pub String);

/// How a member is shown: classifiers removed, generics as `<T>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberDisplay {
    pub text: String,
    /// `*` classifier.
    pub is_abstract: bool,
    /// `$` classifier.
    pub is_static: bool,
}

impl Member {
    pub fn is_method(&self) -> bool {
        self.0.contains('(')
    }

    pub fn display(&self) -> MemberDisplay {
        let mut text = self.0.trim().to_string();
        let mut is_abstract = false;
        let mut is_static = false;
        // Classifiers go at the very end, or right after a method's `)`.
        let classifier_at = match text.rfind(')') {
            Some(i) if text[i + 1..].starts_with(['*', '$']) => Some(i + 1),
            _ if text.ends_with(['*', '$']) => Some(text.len() - 1),
            _ => None,
        };
        if let Some(i) = classifier_at {
            match text.remove(i) {
                '*' => is_abstract = true,
                _ => is_static = true,
            }
        }
        MemberDisplay {
            text: display_generics(&text),
            is_abstract,
            is_static,
        }
    }
}

/// Turns Mermaid's `~` generic delimiters into angle brackets:
/// `List~List~int~~` becomes `List<List<int>>`. A `~` between two word
/// characters opens a parameter list; any other `~` closes one.
pub fn display_generics(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let word = |c: Option<&char>| c.is_some_and(|c| c.is_alphanumeric() || *c == '_');
    let mut depth = 0usize;
    chars
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            if c != '~' {
                return c;
            }
            let opens = word(i.checked_sub(1).and_then(|p| chars.get(p))) && word(chars.get(i + 1));
            if opens || depth == 0 {
                depth += 1;
                '<'
            } else {
                depth -= 1;
                '>'
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Class {
    pub id: String,
    /// Display name, set with `class Id["Label"]`.
    pub label: String,
    /// Type parameters, written `class Name~T~`.
    pub generic: String,
    /// Stereotypes such as `interface`, written `<<interface>>`.
    pub annotations: Vec<String>,
    pub members: Vec<Member>,
    /// Center of the class box in canvas coordinates; `None` until laid out.
    pub pos: Option<Point>,
}

impl Class {
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            label: id.clone(),
            id,
            generic: String::new(),
            annotations: Vec::new(),
            members: Vec::new(),
            pos: None,
        }
    }

    /// Name shown in the header, with generics as `<T>`.
    pub fn title(&self) -> String {
        if self.generic.is_empty() {
            self.label.clone()
        } else {
            format!("{}<{}>", self.label, display_generics(&self.generic))
        }
    }

    /// Indices into `members`: attributes first, then methods, each in
    /// source order. This is the order rows are shown in.
    pub fn display_order(&self) -> Vec<usize> {
        let (methods, attributes): (Vec<usize>, Vec<usize>) =
            (0..self.members.len()).partition(|&i| self.members[i].is_method());
        attributes.into_iter().chain(methods).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Relation {
    pub from: String,
    pub to: String,
    pub from_head: Head,
    pub to_head: Head,
    /// Drawn dashed (`..`) instead of solid (`--`).
    pub dashed: bool,
    pub from_cardinality: String,
    pub to_cardinality: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClassDiagram {
    /// Declared as `classDiagram-v2` rather than `classDiagram`.
    pub v2: bool,
    pub direction: Option<Direction>,
    pub classes: Vec<Class>,
    pub relations: Vec<Relation>,
    /// Lines before the declaration.
    pub preamble: Vec<String>,
    /// Statements kept verbatim (comments, notes, `click`, `style`…).
    pub passthrough: Vec<String>,
}

const RESERVED_IDS: &[&str] = &["class", "direction", "namespace", "note", "classDiagram"];

impl ClassDiagram {
    pub fn is_valid_id(id: &str) -> bool {
        ids::is_valid_id(id, RESERVED_IDS)
    }

    pub fn class(&self, id: &str) -> Option<&Class> {
        self.classes.iter().find(|c| c.id == id)
    }

    pub fn class_mut(&mut self, id: &str) -> Option<&mut Class> {
        self.classes.iter_mut().find(|c| c.id == id)
    }

    /// Returns the class with `id`, creating it when missing.
    pub fn ensure_class(&mut self, id: &str) -> &mut Class {
        let idx = match self.classes.iter().position(|c| c.id == id) {
            Some(idx) => idx,
            None => {
                self.classes.push(Class::new(id));
                self.classes.len() - 1
            }
        };
        &mut self.classes[idx]
    }

    /// Adds a class named `Class`, `Class2`, … at `pos`.
    pub fn add_class(&mut self, annotations: Vec<String>, pos: Point) -> String {
        let id = std::iter::once("Class".to_string())
            .chain((2..).map(|i| format!("Class{i}")))
            .find(|id| self.class(id).is_none())
            .expect("name sequence is infinite");
        self.classes.push(Class {
            annotations,
            pos: Some(pos),
            ..Class::new(id.clone())
        });
        id
    }

    pub fn remove_class(&mut self, id: &str) {
        self.classes.retain(|c| c.id != id);
        self.relations.retain(|r| r.from != id && r.to != id);
    }

    pub fn rename_class(&mut self, old: &str, new: &str) -> Result<(), RenameError> {
        if old == new {
            return Ok(());
        }
        if !Self::is_valid_id(new) {
            return Err(RenameError::Invalid);
        }
        if self.class(new).is_some() {
            return Err(RenameError::Taken);
        }
        if let Some(c) = self.class_mut(old) {
            if c.label == old {
                c.label = new.to_string();
            }
            c.id = new.to_string();
        }
        for r in &mut self.relations {
            if r.from == old {
                r.from = new.to_string();
            }
            if r.to == old {
                r.to = new.to_string();
            }
        }
        Ok(())
    }

    /// Adds an association (`-->`) and returns its index.
    pub fn add_relation(&mut self, from: &str, to: &str) -> usize {
        self.relations.push(Relation {
            from: from.into(),
            to: to.into(),
            to_head: Head::Arrow,
            ..Relation::default()
        });
        self.relations.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_generics() {
        assert_eq!(display_generics("List~int~"), "List<int>");
        assert_eq!(display_generics("List~List~int~~"), "List<List<int>>");
        assert_eq!(display_generics("Map~K, V~"), "Map<K, V>");
        assert_eq!(display_generics("plain"), "plain");
    }

    #[test]
    fn reads_member_classifiers() {
        let m = Member("+getMessages()$ List~string~".into()).display();
        assert_eq!(m.text, "+getMessages() List<string>");
        assert!(m.is_static && !m.is_abstract);
        let m = Member("someAbstractMethod()*".into()).display();
        assert_eq!(m.text, "someAbstractMethod()");
        assert!(m.is_abstract);
        let m = Member("-count$".into()).display();
        assert!(m.is_static);
        assert_eq!(m.text, "-count");
        assert!(Member("draw()".into()).is_method());
        assert!(!Member("+int x".into()).is_method());
    }

    #[test]
    fn orders_attributes_before_methods() {
        let mut c = Class::new("A");
        c.members = ["run()", "+id", "stop()", "name"]
            .map(|m| Member(m.into()))
            .to_vec();
        assert_eq!(c.display_order(), [1, 3, 0, 2]);
    }

    #[test]
    fn title_shows_generics() {
        let mut c = Class::new("Square");
        c.generic = "Shape".into();
        assert_eq!(c.title(), "Square<Shape>");
    }

    #[test]
    fn rename_and_remove_classes() {
        let mut d = ClassDiagram::default();
        let a = d.add_class(Vec::new(), Point::default());
        let b = d.add_class(Vec::new(), Point::default());
        assert_eq!((a.as_str(), b.as_str()), ("Class", "Class2"));
        d.add_relation(&a, &b);
        d.rename_class(&a, "Animal").unwrap();
        assert_eq!(d.relations[0].from, "Animal");
        assert_eq!(d.rename_class(&b, "Animal"), Err(RenameError::Taken));
        assert_eq!(d.rename_class(&b, "note"), Err(RenameError::Invalid));
        d.remove_class("Animal");
        assert!(d.relations.is_empty());
    }
}
