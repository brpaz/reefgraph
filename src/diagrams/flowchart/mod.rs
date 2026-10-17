//! Flowchart domain: model, Mermaid parsing and serialization, and auto layout.
//! Apart from `editor`, independent of GTK so it can be unit tested in isolation.

pub mod layout;
pub mod model;
pub mod parser;
pub mod serializer;

pub use model::{Direction, Edge, Flowchart, Head, Node, NodeShape, Point, Stroke};
pub use parser::parse;
pub use serializer::serialize;

pub mod editor;
