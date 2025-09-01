use crate::{Item, Track};

impl Track {
    /// Delete the clip at a given index. If `replace_with_gap` is true, insert a gap of the
    /// same duration at that position and merge adjacent gaps.
    /// Returns whether a deletion occurred.
    pub fn delete_clip(&mut self, index: usize, replace_with_gap: bool) -> bool {
        if index >= self.items.len() {
            return false;
        }
        match &self.items[index] {
            Item::Clip(c) => {
                let removed_duration = c.source_range.duration.value.max(0.0);
                self.items.remove(index);
                if replace_with_gap && removed_duration > 0.0 {
                    self.items.insert(
                        index.min(self.items.len()),
                        Item::Gap(crate::types::Gap::make_gap(removed_duration)),
                    );
                    // Ensure we do not leave adjacent gaps
                    self.merge_adjacent_gaps();
                }
                true
            }
            Item::Gap(_) => false,
        }
    }
}
