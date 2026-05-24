use crate::{Item, Stack};

impl Stack {
    /// Find an item by id across all tracks. Returns (track_index, item_index, &Item).
    pub fn get_item(&self, item_id: &str) -> Option<(usize, usize, &Item)> {
        for (ti, track) in self.children.iter().enumerate() {
            if let Some((ii, item)) = track.get_item_by_id(item_id) {
                return Some((ti, ii, item));
            }
        }
        None
    }
}
