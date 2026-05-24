use super::InsertItemAtTimeResult;
use crate::{IdMetadataExt, InsertPolicy, Item, OverlapPolicy, Seconds, Stack};

impl Stack {
    /// Insert an item at a given time into the track at `dest_track_index`.
    /// Returns the inserted item's id if insertion occurred.
    pub fn insert_item_at_time(
        &mut self,
        dest_track_index: usize,
        dest_time: Seconds,
        item: Item,
        overlap_policy: OverlapPolicy,
        insert_policy: InsertPolicy,
        linked_audio_clips: Option<Vec<Item>>,
        linked_video_clip: Option<Item>,
    ) -> Option<InsertItemAtTimeResult> {
        if dest_track_index >= self.children.len() {
            return None;
        }
        let touches_linked_group = !self
            .linked_groups_touched_by_insert_at_time(
                dest_track_index,
                dest_time,
                item.duration(),
                overlap_policy,
                insert_policy,
            )
            .is_empty();
        if Self::has_linked_inputs(&linked_audio_clips, &linked_video_clip) || touches_linked_group
        {
            return self.insert_linked_item_at_time(
                dest_track_index,
                dest_time,
                None,
                item,
                overlap_policy,
                insert_policy,
                linked_audio_clips,
                linked_video_clip,
            );
        }

        let inserted_id = item.get_id();
        self.children[dest_track_index].insert_at_time(
            dest_time,
            item,
            overlap_policy,
            insert_policy,
        );
        inserted_id.map(InsertItemAtTimeResult::ItemId)
    }

    /// Insert an item at an index into the track with `dest_track_id`.
    /// Returns the inserted item's id if insertion occurred.
    pub fn insert_item_at_index(
        &mut self,
        dest_track_id: &str,
        dest_index: usize,
        item: Item,
        overlap_policy: OverlapPolicy,
        linked_audio_clips: Option<Vec<Item>>,
        linked_video_clip: Option<Item>,
    ) -> Option<InsertItemAtTimeResult> {
        let dest_track_index = match self.get_track_by_id(dest_track_id) {
            Some((i, _)) => i,
            None => return None,
        };
        if dest_track_index >= self.children.len() {
            return None;
        }

        let touches_linked_group = !self
            .linked_groups_touched_by_insert_at_index(
                dest_track_index,
                dest_index,
                item.duration(),
                overlap_policy,
            )
            .is_empty();
        if Self::has_linked_inputs(&linked_audio_clips, &linked_video_clip) || touches_linked_group
        {
            return self.insert_linked_item_at_time(
                dest_track_index,
                0.0,
                Some(dest_index),
                item,
                overlap_policy,
                InsertPolicy::InsertBefore,
                linked_audio_clips,
                linked_video_clip,
            );
        }

        let inserted_id = item.get_id();
        self.children[dest_track_index].insert_at_index(dest_index, item, overlap_policy);
        inserted_id.map(InsertItemAtTimeResult::ItemId)
    }
}
