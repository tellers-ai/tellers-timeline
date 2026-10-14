//! Inserting a column: a primary item with its sync partners, each on its own
//! track, all starting at the same time.
//!
//! Every insert, move and replace of a clip ends here. The steps, in order:
//! validate the request and resolve the start; place the video partner (it
//! owns the column); pick the audio slots (see `stack_partner_tracks`);
//! give every member its id and sync id; insert the members on their
//! tracks; propagate splits, overrides and pushes to the rest of the
//! cluster.

use std::collections::HashSet;

use super::{
    insertion_start_or_end_for_policy, InsertItemAtTimeResult, SyncedInputs, SyncedInsertResult,
    EPS,
};
use crate::{InsertPolicy, Item, OverlapPolicy, Seconds, Stack, TrackInsertResult, TrackKind};

/// A column to insert. Build it with [`ColumnInsert::new`] and set the
/// optional fields that apply.
pub(crate) struct ColumnInsert<'a> {
    pub dest_track_index: usize,
    pub dest_time: Seconds,
    /// Insert before this item instead of at `dest_time`.
    pub dest_index: Option<usize>,
    pub item: Item,
    pub overlap_policy: OverlapPolicy,
    pub insert_policy: InsertPolicy,
    pub synced_audio_clips: Option<Vec<Item>>,
    pub synced_video_clip: Option<Item>,
    /// The column has a video clip that the caller places itself afterwards
    /// (misaligned Resolve columns): it still counts as a video column.
    pub column_video_placed_separately: bool,
    /// Moves: the video track the column came from, kept when usable.
    pub preferred_video_track_id: Option<&'a str>,
    /// Moves and replaces: the partners' source tracks, in partner order.
    pub preferred_audio_track_indices: Option<&'a [usize]>,
    /// Moves: every source track of the column, never reused by the
    /// fallback search when moving away.
    pub move_source_track_indices: Option<&'a [usize]>,
    /// Moves with Push: where the primary started, so the push propagation
    /// knows the column's own former footprint.
    pub move_previous_start: Option<Seconds>,
}

impl<'a> ColumnInsert<'a> {
    pub(crate) fn new(
        dest_track_index: usize,
        dest_time: Seconds,
        item: Item,
        overlap_policy: OverlapPolicy,
        insert_policy: InsertPolicy,
    ) -> Self {
        Self {
            dest_track_index,
            dest_time,
            dest_index: None,
            item,
            overlap_policy,
            insert_policy,
            synced_audio_clips: None,
            synced_video_clip: None,
            column_video_placed_separately: false,
            preferred_video_track_id: None,
            preferred_audio_track_indices: None,
            move_source_track_indices: None,
            move_previous_start: None,
        }
    }
}

/// The request once validated: the primary item, its partners, where the
/// column starts and how long it is.
struct ColumnPlan {
    primary_item: Item,
    inputs: SyncedInputs,
    start: Seconds,
    duration: Seconds,
    column_span: Seconds,
}

/// Where the column's members go, tracked as track indices that shift when
/// a track is created.
struct ColumnLayout {
    dest_track_index: usize,
    /// The track whose association drives the audio slots: the video track
    /// of a video column, else the destination's group owner.
    owner_track_index: usize,
    cluster: Vec<usize>,
    audio_slots: Vec<usize>,
    created_track_indices: Vec<usize>,
    video: Option<(usize, Item, String)>,
}

impl Stack {
    /// Insert `request` as one column. Returns the primary's id, or the full
    /// result when the column is synced; `None`, with the stack unchanged,
    /// when the request is refused or cannot be applied.
    pub(super) fn insert_synced_item_at_time(
        &mut self,
        request: ColumnInsert<'_>,
    ) -> Option<InsertItemAtTimeResult> {
        let plan = self.plan_column(&request)?;
        let backup = self.clone();
        let mut used_ids = self.collect_timeline_ids();
        let has_synced_clips = !plan.inputs.audio.is_empty() || plan.inputs.video.is_some();
        let sync_clips_id = has_synced_clips.then(|| self.next_sync_clips_id());

        let Some(mut layout) = self.lay_out_column(&request, &plan, sync_clips_id, &mut used_ids)
        else {
            *self = backup;
            return None;
        };
        let Some((members, primary_id, audio_clips)) =
            self.column_members(&plan, &layout, sync_clips_id, &mut used_ids)
        else {
            *self = backup;
            return None;
        };
        let video_clip_id = layout.video.take().map(|(_, _, id)| id);
        if !self.insert_column_members(&request, &plan, &layout, sync_clips_id, members) {
            *self = backup;
            return None;
        }
        self.sanitize_preserving_all_gap_tracks();

        let primary_is_clip = matches!(self.get_item(&primary_id), Some((_, _, Item::Clip(_))));
        Some(if sync_clips_id.is_some() && primary_is_clip {
            InsertItemAtTimeResult::Synced(SyncedInsertResult {
                primary_clip_id: primary_id,
                audio_clips,
                synced_video_clip_id: video_clip_id,
                sync_clips_id,
                created_track_indices: layout.created_track_indices,
            })
        } else {
            InsertItemAtTimeResult::ItemId(primary_id)
        })
    }

    /// Check the request against the placement rules and resolve the start.
    fn plan_column(&self, request: &ColumnInsert<'_>) -> Option<ColumnPlan> {
        let mut primary_item = request.item.clone();
        primary_item.clamp_to_active_available_range();
        let inputs = Self::normalize_synced_inputs(
            request.synced_audio_clips.clone(),
            request.synced_video_clip.clone(),
        );
        let has_synced_clips = !inputs.audio.is_empty() || inputs.video.is_some();
        if has_synced_clips && !matches!(primary_item, Item::Clip(_)) {
            return None;
        }
        if matches!(inputs.video, Some(Item::Gap(_))) {
            return None;
        }
        if !self.column_may_land_on(
            request.dest_track_index,
            matches!(primary_item, Item::Gap(_)),
            inputs.video.is_some() || request.column_video_placed_separately,
        ) {
            return None;
        }
        let duration = primary_item.duration().max(0.0);
        if duration <= EPS {
            return None;
        }
        let column_span = Self::synced_insert_column_span(duration, &inputs);
        // Every member of the column lands at the primary's resolved start.
        let dest = &self.children[request.dest_track_index];
        let start = match request.dest_index {
            Some(dest_index) => dest.start_time_of_item(dest_index),
            None => {
                insertion_start_or_end_for_policy(dest, request.dest_time, request.insert_policy)?
            }
        };
        Some(ColumnPlan {
            primary_item,
            inputs,
            start,
            duration,
            column_span,
        })
    }

    /// Decide the tracks: the video partner first (it owns the column), then
    /// the working cluster, then the audio slots. Creates tracks as needed.
    fn lay_out_column(
        &mut self,
        request: &ColumnInsert<'_>,
        plan: &ColumnPlan,
        sync_clips_id: Option<i64>,
        used_ids: &mut HashSet<String>,
    ) -> Option<ColumnLayout> {
        let has_synced_clips = !plan.inputs.audio.is_empty() || plan.inputs.video.is_some();
        let dest_is_video = self.children[request.dest_track_index].kind == TrackKind::Video;
        let mut layout = ColumnLayout {
            dest_track_index: request.dest_track_index,
            owner_track_index: self.group_owner_of(request.dest_track_index),
            cluster: self.resolve_insert_cluster(
                request.dest_track_index,
                dest_is_video || plan.inputs.video.is_some(),
                plan.inputs
                    .video
                    .as_ref()
                    .and(request.preferred_video_track_id),
            ),
            audio_slots: Vec::new(),
            created_track_indices: Vec::new(),
            video: None,
        };

        if let Some(video_item) = plan.inputs.video.clone() {
            let video_span = Self::sanitized_clip_duration(&video_item).unwrap_or(plan.column_span);
            let track_count_before = self.children.len();
            let video_track_index = self.pick_video_partner_track(
                layout.dest_track_index,
                plan.start,
                plan.start + video_span,
                request.preferred_video_track_id,
                &[],
                sync_clips_id,
                request.overlap_policy,
                request.insert_policy,
                &mut layout.created_track_indices,
            )?;
            if self.children.len() > track_count_before {
                Self::shift_insert_track_indices_after_create(
                    video_track_index,
                    &mut layout.dest_track_index,
                    &mut layout.cluster,
                    &mut layout.audio_slots,
                    &mut [],
                );
            }
            let (video_item, video_id) =
                Self::prepare_synced_item_preserve_duration(video_item, sync_clips_id, used_ids)?;
            layout.video = Some((video_track_index, video_item, video_id));
            layout.owner_track_index = video_track_index;
        }

        // The owner's associated tracks join the working cluster so insert
        // propagation applies to them even while they are empty.
        if has_synced_clips {
            for track_index in self
                .associated_track_indices(layout.dest_track_index)
                .into_iter()
                .chain(self.associated_track_indices(layout.owner_track_index))
                .chain(std::iter::once(layout.owner_track_index))
            {
                if !layout.cluster.contains(&track_index) {
                    layout.cluster.push(track_index);
                }
            }
            layout.cluster.sort_unstable();
        }

        // Override replaces whatever sits under the column on the destination
        // track together with the partners of those clips; an associated
        // track holding anything else there must not lose it, so it is not a
        // slot for this column.
        let audio_durations: Vec<Seconds> = plan
            .inputs
            .audio
            .iter()
            .map(|item| Self::sanitized_clip_duration(item).unwrap_or(plan.duration))
            .collect();
        let overridden_sync_ids: HashSet<i64> = if request.overlap_policy == OverlapPolicy::Override
        {
            self.sync_clips_ids_in_range(
                layout.dest_track_index,
                plan.start,
                plan.start + plan.column_span,
            )
        } else {
            HashSet::new()
        };
        layout.audio_slots = self.assign_audio_partner_slots(
            &mut layout.dest_track_index,
            &mut layout.owner_track_index,
            &mut layout.cluster,
            &mut layout.created_track_indices,
            plan.start,
            &audio_durations,
            request.preferred_audio_track_indices,
            request.move_source_track_indices.unwrap_or(&[]),
            request.overlap_policy,
            &overridden_sync_ids,
        );
        if let Some((video_track_index, _, _)) = layout.video.as_mut() {
            // Audio slot creation may have shifted the video track.
            *video_track_index = layout.owner_track_index;
        }
        Some(layout)
    }

    /// Every member with its id and sync id, as (track, item), the primary
    /// first; plus the primary's id and the audio (id, track) pairs.
    #[allow(clippy::type_complexity)]
    fn column_members(
        &self,
        plan: &ColumnPlan,
        layout: &ColumnLayout,
        sync_clips_id: Option<i64>,
        used_ids: &mut HashSet<String>,
    ) -> Option<(Vec<(usize, Item)>, String, Vec<(String, usize)>)> {
        let (primary_item, primary_id) = match plan.primary_item.clone() {
            item @ Item::Clip(_) => {
                Self::prepare_synced_item(item, plan.duration, sync_clips_id, used_ids)?
            }
            Item::Gap(mut gap) => {
                gap.source_range.duration.set_from_seconds(plan.duration);
                let mut item = Item::Gap(gap);
                let id = Self::ensure_unique_item_id(&mut item, used_ids);
                (item, id)
            }
        };
        let mut partners = Vec::new();
        let mut audio_clips = Vec::new();
        for (audio_item, &track_index) in plan.inputs.audio.iter().zip(&layout.audio_slots) {
            let (audio_item, audio_id) = Self::prepare_synced_item_preserve_duration(
                audio_item.clone(),
                sync_clips_id,
                used_ids,
            )?;
            partners.push((track_index, audio_item));
            audio_clips.push((audio_id, track_index));
        }
        if let Some((track_index, item, _)) = &layout.video {
            partners.push((*track_index, item.clone()));
        }
        partners.sort_by_key(|(track_index, _)| *track_index);
        let mut members = vec![(layout.dest_track_index, primary_item)];
        members.extend(partners);
        Some((members, primary_id, audio_clips))
    }

    /// Insert the members on their tracks, then propagate to the cluster.
    /// Push inserts go track by track in stack order; otherwise the primary
    /// goes first so its overrides are known before the partners land.
    fn insert_column_members(
        &mut self,
        request: &ColumnInsert<'_>,
        plan: &ColumnPlan,
        layout: &ColumnLayout,
        sync_clips_id: Option<i64>,
        mut members: Vec<(usize, Item)>,
    ) -> bool {
        if request.overlap_policy == OverlapPolicy::Push {
            members.sort_by_key(|(track_index, _)| *track_index);
        } else {
            let (mut on_dest, mut partners): (Vec<_>, Vec<_>) = members
                .into_iter()
                .partition(|(track_index, _)| *track_index == layout.dest_track_index);
            partners.sort_by_key(|(track_index, _)| *track_index);
            on_dest.extend(partners);
            members = on_dest;
        }

        let mut updates: Vec<(usize, TrackInsertResult)> = Vec::new();
        for (track_index, item) in members {
            let result = self.children[track_index].insert_at_time(
                plan.start,
                item,
                request.overlap_policy,
                InsertPolicy::SplitAndInsert,
            );
            if !result.success {
                return false;
            }
            updates.push((track_index, result));
        }
        let update_refs: Vec<_> = updates
            .iter()
            .map(|(track_index, result)| (*track_index, result))
            .collect();
        if !self.propagate_insert_to_cluster(
            plan.start,
            plan.duration,
            request.overlap_policy,
            &update_refs,
            &layout.cluster,
            sync_clips_id,
        ) {
            return false;
        }
        if request.overlap_policy == OverlapPolicy::Push
            && !self.propagate_push_to_cluster(
                plan.start,
                plan.start + plan.duration,
                plan.duration,
                &update_refs,
                &layout.cluster,
                request.move_previous_start,
            )
        {
            return false;
        }
        true
    }
}
