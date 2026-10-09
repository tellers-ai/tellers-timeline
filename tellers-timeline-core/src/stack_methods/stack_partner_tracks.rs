//! Choosing the tracks a sync column's partners land on.
//!
//! Every partner slot is resolved the same way, from the primary track's
//! associated tracks (see [`crate::associated_tracks`]):
//!
//! 1. for a move or replace, the partner's own source track when it is
//!    usable over the column and either the column stays on its primary
//!    track (a horizontal move keeps its channel mapping) or the destination
//!    lists that track;
//! 2. the first associated track of the right kind that is usable;
//! 3. the nearest existing track of that kind that is free over the column
//!    and that no track lists as a partner (listed tracks are reserved for
//!    their own primary's columns);
//! 4. for a move, the partner's source track when it is usable and the
//!    destination has no partner of that kind at all (one shared audio track
//!    serves both videos rather than a new track per video);
//! 5. a new track, created on the far side of the primary's existing partners
//!    of that kind (right below the primary when it has none).
//!
//! Each chosen track is recorded on the primary at once, so the next slot of
//! the same column and the next column find it in the list.

use super::{range_is_gap_backed, track_is_empty_boundary};
use crate::{InsertPolicy, OverlapPolicy, Seconds, Stack, TrackKind};
use std::collections::HashSet;

/// One partner slot to resolve.
pub(super) struct PartnerSlotRequest<'a> {
    /// The track receiving the primary clip (never a slot).
    pub primary_track_index: usize,
    /// The track whose association drives the slots: the column's owner
    /// (its video track when it has one), which may differ from the primary
    /// when an audio partner is the clip being moved.
    pub owner_track_index: usize,
    pub kind: TrackKind,
    pub start: Seconds,
    pub end: Seconds,
    /// Tracks already taken by this column.
    pub used: &'a [usize],
    /// The partner's source track for a move or replace.
    pub preferred: Option<usize>,
    /// Whether the column stays on its primary track (horizontal move,
    /// replace): the preferred track is then kept whenever it is usable.
    pub horizontal: bool,
    /// Tracks the fallback search never reuses (the sources of a move away
    /// from a cluster, so no partner is left behind on them).
    pub exclude_fallback: &'a [usize],
}

impl Stack {
    /// Sort key for the partners of a column whose primary sits on
    /// `primary_track_index`: the partner's slot in the primary's association
    /// (its channel), then the track index for tracks the primary does not list.
    /// Moves and replacements process partners in this order so channel 1
    /// lands on slot 1 of the destination.
    pub(super) fn partner_slot_order(
        &self,
        primary_track_index: usize,
        partner_track_index: usize,
    ) -> (usize, usize) {
        let slot = self
            .associated_track_indices(primary_track_index)
            .iter()
            .position(|&index| index == partner_track_index)
            .unwrap_or(usize::MAX);
        (slot, partner_track_index)
    }

    /// Whether an associated track can take a partner over `[start, end]`.
    ///
    /// A free range always can. Otherwise Override may only replace the sync
    /// clips being overridden on the primary (`overridden_sync_ids`), never
    /// unrelated content; Push may take a track that already shares sync clips
    /// with the primary, since the column pushes that content along.
    pub(super) fn partner_track_usable(
        &self,
        primary_track_index: usize,
        track_index: usize,
        start: Seconds,
        end: Seconds,
        overlap_policy: OverlapPolicy,
        overridden_sync_ids: &HashSet<i64>,
    ) -> bool {
        let Some(track) = self.children.get(track_index) else {
            return false;
        };
        if track_is_empty_boundary(track) || range_is_gap_backed(track, start, end) {
            return true;
        }
        match overlap_policy {
            OverlapPolicy::Override => {
                self.range_only_holds_sync_clips(track_index, start, end, overridden_sync_ids)
            }
            OverlapPolicy::Push => self.tracks_share_sync_clips(primary_track_index, track_index),
        }
    }

    /// Where a new partner track of `kind` for `primary_track_index` goes: on
    /// the far side of the primary's existing associated tracks of that kind
    /// (so the list order is also the stack order, away from the primary),
    /// or, when there are none, right below the primary (audio) or right
    /// above the primary's audio run (video).
    pub(super) fn partner_create_index(
        &self,
        primary_track_index: usize,
        kind: TrackKind,
    ) -> usize {
        let associated_of_kind: Vec<usize> = self
            .associated_track_indices(primary_track_index)
            .into_iter()
            .filter(|&index| self.children[index].kind == kind)
            .collect();
        if let (Some(&lowest), Some(&highest)) = (
            associated_of_kind.iter().min(),
            associated_of_kind.iter().max(),
        ) {
            return if lowest > primary_track_index {
                highest + 1
            } else {
                lowest
            };
        }
        match kind {
            TrackKind::Video => {
                let mut index = primary_track_index + 1;
                while index < self.children.len() && self.children[index].kind == TrackKind::Audio {
                    index += 1;
                }
                index
            }
            TrackKind::Audio | TrackKind::Other => primary_track_index,
        }
    }

    /// Create a partner track of `kind` for `primary_track_index` and return
    /// its index. The caller shifts every index it holds that is `>=` the
    /// returned one.
    pub(super) fn create_partner_track(
        &mut self,
        primary_track_index: usize,
        kind: TrackKind,
        created_track_indices: &mut Vec<usize>,
    ) -> usize {
        let insert_at = self.partner_create_index(primary_track_index, kind.clone());
        let track = self.new_numbered_track(kind);
        self.children.insert(insert_at, track);
        for index in created_track_indices.iter_mut() {
            if *index >= insert_at {
                *index += 1;
            }
        }
        created_track_indices.push(insert_at);
        insert_at
    }

    /// Resolve one partner slot (see the module docs). `usable` judges an
    /// existing track over the column. Returns the track index, creating a
    /// track when needed; the caller shifts its own indices when
    /// `self.children.len()` grew.
    pub(super) fn pick_partner_track(
        &mut self,
        request: &PartnerSlotRequest<'_>,
        usable: &dyn Fn(&Stack, usize) -> bool,
        created_track_indices: &mut Vec<usize>,
    ) -> usize {
        let primary = request.primary_track_index;
        let owner = request.owner_track_index;
        // The owner is slot 0 of its group: when the primary sits on a partner
        // track (an audio-only column added on a stem track) the owner takes a
        // partner like any other member.
        let associated: Vec<usize> = std::iter::once(owner)
            .chain(self.associated_track_indices(owner))
            .filter(|&index| index != primary && self.children[index].kind == request.kind)
            .collect();
        let available = |stack: &Stack, index: usize| {
            index != primary && !request.used.contains(&index) && usable(stack, index)
        };

        if let Some(preferred) = request.preferred {
            let kind_matches = self
                .children
                .get(preferred)
                .is_some_and(|track| track.kind == request.kind);
            if kind_matches
                && (request.horizontal || associated.contains(&preferred))
                && available(self, preferred)
            {
                return preferred;
            }
        }
        if let Some(index) = associated
            .iter()
            .copied()
            .find(|&index| available(self, index))
        {
            return index;
        }
        let exclude: Vec<usize> = request
            .used
            .iter()
            .chain(request.exclude_fallback.iter())
            .copied()
            .collect();
        if let Some(index) = self
            .nearest_free_tracks_of_kind(
                request.kind.clone(),
                primary,
                request.start,
                request.end,
                &exclude,
            )
            .into_iter()
            .find(|&index| !self.is_associated_partner(index))
        {
            return index;
        }
        if let Some(preferred) = request.preferred {
            let kind_matches = self
                .children
                .get(preferred)
                .is_some_and(|track| track.kind == request.kind);
            if kind_matches && associated.is_empty() && available(self, preferred) {
                return preferred;
            }
        }
        self.create_partner_track(owner, request.kind.clone(), created_track_indices)
    }

    /// Resolve every audio slot of a column on `dest_track_index`, one per
    /// entry of `durations`, in order. `preferred` are the partners' source
    /// tracks for a move. Keeps `dest_track_index`, `cluster` and
    /// `created_track_indices` valid across track creation and records the
    /// chosen tracks on the destination.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn assign_audio_partner_slots(
        &mut self,
        dest_track_index: &mut usize,
        owner_track_index: &mut usize,
        cluster: &mut Vec<usize>,
        created_track_indices: &mut Vec<usize>,
        start: Seconds,
        durations: &[Seconds],
        preferred: Option<&[usize]>,
        exclude_fallback: &[usize],
        overlap_policy: OverlapPolicy,
        overridden_sync_ids: &HashSet<i64>,
    ) -> Vec<usize> {
        let mut slots: Vec<usize> = Vec::with_capacity(durations.len());
        let mut preferred: Vec<Option<usize>> = (0..durations.len())
            .map(|i| preferred.and_then(|indices| indices.get(i).copied()))
            .collect();
        let mut exclude_fallback: Vec<usize> = exclude_fallback.to_vec();
        let horizontal = preferred.iter().any(|p| p.is_some())
            && (exclude_fallback.is_empty() || exclude_fallback.contains(dest_track_index));

        for (slot, &duration) in durations.iter().enumerate() {
            let end = start + duration;
            let primary = *dest_track_index;
            let owner = *owner_track_index;
            let usable = move |stack: &Stack, index: usize| {
                stack.partner_track_usable(
                    primary,
                    index,
                    start,
                    end,
                    overlap_policy,
                    overridden_sync_ids,
                ) || (owner != primary
                    && stack.partner_track_usable(
                        owner,
                        index,
                        start,
                        end,
                        overlap_policy,
                        overridden_sync_ids,
                    ))
            };
            let track_count_before = self.children.len();
            let track_index = self.pick_partner_track(
                &PartnerSlotRequest {
                    primary_track_index: primary,
                    owner_track_index: *owner_track_index,
                    kind: TrackKind::Audio,
                    start,
                    end,
                    used: &slots,
                    preferred: preferred[slot],
                    horizontal,
                    exclude_fallback: &exclude_fallback,
                },
                &usable,
                created_track_indices,
            );
            if self.children.len() > track_count_before {
                // `created_track_indices` was already shifted by the creation.
                Self::shift_insert_track_indices_after_create(
                    track_index,
                    dest_track_index,
                    cluster,
                    &mut slots,
                    &mut [],
                );
                if track_index <= *owner_track_index {
                    *owner_track_index += 1;
                }
                for index in preferred.iter_mut().flatten() {
                    if *index >= track_index {
                        *index += 1;
                    }
                }
                for index in exclude_fallback.iter_mut() {
                    if *index >= track_index {
                        *index += 1;
                    }
                }
            }
            slots.push(track_index);
            self.record_associated_tracks(*owner_track_index, &[track_index]);
        }

        for &slot in &slots {
            if !cluster.contains(&slot) {
                cluster.push(slot);
            }
        }
        cluster.sort_unstable();
        slots
    }

    /// Resolve the video slot of a column whose primary sits on the audio
    /// track `audio_track_index`: the preferred track when given and usable,
    /// then the video tracks listing that audio track (and the ones it lists),
    /// then a new track when there are some but none is usable, else the
    /// cluster/nearest-free fallback. Returns the track index (the caller
    /// shifts its indices when the stack grew); the video adopts the audio
    /// track as a partner.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn pick_video_partner_track(
        &mut self,
        audio_track_index: usize,
        start: Seconds,
        end: Seconds,
        preferred_video_track_id: Option<&str>,
        used_video: &[usize],
        sync_clips_id: Option<i64>,
        overlap_policy: OverlapPolicy,
        insert_policy: InsertPolicy,
        created_track_indices: &mut Vec<usize>,
    ) -> Option<usize> {
        let reusable = |stack: &Stack, index: usize| {
            !used_video.contains(&index)
                && stack
                    .try_reuse_video_track_for_audio_move(
                        index,
                        audio_track_index,
                        start,
                        end,
                        sync_clips_id,
                        true,
                        overlap_policy,
                        insert_policy,
                    )
                    .is_some()
        };

        // Videos that list this audio track as a partner (the column's owner),
        // nearest first, then videos the audio track itself lists.
        let mut candidates: Vec<usize> = self.tracks_associating(audio_track_index);
        candidates.sort_by_key(|&index| (index.abs_diff(audio_track_index), index));
        candidates.extend(self.associated_track_indices(audio_track_index));
        let mut associated: Vec<usize> = Vec::new();
        for index in candidates {
            if self.children[index].kind == TrackKind::Video && !associated.contains(&index) {
                associated.push(index);
            }
        }

        let mut chosen = preferred_video_track_id
            .and_then(|id| self.get_track_by_id(id))
            .map(|(index, _)| index)
            .filter(|&index| reusable(self, index));
        if chosen.is_none() {
            chosen = associated
                .iter()
                .copied()
                .find(|&index| reusable(self, index));
        }
        if chosen.is_none() && !associated.is_empty() {
            chosen = Some(self.create_partner_track(
                audio_track_index,
                TrackKind::Video,
                created_track_indices,
            ));
        }
        let track_count_before = self.children.len();
        let created_before = created_track_indices.len();
        let track_index = match chosen {
            Some(index) => index,
            None => {
                let index = self.find_or_create_video_track_for_audio(
                    audio_track_index,
                    start,
                    end - start,
                    created_track_indices,
                    sync_clips_id,
                    true,
                    overlap_policy,
                    insert_policy,
                )?;
                if self.children.len() > track_count_before {
                    for created in created_track_indices[..created_before].iter_mut() {
                        if *created >= index {
                            *created += 1;
                        }
                    }
                }
                index
            }
        };
        // The video owns the column: it adopts the audio track as a partner.
        let audio_track_index =
            if self.children.len() > track_count_before && track_index <= audio_track_index {
                audio_track_index + 1
            } else {
                audio_track_index
            };
        self.record_associated_tracks(track_index, &[audio_track_index]);
        Some(track_index)
    }
}
