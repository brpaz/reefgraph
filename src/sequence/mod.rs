//! Sequence diagram domain: model, Mermaid parsing and serialization, and
//! layout. Independent of GTK so it can be unit tested in isolation.

pub mod layout;
pub mod model;
pub mod parser;
pub mod serializer;

pub use model::{
    ArrowHead, BlockKind, Item, LineStyle, Message, Note, NotePlacement, ParticipantKind, Sequence,
};
pub use parser::parse;
pub use serializer::serialize;
