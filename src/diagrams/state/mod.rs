//! State diagram domain: model, Mermaid parsing and serialization, state
//! geometry and auto layout. Apart from `editor`, independent of GTK so it can be unit tested in
//! isolation.

pub mod layout;
pub mod model;
pub mod parser;
pub mod serializer;

pub use model::{State, StateDiagram, StateKind, Transition};
pub use parser::parse;
pub use serializer::serialize;

pub mod editor;
