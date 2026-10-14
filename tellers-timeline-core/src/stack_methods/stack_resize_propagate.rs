//! After a resize: make every sync column that changed on the resized tracks
//! carry the same change on its partner tracks.
//!
//! The change is read from the clip itself (or its surviving fragment), never
//! from the overlap policy, so partners mirror what actually happened:
//!
//! - a column that moved with its length kept (pushed, or pulled back)
//!   ripples its partner tracks by the same delta at its position, once per
//!   track and position, so unrelated content after it on the partner track
//!   moves along as it did on the primary;
//! - a column that got shorter inside its old span (trimmed at either end) is
//!   trimmed in place on the partners, gaps filling what it left and the
//!   media offset following a head trim;
//! - a column that grew past its old end overrides what follows on the
//!   partners, as it did on the primary;
//! - a column wiped out entirely takes its partners with it, gaps staying
//!   where they were.

use std::collections::{HashMap, HashSet};

use super::{
    item_source_start, replace_track_range_with_item, resolve_sync_clips_id, set_item_source_start,
    SyncedClipState, EPS,
};
use crate::{Gap, InsertPolicy, Item, OverlapPolicy, Seconds, Stack};

/// What happened to one sync column on the resized track.
#[derive(Debug, Clone)]
struct ColumnChange {
    sync_clips_id: i64,
    /// Where the column started before the resize (orders the changes).
    primary_start: Seconds,
    start_delta: Seconds,
    old_duration: Seconds,
    new_duration: Seconds,
    /// How far the media offset moved (a head trim advances it).
    source_delta: Seconds,
}

impl ColumnChange {
    /// Shorter, and still within its old span: a trim at either end.
    fn is_trim_in_place(&self) -> bool {
        let new_end = self.start_delta + self.new_duration;
        self.start_delta >= -EPS
            && new_end <= self.old_duration + EPS
            && self.new_duration < self.old_duration - EPS
    }
}

/// Ripples already applied on each partner track: (from time, delta).
type AppliedRipples = HashMap<usize, Vec<(Seconds, Seconds)>>;

impl Stack {
    /// Entry point, run after every resize with the states captured before
    /// it. `modified_track_indices` are the tracks the resize touched;
    /// `excluded_ids` the clips the resize moved itself (they need no mirror).
    pub(super) fn sync_changed_groups_after_resize(
        &mut self,
        before_states: &HashMap<String, SyncedClipState>,
        modified_track_indices: &[usize],
        excluded_ids: &HashSet<String>,
    ) -> bool {
        let (changes, removed) =
            self.column_changes_after_resize(before_states, modified_track_indices, excluded_ids);
        for sync_clips_id in removed {
            self.remove_partners_of_column(sync_clips_id, before_states, modified_track_indices);
        }
        let mut applied = AppliedRipples::new();
        for change in changes {
            for (partner_id, before) in
                partners_of(before_states, change.sync_clips_id, modified_track_indices)
            {
                self.mirror_change_on_partner(&partner_id, &before, &change, &mut applied);
            }
        }
        true
    }

    /// Compare every synced clip on the resized tracks with its state before:
    /// the columns that changed (sorted by their former start) and the
    /// columns whose clip disappeared altogether.
    fn column_changes_after_resize(
        &self,
        before_states: &HashMap<String, SyncedClipState>,
        modified_track_indices: &[usize],
        excluded_ids: &HashSet<String>,
    ) -> (Vec<ColumnChange>, Vec<i64>) {
        let mut seen = HashSet::new();
        let mut changes = Vec::new();
        let mut removed = Vec::new();
        for (id, before) in before_states {
            if excluded_ids.contains(id) || !modified_track_indices.contains(&before.track_index) {
                continue;
            }
            let Some((track_index, item_index, item)) = self
                .get_item(id)
                .or_else(|| self.surviving_sync_fragment(before))
            else {
                if seen.insert(before.sync_clips_id) {
                    removed.push(before.sync_clips_id);
                }
                continue;
            };
            let start = self.children[track_index].start_time_of_item(item_index);
            let duration = item.duration().max(0.0);
            let start_delta = start - before.start;
            let changed = start_delta.abs() > EPS || (duration - before.duration).abs() > EPS;
            if changed && seen.insert(before.sync_clips_id) {
                changes.push(ColumnChange {
                    sync_clips_id: before.sync_clips_id,
                    primary_start: before.start,
                    start_delta,
                    old_duration: before.duration,
                    new_duration: duration,
                    source_delta: item_source_start(item) - before.source_start,
                });
            }
        }
        changes.sort_by(|a, b| a.primary_start.total_cmp(&b.primary_start));
        (changes, removed)
    }

    /// The fragment that survived an edit of a synced clip whose id is gone:
    /// a clip on the same track with the same sync id overlapping the clip's
    /// former range.
    fn surviving_sync_fragment(&self, before: &SyncedClipState) -> Option<(usize, usize, &Item)> {
        let track = self.children.get(before.track_index)?;
        let old_end = before.start + before.duration;
        let mut position: Seconds = 0.0;
        for (item_index, item) in track.items.iter().enumerate() {
            let item_start = position;
            let item_end = position + item.duration().max(0.0);
            position = item_end;
            let Item::Clip(clip) = item else {
                continue;
            };
            if resolve_sync_clips_id(&clip.metadata) != Some(before.sync_clips_id) {
                continue;
            }
            if item_end > before.start + EPS && item_start < old_end - EPS {
                return Some((before.track_index, item_index, item));
            }
        }
        None
    }

    /// Partners of a removed column leave a gap where they were, as the
    /// overridden clip did on the primary track.
    fn remove_partners_of_column(
        &mut self,
        sync_clips_id: i64,
        before_states: &HashMap<String, SyncedClipState>,
        modified_track_indices: &[usize],
    ) {
        for (partner_id, _) in partners_of(before_states, sync_clips_id, modified_track_indices) {
            let Some((track_index, item_index, _)) = self.get_item(&partner_id) else {
                continue;
            };
            self.children[track_index].delete_clip(item_index, true);
            self.children[track_index].sanitize_preserving_all_gap_track();
        }
    }

    /// Apply one column's change to one of its partner clips.
    fn mirror_change_on_partner(
        &mut self,
        partner_id: &str,
        before: &SyncedClipState,
        change: &ColumnChange,
        applied: &mut AppliedRipples,
    ) {
        let Some((track_index, _, _)) = self.get_item(partner_id) else {
            return;
        };
        if change.is_trim_in_place() {
            self.trim_partner_in_place(partner_id, change);
        } else {
            if change.start_delta.abs() > EPS {
                let start =
                    before.start + ripple_already_applied(applied, track_index, before.start);
                self.ripple_partner_track(track_index, start, change.start_delta);
                applied
                    .entry(track_index)
                    .or_default()
                    .push((before.start, change.start_delta));
            }
            self.resize_partner_length(partner_id, change);
            self.shift_partner_source(partner_id, change.source_delta);
        }
        self.children[track_index].sanitize_preserving_all_gap_track();
    }

    /// A trim at either end: gaps fill what the clip left, the media offset
    /// follows a head trim. Nothing else on the track moves.
    fn trim_partner_in_place(&mut self, partner_id: &str, change: &ColumnChange) {
        let Some((track_index, item_index, _)) = self.get_item(partner_id) else {
            return;
        };
        let trailing = change.old_duration - (change.start_delta + change.new_duration);
        if change.start_delta > EPS {
            // A leading gap moves the clip one slot to the right.
            self.resize_synced_clips_with_leading_gap(
                vec![(track_index, item_index)],
                change.start_delta,
                change.new_duration,
            );
            if trailing > EPS {
                self.children[track_index]
                    .items
                    .insert(item_index + 2, Item::Gap(Gap::make_gap(trailing)));
            }
        } else {
            self.resize_synced_clips_with_trailing_gap(
                vec![(track_index, item_index)],
                change.new_duration,
            );
            self.shift_partner_source(partner_id, change.source_delta);
        }
    }

    /// Move everything from `start` on by `delta` on a partner track: insert a
    /// gap (push) or close one (pull back).
    fn ripple_partner_track(&mut self, track_index: usize, start: Seconds, delta: Seconds) {
        let track = &mut self.children[track_index];
        if delta > 0.0 {
            track.insert_at_time(
                start,
                Item::Gap(Gap::make_gap(delta)),
                OverlapPolicy::Push,
                InsertPolicy::SplitAndInsert,
            );
        } else {
            track.delete_range((start + delta).max(0.0), start, false);
        }
    }

    /// Shorter fills with a gap in place; longer overrides what follows.
    fn resize_partner_length(&mut self, partner_id: &str, change: &ColumnChange) {
        let Some((track_index, item_index, item)) = self.get_item(partner_id) else {
            return;
        };
        if change.new_duration < change.old_duration - EPS {
            self.resize_synced_clips_with_trailing_gap(
                vec![(track_index, item_index)],
                change.new_duration,
            );
        } else if change.new_duration > change.old_duration + EPS {
            let start = self.children[track_index].start_time_of_item(item_index);
            let mut item = item.clone();
            item.set_duration(change.new_duration);
            replace_track_range_with_item(
                &mut self.children[track_index],
                start,
                start + change.new_duration,
                item,
            );
        }
    }

    /// Advance the partner's media offset like the primary's.
    fn shift_partner_source(&mut self, partner_id: &str, source_delta: Seconds) {
        if source_delta.abs() <= EPS {
            return;
        }
        let Some((track_index, item_index, _)) = self.get_item(partner_id) else {
            return;
        };
        let item = &mut self.children[track_index].items[item_index];
        let shifted = item_source_start(item) + source_delta;
        set_item_source_start(item, shifted);
    }
}

/// The clips of `sync_clips_id` that were not on a resized track, with their
/// state before the resize.
fn partners_of(
    before_states: &HashMap<String, SyncedClipState>,
    sync_clips_id: i64,
    modified_track_indices: &[usize],
) -> Vec<(String, SyncedClipState)> {
    before_states
        .iter()
        .filter(|(_, state)| {
            state.sync_clips_id == sync_clips_id
                && !modified_track_indices.contains(&state.track_index)
        })
        .map(|(id, state)| (id.clone(), state.clone()))
        .collect()
}

/// Sum of the ripples already applied on `track_index` at or before `time`.
fn ripple_already_applied(applied: &AppliedRipples, track_index: usize, time: Seconds) -> Seconds {
    applied
        .get(&track_index)
        .map(|gaps| {
            gaps.iter()
                .filter(|(from, _)| *from <= time + EPS)
                .map(|(_, delta)| *delta)
                .sum()
        })
        .unwrap_or(0.0)
}
