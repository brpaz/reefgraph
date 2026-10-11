//! Rules for element ids shared by the diagram models.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameError {
    Invalid,
    Taken,
}

impl fmt::Display for RenameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenameError::Invalid => f.write_str("ids may only contain letters, digits and `_`"),
            RenameError::Taken => f.write_str("another element already uses this id"),
        }
    }
}

impl std::error::Error for RenameError {}

pub fn is_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Whether `id` is made of id characters and is not one of the `reserved` keywords.
pub fn is_valid_id(id: &str, reserved: &[&str]) -> bool {
    !id.is_empty() && id.chars().all(is_id_char) && !reserved.contains(&id)
}

/// First id in the sequence `A`..`Z`, `N1`, `N2`… for which `taken` is false.
pub fn next_free_id(taken: impl Fn(&str) -> bool) -> String {
    ('A'..='Z')
        .map(String::from)
        .chain((1..).map(|i| format!("N{i}")))
        .find(|id| !taken(id))
        .expect("id sequence is infinite")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_ids() {
        assert!(is_valid_id("Node_1", &[]));
        assert!(!is_valid_id("has space", &[]));
        assert!(!is_valid_id("", &[]));
        assert!(!is_valid_id("end", &["end"]));
    }

    #[test]
    fn finds_next_free_id() {
        assert_eq!(next_free_id(|id| id == "A"), "B");
        assert_eq!(next_free_id(|id| id.len() == 1), "N1");
    }
}
