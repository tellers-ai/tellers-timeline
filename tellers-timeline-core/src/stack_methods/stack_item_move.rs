use crate::{InsertPolicy, OverlapPolicy, Seconds, Stack};

impl Stack {
    /// Move an item identified by `item_id` to `dest_time` on the track with `dest_track_id`.
    ///
    /// When the selected clip belongs to a Tellers group, every sub-unit of the
    /// group (each sync column, and each standalone clip) shifts by the same time
    /// delta as the selected clip. Only the selected clip changes track; the other
    /// members stay on their own tracks. Otherwise the move falls through to the
    /// regular single-item path.
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
        if let Some(plan) = self.tellers_group_move_plan(item_id, dest_track_id, dest_time) {
            let backup = self.clone();
            // The plan is ordered by current start time so members never collide
            // while shifting: backward moves go smallest-start first, forward
            // moves go biggest-start first. The selected clip is just one entry.
            for (rep_id, track_id, rep_dest_time) in plan {
                if !self.move_item_at_time_single(
                    &rep_id,
                    &track_id,
                    rep_dest_time,
                    replace_with_gap,
                    insert_policy,
                    overlap_policy,
                ) {
                    *self = backup;
                    return false;
                }
            }
            return true;
        }

        self.move_item_at_time_single(
            item_id,
            dest_track_id,
            dest_time,
            replace_with_gap,
            insert_policy,
            overlap_policy,
        )
    }

    /// Move a single sub-unit: a sync column (aligned video/audio partners) moves
    /// as a unit; all other clips, including link groups and unsynced items, share
    /// the same delete + insert path with cluster propagation. This is the
    /// group-unaware move used as a building block by `move_item_at_time`.
    fn move_item_at_time_single(
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

        self.move_linked_items_at_time(
            item_id,
            dest_track_id,
            dest_time,
            replace_with_gap,
            insert_policy,
            overlap_policy,
        )
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
                dest_index,
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
