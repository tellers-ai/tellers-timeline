use crate::{Item, Stack};

impl Stack {
    /// Delete an item by id. Synced clips in the same link group are deleted
    /// together. When `replace_with_gap` is true, each removed clip is replaced
    /// with a gap of the same duration. When false, the column is collapsed
    /// across the sync track cluster.
    /// Returns removed items with their source track indices.
    pub fn delete_item(&mut self, item_id: &str, replace_with_gap: bool) -> Vec<(usize, Item)> {
        if replace_with_gap {
            return self.delete_item_replace_with_gap(item_id);
        }

        let Some((track_index, item_index, item)) = self.get_item(item_id) else {
            return Vec::new();
        };

        let column_start = self.children[track_index].start_time_of_item(item_index);
        let column_duration = item.duration().max(0.0);
        let cluster = self.boundary_group_indices(track_index);
        let sync_clips_id = match &item {
            Item::Clip(clip) => super::resolve_sync_clips_id(&clip.metadata),
            Item::Gap(_) => None,
        };

        self.delete_item_collapse(
            item_id,
            &cluster,
            column_start,
            column_duration,
            sync_clips_id,
        )
    }
}
