use super::InsertItemAtTimeResult;
use crate::{InsertPolicy, Item, OverlapPolicy, Seconds, Stack};

impl Stack {
    /// Insert an item at a given time into the track at `dest_track_index`.
    /// Returns the inserted item's id if insertion occurred.
    pub fn insert_item_at_time(
        &mut self,
        dest_track_index: usize,
        dest_time: Seconds,
        mut item: Item,
        overlap_policy: OverlapPolicy,
        insert_policy: InsertPolicy,
        synced_audio_clips: Option<Vec<Item>>,
    ) -> Option<InsertItemAtTimeResult> {
        if dest_track_index >= self.children.len() {
            return None;
        }
        if synced_audio_clips.as_ref().map_or(true, Vec::is_empty)
            && !self.destination_boundary_has_synced_clips(dest_track_index)
        {
            item.clamp_to_active_available_range();
            if item.duration().max(0.0) <= super::EPS {
                return None;
            }
            let mut used_ids = self.collect_timeline_ids();
            let item_id = Self::ensure_unique_item_id(&mut item, &mut used_ids);
            self.children[dest_track_index].insert_at_time(
                dest_time,
                item,
                overlap_policy,
                insert_policy,
            );
            return Some(InsertItemAtTimeResult::ItemId(item_id));
        }
        self.insert_synced_item_at_time(
            dest_track_index,
            dest_time,
            None,
            item,
            overlap_policy,
            insert_policy,
            synced_audio_clips,
        )
    }

    /// Insert an item at an index into the track with `dest_track_id`.
    /// Returns the inserted item's id if insertion occurred.
    pub fn insert_item_at_index(
        &mut self,
        dest_track_id: &str,
        dest_index: usize,
        mut item: Item,
        overlap_policy: OverlapPolicy,
        synced_audio_clips: Option<Vec<Item>>,
    ) -> Option<InsertItemAtTimeResult> {
        let dest_track_index = match self.get_track_by_id(dest_track_id) {
            Some((i, _)) => i,
            None => return None,
        };
        if dest_track_index >= self.children.len() {
            return None;
        }
        if synced_audio_clips.as_ref().map_or(true, Vec::is_empty)
            && !self.destination_boundary_has_synced_clips(dest_track_index)
        {
            item.clamp_to_active_available_range();
            if item.duration().max(0.0) <= super::EPS {
                return None;
            }
            let mut used_ids = self.collect_timeline_ids();
            let item_id = Self::ensure_unique_item_id(&mut item, &mut used_ids);
            self.children[dest_track_index].insert_at_index(dest_index, item, overlap_policy);
            return Some(InsertItemAtTimeResult::ItemId(item_id));
        }
        self.insert_synced_item_at_time(
            dest_track_index,
            0.0,
            Some(dest_index),
            item,
            overlap_policy,
            InsertPolicy::InsertBefore,
            synced_audio_clips,
        )
    }

    fn destination_boundary_has_synced_clips(&self, dest_track_index: usize) -> bool {
        self.boundary_group_indices(dest_track_index)
            .into_iter()
            .any(|track_index| {
                self.children[track_index]
                    .items
                    .iter()
                    .any(|item| match item {
                        Item::Clip(clip) => super::resolve_sync_clips_id(&clip.metadata).is_some(),
                        Item::Gap(_) => false,
                    })
            })
    }
}
