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
use crate::{Item, Seconds, Stack};

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
                        let offset = crate::metadata::resolve_sync_offset(&clip.metadata);
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
        if self.misaligned_sync_clips_ids().is_subset(&before) {
            result
        } else {
            *self = backup;
            refused
        }
    }
}
