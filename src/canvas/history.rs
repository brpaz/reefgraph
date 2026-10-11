const LIMIT: usize = 200;

/// Snapshot-based undo/redo stack.
#[derive(Debug)]
pub struct History<T> {
    undo: Vec<T>,
    redo: Vec<T>,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }
}

impl<T> History<T> {
    /// Stores the state as it was before an edit.
    pub fn record(&mut self, before: T) {
        self.undo.push(before);
        if self.undo.len() > LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, current: T) -> Option<T> {
        let previous = self.undo.pop()?;
        self.redo.push(current);
        Some(previous)
    }

    pub fn redo(&mut self, current: T) -> Option<T> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_and_redo_walk_the_stack() {
        let mut history = History::default();
        history.record(1);
        history.record(2);
        assert_eq!(history.undo(3), Some(2));
        assert_eq!(history.undo(2), Some(1));
        assert_eq!(history.undo(1), None);
        assert_eq!(history.redo(1), Some(2));
        assert_eq!(history.redo(2), Some(3));
        assert!(!history.can_redo());
    }

    #[test]
    fn recording_clears_redo() {
        let mut history = History::default();
        history.record(1);
        history.undo(2);
        history.record(5);
        assert!(!history.can_redo());
    }
}
