use crate::{Item, Track};

impl Track {
    /// Replace item at index.
    /// Returns true if the index was valid and the item was replaced.
    pub fn replace_item_by_index(&mut self, index: usize, item: Item) -> bool {
        if index >= self.items.len() {
            return false;
        }
        let mut item = item;
        item.clamp_to_active_available_range();
        self.items[index] = item;
        true
    }
}
