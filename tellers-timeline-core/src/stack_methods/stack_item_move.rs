use crate::{InsertPolicy, OverlapPolicy, Seconds, Stack};

impl Stack {
    /// Move an item identified by `item_id` to `dest_time` on the track with `dest_track_id`.
    ///
    /// Chooses the appropriate strategy automatically:
    /// - **Synced clips** (same link group, same start, same duration): copies the sync
    ///   set, deletes the source column with gap placeholders, then re-inserts at the
    ///   destination via the synced insert path.
    /// - **Linked / grouped clips** (Resolve link group or Tellers group with offsets):
    ///   moves the primary to the destination and re-inserts partners on their tracks
    ///   with preserved relative offsets, using insert so cluster column padding applies.
    /// - **Unsynced items**: delete + insert at the destination time.
    ///
    /// Returns true if the item was successfully moved.
    pub fn move_item_at_time(
        &mut self,
        item_id: &str,
        dest_track_id: &str,
        dest_time: Seconds,
        replace_with_gap: bool,
        insert_policy: InsertPolicy,
        overlap_policy: OverlapPolicy,
    ) -> bool {
        if let Some(items_to_move) = self.synced_move_items(item_id) {
            let dest_track_index = match self.get_track_by_id(dest_track_id) {
                Some((index, _)) => index,
                None => return false,
            };
            return self.move_synced_items_at_time_via_insert(
                items_to_move,
                dest_track_index,
                dest_time,
                replace_with_gap,
                insert_policy,
                overlap_policy,
            );
        }

        let item_to_move = match self.get_item(item_id) {
            Some((_ti, _ii, item)) => item.clone(),
            None => return false,
        };
        if !self.linked_item_ids_for_move(item_id, &item_to_move).is_empty() {
            return self.move_linked_items_at_time(
                item_id,
                dest_track_id,
                dest_time,
                replace_with_gap,
                insert_policy,
                overlap_policy,
            );
        }

        let dest_track_index = match self.get_track_by_id(dest_track_id) {
            Some((index, _)) => index,
            None => return false,
        };

        let backup = self.clone();
        if self.delete_one_item(item_id, replace_with_gap).is_none() {
            return false;
        }

        if self
            .insert_item_at_time(
                dest_track_index,
                dest_time,
                item_to_move,
                overlap_policy,
                insert_policy,
                None,
                None,
            )
            .is_some()
        {
            self.sanitize();
            true
        } else {
            *self = backup;
            false
        }
    }

    /// Move an item identified by `item_id` to `dest_index` on the track with `dest_track_id`.
    /// Returns true if the item was successfully moved.
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

        let backup = self.clone();
        let dest_track_index = match self.get_track_by_id(dest_track_id) {
            Some((i, _)) => i,
            None => return false,
        };
        if let Some(items_to_move) = self.synced_move_items(item_id) {
            return self.move_synced_items(
                items_to_move,
                dest_track_index,
                replace_with_gap,
                overlap_policy,
                super::SyncedMovePlacement::Index { dest_index },
            );
        }

        if self.delete_one_item(item_id, replace_with_gap).is_none() {
            return false;
        }

        if self
            .insert_item_at_index(
                dest_track_id,
                dest_index,
                item_to_move,
                overlap_policy,
                None,
                None,
            )
            .is_some()
        {
            self.sanitize();
            true
        } else {
            *self = backup;
            false
        }
    }
}
