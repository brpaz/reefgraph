//! Use case diagram domain: model, Mermaid parsing and serialization,
//! element geometry, boundary frames and auto layout. Independent of GTK so
//! it can be unit tested in isolation.

pub mod layout;
pub mod model;
pub mod parser;
pub mod serializer;

pub use model::{Element, ElementKind, Label, Relation, RelationKind, UseCaseDiagram};
pub use parser::parse;
pub use serializer::serialize;
