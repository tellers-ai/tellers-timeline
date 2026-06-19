use crate::{Item, Stack};

impl Stack {
    /// Delete an item by id. Synced clips in the same link group are deleted
    /// together. When `replace_with_gap` is true, each removed clip is replaced
    /// with a gap of the same duration. When false, the column is collapsed
    /// across the sync track cluster.
    /// Returns removed items with their source track indices.
    pub fn delete_item(&mut self, item_id: &str, replace_with_gap: bool) -> Vec<(usize, Item)> {
        if replace_with_gap {
            self.delete_item_replace_with_gap(item_id)
        } else {
            self.delete_item_collapse(item_id)
        }
    }
}
