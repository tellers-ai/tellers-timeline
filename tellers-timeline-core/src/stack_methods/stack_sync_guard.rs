//! An edit never puts a sync column out of step.
//!
//! The clips of one sync column (same Link Group ID) start together on every
//! track they sit on: a video and its audio channels play from the same
//! instant. (A channel may be shorter than the video, so only starts are
//! compared.) A clip trimmed on its own for a J or L cut starts its Sync
//! Offset away from the column, and is compared at `start - offset`. The edit entry points run through [`Stack::keeping_sync_columns`],
//! which undoes an edit that leaves an aligned column out of step, so a missed case
//! in the move/resize/insert logic is refused instead of saved.

use std::collections::{HashMap, HashSet};

use super::resolve_sync_clips_id;
use crate::metadata::{resolve_sync_offset, set_sync_offset};
use crate::{IdMetadataExt, Item, Seconds, Stack, TrackKind};

/// Two times this close are the same instant.
const TIME_EPS: Seconds = 1e-6;

impl Stack {
    /// The sync clips ids whose clips don't start at the same times on every
    /// track they sit on.
    pub(crate) fn misaligned_sync_clips_ids(&self) -> HashSet<i64> {
        let mut starts: HashMap<i64, HashMap<usize, Vec<Seconds>>> = HashMap::new();
        for (track_index, track) in self.children.iter().enumerate() {
            let mut start = 0.0;
            for item in &track.items {
                let duration = item.duration().max(0.0);
                if let Item::Clip(clip) = item {
                    if let Some(sync_clips_id) = resolve_sync_clips_id(&clip.metadata) {
                        // A clip trimmed on its own (J/L cut) is in step
                        // `offset` away from its column.
                        let offset = resolve_sync_offset(&clip.metadata);
                        starts
                            .entry(sync_clips_id)
                            .or_default()
                            .entry(track_index)
                            .or_default()
                            .push(start - offset);
                    }
                }
                start += duration;
            }
        }
        let same = |a: &[Seconds], b: &[Seconds]| {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() <= TIME_EPS)
        };
        starts
            .into_iter()
            .filter_map(|(sync_clips_id, per_track)| {
                let mut tracks = per_track.into_values().map(|mut v| {
                    v.sort_by(|a, b| a.total_cmp(b));
                    v
                });
                let first = tracks.next()?;
                tracks
                    .any(|other| !same(&first, &other))
                    .then_some(sync_clips_id)
            })
            .collect()
    }

    /// Run `edit`; when it leaves a sync column out of step across its tracks
    /// that was aligned before, put the stack back and return `refused`. Columns
    /// already out of step (an older project) don't block an edit.
    pub(crate) fn keeping_sync_columns<R>(
        &mut self,
        refused: R,
        edit: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let before = self.misaligned_sync_clips_ids();
        let backup = self.clone();
        let result = edit(self);
        let after = self.misaligned_sync_clips_ids();
        if after.is_subset(&before) {
            return result;
        }
        let newly: HashSet<i64> = after.difference(&before).copied().collect();
        if self.offset_head_trimmed_clips(&backup, &newly)
            && self.misaligned_sync_clips_ids().is_subset(&before)
        {
            return result;
        }
        *self = backup;
        refused
    }

    /// In columns `sync_clips_ids`, record as its Sync Offset the start move
    /// of each clip whose head was trimmed (its source start moved with it,
    /// as when a clip dropped over it overwrote its head): the clip still
    /// plays in step with its partners, now a split edit. Each column's
    /// offsets are then taken from its video (else its first clip), which
    /// stays at offset 0. True when any clip was given an offset.
    fn offset_head_trimmed_clips(&mut self, before: &Stack, sync_clips_ids: &HashSet<i64>) -> bool {
        let mut trimmed = Vec::new();
        for (track_index, track) in self.children.iter().enumerate() {
            let mut start = 0.0;
            for (item_index, item) in track.items.iter().enumerate() {
                let item_start = start;
                start += item.duration().max(0.0);
                let Item::Clip(clip) = item else {
                    continue;
                };
                if !resolve_sync_clips_id(&clip.metadata)
                    .is_some_and(|id| sync_clips_ids.contains(&id))
                {
                    continue;
                }
                let Some((bt, bi, Item::Clip(old))) =
                    clip.get_id().and_then(|id| before.get_item(&id))
                else {
                    continue;
                };
                let moved = item_start - before.children[bt].start_time_of_item(bi);
                if moved.abs() <= TIME_EPS || clip.is_reversed() {
                    continue;
                }
                let source_moved = clip.source_range.start_time.to_seconds()
                    - old.source_range.start_time.to_seconds();
                let expected = if moved > 0.0 {
                    old.source_offset(moved)
                } else {
                    -clip.source_offset(-moved)
                };
                if (source_moved - expected).abs() <= TIME_EPS {
                    trimmed.push((track_index, item_index, moved));
                }
            }
        }
        if trimmed.is_empty() {
            return false;
        }
        for &(track_index, item_index, moved) in &trimmed {
            if let Item::Clip(clip) = &mut self.children[track_index].items[item_index] {
                let offset = resolve_sync_offset(&clip.metadata);
                set_sync_offset(&mut clip.metadata, offset + moved);
            }
        }
        for &sync_clips_id in sync_clips_ids {
            let mut members = self.synced_clips_targets(sync_clips_id);
            members.sort_by_key(|&(ti, _)| self.children[ti].kind != TrackKind::Video);
            let Some(&(ti, ii)) = members.first() else {
                continue;
            };
            let Item::Clip(anchor) = &self.children[ti].items[ii] else {
                continue;
            };
            let base = resolve_sync_offset(&anchor.metadata);
            if base.abs() <= TIME_EPS {
                continue;
            }
            for (ti, ii) in members {
                if let Item::Clip(clip) = &mut self.children[ti].items[ii] {
                    let offset = resolve_sync_offset(&clip.metadata);
                    set_sync_offset(&mut clip.metadata, offset - base);
                }
            }
        }
        true
    }
}
