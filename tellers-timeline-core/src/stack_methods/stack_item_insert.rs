use super::InsertItemAtTimeResult;
use crate::{InsertPolicy, Item, OverlapPolicy, Seconds, Stack};

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
        synced_audio_clips: Option<Vec<Item>>,
        synced_video_clip: Option<Item>,
    ) -> Option<InsertItemAtTimeResult> {
        if dest_track_index >= self.children.len() {
            return None;
        }
        self.insert_synced_item_at_time(
            dest_track_index,
            dest_time,
            None,
            item,
            overlap_policy,
            insert_policy,
            synced_audio_clips,
            synced_video_clip,
            None::<&str>,
            None::<&[usize]>,
            None::<&[usize]>,
        )
    }

    /// Insert an item at an index into the track with `dest_track_id`.
    /// Returns the inserted item's id if insertion occurred.
    pub fn insert_item_at_index(
        &mut self,
        dest_track_id: &str,
        dest_index: usize,
        item: Item,
        overlap_policy: OverlapPolicy,
        synced_audio_clips: Option<Vec<Item>>,
        synced_video_clip: Option<Item>,
    ) -> Option<InsertItemAtTimeResult> {
        let dest_track_index = match self.get_track_by_id(dest_track_id) {
            Some((i, _)) => i,
            None => return None,
        };
        if dest_track_index >= self.children.len() {
            return None;
        }
        self.insert_synced_item_at_time(
            dest_track_index,
            0.0,
            Some(dest_index),
            item,
            overlap_policy,
            InsertPolicy::InsertBefore,
            synced_audio_clips,
            synced_video_clip,
            None::<&str>,
            None::<&[usize]>,
            None::<&[usize]>,
        )
    }
}
