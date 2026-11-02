use crate::{Item, Stack};

impl Stack {
    /// Delete an item by id, together with its synced clips (the linked
    /// audio of a video, the linked video of an audio clip) and its group
    /// members. There is no way to delete one side of a sync column: a kept
    /// partner would stay behind as a stray clip, and a later push or ripple
    /// on its track would drift the remaining columns out of sync. To drop a
    /// single channel, delete its track ([`Stack::delete_track`]).
    /// When `replace_with_gap` is true, each removed clip is replaced
    /// with a gap of the same duration. When false, the column is collapsed
    /// across the sync track cluster.
    /// Returns removed items with their source track indices.
    pub fn delete_item(&mut self, item_id: &str, replace_with_gap: bool) -> Vec<(usize, Item)> {
        self.keeping_sync_columns(Vec::new(), |stack| {
            stack.delete_item_unchecked(item_id, replace_with_gap)
        })
    }

    fn delete_item_unchecked(
        &mut self,
        item_id: &str,
        replace_with_gap: bool,
    ) -> Vec<(usize, Item)> {
        if replace_with_gap {
            self.delete_item_replace_with_gap(item_id)
        } else {
            self.delete_item_collapse(item_id)
        }
    }
}
