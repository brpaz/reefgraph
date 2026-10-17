pub use crate::shared::geometry::Point;
pub use crate::shared::ids::RenameError;

use crate::shared::ids;

/// Which C4 view the diagram is, from its declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level {
    #[default]
    Context,
    Container,
    Component,
}

impl Level {
    pub const ALL: [Level; 3] = [Level::Context, Level::Container, Level::Component];

    pub fn keyword(self) -> &'static str {
        match self {
            Level::Context => "C4Context",
            Level::Container => "C4Container",
            Level::Component => "C4Component",
        }
    }

    pub fn from_keyword(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.keyword() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ElementKind {
    Person,
    #[default]
    System,
    Container,
    Component,
}

impl ElementKind {
    pub const ALL: [ElementKind; 4] = [
        ElementKind::Person,
        ElementKind::System,
        ElementKind::Container,
        ElementKind::Component,
    ];

    fn keyword(self) -> &'static str {
        match self {
            ElementKind::Person => "Person",
            ElementKind::System => "System",
            ElementKind::Container => "Container",
            ElementKind::Component => "Component",
        }
    }

    /// Containers and components name their technology; people and systems do not.
    pub fn has_technology(self) -> bool {
        matches!(self, ElementKind::Container | ElementKind::Component)
    }

    /// People are always drawn as people, never as databases or queues.
    pub fn has_shapes(self) -> bool {
        self != ElementKind::Person
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ElementShape {
    #[default]
    Box,
    Database,
    Queue,
}

impl ElementShape {
    pub const ALL: [ElementShape; 3] = [
        ElementShape::Box,
        ElementShape::Database,
        ElementShape::Queue,
    ];

    fn suffix(self) -> &'static str {
        match self {
            ElementShape::Box => "",
            ElementShape::Database => "Db",
            ElementShape::Queue => "Queue",
        }
    }
}

/// A person, software system, container or component.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Element {
    /// The alias other statements refer to the element by.
    pub id: String,
    pub label: String,
    pub kind: ElementKind,
    pub shape: ElementShape,
    pub external: bool,
    /// Only kept for kinds that have a technology.
    pub technology: String,
    pub description: String,
    /// Arguments after the description (sprite, tags, link, `$name=` options),
    /// kept as written.
    pub extra: Vec<String>,
    /// Boundary the element is declared in.
    pub boundary: Option<String>,
    /// Center of the element box in canvas coordinates; `None` until laid out.
    pub pos: Option<Point>,
}

impl Element {
    pub fn new(id: impl Into<String>, kind: ElementKind) -> Self {
        let id = id.into();
        Self {
            label: id.clone(),
            id,
            kind,
            ..Self::default()
        }
    }

    /// Mermaid statement name, such as `ContainerDb_Ext`.
    pub fn keyword(&self) -> String {
        let shape = if self.kind.has_shapes() {
            self.shape.suffix()
        } else {
            ""
        };
        let ext = if self.external { "_Ext" } else { "" };
        format!("{}{shape}{ext}", self.kind.keyword())
    }

    /// Kind, shape and whether it is external, for a statement name.
    pub fn parse_keyword(keyword: &str) -> Option<(ElementKind, ElementShape, bool)> {
        let (base, external) = match keyword.strip_suffix("_Ext") {
            Some(base) => (base, true),
            None => (keyword, false),
        };
        ElementKind::ALL.into_iter().find_map(|kind| {
            let rest = base.strip_prefix(kind.keyword())?;
            let shape = ElementShape::ALL.into_iter().find(|s| s.suffix() == rest)?;
            (shape == ElementShape::Box || kind.has_shapes()).then_some((kind, shape, external))
        })
    }

    /// `«…»` line shown above the name, as in C4-PlantUML.
    pub fn stereotype(&self) -> String {
        let kind = match self.kind {
            ElementKind::Person => "person",
            ElementKind::System => "system",
            ElementKind::Container => "container",
            ElementKind::Component => "component",
        };
        let shape = match (self.kind.has_shapes(), self.shape) {
            (true, ElementShape::Database) => "_db",
            (true, ElementShape::Queue) => "_queue",
            _ => "",
        };
        let ext = if self.external { "external_" } else { "" };
        format!("«{ext}{kind}{shape}»")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoundaryKind {
    Generic,
    Enterprise,
    #[default]
    System,
    Container,
}

impl BoundaryKind {
    pub const ALL: [BoundaryKind; 4] = [
        BoundaryKind::Generic,
        BoundaryKind::Enterprise,
        BoundaryKind::System,
        BoundaryKind::Container,
    ];

    pub fn keyword(self) -> &'static str {
        match self {
            BoundaryKind::Generic => "Boundary",
            BoundaryKind::Enterprise => "Enterprise_Boundary",
            BoundaryKind::System => "System_Boundary",
            BoundaryKind::Container => "Container_Boundary",
        }
    }

    pub fn from_keyword(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.keyword() == s)
    }
}

/// A named group drawn as a frame around the elements declared in it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Boundary {
    pub id: String,
    pub label: String,
    pub kind: BoundaryKind,
    /// Arguments after the label (type, tags, link), kept as written.
    pub extra: Vec<String>,
    /// Enclosing boundary.
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RelationKind {
    #[default]
    Rel,
    BiRel,
    /// Layout hints of `Rel_U`, `Rel_D`, `Rel_L` and `Rel_R`, kept as written.
    Up,
    Down,
    Left,
    Right,
}

impl RelationKind {
    pub fn keyword(self) -> &'static str {
        match self {
            RelationKind::Rel => "Rel",
            RelationKind::BiRel => "BiRel",
            RelationKind::Up => "Rel_U",
            RelationKind::Down => "Rel_D",
            RelationKind::Left => "Rel_L",
            RelationKind::Right => "Rel_R",
        }
    }

    pub fn from_keyword(s: &str) -> Option<Self> {
        Some(match s {
            "Rel" => RelationKind::Rel,
            "BiRel" => RelationKind::BiRel,
            "Rel_U" | "Rel_Up" => RelationKind::Up,
            "Rel_D" | "Rel_Down" => RelationKind::Down,
            "Rel_L" | "Rel_Left" => RelationKind::Left,
            "Rel_R" | "Rel_Right" => RelationKind::Right,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Relation {
    pub from: String,
    pub to: String,
    pub label: String,
    pub technology: String,
    pub kind: RelationKind,
    /// Arguments after the technology, kept as written.
    pub extra: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct C4Diagram {
    pub level: Level,
    pub title: Option<String>,
    pub elements: Vec<Element>,
    pub boundaries: Vec<Boundary>,
    pub relations: Vec<Relation>,
    /// Lines before the declaration.
    pub preamble: Vec<String>,
    /// Statements kept verbatim (comments, `UpdateElementStyle`…).
    pub passthrough: Vec<String>,
}

impl C4Diagram {
    pub fn is_valid_id(id: &str) -> bool {
        ids::is_valid_id(id, &[])
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

    /// Elements and boundaries share one namespace of aliases.
    pub fn is_taken(&self, id: &str) -> bool {
        self.element(id).is_some() || self.boundary(id).is_some()
    }

    fn fresh_id(&self, base: &str) -> String {
        if self.is_taken(base) {
            ids::copy_id(base, |id| self.is_taken(id))
        } else {
            base.to_string()
        }
    }

    /// Adds an element named after its kind (`person`, `person2`…) at `pos`.
    pub fn add_element(
        &mut self,
        kind: ElementKind,
        shape: ElementShape,
        external: bool,
        pos: Point,
    ) -> String {
        let base = match kind {
            ElementKind::Person => "person",
            ElementKind::System => "system",
            ElementKind::Container => "container",
            ElementKind::Component => "component",
        };
        let id = self.fresh_id(base);
        let label = match kind {
            ElementKind::Person => "Person",
            ElementKind::System => "System",
            ElementKind::Container => "Container",
            ElementKind::Component => "Component",
        };
        self.elements.push(Element {
            label: label.to_string(),
            shape,
            external,
            pos: Some(pos),
            ..Element::new(id.clone(), kind)
        });
        id
    }

    /// Adds a copy of `element` at `pos` under a numbered variant of its id,
    /// returning the new id. A label that just repeats the id follows it.
    /// The copy stays in the same boundary when that still exists.
    pub fn paste_element(&mut self, element: &Element, pos: Point) -> String {
        let id = ids::copy_id(&element.id, |id| self.is_taken(id));
        let boundary = element
            .boundary
            .clone()
            .filter(|b| self.boundary(b).is_some());
        self.elements.push(Element {
            label: if element.label == element.id {
                id.clone()
            } else {
                element.label.clone()
            },
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

    pub fn rename_element(&mut self, old: &str, new: &str) -> Result<(), RenameError> {
        self.check_rename(old, new)?;
        if let Some(e) = self.element_mut(old) {
            if e.label == old {
                e.label = new.to_string();
            }
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

    fn check_rename(&self, old: &str, new: &str) -> Result<(), RenameError> {
        if !Self::is_valid_id(new) {
            return Err(RenameError::Invalid);
        }
        if old != new && self.is_taken(new) {
            return Err(RenameError::Taken);
        }
        Ok(())
    }

    /// Adds a relation labelled "Uses" and returns its index.
    pub fn add_relation(&mut self, from: &str, to: &str) -> usize {
        self.relations.push(Relation {
            from: from.to_string(),
            to: to.to_string(),
            label: "Uses".to_string(),
            ..Relation::default()
        });
        self.relations.len() - 1
    }

    /// Adds a top-level system boundary and returns its id.
    pub fn add_boundary(&mut self) -> String {
        let id = self.fresh_id("boundary");
        self.boundaries.push(Boundary {
            id: id.clone(),
            label: "Boundary".to_string(),
            ..Boundary::default()
        });
        id
    }

    /// Removes the boundary; its elements and nested boundaries move to its parent.
    pub fn remove_boundary(&mut self, id: &str) {
        let Some(index) = self.boundaries.iter().position(|b| b.id == id) else {
            return;
        };
        let parent = self.boundaries.remove(index).parent;
        for e in &mut self.elements {
            if e.boundary.as_deref() == Some(id) {
                e.boundary.clone_from(&parent);
            }
        }
        for b in &mut self.boundaries {
            if b.parent.as_deref() == Some(id) {
                b.parent.clone_from(&parent);
            }
        }
    }

    pub fn rename_boundary(&mut self, old: &str, new: &str) -> Result<(), RenameError> {
        self.check_rename(old, new)?;
        if let Some(b) = self.boundary_mut(old) {
            b.id = new.to_string();
        }
        for e in &mut self.elements {
            if e.boundary.as_deref() == Some(old) {
                e.boundary = Some(new.to_string());
            }
        }
        for b in &mut self.boundaries {
            if b.parent.as_deref() == Some(old) {
                b.parent = Some(new.to_string());
            }
        }
        Ok(())
    }

    /// Boundaries directly inside `parent`, or at the top level for `None`.
    pub fn child_boundaries<'a>(
        &'a self,
        parent: Option<&'a str>,
    ) -> impl Iterator<Item = &'a Boundary> + 'a {
        self.boundaries
            .iter()
            .filter(move |b| b.parent.as_deref() == parent)
    }

    /// Elements directly inside `boundary`, or at the top level for `None`.
    pub fn child_elements<'a>(
        &'a self,
        boundary: Option<&'a str>,
    ) -> impl Iterator<Item = &'a Element> + 'a {
        self.elements
            .iter()
            .filter(move |e| e.boundary.as_deref() == boundary)
    }

    /// Nesting depth of a boundary: 0 at the top level.
    pub fn boundary_depth(&self, id: &str) -> usize {
        let mut depth = 0;
        let mut current = self.boundary(id).and_then(|b| b.parent.as_deref());
        while let Some(parent) = current {
            depth += 1;
            current = self.boundary(parent).and_then(|b| b.parent.as_deref());
        }
        depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_keywords_round_trip() {
        for keyword in [
            "Person",
            "Person_Ext",
            "System",
            "SystemDb_Ext",
            "ContainerQueue",
            "ComponentDb",
        ] {
            let (kind, shape, external) = Element::parse_keyword(keyword).unwrap();
            let e = Element {
                kind,
                shape,
                external,
                ..Element::default()
            };
            assert_eq!(e.keyword(), keyword);
        }
        assert_eq!(Element::parse_keyword("PersonDb"), None);
        assert_eq!(Element::parse_keyword("Rel"), None);
    }

    #[test]
    fn ids_are_shared_between_elements_and_boundaries() {
        let mut d = C4Diagram::default();
        let b = d.add_boundary();
        let e = d.add_element(
            ElementKind::System,
            ElementShape::Box,
            false,
            Point::default(),
        );
        assert_eq!((b.as_str(), e.as_str()), ("boundary", "system"));
        assert_eq!(
            d.rename_element("system", "boundary"),
            Err(RenameError::Taken)
        );
    }

    #[test]
    fn removing_a_boundary_moves_members_to_its_parent() {
        let mut d = C4Diagram::default();
        let outer = d.add_boundary();
        let inner = d.add_boundary();
        d.boundary_mut(&inner).unwrap().parent = Some(outer.clone());
        let e = d.add_element(
            ElementKind::Container,
            ElementShape::Box,
            false,
            Point::default(),
        );
        d.element_mut(&e).unwrap().boundary = Some(inner.clone());
        assert_eq!(d.boundary_depth(&inner), 1);

        d.remove_boundary(&inner);
        assert_eq!(d.element(&e).unwrap().boundary.as_deref(), Some("boundary"));
    }

    #[test]
    fn renaming_updates_references() {
        let mut d = C4Diagram::default();
        let b = d.add_boundary();
        let a = d.add_element(
            ElementKind::Person,
            ElementShape::Box,
            false,
            Point::default(),
        );
        let s = d.add_element(
            ElementKind::System,
            ElementShape::Box,
            false,
            Point::default(),
        );
        d.element_mut(&s).unwrap().boundary = Some(b.clone());
        d.add_relation(&a, &s);

        d.rename_element("system", "bank").unwrap();
        d.rename_boundary("boundary", "core").unwrap();
        assert_eq!(d.relations[0].to, "bank");
        assert_eq!(d.element("bank").unwrap().boundary.as_deref(), Some("core"));
    }
}
