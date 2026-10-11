//! Entity relationship diagram domain: model, Mermaid parsing and
//! serialization, entity box geometry and auto layout. Independent of GTK so
//! it can be unit tested in isolation.

pub mod layout;
pub mod model;
pub mod parser;
pub mod serializer;

pub use model::{Attribute, Cardinality, Entity, Erd, Key, Relationship};
pub use parser::parse;
pub use serializer::serialize;
