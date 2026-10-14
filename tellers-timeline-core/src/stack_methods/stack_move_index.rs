//! Moving a sync column to an index on another track.
//!
//! The time-based move lifts the column and re-inserts it through the column
//! insert; the index-based move cannot, since the destination index only
//! means something once the column is lifted. So it places the members
//! itself: lift every member, decide a track for each partner (same rules
//! as the column insert), pad the other tracks of the cluster with a gap,
//! split what sits under the column, and insert.

use std::collections::HashSet;

use super::stack_item_split::SyncSplitIdPolicy;
use super::stack_partner_tracks::PartnerSlotRequest;
use super::{
    range_is_gap_backed, resolve_sync_clips_id, shift_track_index_after_insert,
    shift_track_indices_after_insert, SyncedMoveItem, EPS,
};
use crate::{Gap, IdMetadataExt, InsertPolicy, Item, OverlapPolicy, Seconds, Stack, TrackKind};

/// Where the moved members go, as track indices that all shift together
/// when a track is created.
struct MoveLayout {
    dest_track_index: usize,
    /// The track whose association drives the audio slots: the video
    /// partner's track once placed, else the destination.
    owner_track_index: usize,
    /// (track, item, is the selected member)
    placements: Vec<(usize, Item, bool)>,
    used_audio_tracks: Vec<usize>,
    used_video_tracks: Vec<usize>,
    /// Every source track of the column: never reused by the fallback search.
    source_tracks: Vec<usize>,
    created_track_indices: Vec<usize>,
}

impl MoveLayout {
    /// A track was inserted at `at`: every index at or past it moved up by
    /// one, except `at` itself when it is the track just chosen.
    fn track_created_at(&mut self, at: usize, chosen: usize) {
        shift_track_index_after_insert(&mut self.dest_track_index, at);
        for (track_index, _, _) in &mut self.placements {
            shift_track_index_after_insert(track_index, at);
        }
        shift_track_indices_after_insert(&mut self.used_audio_tracks, at);
        shift_track_indices_after_insert(&mut self.used_video_tracks, at);
        shift_track_indices_after_insert(&mut self.source_tracks, at);
        if self.owner_track_index != chosen {
            shift_track_index_after_insert(&mut self.owner_track_index, at);
        }
    }

    fn note_used(&mut self, kind: &TrackKind, track_index: usize) {
        match kind {
            TrackKind::Audio => self.used_audio_tracks.push(track_index),
            _ => self.used_video_tracks.push(track_index),
        }
    }
}

impl Stack {
    /// Move `items_to_move` (one column, the selected member first on
    /// `dest_track_index` at `dest_index`). Returns false, with the stack
    /// unchanged, when the move is refused or cannot be applied.
    pub(super) fn move_synced_items(
        &mut self,
        items_to_move: Vec<SyncedMoveItem>,
        dest_track_index: usize,
        replace_with_gap: bool,
        overlap_policy: OverlapPolicy,
        dest_index: usize,
    ) -> bool {
        let backup = self.clone();
        if dest_track_index >= self.children.len() {
            return false;
        }
        let Some(selected_position) = items_to_move.iter().position(|item| item.is_selected) else {
            return false;
        };
        let selected = &items_to_move[selected_position];
        let Some(selected_id) = selected.item.get_id() else {
            return false;
        };
        let Some(sync_clips_id) = (match &selected.item {
            Item::Clip(clip) => resolve_sync_clips_id(&clip.metadata),
            Item::Gap(_) => None,
        }) else {
            return false;
        };
        let moved_duration = selected.item.duration().max(0.0);
        let same_length = items_to_move
            .iter()
            .all(|item| (item.item.duration().max(0.0) - moved_duration).abs() <= EPS);
        if moved_duration <= EPS || !same_length {
            return false;
        }
        let planned_start = self.children[dest_track_index].start_time_of_item(dest_index);
        let column_has_video = items_to_move
            .iter()
            .any(|item| item.track_kind == TrackKind::Video);
        if !self.partner_move_target_allowed(
            selected,
            column_has_video,
            dest_track_index,
            planned_start,
            planned_start + moved_duration,
        ) {
            return false;
        }

        let selected_source_track = selected.track_index;
        let intra_cluster_move = self.is_intra_cluster_sync_move(dest_track_index, &items_to_move);
        let source_tracks: Vec<usize> = items_to_move.iter().map(|item| item.track_index).collect();
        // Video first (it owns the column), then audio in the owner's slot order.
        let source_owner = self
            .column_owner_track_index(&source_tracks)
            .unwrap_or(selected_source_track);
        let mut partners: Vec<SyncedMoveItem> = items_to_move
            .iter()
            .filter(|item| !item.is_selected)
            .cloned()
            .collect();
        partners.sort_by_key(|item| {
            (
                item.track_kind != TrackKind::Video,
                self.partner_slot_order(source_owner, item.track_index),
            )
        });

        let mut used_ids = self.collect_timeline_ids();
        if !self.lift_moved_items(&items_to_move, replace_with_gap, &mut used_ids) {
            *self = backup;
            return false;
        }
        let moved_start = self.children[dest_track_index].start_time_of_item(dest_index);

        let mut layout = MoveLayout {
            dest_track_index,
            owner_track_index: dest_track_index,
            placements: vec![(dest_track_index, selected.item.clone(), true)],
            used_audio_tracks: Vec::new(),
            used_video_tracks: Vec::new(),
            source_tracks,
            created_track_indices: Vec::new(),
        };
        layout.note_used(&self.children[dest_track_index].kind, dest_track_index);
        let horizontal = selected_source_track == dest_track_index;
        for position in 0..partners.len() {
            let partner = partners[position].clone();
            let placed = if intra_cluster_move {
                Some(partner.track_index)
            } else {
                self.place_moved_partner(
                    &partner,
                    &mut layout,
                    moved_start,
                    moved_duration,
                    sync_clips_id,
                    selected_source_track,
                    horizontal,
                    overlap_policy,
                    &mut partners[position + 1..],
                )
            };
            let Some(track_index) = placed else {
                *self = backup;
                return false;
            };
            layout.note_used(&partner.track_kind, track_index);
            layout.placements.push((track_index, partner.item, false));
        }

        self.pad_cluster_with_gaps(&mut layout, sync_clips_id, moved_duration, &mut used_ids);
        if !self.apply_move_placements(
            layout,
            moved_start,
            moved_duration,
            dest_index,
            overlap_policy,
        ) || self.get_item(&selected_id).is_none()
        {
            *self = backup;
            return false;
        }
        self.sanitize_preserving_all_gap_tracks();
        true
    }

    /// Remove every member from its track, last index first so the earlier
    /// ones stay valid.
    fn lift_moved_items(
        &mut self,
        items_to_move: &[SyncedMoveItem],
        replace_with_gap: bool,
        used_ids: &mut HashSet<String>,
    ) -> bool {
        let mut targets: Vec<(usize, usize)> = items_to_move
            .iter()
            .map(|item| (item.track_index, item.item_index))
            .collect();
        targets.sort_by(|a, b| b.cmp(a));
        targets.into_iter().all(|(track_index, item_index)| {
            self.remove_item_at_for_move(track_index, item_index, replace_with_gap, used_ids)
        })
    }

    /// Pick the track for one partner, creating one when needed and keeping
    /// every index in `layout` and the pending partners valid.
    #[allow(clippy::too_many_arguments)]
    fn place_moved_partner(
        &mut self,
        partner: &SyncedMoveItem,
        layout: &mut MoveLayout,
        moved_start: Seconds,
        moved_duration: Seconds,
        sync_clips_id: i64,
        selected_source_track: usize,
        horizontal: bool,
        overlap_policy: OverlapPolicy,
        pending: &mut [SyncedMoveItem],
    ) -> Option<usize> {
        let moved_end = moved_start + moved_duration;
        let track_count_before = self.children.len();
        let track_index = match partner.track_kind {
            TrackKind::Audio => {
                let overridden_sync_ids: HashSet<i64> = if overlap_policy == OverlapPolicy::Override
                {
                    self.sync_clips_ids_in_range(layout.dest_track_index, moved_start, moved_end)
                } else {
                    HashSet::new()
                };
                let primary = layout.dest_track_index;
                let owner = layout.owner_track_index;
                let usable = |stack: &Stack, index: usize| {
                    let usable_for = |anchor: usize| {
                        stack.partner_track_usable(
                            anchor,
                            index,
                            moved_start,
                            moved_end,
                            overlap_policy,
                            &overridden_sync_ids,
                        )
                    };
                    usable_for(primary) || (owner != primary && usable_for(owner))
                };
                // A swap: the channel sitting on the destination takes the
                // moved channel's old track.
                let preferred = if partner.track_index == layout.dest_track_index {
                    selected_source_track
                } else {
                    partner.track_index
                };
                self.pick_partner_track(
                    &PartnerSlotRequest {
                        primary_track_index: layout.dest_track_index,
                        owner_track_index: layout.owner_track_index,
                        kind: TrackKind::Audio,
                        start: moved_start,
                        end: moved_end,
                        used: &layout.used_audio_tracks,
                        preferred: Some(preferred),
                        horizontal,
                        exclude_fallback: &layout.source_tracks,
                    },
                    &usable,
                    &mut layout.created_track_indices,
                )
            }
            TrackKind::Video => {
                let track_index = self.pick_video_partner_track(
                    layout.dest_track_index,
                    moved_start,
                    moved_end,
                    None,
                    &layout.used_video_tracks,
                    Some(sync_clips_id),
                    overlap_policy,
                    InsertPolicy::SplitAndInsert,
                    &mut layout.created_track_indices,
                )?;
                if layout.used_video_tracks.contains(&track_index) {
                    self.children
                        .insert(track_index, self.new_numbered_track(TrackKind::Video));
                    layout.created_track_indices.push(track_index);
                }
                layout.owner_track_index = track_index;
                track_index
            }
            TrackKind::Other => return None,
        };
        if self.children.len() > track_count_before {
            layout.track_created_at(track_index, track_index);
            for item in pending {
                shift_track_index_after_insert(&mut item.track_index, track_index);
            }
        }
        Some(track_index)
    }

    /// Every track of the destination cluster that receives no member gets a
    /// gap of the column's length, so the cluster stays aligned.
    fn pad_cluster_with_gaps(
        &mut self,
        layout: &mut MoveLayout,
        sync_clips_id: i64,
        moved_duration: Seconds,
        used_ids: &mut HashSet<String>,
    ) {
        let mut tracks = self.boundary_track_indices_for_anchors(
            &[sync_clips_id],
            &[layout.dest_track_index],
            &[],
        );
        tracks.extend(
            layout
                .placements
                .iter()
                .map(|(track_index, _, _)| *track_index),
        );
        tracks.sort_unstable();
        tracks.dedup();
        for track_index in tracks {
            if layout
                .placements
                .iter()
                .any(|(placed, _, _)| *placed == track_index)
            {
                continue;
            }
            let mut gap = Item::Gap(Gap::make_gap(moved_duration));
            Self::ensure_unique_item_id(&mut gap, used_ids);
            layout.placements.push((track_index, gap, false));
        }
        layout
            .placements
            .sort_by_key(|(track_index, _, _)| *track_index);
    }

    /// Split whatever the column lands on, then insert every placement: the
    /// selected member at `dest_index`, the others at the column's start.
    fn apply_move_placements(
        &mut self,
        layout: MoveLayout,
        moved_start: Seconds,
        moved_duration: Seconds,
        dest_index: usize,
        overlap_policy: OverlapPolicy,
    ) -> bool {
        let cluster = self.boundary_group_indices(layout.dest_track_index);
        if !self.apply_sync_splits_for_column_insert(
            moved_start,
            moved_duration,
            InsertPolicy::SplitAndInsert,
            overlap_policy,
            SyncSplitIdPolicy::KeepShared,
            Some(&cluster),
        ) {
            return false;
        }
        for (track_index, item, is_selected) in layout.placements {
            if is_selected {
                let _ =
                    self.children[track_index].insert_at_index(dest_index, item, overlap_policy);
                continue;
            }
            let moved_end = moved_start + item.duration().max(0.0);
            if range_is_gap_backed(&self.children[track_index], moved_start, moved_end) {
                if !self.insert_gap_only(track_index, moved_start, item) {
                    return false;
                }
            } else {
                self.children[track_index].insert_at_time(
                    moved_start,
                    item,
                    overlap_policy,
                    InsertPolicy::SplitAndInsert,
                );
            }
        }
        true
    }
}
