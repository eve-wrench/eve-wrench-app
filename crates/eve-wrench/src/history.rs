// Undo/redo over a snapshot of an editor's whole state. Consecutive edits
// with the same key (typing into one field, one drag) collapse into a single
// step.

pub struct History<S> {
    undo: Vec<S>,
    redo: Vec<S>,
    last_key: Option<String>,
}

impl<S: PartialEq> History<S> {
    const LIMIT: usize = 200;

    pub fn record(&mut self, before: S, key: Option<String>) {
        if key.is_some() && key == self.last_key {
            return;
        }
        if self.undo.last() != Some(&before) {
            self.undo.push(before);
            if self.undo.len() > Self::LIMIT {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last_key = key;
    }

    pub fn undo(&mut self, current: S) -> Option<S> {
        let previous = self.undo.pop()?;
        self.redo.push(current);
        self.last_key = None;
        Some(previous)
    }

    pub fn redo(&mut self, current: S) -> Option<S> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        self.last_key = None;
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
        self.last_key = None;
    }
}

impl<S> Default for History<S> {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            last_key: None,
        }
    }
}
