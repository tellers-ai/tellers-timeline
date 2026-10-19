use crate::{IdMetadataExt, Seconds, Track};

use super::track_item_insert::SplitClipInfo;

impl Track {
    /// Split the item at `split_time`. Returns split metadata when a clip was split.
    pub(crate) fn split_at_time(&mut self, split_time: Seconds) -> Option<SplitClipInfo> {
        const EPS: Seconds = 1e-9;

        let Some(item_index) = self.get_item_at_time(split_time) else {
            return None;
        };

        // Compute the offset from the start of the item
        let item_start_time = self.start_time_of_item(item_index);
        let local_offset = split_time - item_start_time;

        if local_offset <= EPS {
            return None;
        }

        // Move the item out to avoid borrow issues
        let original = self.items.remove(item_index);

        match original {
            crate::Item::Clip(mut clip) => {
                clip.clamp_to_active_available_range();
                let total = clip.source_range.duration.to_seconds().max(0.0);
                if local_offset >= total - EPS {
                    // Nothing to split, put the original back
                    self.items.insert(item_index, crate::Item::Clip(clip));
                    return None;
                }

                let left_duration = local_offset.max(0.0);
                let right_duration = (total - left_duration).max(0.0);
                let old_clip_id = clip.get_id().unwrap_or_default();
                let sync_clips_id = clip.sync_clips_id();

                let mut left_clip = clip.clone();
                left_clip
                    .source_range
                    .duration
                    .set_from_seconds(left_duration);

                // Right clip keeps the rest, media_start advances by left_duration
                clip.source_range.duration.set_from_seconds(right_duration);
                let right_source_start = clip.source_range.start_time.to_seconds() + left_duration;
                clip.source_range
                    .start_time
                    .set_from_seconds(right_source_start);

                // Each marker stays on the half whose source range holds its
                // start (source time is shared, so no time shift is needed).
                let split_source_time = right_source_start;
                left_clip
                    .markers
                    .retain(|m| m.start() < split_source_time - EPS);
                clip.markers.retain(|m| m.start() >= split_source_time - EPS);

                // Ensure the right-hand piece receives a fresh unique id
                crate::metadata::IdMetadataExt::set_id(
                    &mut clip,
                    Some(crate::types::gen_hex_id_12()),
                );
                let right_clip_id = clip.get_id();

                self.items.insert(item_index, crate::Item::Clip(left_clip));
                self.items.insert(item_index + 1, crate::Item::Clip(clip));

                if old_clip_id.is_empty() {
                    return None;
                }

                Some(SplitClipInfo {
                    old_clip_id,
                    left_clip_id: self.items[item_index].get_id(),
                    right_clip_id,
                    sync_clips_id,
                    split_time,
                })
            }
            crate::Item::Composite(mut stack) => {
                let total = stack.duration().max(0.0);
                if local_offset >= total - EPS {
                    self.items.insert(item_index, crate::Item::Composite(stack));
                    return None;
                }

                // Both halves play the same child tracks: the left one up to
                // the split, the right one from there (its source start
                // advances by the left part's length, like a clip's).
                let left_duration = local_offset.max(0.0);
                let right_duration = (total - left_duration).max(0.0);
                let right_source_start = stack.source_start() + left_duration;

                let mut left_stack = stack.clone();
                left_stack.set_duration(left_duration);
                stack.set_duration(right_duration);
                stack.set_source_start(right_source_start);

                left_stack
                    .markers
                    .retain(|m| m.start() < right_source_start - EPS);
                stack
                    .markers
                    .retain(|m| m.start() >= right_source_start - EPS);

                // The right-hand copy gets fresh ids, down to its nested items.
                let old_clip_id = stack.get_id().unwrap_or_default();
                stack.set_id(Some(crate::types::gen_hex_id_12()));
                stack.assign_new_child_ids();
                let right_clip_id = stack.get_id();

                self.items.insert(item_index, crate::Item::Composite(left_stack));
                self.items.insert(item_index + 1, crate::Item::Composite(stack));

                if old_clip_id.is_empty() {
                    return None;
                }

                Some(SplitClipInfo {
                    old_clip_id,
                    left_clip_id: self.items[item_index].get_id(),
                    right_clip_id,
                    sync_clips_id: None,
                    split_time,
                })
            }
            crate::Item::Gap(mut gap) => {
                let total = gap.source_range.duration.to_seconds().max(0.0);
                if local_offset >= total - EPS {
                    self.items.insert(item_index, crate::Item::Gap(gap));
                    return None;
                }

                let left_duration = local_offset.max(0.0);
                let right_duration = (total - left_duration).max(0.0);

                let mut left_gap = gap.clone();
                left_gap
                    .source_range
                    .duration
                    .set_from_seconds(left_duration);

                gap.source_range.duration.set_from_seconds(right_duration);
                // The right gap restarts at the gap's source start, so its
                // markers move back by the left part's length.
                let gap_source_start = gap.source_range.start_time.to_seconds();
                let split_source_time = gap_source_start + left_duration;
                left_gap
                    .markers
                    .retain(|m| m.start() < split_source_time - EPS);
                gap.markers.retain(|m| m.start() >= split_source_time - EPS);
                for m in &mut gap.markers {
                    let start = m.start() - left_duration;
                    m.marked_range.set_start_time(start);
                }
                crate::metadata::IdMetadataExt::set_id(
                    &mut gap,
                    Some(crate::types::gen_hex_id_12()),
                );

                self.items.insert(item_index, crate::Item::Gap(left_gap));
                self.items.insert(item_index + 1, crate::Item::Gap(gap));
                None
            }
            // `get_item_at_time` never lands on a zero-duration transition, so
            // this arm is unreachable in practice; restore the item to be safe.
            crate::Item::Transition(transition) => {
                self.items
                    .insert(item_index, crate::Item::Transition(transition));
                None
            }
        }
    }
}
