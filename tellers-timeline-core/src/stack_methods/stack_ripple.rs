//! Ripple edits that keep every sync column aligned.
//!
//! A push (insert) or a pull (ripple delete) at a time shifts what follows on
//! the edited tracks. Every other track holding a clip of a sync column that
//! shifted must shift by the same amount at the same time, whether or not it
//! is associated with the column's owner; the tracks it shifts in turn carry
//! their own sync columns along, so the shift follows the links across the
//! stack. A ripple only ever adds or removes gap time on those tracks: a clip
//! is never cut, and an edit that would need to cut one is refused.

use std::collections::{HashMap, HashSet};

use crate::{Gap, IdMetadataExt, InsertPolicy, Item, OverlapPolicy, Seconds, Stack};

use super::EPS;

/// Spans removed per track by [`Stack::ripple_remove_items`], in the times
/// the stack had before the removal.
#[derive(Debug, Default)]
pub(super) struct RippleSpans(HashMap<usize, Vec<(Seconds, Seconds)>>);

impl RippleSpans {
    /// Where `time` (before the removal) is on `track_index` after it: earlier
    /// by every span removed before it; a time inside a removed span lands on
    /// the span's start.
    pub(super) fn map_time(&self, track_index: usize, time: Seconds) -> Seconds {
        let Some(spans) = self.0.get(&track_index) else {
            return time;
        };
        let mut removed = 0.0;
        for &(start, duration) in spans {
            if time >= start + duration - EPS {
                removed += duration;
            } else if time > start {
                removed += time - start;
            }
        }
        time - removed
    }

    /// Tracks that rippled.
    pub(super) fn contains(&self, track_index: usize) -> bool {
        self.0.contains_key(&track_index)
    }
}

impl Stack {
    /// Sync ids of the clips starting at or after `time` on a track.
    pub(super) fn sync_ids_starting_at_or_after(
        &self,
        track_index: usize,
        time: Seconds,
    ) -> Vec<i64> {
        let Some(track) = self.children.get(track_index) else {
            return Vec::new();
        };
        let mut ids = Vec::new();
        let mut pos = 0.0;
        for item in &track.items {
            if pos >= time - EPS {
                if let Item::Clip(clip) = item {
                    if let Some(sync_id) = clip.sync_clips_id() {
                        ids.push(sync_id);
                    }
                }
            }
            pos += item.duration().max(0.0);
        }
        ids
    }

    /// Push every track holding a clip of the `pending` sync columns, other
    /// than `pushed_tracks` (already pushed), by `duration` at `start`, and
    /// follow the sync columns on the tracks this pushes.
    /// Clips in `placed_ids` (a column just placed at `start` without pushing)
    /// stay where they are: the push goes in after them.
    pub(super) fn push_sync_partner_tracks(
        &mut self,
        start: Seconds,
        duration: Seconds,
        mut pushed_tracks: HashSet<usize>,
        mut pending: Vec<i64>,
        placed_ids: &HashSet<String>,
        mut seen_ids: HashSet<i64>,
    ) -> bool {
        if duration <= EPS {
            return true;
        }
        let mut used_ids = self.collect_timeline_ids();
        while let Some(sync_id) = pending.pop() {
            if !seen_ids.insert(sync_id) {
                continue;
            }
            for (track_index, _) in self.synced_clips_targets(sync_id) {
                if !pushed_tracks.insert(track_index) {
                    continue;
                }
                pending.extend(self.sync_ids_starting_at_or_after(track_index, start));
                if !self.push_track_at(track_index, start, duration, placed_ids, &mut used_ids) {
                    return false;
                }
            }
        }
        true
    }

    /// Start of every clip at or after `time`, by id, with its track.
    pub(super) fn clip_starts_from(&self, time: Seconds) -> HashMap<String, (usize, Seconds)> {
        let mut starts = HashMap::new();
        for (track_index, track) in self.children.iter().enumerate() {
            let mut pos = 0.0;
            for item in &track.items {
                if pos >= time - EPS && matches!(item, Item::Clip(_) | Item::Composite(_)) {
                    if let Some(id) = item.get_id() {
                        starts.insert(id, (track_index, pos));
                    }
                }
                pos += item.duration().max(0.0);
            }
        }
        starts
    }

    /// After placing the column `placed_ids` (sync id `own_sync_id`) at
    /// `start`, push the partners of every clip the placement shifted by
    /// `duration` (compared with `before`, from [`Self::clip_starts_from`]).
    pub(super) fn push_partners_of_shifted_clips(
        &mut self,
        before: &HashMap<String, (usize, Seconds)>,
        start: Seconds,
        duration: Seconds,
        placed_ids: &HashSet<String>,
        own_sync_id: Option<i64>,
    ) -> bool {
        let mut pushed_tracks = HashSet::new();
        let mut pending = Vec::new();
        for (track_index, track) in self.children.iter().enumerate() {
            let mut pos = 0.0;
            for item in &track.items {
                let id = item
                    .get_id()
                    .filter(|_| !matches!(item, Item::Gap(_) | Item::Transition(_)));
                if let Some(id) = id {
                    let shifted = before.get(&id).is_some_and(|&(old_track, old_start)| {
                        old_track == track_index && (pos - old_start - duration).abs() <= EPS
                    });
                    if shifted {
                        pushed_tracks.insert(track_index);
                        if let Some(sync_id) = crate::item_link_group_id(item) {
                            pending.push(sync_id);
                        }
                    }
                }
                pos += item.duration().max(0.0);
            }
        }
        self.push_sync_partner_tracks(
            start,
            duration,
            pushed_tracks,
            pending,
            placed_ids,
            own_sync_id.into_iter().collect(),
        )
    }

    /// Shift everything on a track from `time` on by `duration`, adding gap
    /// time. A clip straddling `time` stays whole: the gap goes in after it.
    fn push_track_at(
        &mut self,
        track_index: usize,
        time: Seconds,
        duration: Seconds,
        placed_ids: &HashSet<String>,
        used_ids: &mut HashSet<String>,
    ) -> bool {
        let track = &self.children[track_index];
        let mut at = Self::boundary_at_or_after(track, time);
        if let Some(index) = track.get_item_at_time(at) {
            if track.items[index]
                .get_id()
                .is_some_and(|id| placed_ids.contains(&id))
            {
                at += track.items[index].duration().max(0.0);
            }
        }
        if at >= track.total_duration() - EPS {
            return true;
        }
        let mut gap = Item::Gap(Gap::make_gap(duration));
        Self::ensure_unique_item_id(&mut gap, used_ids);
        self.children[track_index]
            .insert_at_time(at, gap, OverlapPolicy::Push, InsertPolicy::SplitAndInsert)
            .success
    }

    /// `time`, or the end of the clip straddling it.
    fn boundary_at_or_after(track: &crate::Track, time: Seconds) -> Seconds {
        match track.get_item_at_time(time) {
            Some(index) if !matches!(track.items[index], Item::Gap(_) | Item::Transition(_)) => {
                let start = track.start_time_of_item(index);
                if time - start <= EPS {
                    start
                } else {
                    start + track.items[index].duration()
                }
            }
            _ => time,
        }
    }

    /// Ripple-delete a column: remove the items at `targets` (all starting at
    /// the same time with the same duration) without leaving gaps, and pull
    /// every track holding a later sync clip linked to what shifted by the same
    /// duration. Returns the removed items (in target order) and the spans
    /// removed per track, or `None`, with the stack unchanged, when a track
    /// would need a clip cut to stay aligned.
    pub(super) fn ripple_remove_items(
        &mut self,
        targets: &[(usize, usize)],
    ) -> Option<(Vec<(usize, Item)>, RippleSpans)> {
        let &(first_track, first_item) = targets.first()?;
        let start = self
            .children
            .get(first_track)?
            .start_time_of_item(first_item);
        let duration = self.children[first_track]
            .items
            .get(first_item)?
            .duration()
            .max(0.0);
        let mut own_ids = HashSet::new();
        for &(track_index, item_index) in targets {
            let track = self.children.get(track_index)?;
            let item = track.items.get(item_index)?;
            if (track.start_time_of_item(item_index) - start).abs() > EPS
                || (item.duration().max(0.0) - duration).abs() > EPS
            {
                return None;
            }
            if let Item::Clip(clip) = item {
                own_ids.extend(clip.sync_clips_id());
            }
        }
        let backup = self.clone();
        let mut pending: Vec<i64> = Vec::new();
        for &(track_index, _) in targets {
            pending.extend(self.sync_ids_starting_at_or_after(track_index, start + duration));
        }
        let mut sorted: Vec<_> = targets.to_vec();
        sorted.sort_by(|a, b| b.cmp(a));
        sorted.dedup();
        let mut removed = Vec::new();
        let mut spans = RippleSpans::default();
        for (track_index, item_index) in sorted {
            removed.push((
                track_index,
                self.children[track_index].items.remove(item_index),
            ));
            spans.0.insert(track_index, vec![(start, duration)]);
        }
        removed.reverse();
        if duration <= EPS {
            return Some((removed, spans));
        }
        let mut seen_ids = own_ids.clone();
        while let Some(sync_id) = pending.pop() {
            if !seen_ids.insert(sync_id) {
                continue;
            }
            for (track_index, _) in self.synced_clips_targets(sync_id) {
                if spans.contains(track_index) {
                    continue;
                }
                pending.extend(
                    self.sync_ids_starting_at_or_after(track_index, start)
                        .into_iter()
                        .filter(|id| !own_ids.contains(id)),
                );
                let Some(track_spans) = self.pull_track_at(track_index, start, duration) else {
                    *self = backup;
                    return None;
                };
                spans.0.insert(track_index, track_spans);
            }
        }
        Some((removed, spans))
    }

    /// Take `duration` of gap time out of a track from `time` on (after the
    /// clip straddling it), skipping over unsynced clips. `None` when a sync
    /// clip comes before enough gap time: it would not shift with its column.
    fn pull_track_at(
        &mut self,
        track_index: usize,
        time: Seconds,
        duration: Seconds,
    ) -> Option<Vec<(Seconds, Seconds)>> {
        let track = &mut self.children[track_index];
        let at = Self::boundary_at_or_after(track, time);
        let mut spans = Vec::new();
        let mut remaining = duration;
        let mut pos = 0.0;
        let mut index = 0;
        while index < track.items.len() && remaining > EPS {
            let item_duration = track.items[index].duration().max(0.0);
            if pos + item_duration <= at + EPS {
                pos += item_duration;
                index += 1;
                continue;
            }
            match &mut track.items[index] {
                Item::Gap(gap) => {
                    let take = remaining.min(item_duration);
                    spans.push((pos.max(at), take));
                    remaining -= take;
                    pos += item_duration;
                    if item_duration - take <= EPS {
                        track.items.remove(index);
                    } else {
                        gap.source_range
                            .duration
                            .set_from_seconds(item_duration - take);
                        index += 1;
                    }
                }
                Item::Clip(clip) if clip.sync_clips_id().is_some() => return None,
                Item::Clip(_) | Item::Composite(_) => {
                    pos += item_duration;
                    index += 1;
                }
                // Takes no time: nothing to free, step over it.
                Item::Transition(_) => {
                    index += 1;
                }
            }
        }
        Some(spans)
    }
}
