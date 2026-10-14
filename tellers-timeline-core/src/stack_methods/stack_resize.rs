//! Resizing an item: change its length and, optionally, where it starts.
//!
//! A synced clip is resized together with every clip of its column. The
//! edit lands on the primary track first, in one of three ways depending on
//! where the new start falls, then [`Stack::sync_changed_groups_after_resize`]
//! mirrors whatever happened on the partner tracks of every other column
//! the edit touched.

use std::collections::HashSet;

use super::{replace_track_range_with_item, resize_effective_duration, resolve_sync_clips_id, EPS};
use crate::{Gap, IdMetadataExt, InsertPolicy, Item, OverlapPolicy, Seconds, Stack};

/// One resize, resolved: the clips to resize (the selected clip and its sync
/// column), their common new length, how far the start moves, and how
/// overlaps are handled.
struct ResizeEdit {
    target_ids: Vec<String>,
    duration: Seconds,
    start_delta: Seconds,
    overlap_policy: OverlapPolicy,
    clamp_to_media: bool,
}

impl ResizeEdit {
    /// A copy of `item` at the edit's length.
    fn resized_copy(&self, item: &Item) -> Item {
        let mut item = item.clone();
        item.set_duration(self.duration);
        if self.clamp_to_media {
            item.clamp_to_active_available_range();
        }
        item
    }
}

/// Position of a target before the edit.
struct TargetSpan {
    track_index: usize,
    item_index: usize,
    start: Seconds,
    end: Seconds,
}

impl Stack {
    /// Resize the item with `item_id` to start at `new_start_time` and last
    /// `new_duration`. A synced clip takes its whole column along. Returns
    /// false, with the stack unchanged, when the item is unknown or the edit
    /// cannot be applied.
    pub fn resize_item(
        &mut self,
        item_id: &str,
        new_start_time: Seconds,
        new_duration: Seconds,
        overlap_policy: OverlapPolicy,
        clamp_to_media: bool,
    ) -> bool {
        let Some((track_index, item_index, item)) = self.get_item(item_id) else {
            return false;
        };
        let before_states = self.synced_clip_states();
        if matches!(item, Item::Gap(_)) {
            return self.resize_gap(track_index, item_index, new_duration, &before_states);
        }

        let target_ids = self.resize_target_ids(item_id, item);
        let excluded_ids: HashSet<String> = target_ids.iter().cloned().collect();
        let edit = ResizeEdit {
            duration: self.effective_resize_duration(&target_ids, new_duration, clamp_to_media),
            start_delta: new_start_time - self.children[track_index].start_time_of_item(item_index),
            target_ids,
            overlap_policy,
            clamp_to_media,
        };

        let backup = self.clone();
        let modified_tracks = if edit.start_delta.abs() <= EPS {
            self.resize_keeping_start(&edit)
        } else if edit.start_delta < 0.0 {
            self.resize_starting_earlier(&edit)
        } else {
            self.resize_starting_later(&edit)
        };
        let Some(modified_tracks) = modified_tracks else {
            *self = backup;
            return false;
        };
        if !self.sync_changed_groups_after_resize(&before_states, &modified_tracks, &excluded_ids) {
            *self = backup;
            return false;
        }
        self.sanitize_preserving_all_gap_tracks();
        true
    }

    /// A gap just changes length; what follows on its track moves, and the
    /// columns that moved are mirrored on their partners.
    fn resize_gap(
        &mut self,
        track_index: usize,
        item_index: usize,
        new_duration: Seconds,
        before_states: &std::collections::HashMap<String, super::SyncedClipState>,
    ) -> bool {
        let backup = self.clone();
        let Some(item) = self
            .children
            .get_mut(track_index)
            .and_then(|track| track.items.get_mut(item_index))
        else {
            return false;
        };
        item.set_duration(new_duration.max(0.0));
        self.children[track_index].sanitize_preserving_all_gap_track();
        if !self.sync_changed_groups_after_resize(before_states, &[track_index], &HashSet::new()) {
            *self = backup;
            return false;
        }
        self.sanitize_preserving_all_gap_tracks();
        true
    }

    /// The clip and, when it is synced, every clip of its column.
    fn resize_target_ids(&self, item_id: &str, item: &Item) -> Vec<String> {
        let Item::Clip(clip) = item else {
            return vec![item_id.to_string()];
        };
        resolve_sync_clips_id(&clip.metadata)
            .map(|sync_clips_id| {
                self.synced_clips_targets(sync_clips_id)
                    .into_iter()
                    .filter_map(|(track_index, item_index)| {
                        self.children
                            .get(track_index)
                            .and_then(|track| track.items.get(item_index))
                            .and_then(Item::get_id)
                    })
                    .collect()
            })
            .filter(|ids: &Vec<String>| ids.len() > 1)
            .unwrap_or_else(|| vec![item_id.to_string()])
    }

    /// The length every target can take: the requested one, capped by each
    /// target's media when clamping.
    fn effective_resize_duration(
        &self,
        target_ids: &[String],
        new_duration: Seconds,
        clamp_to_media: bool,
    ) -> Seconds {
        target_ids
            .iter()
            .filter_map(|id| {
                self.get_item(id).map(|(_, _, item)| {
                    resize_effective_duration(item, new_duration, clamp_to_media)
                })
            })
            .fold(new_duration.max(0.0), Seconds::min)
    }

    /// Where each target sits right now. `None` when one is missing.
    fn target_spans(&self, target_ids: &[String]) -> Option<Vec<TargetSpan>> {
        target_ids
            .iter()
            .map(|id| {
                let (track_index, item_index, item) = self.get_item(id)?;
                let start = self.children[track_index].start_time_of_item(item_index);
                Some(TargetSpan {
                    track_index,
                    item_index,
                    start,
                    end: start + item.duration().max(0.0),
                })
            })
            .collect()
    }

    /// Same start, new length. Growing with Override overrides what follows
    /// (when every target is a clip that grows); otherwise the length is set
    /// in place and what follows moves: a shrink closes up, a Push growth
    /// pushes.
    fn resize_keeping_start(&mut self, edit: &ResizeEdit) -> Option<Vec<usize>> {
        let spans = self.target_spans(&edit.target_ids)?;
        if edit.overlap_policy == OverlapPolicy::Override {
            let all_clips_grow = spans.iter().all(|span| {
                let item = &self.children[span.track_index].items[span.item_index];
                matches!(item, Item::Clip(_)) && edit.duration > item.duration().max(0.0) + EPS
            });
            if all_clips_grow {
                for span in &spans {
                    let item =
                        edit.resized_copy(&self.children[span.track_index].items[span.item_index]);
                    replace_track_range_with_item(
                        &mut self.children[span.track_index],
                        span.start,
                        span.start + edit.duration,
                        item,
                    );
                }
                return Some(modified_tracks(&spans));
            }
        }
        for span in &spans {
            let track = &mut self.children[span.track_index];
            let item = track.items.get_mut(span.item_index)?;
            item.set_duration(edit.duration);
            if edit.clamp_to_media {
                item.clamp_to_active_available_range();
            }
            track.sanitize_preserving_all_gap_track();
        }
        Some(modified_tracks(&spans))
    }

    /// The clip starts earlier: it is lifted (a gap keeps every position),
    /// then overrides `[new start, old end]`, cutting into what precedes it.
    fn resize_starting_earlier(&mut self, edit: &ResizeEdit) -> Option<Vec<usize>> {
        let spans = self.target_spans(&edit.target_ids)?;
        let copies: Vec<Item> = spans
            .iter()
            .map(|span| edit.resized_copy(&self.children[span.track_index].items[span.item_index]))
            .collect();
        // Lift from the last item down so earlier indices stay valid.
        let mut lift: Vec<&TargetSpan> = spans.iter().collect();
        lift.sort_by(|a, b| (b.track_index, b.item_index).cmp(&(a.track_index, a.item_index)));
        for span in lift {
            let track = &mut self.children[span.track_index];
            if span.item_index >= track.items.len() {
                return None;
            }
            track.items.remove(span.item_index);
            track.items.insert(
                span.item_index,
                Item::Gap(Gap::make_gap(span.end - span.start)),
            );
        }
        for (span, item) in spans.iter().zip(copies) {
            replace_track_range_with_item(
                &mut self.children[span.track_index],
                span.start + edit.start_delta,
                span.end,
                item,
            );
        }
        Some(modified_tracks(&spans))
    }

    /// The clip starts later. Within its old span (a trim from the left) it
    /// is replaced in place: a gap, the shorter clip, and a gap for whatever
    /// is left at the end; nothing else moves. Past its old end it is lifted
    /// and lands with the overlap policy: Override cuts into what follows,
    /// Push moves it.
    fn resize_starting_later(&mut self, edit: &ResizeEdit) -> Option<Vec<usize>> {
        let spans = self.target_spans(&edit.target_ids)?;
        for span in &spans {
            let track = &mut self.children[span.track_index];
            let item = edit.resized_copy(&track.items[span.item_index]);
            let new_start = span.start + edit.start_delta;
            let new_end = new_start + item.duration().max(0.0);
            // Later targets on the same track keep their index only when the
            // item count does not change before them, so re-find the clip.
            let Some(id) = item.get_id() else {
                return None;
            };
            let item_index = track.get_item_by_id(&id)?.0;
            if new_end <= span.end + EPS {
                track.items.remove(item_index);
                let trailing = span.end - new_end;
                if trailing > EPS {
                    track
                        .items
                        .insert(item_index, Item::Gap(Gap::make_gap(trailing)));
                }
                track.items.insert(item_index, item);
                track
                    .items
                    .insert(item_index, Item::Gap(Gap::make_gap(edit.start_delta)));
                track.sanitize_preserving_all_gap_track();
            } else if edit.overlap_policy == OverlapPolicy::Override {
                track.items.remove(item_index);
                track
                    .items
                    .insert(item_index, Item::Gap(Gap::make_gap(span.end - span.start)));
                replace_track_range_with_item(track, new_start, new_end, item);
            } else {
                track.items.remove(item_index);
                track.sanitize_preserving_all_gap_track();
                track.insert_at_time(
                    new_start,
                    item,
                    edit.overlap_policy,
                    InsertPolicy::SplitAndInsert,
                );
            }
        }
        Some(modified_tracks(&spans))
    }
}

/// The distinct tracks of `spans`, ascending.
fn modified_tracks(spans: &[TargetSpan]) -> Vec<usize> {
    let mut tracks: Vec<usize> = spans.iter().map(|span| span.track_index).collect();
    tracks.sort_unstable();
    tracks.dedup();
    tracks
}
