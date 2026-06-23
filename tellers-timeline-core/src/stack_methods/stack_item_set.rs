use crate::{IdMetadataExt, InsertPolicy, Item, OverlapPolicy, Seconds, Stack};
use std::collections::HashSet;

const EPS: Seconds = 1e-9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClampPolicy {
    /// Allow a gap to form between clips.
    ReplaceGap,
    /// Clamp our edge at the neighboring clip boundary so no gap forms.
    /// The neighboring clip is not modified.
    ClampByPulling,
}

impl Stack {
    /// Move the left edge of an item; the right edge stays fixed so duration adjusts.
    /// `clamp_policy` applies only when `new_start_time > old_start` and the previous
    /// neighbor is a clip (not a gap).
    ///
    /// For gaps:
    ///   - new_start < old_start: insert the enlarged gap at new_start with Override,
    ///     consuming whatever is to the left.
    ///   - new_start > old_start: equivalent to shrinking the gap's duration; the following
    ///     item is pushed left (delete-collapse + re-insert with Push).
    pub fn set_item_start_time(
        &mut self,
        item_id: &str,
        new_start_time: Seconds,
        overlap_policy: OverlapPolicy,
        clamp_policy: ClampPolicy,
    ) -> bool {
        let Some((track_index, item_index, _)) = self.get_item(item_id) else {
            return false;
        };
        let old_start = self.children[track_index].start_time_of_item(item_index);
        let old_duration = self.children[track_index].items[item_index].duration().max(0.0);
        let old_end = old_start + old_duration;
        if matches!(self.children[track_index].items[item_index], Item::Gap(_)) {
            unimplemented!("set_item_start_time on gaps is not yet implemented");
        }

        // Clamping only applies to clips moving right away from a neighbouring clip.
        let effective_start = if new_start_time > old_start + EPS
            && clamp_policy == ClampPolicy::ClampByPulling
            && item_index > 0
            && matches!(self.children[track_index].items[item_index - 1], Item::Clip(_))
        {
            let prev_end = self.children[track_index].start_time_of_item(item_index - 1)
                + self.children[track_index].items[item_index - 1]
                    .duration()
                    .max(0.0);
            new_start_time.min(prev_end)
        } else {
            new_start_time
        };

        let start_delta = effective_start - old_start;
        let new_duration = (old_end - effective_start).max(0.0);

        let targets = self.synced_clip_targets_for_item(item_id);
        let excluded_ids: HashSet<_> = targets
            .iter()
            .filter_map(|(ti, ii)| self.children[*ti].items[*ii].get_id().map(String::from))
            .collect();

        let backup = self.clone();
        let before_states = self.synced_clip_states();
        let mut modified_track_indices = Vec::new();

        for (ti, ii) in targets {
            let track_old_start = self.children[ti].start_time_of_item(ii);
            let track_new_start = track_old_start + start_delta;
            let mut new_item = self.children[ti].items[ii].clone();
            new_item.set_duration(new_duration);
            let track = &mut self.children[ti];
            if track.delete_clip_at(ii, true).is_none() {
                *self = backup;
                return false;
            }
            track.insert_at_time(
                track_new_start,
                new_item,
                OverlapPolicy::Override,
                InsertPolicy::SplitAndInsert,
            );
            modified_track_indices.push(ti);
        }

        modified_track_indices.sort_unstable();
        modified_track_indices.dedup();
        if !self.sync_changed_groups_after_resize(
            &before_states,
            &modified_track_indices,
            &excluded_ids,
            overlap_policy,
        ) {
            *self = backup;
            return false;
        }
        self.sanitize_preserving_all_gap_tracks();
        true
    }

    /// Change the duration of an item; the left edge stays fixed.
    /// `clamp_policy` applies only when `new_duration < old_duration` and the following
    /// neighbor is a clip (not a gap).
    ///
    /// For gaps: delete-collapse then re-insert with Push so the following item is pushed
    /// right (growing) or left (shrinking) by the duration difference.
    pub fn set_item_duration(
        &mut self,
        item_id: &str,
        new_duration: Seconds,
        overlap_policy: OverlapPolicy,
        clamp_policy: ClampPolicy,
    ) -> bool {
        let Some((track_index, item_index, _)) = self.get_item(item_id) else {
            return false;
        };
        if matches!(self.children[track_index].items[item_index], Item::Gap(_)) {
            unimplemented!("set_item_duration on gaps is not yet implemented");
        }

        let old_start = self.children[track_index].start_time_of_item(item_index);
        let old_duration = self.children[track_index].items[item_index].duration().max(0.0);

        let effective_duration = if new_duration < old_duration - EPS
            && clamp_policy == ClampPolicy::ClampByPulling
        {
            let next_index = item_index + 1;
            if next_index < self.children[track_index].items.len()
                && matches!(self.children[track_index].items[next_index], Item::Clip(_))
            {
                let next_start = self.children[track_index].start_time_of_item(next_index);
                new_duration.max(next_start - old_start)
            } else {
                new_duration
            }
        } else {
            new_duration
        };

        let targets = self.synced_clip_targets_for_item(item_id);
        let excluded_ids: HashSet<_> = targets
            .iter()
            .filter_map(|(ti, ii)| self.children[*ti].items[*ii].get_id().map(String::from))
            .collect();

        let backup = self.clone();
        let before_states = self.synced_clip_states();
        let mut modified_track_indices = Vec::new();

        for (ti, ii) in targets {
            let track_old_start = self.children[ti].start_time_of_item(ii);
            let mut new_item = self.children[ti].items[ii].clone();
            new_item.set_duration(effective_duration);
            let track = &mut self.children[ti];
            if track.delete_clip_at(ii, true).is_none() {
                *self = backup;
                return false;
            }
            track.insert_at_time(
                track_old_start,
                new_item,
                OverlapPolicy::Override,
                InsertPolicy::SplitAndInsert,
            );
            modified_track_indices.push(ti);
        }

        modified_track_indices.sort_unstable();
        modified_track_indices.dedup();
        if !self.sync_changed_groups_after_resize(
            &before_states,
            &modified_track_indices,
            &excluded_ids,
            overlap_policy,
        ) {
            *self = backup;
            return false;
        }
        self.sanitize_preserving_all_gap_tracks();
        true
    }
}
