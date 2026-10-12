//! Mindmap domain: tree model, Mermaid parsing and serialization, node
//! geometry and layout. Independent of GTK so it can be unit tested in
//! isolation.

pub mod layout;
pub mod model;
pub mod parser;
pub mod serializer;

pub use model::{Mindmap, Shape};
pub use parser::parse;
pub use serializer::serialize;
