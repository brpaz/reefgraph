//! C4 diagram domain (context, container and component views): model,
//! Mermaid parsing and serialization, element geometry, boundary frames and
//! auto layout. Independent of GTK so it can be unit tested in isolation.

pub mod layout;
pub mod model;
pub mod parser;
pub mod serializer;

pub use model::{
    Boundary, BoundaryKind, C4Diagram, Element, ElementKind, ElementShape, Level, Relation,
    RelationKind,
};
pub use parser::parse;
pub use serializer::serialize;
