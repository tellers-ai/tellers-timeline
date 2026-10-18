use crate::{Item, Stack};

impl Stack {
    /// Delete an item by id. When `with_sync_clips` is true, synced clips in
    /// the same link group are deleted together; when false, only the item
    /// (and its group members) is deleted and its sync partners stay in place.
    /// When `replace_with_gap` is true, each removed clip is replaced
    /// with a gap of the same duration. When false, the column is collapsed
    /// across the sync track cluster.
    /// Collapsing while keeping sync partners is not allowed (the kept partner
    /// would block the column): `replace_with_gap == false` with
    /// `with_sync_clips == false` deletes nothing.
    /// Returns removed items with their source track indices.
    pub fn delete_item(
        &mut self,
        item_id: &str,
        replace_with_gap: bool,
        with_sync_clips: bool,
    ) -> Vec<(usize, Item)> {
        if replace_with_gap {
            self.delete_item_replace_with_gap(item_id, with_sync_clips)
        } else if with_sync_clips {
            self.delete_item_collapse(item_id)
        } else {
            Vec::new()
        }
    }
}
