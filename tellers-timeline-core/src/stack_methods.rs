use crate::{InsertPolicy, Item, OverlapPolicy, Seconds, Stack};

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

    /// Delete an item by id across all tracks. If replace_with_gap is true and the
    /// removed item has a positive duration, a gap of equal duration is inserted.
    /// Returns (track_index, removed_item) on success.
    pub fn delete_item(&mut self, item_id: &str, replace_with_gap: bool) -> Option<(usize, Item)> {
        for ti in 0..self.children.len() {
            if let Some((ii, _)) = self.children[ti].get_item_by_id(item_id) {
                let removed = self.children[ti].items[ii].clone();
                // Use the track API for deletion and optional gap insertion/merge behavior
                let deleted = self.children[ti].delete_clip(ii, replace_with_gap);
                if deleted {
                    return Some((ti, removed));
                } else {
                    return None;
                }
            }
        }
        None
    }

    /// Insert an item at a given time into the track at `dest_track_index`.
    /// Returns true if the destination exists and insertion occurred.
    pub fn insert_item_at_time(
        &mut self,
        dest_track_index: usize,
        dest_time: Seconds,
        item: Item,
        overlap_policy: OverlapPolicy,
        insert_policy: InsertPolicy,
    ) -> bool {
        if dest_track_index >= self.children.len() {
            return false;
        }
        self.children[dest_track_index].insert_at_time(
            dest_time,
            item,
            overlap_policy,
            insert_policy,
        );
        true
    }

    /// Insert an item at an index into the track with `dest_track_id`.
    /// Returns true if the destination track is found and insertion occurred.
    pub fn insert_item_at_index(
        &mut self,
        dest_track_id: &str,
        dest_index: usize,
        item: Item,
        overlap_policy: OverlapPolicy,
    ) -> bool {
        let dest_track_index = match self.get_track_by_id(dest_track_id) {
            Some((i, _)) => i,
            None => return false,
        };
        if dest_track_index >= self.children.len() {
            return false;
        }

        self.children[dest_track_index].insert_at_index(dest_index, item, overlap_policy);
        true
    }

    /// Move a clip identified by `item_id` to `dest_time` on the track with `dest_track_id`.
    /// Returns true if item was successfully moved.
    pub fn move_item_at_time(
        &mut self,
        item_id: &str,
        dest_track_id: &str,
        dest_time: Seconds,
        replace_with_gap: bool,
        insert_policy: InsertPolicy,
        overlap_policy: OverlapPolicy,
    ) -> bool {
        let item_to_move = match self.get_item(item_id) {
            Some((_ti, _ii, it)) => it.clone(),
            None => return false,
        };

        if self.delete_item(item_id, replace_with_gap).is_none() {
            return false;
        }

        let dest_track_index = match self.get_track_by_id(dest_track_id) {
            Some((i, _)) => i,
            None => return false,
        };

        self.insert_item_at_time(
            dest_track_index,
            dest_time,
            item_to_move,
            overlap_policy,
            insert_policy,
        )
    }

    /// Move a clip identified by `item_id` to `dest_index` on the track with `dest_track_id`.
    /// Returns true if item was successfully moved.
    pub fn move_item_at_index(
        &mut self,
        item_id: &str,
        dest_track_id: &str,
        dest_index: usize,
        replace_with_gap: bool,
        overlap_policy: OverlapPolicy,
    ) -> bool {
        let item_to_move = match self.get_item(item_id) {
            Some((_ti, _ii, it)) => it.clone(),
            None => return false,
        };

        if self.delete_item(item_id, replace_with_gap).is_none() {
            return false;
        }

        self.insert_item_at_index(dest_track_id, dest_index, item_to_move, overlap_policy)
    }
}
