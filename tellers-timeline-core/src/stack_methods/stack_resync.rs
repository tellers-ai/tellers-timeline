//! Finding sync columns out of sync, and putting them back.
//!
//! A clip is out of sync when it starts away from its column by more than
//! its Sync Offset: its media no longer plays in step with its partners'
//! (an older project, or clips edited apart). A J / L cut is not: its
//! Sync Offset records how far it starts from the column on purpose.

use super::{remove_resolve_sync_clips_id, resolve_sync_clips_id, set_resolve_sync_clips_id};
use crate::metadata::resolve_sync_offset;
use crate::{Gap, IdMetadataExt, InsertPolicy, Item, OverlapPolicy, Seconds, Stack, TrackKind};

/// Two times this close are the same instant.
const SYNC_EPS: Seconds = 1e-6;

/// One clip of a sync column: where it is and where it should be.
struct Member {
    id: String,
    track_index: usize,
    item_index: usize,
    start: Seconds,
    offset: Seconds,
}

impl Stack {
    /// Every clip out of sync with its column, with how far: positive when
    /// it starts later than it should, like the red numbers Premiere and
    /// Resolve show. Each column is measured from its video (else its first
    /// clip) at Sync Offset 0. A J / L cut (a clip at its Sync Offset) is in
    /// sync and not listed.
    pub fn out_of_sync_items(&self) -> Vec<(String, Seconds)> {
        let mut ids: Vec<i64> = self.misaligned_sync_clips_ids().into_iter().collect();
        ids.sort_unstable();
        ids.into_iter()
            .flat_map(|sync_clips_id| self.column_drifts(sync_clips_id))
            .collect()
    }

    /// Put `item_id`'s sync column back in sync: every clip out of sync
    /// (see [`Self::out_of_sync_items`]) moves on its track to where it
    /// plays in step with the column (Premiere's "Move into Sync"). With
    /// `keep_split_edits` false, each J / L cut is also trimmed at its head
    /// back to the column, its Sync Offset cleared; that needs the media
    /// before its start when it starts after the column. Clips it lands on
    /// are overwritten. False, leaving the stack unchanged, when `item_id`
    /// is not a synced clip or a clip cannot be put back.
    pub fn resync_item(&mut self, item_id: &str, keep_split_edits: bool) -> bool {
        self.keeping_sync_columns(false, |stack| {
            let backup = stack.clone();
            let resynced = stack.resync_item_unchecked(item_id, keep_split_edits);
            if !resynced {
                *stack = backup;
            }
            resynced
        })
    }

    fn resync_item_unchecked(&mut self, item_id: &str, keep_split_edits: bool) -> bool {
        let Some((_, _, Item::Clip(clip))) = self.get_item(item_id) else {
            return false;
        };
        let Some(sync_clips_id) = resolve_sync_clips_id(&clip.metadata) else {
            return false;
        };

        // Shift each clip out of sync along its track into place, over
        // what is there. Its column's ids are set aside meanwhile.
        let drifts = self.column_drifts(sync_clips_id);
        if !drifts.is_empty() {
            let members = self.column_members(sync_clips_id);
            for member in &members {
                if let Item::Clip(clip) =
                    &mut self.children[member.track_index].items[member.item_index]
                {
                    remove_resolve_sync_clips_id(&mut clip.metadata);
                }
            }
            for (id, drift) in &drifts {
                let Some((track_index, item_index, _)) = self.get_item(id) else {
                    return false;
                };
                let track = &mut self.children[track_index];
                let start = track.start_time_of_item(item_index) - drift;
                if start < -SYNC_EPS {
                    return false;
                }
                let duration = track.items[item_index].duration().max(0.0);
                let item = std::mem::replace(
                    &mut track.items[item_index],
                    Item::Gap(Gap::make_gap(duration)),
                );
                if !track
                    .insert_at_time(
                        start.max(0.0),
                        item,
                        OverlapPolicy::Override,
                        InsertPolicy::SplitAndInsert,
                    )
                    .success
                {
                    return false;
                }
            }
            for member in &members {
                let Some((ti, ii, _)) = self.get_item(&member.id) else {
                    return false;
                };
                if let Item::Clip(clip) = &mut self.children[ti].items[ii] {
                    set_resolve_sync_clips_id(&mut clip.metadata, sync_clips_id);
                }
            }
            self.sanitize_preserving_all_gap_tracks();
        }

        if !keep_split_edits {
            for member in self.column_members(sync_clips_id) {
                if member.offset.abs() > SYNC_EPS && !self.trim_split_edit_back(&member) {
                    return false;
                }
            }
        }
        self.column_drifts(sync_clips_id).is_empty()
    }

    /// Trim a J / L cut's head back to its column: it starts `offset`
    /// earlier (a positive offset) or later (a negative one).
    fn trim_split_edit_back(&mut self, member: &Member) -> bool {
        let item = &self.children[member.track_index].items[member.item_index];
        let duration = item.duration().max(0.0);
        let Item::Clip(clip) = item else {
            return false;
        };
        if clip.is_reversed() {
            return false;
        }
        let source_start = clip.source_range.start_time.to_seconds();
        let source_start = if member.offset < 0.0 {
            source_start + clip.source_offset(-member.offset)
        } else {
            source_start - clip.source_offset(member.offset)
        };
        if source_start < -SYNC_EPS {
            return false;
        }
        let trimmed = self.modify_synced_clip_alone(
            &member.id,
            source_start.max(0.0),
            duration + member.offset,
            true,
            true,
            false,
        );
        trimmed == Some(true)
            && self
                .get_item(&member.id)
                .is_some_and(|(_, _, item)| match item {
                    Item::Clip(clip) => resolve_sync_offset(&clip.metadata).abs() <= SYNC_EPS,
                    _ => false,
                })
    }

    /// The clips of column `sync_clips_id` that have an id.
    fn column_members(&self, sync_clips_id: i64) -> Vec<Member> {
        self.synced_clips_targets(sync_clips_id)
            .into_iter()
            .filter_map(|(track_index, item_index)| {
                let track = &self.children[track_index];
                let Item::Clip(clip) = &track.items[item_index] else {
                    return None;
                };
                Some(Member {
                    id: clip.get_id()?,
                    track_index,
                    item_index,
                    start: track.start_time_of_item(item_index),
                    offset: resolve_sync_offset(&clip.metadata),
                })
            })
            .collect()
    }

    /// The clips of column `sync_clips_id` out of sync with it, and by how
    /// much; see [`Self::out_of_sync_items`].
    fn column_drifts(&self, sync_clips_id: i64) -> Vec<(String, Seconds)> {
        let members = self.column_members(sync_clips_id);
        let is_video = |m: &Member| self.children[m.track_index].kind == TrackKind::Video;
        let reference = members
            .iter()
            .filter(|m| m.offset.abs() <= SYNC_EPS)
            .min_by_key(|m| !is_video(m))
            .or_else(|| members.first());
        let Some(reference) = reference else {
            return Vec::new();
        };
        let column_start = reference.start - reference.offset;
        members
            .iter()
            .map(|m| (m.id.clone(), m.start - m.offset - column_start))
            .filter(|(_, drift)| drift.abs() > SYNC_EPS)
            .collect()
    }
}
