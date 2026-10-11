//! Helpers for combo rows backed by a fixed list of enum values.

pub fn string_list(items: impl IntoIterator<Item = String>) -> gtk::StringList {
    let items: Vec<String> = items.into_iter().collect();
    gtk::StringList::new(&items.iter().map(String::as_str).collect::<Vec<_>>())
}

/// Position of `value` in `all`, as a combo row selection index.
pub fn index_of<T: PartialEq>(all: &[T], value: &T) -> u32 {
    all.iter().position(|v| v == value).unwrap_or(0) as u32
}
