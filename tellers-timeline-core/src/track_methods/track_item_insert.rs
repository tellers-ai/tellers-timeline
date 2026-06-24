use crate::{IdMetadataExt, Item, Seconds, Track};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlapPolicy {
    Override,
    Push,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertPolicy {
    /// If inserting inside a gap, split the gap into left-gap/item/right-gap.
    /// If inside a non-gap item, falls back to BeforeOrAfter.
    SplitAndInsert,
    /// If inserting inside an item, adjust start to the item's start.
    InsertBefore,
    /// If inserting inside an item, adjust start to the item's end.
    InsertAfter,
    /// If inserting inside an item, choose the closer boundary (start or end).
    InsertBeforeOrAfter,
}

/// A clip that was split during insert. `left_clip_id` and `right_clip_id` are
/// optional because override insertion may trim away one side.
#[derive(Debug, Clone, PartialEq)]
pub struct SplitClipInfo {
    pub old_clip_id: String,
    pub left_clip_id: Option<String>,
    pub right_clip_id: Option<String>,
    pub sync_clips_id: Option<i64>,
    pub split_time: Seconds,
}

/// A clip removed during override insert.
#[derive(Debug, Clone, PartialEq)]
pub struct DeletedClipInfo {
    pub clip_id: String,
    pub sync_clips_id: Option<i64>,
}

/// Result of a track-level insert: deleted clips, splits, and success flag.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrackInsertResult {
    pub success: bool,
    pub deleted_clips: Vec<DeletedClipInfo>,
    pub split_clips: Vec<SplitClipInfo>,
}

impl TrackInsertResult {
    pub(crate) fn merge(&mut self, other: TrackInsertResult) {
        self.success = self.success && other.success;
        self.deleted_clips.extend(other.deleted_clips);
        self.split_clips.extend(other.split_clips);
    }
}

impl Track {
    pub(crate) fn insert_at_index(
        &mut self,
        index: usize,
        mut item: Item,
        overlap_policy: OverlapPolicy,
    ) -> TrackInsertResult {
        item.clamp_to_active_available_range();
        if overlap_policy == OverlapPolicy::Push {
            self.insert_and_push(index, item);
            return TrackInsertResult {
                success: true,
                ..Default::default()
            };
        }

        self.insert_and_override(index, item)
    }

    pub(crate) fn insert_and_push(&mut self, index: usize, mut item: Item) {
        item.clamp_to_active_available_range();
        self.items.insert(index, item);
    }

    pub(crate) fn insert_and_override(&mut self, index: usize, mut item: Item) -> TrackInsertResult {
        const EPS: Seconds = 1e-9;
        item.clamp_to_active_available_range();

        let mut result = TrackInsertResult {
            success: true,
            ..Default::default()
        };

        let mut insert_index = index.min(self.items.len());
        let insert_start = self.start_time_of_item(insert_index);
        let insert_end = insert_start + item.duration().max(0.0);

        if item.duration() <= EPS {
            self.items.insert(insert_index, item);
            self.sanitize_preserving_all_gap_track();
            return result;
        }

        // If the insertion start falls strictly inside an item at insert_index, split at start
        if let Some(containing_idx) = self.get_item_at_time(insert_start) {
            let containing_start = self.start_time_of_item(containing_idx);
            if insert_start > containing_start + EPS && containing_idx <= insert_index {
                if let Some(split) = self.split_at_time(insert_start) {
                    result.split_clips.push(split);
                }
                // After split, the right piece is at containing_idx + 1; our insertion point is after the left piece
                if insert_index <= containing_idx {
                    insert_index = containing_idx + 1;
                } else {
                    insert_index += 1;
                }
            }
        }

        // Split at end boundary if it falls inside an item
        if self.get_item_at_time(insert_end).is_some() {
            if let Some(split) = self.split_at_time(insert_end) {
                result.split_clips.push(split);
            }
        }

        // After splitting at start and end, remove the exact range of items fully inside [insert_start, insert_end).
        let end_index = self
            .get_item_at_time(insert_end)
            .unwrap_or_else(|| self.items.len());
        if end_index > insert_index {
            let remove_count = end_index - insert_index;
            for _ in 0..remove_count {
                if let Item::Clip(clip) = &self.items[insert_index] {
                    result.deleted_clips.push(DeletedClipInfo {
                        clip_id: clip.get_id().unwrap_or_default(),
                        sync_clips_id: clip.sync_clips_id(),
                    });
                }
                self.items.remove(insert_index);
            }
        }

        self.items.insert(insert_index, item);
        self.sanitize_preserving_all_gap_track();
        result
    }

    /// Insert an item at a timeline time, controlling how overlaps are handled
    /// and how to place the item relative to neighbors.
    pub(crate) fn insert_at_time(
        &mut self,
        insert_time: Seconds,
        mut item: Item,
        overlap_policy: OverlapPolicy,
        insert_policy: InsertPolicy,
    ) -> TrackInsertResult {
        item.clamp_to_active_available_range();
        let mut effective_insert_time = insert_time;
        let total_track_duration = self.total_duration();

        if effective_insert_time < 0.0 {
            effective_insert_time = total_track_duration - effective_insert_time;
        }

        if effective_insert_time < 0.0 {
            panic!(
                "Negative insert start time ({}) is bigger than track duration ({})",
                insert_time, total_track_duration
            );
        }

        if effective_insert_time > total_track_duration {
            let gap_duration: Seconds = (effective_insert_time - total_track_duration).max(0.0);
            self.items
                .push(Item::Gap(crate::types::Gap::make_gap(gap_duration)));
            self.items.push(item);
            self.sanitize_preserving_all_gap_track();
            return TrackInsertResult {
                success: true,
                ..Default::default()
            };
        }

        let containing_index = self.get_item_at_time(effective_insert_time);

        let insert_index = self.get_insertion_index(effective_insert_time, insert_policy);

        let mut result = TrackInsertResult {
            success: true,
            ..Default::default()
        };

        let gap_fill_insert = containing_index.and_then(|i| {
            const EPS: Seconds = 1e-9;
            match self.items.get(i)? {
                Item::Gap(gap) => {
                    let gap_start = self.start_time_of_item(i);
                    let gap_end = gap_start + gap.source_range.duration.to_seconds().max(0.0);
                    let offset = effective_insert_time - gap_start;
                    if offset <= EPS {
                        return None;
                    }
                    let insert_end = effective_insert_time + item.duration().max(0.0);
                    if insert_end <= gap_end + EPS {
                        Some(())
                    } else {
                        None
                    }
                }
                _ => None,
            }
        });

        if let (InsertPolicy::SplitAndInsert, Some(_i)) = (insert_policy, containing_index) {
            // Create the boundary at the insertion time before inserting.
            if let Some(split) = self.split_at_time(effective_insert_time) {
                result.split_clips.push(split);
            }
        }

        // Push into interior gap space should consume the gap, not ripple later clips.
        let effective_overlap = if overlap_policy == OverlapPolicy::Push && gap_fill_insert.is_some() {
            OverlapPolicy::Override
        } else {
            overlap_policy
        };

        result.merge(self.insert_at_index(insert_index, item, effective_overlap));
        result
    }

    /// Compute the insertion index according to the policy without.
    fn get_insertion_index(&self, t: Seconds, policy: InsertPolicy) -> usize {
        let i = self.get_item_at_time(t).unwrap_or(self.items.len());

        // If t is at or beyond the end of the track, insert at the end for all policies.
        // This avoids out-of-bounds indexing when i == self.items.len().
        if i == self.items.len() {
            return self.items.len();
        }

        match policy {
            InsertPolicy::InsertBefore => i,
            InsertPolicy::InsertAfter => i + 1,
            InsertPolicy::InsertBeforeOrAfter => {
                let start = self.start_time_of_item(i);
                let end = start + self.items[i].duration().max(0.0);
                let d_start = (t - start).abs();
                let d_end = (end - t).abs();
                if d_start <= d_end {
                    i
                } else {
                    i + 1
                }
            }
            InsertPolicy::SplitAndInsert => {
                // If t is at the start boundary of item i, insert at i; otherwise insert after i.
                const EPS: Seconds = 1e-9;
                let start = self.start_time_of_item(i);
                if (t - start).abs() <= EPS {
                    i
                } else {
                    i + 1
                }
            }
        }
    }
}
