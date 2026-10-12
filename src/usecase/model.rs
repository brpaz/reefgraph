pub use crate::geometry::Point;
pub use crate::ids::RenameError;
pub use crate::layered::Direction;

use crate::{ids, syntax};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ElementKind {
    Actor,
    /// Use case drawn as an ellipse: `Id(label)`.
    #[default]
    UseCase,
    /// Use case drawn as a rectangle: `Id[label]`.
    UseCaseRect,
}

impl ElementKind {
    pub const ALL: [ElementKind; 3] = [
        ElementKind::Actor,
        ElementKind::UseCase,
        ElementKind::UseCaseRect,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RelationKind {
    /// `--`
    Line,
    /// `-->`
    #[default]
    Arrow,
    /// `<--`
    ArrowBack,
    /// `--o`
    CircleTo,
    /// `o--`
    CircleFrom,
    /// `--x`
    CrossTo,
    /// `x--`
    CrossFrom,
    /// `..> : include`
    Include,
    /// `..> : extend`
    Extend,
    /// `--|>`
    Generalization,
}

impl RelationKind {
    pub const ALL: [RelationKind; 10] = [
        RelationKind::Arrow,
        RelationKind::Line,
        RelationKind::ArrowBack,
        RelationKind::CircleTo,
        RelationKind::CircleFrom,
        RelationKind::CrossTo,
        RelationKind::CrossFrom,
        RelationKind::Include,
        RelationKind::Extend,
        RelationKind::Generalization,
    ];

    /// Associations that can carry a label and extra length.
    pub fn is_labelled(self) -> bool {
        match self {
            RelationKind::Line | RelationKind::Arrow => true,
            RelationKind::ArrowBack
            | RelationKind::CircleTo
            | RelationKind::CircleFrom
            | RelationKind::CrossTo
            | RelationKind::CrossFrom
            | RelationKind::Include
            | RelationKind::Extend
            | RelationKind::Generalization => false,
        }
    }
}

/// Label text as written between its delimiters: plain text, a `"string"`
/// or a "`markdown string`". Empty when the element shows its id.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Label(pub String);

impl Label {
    /// Label for text typed by the user, quoted so any character is allowed.
    pub fn from_text(text: &str) -> Self {
        if text.is_empty() {
            return Self::default();
        }
        Self(syntax::quote_label(text))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Text shown for the label, or `fallback` when there is none.
    pub fn display(&self, fallback: &str) -> String {
        if self.0.trim().is_empty() {
            fallback.to_string()
        } else {
            syntax::label_text(&self.0)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub id: String,
    pub label: Label,
    pub kind: ElementKind,
    /// System boundary the element is declared in.
    pub boundary: Option<String>,
    /// Metadata, stereotype and classes after the label, kept as written.
    pub extra: String,
    /// Center in canvas coordinates; `None` until laid out.
    pub pos: Option<Point>,
}

impl Element {
    pub fn new(id: impl Into<String>, kind: ElementKind) -> Self {
        Self {
            id: id.into(),
            label: Label::default(),
            kind,
            boundary: None,
            extra: String::new(),
            pos: None,
        }
    }

    pub fn name(&self) -> String {
        self.label.display(&self.id)
    }

    /// The `<<stereotype>>` written after the label, if any.
    pub fn stereotype(&self) -> Option<&str> {
        let start = self.extra.find("<<")? + 2;
        let len = self.extra[start..].find(">>")?;
        Some(self.extra[start..start + len].trim())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Boundary {
    pub id: String,
    pub title: Label,
    /// Metadata and classes after the title, kept as written.
    pub extra: String,
}

impl Boundary {
    pub fn name(&self) -> String {
        self.title.display(&self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Relation {
    pub from: String,
    pub to: String,
    pub kind: RelationKind,
    pub label: Label,
    /// Dashes beyond the shortest operator, asking for a longer edge.
    pub extra_dashes: usize,
    /// Explicit edge id, written `id@` before the operator.
    pub edge_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct UseCaseDiagram {
    pub direction: Option<Direction>,
    pub elements: Vec<Element>,
    pub boundaries: Vec<Boundary>,
    pub relations: Vec<Relation>,
    /// Lines before the declaration.
    pub preamble: Vec<String>,
    /// Statements kept verbatim (comments, notes, `classDef`, `style`…).
    pub passthrough: Vec<String>,
}

const RESERVED_IDS: &[&str] = &[
    "actor",
    "systemBoundary",
    "end",
    "note",
    "json",
    "direction",
    "classDef",
    "class",
    "style",
    "accTitle",
    "accDescr",
];

impl UseCaseDiagram {
    pub fn is_valid_id(id: &str) -> bool {
        ids::is_valid_id(id, RESERVED_IDS)
    }

    pub fn element(&self, id: &str) -> Option<&Element> {
        self.elements.iter().find(|e| e.id == id)
    }

    pub fn element_mut(&mut self, id: &str) -> Option<&mut Element> {
        self.elements.iter_mut().find(|e| e.id == id)
    }

    pub fn boundary(&self, id: &str) -> Option<&Boundary> {
        self.boundaries.iter().find(|b| b.id == id)
    }

    pub fn boundary_mut(&mut self, id: &str) -> Option<&mut Boundary> {
        self.boundaries.iter_mut().find(|b| b.id == id)
    }

    /// Whether `id` names an element, a boundary or an edge; they share one
    /// namespace.
    fn is_taken(&self, id: &str) -> bool {
        self.element(id).is_some()
            || self.boundary(id).is_some()
            || self
                .relations
                .iter()
                .any(|r| r.edge_id.as_deref() == Some(id))
    }

    /// Returns the element with `id`, creating a use case when missing.
    pub fn ensure_element(&mut self, id: &str) -> &mut Element {
        let idx = match self.elements.iter().position(|e| e.id == id) {
            Some(idx) => idx,
            None => {
                self.elements.push(Element::new(id, ElementKind::UseCase));
                self.elements.len() - 1
            }
        };
        &mut self.elements[idx]
    }

    fn free_id(&self, stem: &str) -> String {
        std::iter::once(stem.to_string())
            .chain((2..).map(|i| format!("{stem}{i}")))
            .find(|id| !self.is_taken(id))
            .expect("name sequence is infinite")
    }

    /// Adds an element of `kind` at `pos` and returns its id.
    pub fn add_element(&mut self, kind: ElementKind, pos: Point) -> String {
        let stem = match kind {
            ElementKind::Actor => "Actor",
            ElementKind::UseCase | ElementKind::UseCaseRect => "UseCase",
        };
        let id = self.free_id(stem);
        self.elements.push(Element {
            pos: Some(pos),
            ..Element::new(id.clone(), kind)
        });
        id
    }

    /// Adds a copy of `element` at `pos` under a numbered variant of its id,
    /// in the same boundary when this diagram has it.
    pub fn paste_element(&mut self, element: &Element, pos: Point) -> String {
        let id = ids::copy_id(&element.id, |id| self.is_taken(id));
        let boundary = element
            .boundary
            .clone()
            .filter(|b| self.boundary(b).is_some());
        self.elements.push(Element {
            id: id.clone(),
            boundary,
            pos: Some(pos),
            ..element.clone()
        });
        id
    }

    pub fn remove_element(&mut self, id: &str) {
        self.elements.retain(|e| e.id != id);
        self.relations.retain(|r| r.from != id && r.to != id);
    }

    fn check_new_id(&self, old: &str, new: &str) -> Result<bool, RenameError> {
        if old == new {
            return Ok(false);
        }
        if !Self::is_valid_id(new) {
            return Err(RenameError::Invalid);
        }
        if self.is_taken(new) {
            return Err(RenameError::Taken);
        }
        Ok(true)
    }

    pub fn rename_element(&mut self, old: &str, new: &str) -> Result<(), RenameError> {
        if !self.check_new_id(old, new)? {
            return Ok(());
        }
        if let Some(e) = self.element_mut(old) {
            e.id = new.to_string();
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
            ..Relation::default()
        });
        self.relations.len() - 1
    }

    /// Adds an empty boundary named `System`, `System2`… and returns its id.
    pub fn add_boundary(&mut self) -> String {
        let id = self.free_id("System");
        self.boundaries.push(Boundary {
            id: id.clone(),
            title: Label::default(),
            extra: String::new(),
        });
        id
    }

    /// Removes a boundary; its members stay, outside any boundary.
    pub fn remove_boundary(&mut self, id: &str) {
        self.boundaries.retain(|b| b.id != id);
        for e in &mut self.elements {
            if e.boundary.as_deref() == Some(id) {
                e.boundary = None;
            }
        }
    }

    pub fn rename_boundary(&mut self, old: &str, new: &str) -> Result<(), RenameError> {
        if !self.check_new_id(old, new)? {
            return Ok(());
        }
        if let Some(b) = self.boundary_mut(old) {
            b.id = new.to_string();
        }
        for e in &mut self.elements {
            if e.boundary.as_deref() == Some(old) {
                e.boundary = Some(new.to_string());
            }
        }
        Ok(())
    }

    /// Elements declared directly in `boundary` (the top level for `None`).
    pub fn members(&self, boundary: Option<&str>) -> impl Iterator<Item = &Element> {
        self.elements
            .iter()
            .filter(move |e| e.boundary.as_deref() == boundary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_labels_show_the_fallback() {
        assert_eq!(Label::default().display("Id"), "Id");
        assert_eq!(Label::from_text("").display("Id"), "Id");
        assert_eq!(Label::from_text("Sign in").display("Id"), "Sign in");
    }

    #[test]
    fn ids_are_shared_by_elements_boundaries_and_edges() {
        let mut d = UseCaseDiagram::default();
        let a = d.add_element(ElementKind::Actor, Point::default());
        let u = d.add_element(ElementKind::UseCase, Point::default());
        let b = d.add_boundary();
        assert_eq!(
            (a.as_str(), u.as_str(), b.as_str()),
            ("Actor", "UseCase", "System")
        );
        assert_eq!(d.rename_element(&a, "System"), Err(RenameError::Taken));
        assert_eq!(d.rename_element(&a, "end"), Err(RenameError::Invalid));
        d.add_relation(&a, &u);
        d.rename_element(&u, "Login").unwrap();
        assert_eq!(d.relations[0].to, "Login");
    }

    #[test]
    fn removing_a_boundary_keeps_its_members() {
        let mut d = UseCaseDiagram::default();
        let b = d.add_boundary();
        let u = d.add_element(ElementKind::UseCase, Point::default());
        d.element_mut(&u).unwrap().boundary = Some(b.clone());
        d.rename_boundary(&b, "Shop").unwrap();
        assert_eq!(d.element(&u).unwrap().boundary.as_deref(), Some("Shop"));
        d.remove_boundary("Shop");
        assert_eq!(d.element(&u).unwrap().boundary, None);
    }

    #[test]
    fn reads_stereotypes_from_extra() {
        let mut e = Element::new("A", ElementKind::Actor);
        e.extra = "@{ type: hollow } <<Employee>>:::staff".into();
        assert_eq!(e.stereotype(), Some("Employee"));
    }
}
