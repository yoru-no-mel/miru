use crate::source::Entry;

/// Playlist: entries + cursor. Cursor clamps, no wrap in v1.
#[derive(Debug)]
pub struct Playlist {
    entries: Vec<Entry>,
    cursor: usize,
}

impl Playlist {
    pub fn new(entries: Vec<Entry>, cursor: usize) -> Self {
        let mut p = Self { entries, cursor: 0 };
        p.seek(cursor);
        p
    }

    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            cursor: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn current(&self) -> Option<&Entry> {
        self.entries.get(self.cursor)
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn replace(&mut self, entries: Vec<Entry>, cursor: usize) {
        self.entries = entries;
        self.cursor = 0;
        self.seek(cursor);
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> bool {
        if self.cursor + 1 < self.entries.len() {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    pub fn prev(&mut self) -> bool {
        if self.cursor > 0 && !self.entries.is_empty() {
            self.cursor -= 1;
            true
        } else {
            false
        }
    }

    pub fn seek(&mut self, index: usize) {
        if self.entries.is_empty() {
            self.cursor = 0;
        } else {
            self.cursor = index.min(self.entries.len() - 1);
        }
    }

    pub fn first(&mut self) {
        self.seek(0);
    }

    pub fn last(&mut self) {
        if !self.entries.is_empty() {
            self.cursor = self.entries.len() - 1;
        }
    }

    pub fn at_start(&self) -> bool {
        self.cursor == 0
    }

    pub fn at_end(&self) -> bool {
        self.entries.is_empty() || self.cursor + 1 >= self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample(n: usize) -> Playlist {
        let entries = (0..n)
            .map(|i| Entry::file(PathBuf::from(format!("{i:02}.png"))))
            .collect();
        Playlist::new(entries, 0)
    }

    #[test]
    fn cursor_clamps() {
        let mut p = sample(3);
        p.seek(99);
        assert_eq!(p.cursor(), 2);
        p.seek(usize::MAX);
        assert_eq!(p.cursor(), 2);
    }

    #[test]
    fn next_prev_no_wrap() {
        let mut p = sample(2);
        assert!(p.at_start());
        assert!(p.next());
        assert!(p.at_end());
        assert!(!p.next());
        assert_eq!(p.cursor(), 1);
        assert!(p.prev());
        assert!(!p.prev());
        assert_eq!(p.cursor(), 0);
    }

    #[test]
    fn empty_playlist_safe() {
        let mut p = Playlist::empty();
        assert!(!p.next());
        assert!(!p.prev());
        assert!(p.current().is_none());
        p.seek(5);
        assert_eq!(p.cursor(), 0);
    }

    #[test]
    fn first_last() {
        let mut p = sample(5);
        p.last();
        assert_eq!(p.cursor(), 4);
        p.first();
        assert_eq!(p.cursor(), 0);
    }
}
