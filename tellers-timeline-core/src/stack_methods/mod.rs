use crate::{
    Clip, Gap, IdMetadataExt, InsertPolicy, Item, OverlapPolicy, Seconds, Stack, Track, TrackKind,
    TrackInsertResult,
};
use std::collections::{HashMap, HashSet};

mod stack_insert_propagate;
mod stack_item_delete;
mod stack_item_get;
mod stack_item_insert;
mod stack_item_link;
mod stack_item_move;
mod stack_item_replace;
mod stack_item_split;
mod stack_track;

use stack_item_split::SyncSplitIdPolicy;

const EPS: Seconds = 1e-9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncedInsertResult {
    pub primary_clip_id: String,
    pub audio_clips: Vec<(String, usize)>,
    pub synced_video_clip_id: Option<String>,
    pub sync_clips_id: Option<i64>,
    pub created_track_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertItemAtTimeResult {
    ItemId(String),
    Synced(SyncedInsertResult),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncTrackInfo {
    pub track_indices: Vec<usize>,
    pub track_ids: Vec<Option<String>>,
}

#[derive(Debug, Clone)]
struct SyncedInputs {
    audio: Vec<Item>,
    video: Option<Item>,
}

#[derive(Debug, Clone)]
struct SyncedMoveItem {
    track_index: usize,
    item_index: usize,
    track_kind: TrackKind,
    item: Item,
    is_selected: bool,
}

#[derive(Debug, Clone)]
struct SyncedClipState {
    track_index: usize,
    start: Seconds,
    duration: Seconds,
    sync_clips_id: i64,
}

#[derive(Debug, Clone)]
struct BoundarySegment {
    start: Seconds,
    end: Seconds,
    sync_clips_id: Option<i64>,
}

#[derive(Debug, Clone)]
struct FlattenedBoundary {
    track_indices: Vec<usize>,
}

enum SyncedMovePlacement {
    Time {
        dest_time: Seconds,
        insert_policy: InsertPolicy,
    },
    Index {
        dest_index: usize,
    },
}

fn shift_track_index_after_insert(track_index: &mut usize, inserted_track_index: usize) {
    if inserted_track_index <= *track_index {
        *track_index += 1;
    }
}

fn shift_track_indices_after_insert(track_indices: &mut [usize], inserted_track_index: usize) {
    for track_index in track_indices {
        shift_track_index_after_insert(track_index, inserted_track_index);
    }
}

fn shift_move_placements_after_insert(
    placements: &mut [(usize, Item, bool)],
    inserted_track_index: usize,
) {
    for (track_index, _, _) in placements {
        shift_track_index_after_insert(track_index, inserted_track_index);
    }
}

fn clamp_insertion_index(len: usize, index: isize) -> usize {
    if index < 0 {
        let pos = len as isize + index;
        if pos <= 0 {
            0
        } else if pos >= len as isize {
            len
        } else {
            pos as usize
        }
    } else {
        let idx = index as usize;
        if idx > len {
            len
        } else {
            idx
        }
    }
}

impl Stack {
    fn collect_timeline_ids(&self) -> HashSet<String> {
        let mut ids = HashSet::new();
        for track in &self.children {
            if let Some(id) = track.get_id() {
                if !id.is_empty() {
                    ids.insert(id);
                }
            }
            for item in &track.items {
                if let Some(id) = item.get_id() {
                    if !id.is_empty() {
                        ids.insert(id);
                    }
                }
            }
        }
        ids
    }

    fn ensure_unique_item_id(item: &mut Item, used_ids: &mut HashSet<String>) -> String {
        let current = item.get_id().filter(|id| !id.is_empty());
        if let Some(id) = current {
            if used_ids.insert(id.clone()) {
                return id;
            }
        }

        loop {
            let id = crate::types::gen_hex_id_12();
            if used_ids.insert(id.clone()) {
                item.set_id(Some(id.clone()));
                return id;
            }
        }
    }

    fn next_sync_clips_id(&self) -> i64 {
        self.children
            .iter()
            .flat_map(|track| track.items.iter())
            .filter_map(|item| match item {
                Item::Clip(clip) => resolve_sync_clips_id(&clip.metadata),
                Item::Gap(_) => None,
            })
            .max()
            .unwrap_or(0)
            + 1
    }

    fn insert_gap_only(&mut self, track_index: usize, dest_time: Seconds, item: Item) -> bool {
        if track_index >= self.children.len() || item.duration() <= EPS {
            return false;
        }

        let duration = item.duration().max(0.0);
        let mut start = dest_time;
        let total = self.children[track_index].total_duration();
        if start < 0.0 {
            start = total - start;
        }
        if start < 0.0 {
            return false;
        }
        let end = start + duration;

        if start >= total - EPS {
            self.children[track_index].insert_at_time(
                start,
                item,
                OverlapPolicy::Override,
                InsertPolicy::InsertBefore,
            );
            return true;
        }

        if !range_is_gap_backed(&self.children[track_index], start, end) {
            return false;
        }

        let track = &mut self.children[track_index];
        split_gap_boundary(track, end);
        split_gap_boundary(track, start);

        self.children[track_index].insert_at_time(
            start,
            item,
            OverlapPolicy::Override,
            InsertPolicy::InsertBefore,
        );
        true
    }

    fn sync_split_id_policy_for_inserted_item(item: &Item) -> SyncSplitIdPolicy {
        match item {
            Item::Clip(clip) if resolve_sync_clips_id(&clip.metadata).is_some() => {
                SyncSplitIdPolicy::KeepShared
            }
            _ => SyncSplitIdPolicy::AssignNewIdToRight,
        }
    }

    fn apply_sync_splits_for_column_insert(
        &mut self,
        start: Seconds,
        duration: Seconds,
        insert_policy: InsertPolicy,
        overlap_policy: OverlapPolicy,
        id_policy: SyncSplitIdPolicy,
        scope_track_indices: Option<&[usize]>,
    ) -> bool {
        let mut split_times = Vec::new();
        if matches!(insert_policy, InsertPolicy::SplitAndInsert) {
            split_times.push(start);
        }
        if overlap_policy == OverlapPolicy::Override && duration > EPS {
            if !split_times
                .iter()
                .any(|time| (*time - start).abs() <= EPS)
            {
                split_times.push(start);
            }
            if matches!(id_policy, SyncSplitIdPolicy::KeepShared) {
                split_times.push(start + duration);
            }
        }
        split_times.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
        split_times.dedup_by(|left, right| (*left - *right).abs() <= EPS);
        for split_time in split_times {
            if !self.split_sync_clips_at_time(split_time, id_policy, scope_track_indices) {
                return false;
            }
        }
        true
    }

    fn prepare_sync_splits_for_insert(
        &mut self,
        track_index: usize,
        insert_time: Seconds,
        item: &Item,
        insert_policy: InsertPolicy,
        overlap_policy: OverlapPolicy,
    ) {
        let Some(start) = insertion_start_or_end_for_policy(
            &self.children[track_index],
            insert_time,
            insert_policy,
        ) else {
            return;
        };
        let cluster = self.boundary_group_indices(track_index);
        let id_policy = Self::sync_split_id_policy_for_inserted_item(item);
        let _ = self.apply_sync_splits_for_column_insert(
            start,
            item.duration().max(0.0),
            insert_policy,
            overlap_policy,
            id_policy,
            Some(&cluster),
        );
    }

    fn insert_at_time_with_sync_splits(
        &mut self,
        track_index: usize,
        insert_time: Seconds,
        item: Item,
        overlap_policy: OverlapPolicy,
        insert_policy: InsertPolicy,
    ) {
        if overlap_policy == OverlapPolicy::Push {
            let _ = self.children[track_index].insert_at_time(
                insert_time,
                item,
                overlap_policy,
                insert_policy,
            );
            return;
        }

        let prefer_cluster_with_video = self.children.get(track_index).is_some_and(|track| {
            track.kind == TrackKind::Video
        });
        let cluster = self.preferred_cluster_indices(track_index, prefer_cluster_with_video);
        let start = insertion_start_or_end_for_policy(
            &self.children[track_index],
            insert_time,
            insert_policy,
        )
        .unwrap_or(insert_time);
        let duration = item.duration().max(0.0);

        let primary_result = self.children[track_index].insert_at_time(
            insert_time,
            item,
            overlap_policy,
            insert_policy,
        );
        if primary_result.success {
            let updates = [(track_index, &primary_result)];
            let _ = self.propagate_insert_to_cluster(
                start,
                duration,
                overlap_policy,
                &updates,
                &cluster,
                None,
            );
        }
    }

    fn find_or_create_audio_track(
        &mut self,
        track_index: usize,
        dest_time: Seconds,
        duration: Seconds,
        created_track_indices: &mut Vec<usize>,
        used_audio_indices: &[usize],
        used_audio_boundary_indices: &[usize],
        sync_clips_id: Option<i64>,
        use_sync_backed_track: bool,
        overlap_policy: OverlapPolicy,
    ) -> Option<usize> {
        let end_time = dest_time + duration;
        if self.children.get(track_index)?.kind == TrackKind::Video {
            let mut index = track_index;
            while index > 0 && self.children[index - 1].kind == TrackKind::Audio {
                index -= 1;
                if track_is_empty_boundary(&self.children[index])
                    || used_audio_boundary_indices.contains(&index)
                {
                    break;
                }
            }
            for audio_index in (index..track_index).rev() {
                if used_audio_indices.contains(&audio_index) {
                    if used_audio_boundary_indices.contains(&audio_index) {
                        break;
                    }
                    continue;
                }
                if range_is_gap_backed(&self.children[audio_index], dest_time, end_time) {
                    return Some(audio_index);
                }
                let has_blocking_clip = range_has_blocking_clip(
                    &self.children[audio_index],
                    dest_time,
                    end_time,
                    sync_clips_id,
                );
                let can_push_existing_boundary = overlap_policy == OverlapPolicy::Push
                    && self.track_matches_primary_sync_boundary(track_index, audio_index);
                if !has_blocking_clip || can_push_existing_boundary {
                    if use_sync_backed_track {
                        return Some(audio_index);
                    }
                    continue;
                }
                let insert_at = audio_index + 1;
                let track = self.new_numbered_track(TrackKind::Audio);
                self.children.insert(insert_at, track);
                created_track_indices.push(insert_at);
                return Some(insert_at);
            }

            // No reusable audio track above the video. In the common video-over-audio
            // layout the sync track belongs on the existing audio track below the video,
            // so reuse an adjacent gap-backed audio track there before creating a new one.
            let mut below_index = track_index + 1;
            while below_index < self.children.len()
                && self.children[below_index].kind == TrackKind::Audio
            {
                if used_audio_indices.contains(&below_index) {
                    if used_audio_boundary_indices.contains(&below_index) {
                        break;
                    }
                    below_index += 1;
                    continue;
                }
                if range_is_gap_backed(&self.children[below_index], dest_time, end_time) {
                    return Some(below_index);
                }
                let has_blocking_clip = range_has_blocking_clip(
                    &self.children[below_index],
                    dest_time,
                    end_time,
                    sync_clips_id,
                );
                let can_push_existing_boundary = overlap_policy == OverlapPolicy::Push
                    && self.track_matches_primary_sync_boundary(track_index, below_index);
                if !has_blocking_clip || can_push_existing_boundary {
                    if use_sync_backed_track {
                        return Some(below_index);
                    }
                    below_index += 1;
                    continue;
                }
                break;
            }

            let track = self.new_numbered_track(TrackKind::Audio);
            self.children.insert(track_index, track);
            created_track_indices.push(track_index);
            return Some(track_index);
        }

        let mut index = match self.children.get(track_index)?.kind {
            TrackKind::Audio => {
                let mut audio_start = track_index;
                while audio_start > 0 && self.children[audio_start - 1].kind == TrackKind::Audio {
                    audio_start -= 1;
                    if track_is_empty_boundary(&self.children[audio_start])
                        || used_audio_boundary_indices.contains(&audio_start)
                    {
                        break;
                    }
                }
                audio_start
            }
            TrackKind::Video | TrackKind::Other => return None,
        };

        while index < self.children.len() && self.children[index].kind == TrackKind::Audio {
            if used_audio_indices.contains(&index) {
                if used_audio_boundary_indices.contains(&index) {
                    break;
                }
                index += 1;
                continue;
            }
            if range_is_gap_backed(&self.children[index], dest_time, end_time) {
                return Some(index);
            }
            let has_blocking_clip =
                range_has_blocking_clip(&self.children[index], dest_time, end_time, sync_clips_id);
            let can_push_existing_boundary = overlap_policy == OverlapPolicy::Push
                && self.track_matches_primary_sync_boundary(track_index, index);
            if !has_blocking_clip || can_push_existing_boundary {
                if use_sync_backed_track {
                    return Some(index);
                }
                index += 1;
                continue;
            }
            let track = self.new_numbered_track(TrackKind::Audio);
            self.children.insert(index, track);
            created_track_indices.push(index);
            return Some(index);
        }

        let insert_at = index;
        let track = self.new_numbered_track(TrackKind::Audio);
        self.children.insert(insert_at, track);
        created_track_indices.push(insert_at);
        Some(insert_at)
    }

    fn find_or_create_move_audio_track(
        &mut self,
        primary_track_index: usize,
        dest_time: Seconds,
        duration: Seconds,
        created_track_indices: &mut Vec<usize>,
        used_audio_indices: &[usize],
        used_audio_boundary_indices: &[usize],
    ) -> Option<usize> {
        let end_time = dest_time + duration;
        match self.children.get(primary_track_index)?.kind {
            TrackKind::Video => {
                let mut audio_start = primary_track_index;
                while audio_start > 0 && self.children[audio_start - 1].kind == TrackKind::Audio {
                    audio_start -= 1;
                }

                let mut crossed_used_audio_boundary = false;
                for audio_index in (audio_start..primary_track_index).rev() {
                    if used_audio_indices.contains(&audio_index) {
                        if used_audio_boundary_indices.contains(&audio_index) {
                            crossed_used_audio_boundary = true;
                        }
                        continue;
                    }
                    if crossed_used_audio_boundary
                        && !track_is_empty_boundary(&self.children[audio_index])
                    {
                        continue;
                    }
                    if range_is_gap_backed(&self.children[audio_index], dest_time, end_time) {
                        return Some(audio_index);
                    }
                    if self.track_matches_primary_sync_boundary(primary_track_index, audio_index) {
                        return Some(audio_index);
                    }
                }

                // No reusable audio track above the video. In the common video-over-audio
                // layout the sync clip belongs on the existing audio track below the video,
                // so reuse an adjacent audio track there before creating a new one.
                let mut crossed_used_audio_boundary_below = false;
                let mut below_index = primary_track_index + 1;
                while below_index < self.children.len()
                    && self.children[below_index].kind == TrackKind::Audio
                {
                    if used_audio_indices.contains(&below_index) {
                        if used_audio_boundary_indices.contains(&below_index) {
                            crossed_used_audio_boundary_below = true;
                        }
                        below_index += 1;
                        continue;
                    }
                    if crossed_used_audio_boundary_below
                        && !track_is_empty_boundary(&self.children[below_index])
                    {
                        below_index += 1;
                        continue;
                    }
                    if range_is_gap_backed(&self.children[below_index], dest_time, end_time) {
                        return Some(below_index);
                    }
                    if self.track_matches_primary_sync_boundary(primary_track_index, below_index) {
                        return Some(below_index);
                    }
                    below_index += 1;
                }

                let insert_at = if audio_start < primary_track_index {
                    audio_start
                } else {
                    primary_track_index
                };
                self.children
                    .insert(insert_at, self.new_numbered_track(TrackKind::Audio));
                created_track_indices.push(insert_at);
                Some(insert_at)
            }
            TrackKind::Audio => {
                let mut audio_start = primary_track_index;
                while audio_start > 0 && self.children[audio_start - 1].kind == TrackKind::Audio {
                    audio_start -= 1;
                }
                let mut audio_end = primary_track_index + 1;
                while audio_end < self.children.len()
                    && self.children[audio_end].kind == TrackKind::Audio
                {
                    audio_end += 1;
                }

                let mut candidates: Vec<_> = (audio_start..audio_end)
                    .filter(|track_index| {
                        *track_index != primary_track_index
                            && !used_audio_indices.contains(track_index)
                            && range_is_gap_backed(
                                &self.children[*track_index],
                                dest_time,
                                end_time,
                            )
                    })
                    .collect();
                candidates.sort_by_key(|track_index| {
                    (track_index.abs_diff(primary_track_index), *track_index)
                });
                if let Some(track_index) = candidates.into_iter().next() {
                    return Some(track_index);
                }

                self.children
                    .insert(audio_end, self.new_numbered_track(TrackKind::Audio));
                created_track_indices.push(audio_end);
                Some(audio_end)
            }
            TrackKind::Other => None,
        }
    }

    fn try_reuse_video_track_for_audio_move(
        &self,
        track_index: usize,
        audio_track_index: usize,
        dest_time: Seconds,
        end_time: Seconds,
        sync_clips_id: Option<i64>,
        use_sync_backed_track: bool,
        overlap_policy: OverlapPolicy,
        insert_policy: InsertPolicy,
    ) -> Option<usize> {
        if self.children.get(track_index)?.kind != TrackKind::Video {
            return None;
        }
        if range_is_gap_backed(&self.children[track_index], dest_time, end_time) {
            return Some(track_index);
        }
        let has_blocking_clip = range_has_blocking_clip(
            &self.children[track_index],
            dest_time,
            end_time,
            sync_clips_id,
        );
        let can_push_existing_boundary = overlap_policy == OverlapPolicy::Push
            && self.track_matches_primary_sync_boundary(audio_track_index, track_index);
        // Override + SplitAndInsert splits destination clips at the insert
        // point before the column lands, so an occupied cluster video track
        // is still the correct target (e.g. cut into link group 7 on Video 2).
        let will_split_for_override = overlap_policy == OverlapPolicy::Override
            && matches!(insert_policy, InsertPolicy::SplitAndInsert);
        if !has_blocking_clip || can_push_existing_boundary || will_split_for_override {
            return use_sync_backed_track.then_some(track_index);
        }
        None
    }

    fn find_or_create_video_track_for_audio(
        &mut self,
        audio_track_index: usize,
        dest_time: Seconds,
        duration: Seconds,
        created_track_indices: &mut Vec<usize>,
        sync_clips_id: Option<i64>,
        use_sync_backed_track: bool,
        overlap_policy: OverlapPolicy,
        insert_policy: InsertPolicy,
    ) -> Option<usize> {
        let end_time = dest_time + duration;

        // Prefer the destination boundary cluster's own video track before a
        // physically adjacent video from a neighboring cluster (e.g. reuse
        // Video 2 when inserting on A9, not Main Video sitting above A9).
        let cluster = self.boundary_group_indices(audio_track_index);
        let mut cluster_video_tracks: Vec<usize> = cluster
            .iter()
            .copied()
            .filter(|&track_index| {
                self.children
                    .get(track_index)
                    .is_some_and(|track| track.kind == TrackKind::Video)
            })
            .collect();
        // Prefer the cluster video below the destination audio (Resolve
        // audio-over-video layout) before any video above the group.
        cluster_video_tracks.sort_by_key(|track_index| {
            if *track_index > audio_track_index {
                0
            } else {
                1
            }
        });
        for track_index in cluster_video_tracks {
            if let Some(track_index) = self.try_reuse_video_track_for_audio_move(
                track_index,
                audio_track_index,
                dest_time,
                end_time,
                sync_clips_id,
                use_sync_backed_track,
                overlap_policy,
                insert_policy,
            ) {
                return Some(track_index);
            }
        }

        let mut audio_end = audio_track_index + 1;
        while audio_end < self.children.len() && self.children[audio_end].kind == TrackKind::Audio {
            audio_end += 1;
        }

        // Create the new video track directly below the moving audio group.
        let insert_at = audio_end;
        let track = self.new_numbered_track(TrackKind::Video);
        self.children.insert(insert_at, track);
        created_track_indices.push(insert_at);
        Some(insert_at)
    }

    fn item_occupies_column(
        &self,
        track_index: usize,
        item_index: usize,
        column_start: Seconds,
        column_duration: Seconds,
    ) -> bool {
        let Some(track) = self.children.get(track_index) else {
            return false;
        };
        if item_index >= track.items.len() {
            return false;
        }
        (track.start_time_of_item(item_index) - column_start).abs() <= EPS
            && (track.items[item_index].duration() - column_duration).abs() <= EPS
    }

    fn item_at_column(
        &self,
        track_index: usize,
        column_start: Seconds,
        column_duration: Seconds,
    ) -> Option<usize> {
        let track = self.children.get(track_index)?;
        let item_index = track.get_item_at_time(column_start)?;
        self.item_occupies_column(track_index, item_index, column_start, column_duration)
            .then_some(item_index)
    }

    fn delete_clips_at_indices(
        &mut self,
        mut targets: Vec<(usize, usize)>,
        replace_with_gap: bool,
    ) -> Vec<(usize, Item)> {
        targets.sort_by(|a, b| b.cmp(a));
        targets.dedup();
        let mut removed = Vec::new();
        let mut used_ids = replace_with_gap.then(|| self.collect_timeline_ids());
        for (ti, ii) in targets {
            if ti >= self.children.len() {
                continue;
            }
            let track = &mut self.children[ti];
            if ii >= track.items.len() {
                continue;
            }
            if matches!(track.items[ii], Item::Gap(_)) && replace_with_gap {
                continue;
            }
            let start = track.start_time_of_item(ii);
            let end = start + track.items[ii].duration().max(0.0);
            let mut removed_items = track.delete_range(start, end, replace_with_gap);
            let Some(removed_item) = removed_items.pop() else {
                continue;
            };
            if replace_with_gap {
                if let (Some(used_ids), Some(item)) = (
                    &mut used_ids,
                    track
                        .get_item_at_time(start)
                        .and_then(|gap_index| track.items.get_mut(gap_index)),
                ) {
                    if matches!(item, Item::Gap(_)) {
                        Self::ensure_unique_item_id(item, used_ids);
                    }
                }
            }
            removed.push((ti, removed_item));
        }
        removed.reverse();
        removed
    }

    fn delete_item_targets(&self, item_id: &str) -> Option<Vec<(usize, usize)>> {
        let (track_index, item_index, item) = self.get_item(item_id)?;
        if matches!(item, Item::Gap(_)) {
            return None;
        }
        let sync_clips_id = match &item {
            Item::Clip(clip) => resolve_sync_clips_id(&clip.metadata),
            Item::Gap(_) => None,
        };
        if let Some(sync_id) = sync_clips_id {
            Some(self.synced_clips_targets(sync_id))
        } else {
            Some(vec![(track_index, item_index)])
        }
    }

    fn delete_item_replace_with_gap(&mut self, item_id: &str) -> Vec<(usize, Item)> {
        let Some(targets) = self.delete_item_targets(item_id) else {
            return Vec::new();
        };
        let removed = self.delete_clips_at_indices(targets, true);
        if !removed.is_empty() {
            self.sanitize();
        }
        removed
    }

    fn sync_groups_behind_clips(&self, clips_to_delete: &[(usize, usize)]) -> HashSet<i64> {
        let mut sync_groups = HashSet::new();
        for &(ti, ii) in clips_to_delete {
            let Some(track) = self.children.get(ti) else {
                continue;
            };
            for item in track.items.iter().skip(ii + 1) {
                if let Item::Clip(clip) = item {
                    if let Some(sync_id) = resolve_sync_clips_id(&clip.metadata) {
                        sync_groups.insert(sync_id);
                    }
                }
            }
        }
        sync_groups
    }

    fn collapse_mutation_tracks(
        &self,
        clips_to_delete: &[(usize, usize)],
        sync_groups_behind: &HashSet<i64>,
    ) -> HashSet<usize> {
        let mut tracks: HashSet<usize> = clips_to_delete.iter().map(|(ti, _)| *ti).collect();
        for sync_id in sync_groups_behind {
            for (ti, _) in self.synced_clips_targets(*sync_id) {
                tracks.insert(ti);
            }
        }
        tracks
    }

    fn delete_item_collapse(&mut self, item_id: &str) -> Vec<(usize, Item)> {
        let Some((track_index, item_index, item)) = self.get_item(item_id) else {
            return Vec::new();
        };

        if matches!(item, Item::Gap(_)) {
            let removed = self.delete_clips_at_indices(vec![(track_index, item_index)], false);
            if !removed.is_empty() {
                self.sanitize();
            }
            return removed;
        }

        let Some(clips_to_delete) = self.delete_item_targets(item_id) else {
            return Vec::new();
        };
        if clips_to_delete.is_empty() {
            return Vec::new();
        }

        let column_start = self.children[track_index].start_time_of_item(item_index);
        let column_end = column_start + item.duration().max(0.0);
        let sync_groups_behind = self.sync_groups_behind_clips(&clips_to_delete);
        let tracks_to_mutate =
            self.collapse_mutation_tracks(&clips_to_delete, &sync_groups_behind);

        let mut removed = self.delete_clips_at_indices(clips_to_delete, false);
        let mutated_tracks: HashSet<usize> = removed.iter().map(|(ti, _)| *ti).collect();

        let mut pending_range_tracks: Vec<_> = tracks_to_mutate
            .into_iter()
            .filter(|ti| !mutated_tracks.contains(ti))
            .collect();
        pending_range_tracks.sort_unstable();
        pending_range_tracks.dedup();

        for ti in pending_range_tracks {
            let Some(track) = self.children.get_mut(ti) else {
                continue;
            };
            for removed_item in track.delete_range(column_start, column_end, false) {
                removed.push((ti, removed_item));
            }
        }

        if !removed.is_empty() {
            self.sanitize();
        }
        removed
    }

    fn delete_sync_clips(
        &mut self,
        sync_clips_id: i64,
        replace_with_gap: bool,
    ) -> Vec<(usize, Item)> {
        self.delete_clips_at_indices(self.synced_clips_targets(sync_clips_id), replace_with_gap)
    }

    fn clip_target(&self, item_id: &str) -> Option<(usize, usize)> {
        let (track_index, item_index, item) = self.get_item(item_id)?;
        matches!(item, Item::Clip(_)).then_some((track_index, item_index))
    }

    fn synced_clips_targets(&self, sync_clips_id: i64) -> Vec<(usize, usize)> {
        let mut targets = Vec::new();
        for (ti, track) in self.children.iter().enumerate() {
            for (ii, item) in track.items.iter().enumerate() {
                if let Item::Clip(clip) = item {
                    if resolve_sync_clips_id(&clip.metadata) == Some(sync_clips_id) {
                        targets.push((ti, ii));
                    }
                }
            }
        }
        targets
    }

    fn sync_column_targets_at(
        &self,
        sync_clips_id: i64,
        column_start: Seconds,
        column_duration: Seconds,
    ) -> Vec<(usize, usize)> {
        self.synced_clips_targets(sync_clips_id)
            .into_iter()
            .filter(|(track_index, item_index)| {
                self.item_occupies_column(*track_index, *item_index, column_start, column_duration)
            })
            .collect()
    }

    fn is_independent_sync_column(
        &self,
        sync_clips_id: i64,
        column_start: Seconds,
        column_duration: Seconds,
    ) -> bool {
        self.sync_column_targets_at(sync_clips_id, column_start, column_duration)
            .len()
            > 1
    }

    fn item_is_unsynced(item: &Item) -> bool {
        match item {
            Item::Clip(clip) => resolve_sync_clips_id(&clip.metadata).is_none(),
            Item::Gap(_) => true,
        }
    }

    /// Every track in the cluster has an unsynced item at `column_start` with `duration`.
    /// This is the column shape produced by `insert_synced_item_at_time` padding.
    fn is_cluster_column_at(
        &self,
        cluster: &[usize],
        column_start: Seconds,
        duration: Seconds,
    ) -> bool {
        cluster.len() > 1
            && cluster.iter().all(|&track_index| {
                let Some(track) = self.children.get(track_index) else {
                    return false;
                };
                let Some(item_index) = track.get_item_at_time(column_start) else {
                    return false;
                };
                (track.start_time_of_item(item_index) - column_start).abs() <= EPS
                    && Self::item_is_unsynced(&track.items[item_index])
                    && (track.items[item_index].duration() - duration).abs() <= EPS
            })
    }

    /// Resize an insert-style cluster column: unsynced items at the same start on
    /// every bound track, then realign synced assets across the cluster.
    fn apply_cluster_column_duration_change(
        &mut self,
        item_id: &str,
        cluster: &[usize],
        column_start: Seconds,
        old_duration: Seconds,
        new_duration: Seconds,
        source_start_time: Seconds,
        clamp_to_media: bool,
    ) -> bool {
        for &track_index in cluster {
            let Some(track) = self.children.get_mut(track_index) else {
                return false;
            };
            let Some(item_index) = track.get_item_at_time(column_start) else {
                return false;
            };
            if (track.start_time_of_item(item_index) - column_start).abs() > EPS {
                return false;
            }
            let item = &mut track.items[item_index];
            if !Self::item_is_unsynced(item) {
                return false;
            }
            if (item.duration() - old_duration).abs() > EPS {
                return false;
            }
            if item.get_id().as_deref() == Some(item_id) {
                if let Item::Clip(clip) = item {
                    let _ = clip;
                    set_item_source_start(item, source_start_time);
                }
            }
            item.set_duration(new_duration);
            if clamp_to_media {
                item.clamp_to_active_available_range();
            }
            track.sanitize_preserving_all_gap_track();
        }

        true
    }

    fn synced_clip_states(&self) -> HashMap<String, SyncedClipState> {
        let mut states = HashMap::new();
        for (track_index, track) in self.children.iter().enumerate() {
            for (item_index, item) in track.items.iter().enumerate() {
                let Item::Clip(clip) = item else {
                    continue;
                };
                let Some(sync_clips_id) = resolve_sync_clips_id(&clip.metadata) else {
                    continue;
                };
                let Some(id) = item.get_id() else {
                    continue;
                };
                states.insert(
                    id,
                    SyncedClipState {
                        track_index,
                        start: track.start_time_of_item(item_index),
                        duration: item.duration().max(0.0),
                        sync_clips_id,
                    },
                );
            }
        }
        states
    }

    fn cleanup_singleton_sync_clips(&mut self, sync_clips_ids: &[i64]) -> usize {
        let mut count = 0;
        let mut seen = HashSet::new();
        for sync_clips_id in sync_clips_ids {
            if !seen.insert(*sync_clips_id) {
                continue;
            }
            let targets = self.synced_clips_targets(*sync_clips_id);
            if targets.len() > 1 {
                continue;
            }
            for (track_index, item_index) in targets {
                let Some(Item::Clip(clip)) = self
                    .children
                    .get_mut(track_index)
                    .and_then(|track| track.items.get_mut(item_index))
                else {
                    continue;
                };
                if remove_resolve_sync_clips_id(&mut clip.metadata) {
                    count += 1;
                }
            }
        }
        count
    }

    fn set_item_sync_clips(item: &mut Item, sync_clips_id: Option<i64>) {
        let Item::Clip(clip) = item else {
            return;
        };
        if let Some(sync_clips_id) = sync_clips_id {
            set_resolve_sync_clips_id(&mut clip.metadata, sync_clips_id);
        } else {
            remove_resolve_sync_clips_id(&mut clip.metadata);
        }
    }

    fn new_numbered_track(&self, kind: TrackKind) -> Track {
        let prefix = match kind {
            TrackKind::Audio => "A",
            TrackKind::Video => "V",
            TrackKind::Other => "T",
        };
        let used: HashSet<_> = self
            .children
            .iter()
            .flat_map(|track| [track.get_id(), track.name.clone()])
            .flatten()
            .collect();
        let id = (1..=99)
            .map(|index| format!("{prefix}{index}"))
            .find(|candidate| !used.contains(candidate))
            .unwrap_or_else(crate::types::gen_hex_id_12);
        let mut track = Track::new(kind, Some(id.clone()));
        track.name = Some(id);
        track
    }

    fn prepare_synced_item(
        item: Item,
        duration: Seconds,
        sync_clips_id: Option<i64>,
        used_ids: &mut HashSet<String>,
    ) -> Option<(Item, String)> {
        let Item::Clip(mut clip) = item else {
            return None;
        };
        clip.source_range.duration.set_from_seconds(duration);
        clamp_clip_to_active_available_range(&mut clip);
        let clamped_duration = clip.source_range.duration.to_seconds();
        if clamped_duration + EPS < duration || clamped_duration <= EPS
        {
            return None;
        }

        let mut item = Item::Clip(clip);
        Self::set_item_sync_clips(&mut item, sync_clips_id);
        let id = Self::ensure_unique_item_id(&mut item, used_ids);
        Some((item, id))
    }

    fn prepare_synced_item_preserve_duration(
        item: Item,
        sync_clips_id: Option<i64>,
        used_ids: &mut HashSet<String>,
    ) -> Option<(Item, String)> {
        let Item::Clip(mut clip) = item else {
            return None;
        };
        clamp_clip_to_active_available_range(&mut clip);
        if clip.source_range.duration.to_seconds() <= EPS {
            return None;
        }

        let mut item = Item::Clip(clip);
        Self::set_item_sync_clips(&mut item, sync_clips_id);
        let id = Self::ensure_unique_item_id(&mut item, used_ids);
        Some((item, id))
    }

    fn synced_insert_column_span(primary_duration: Seconds, inputs: &SyncedInputs) -> Seconds {
        let mut span = primary_duration;
        for item in &inputs.audio {
            if let Some(duration) = Self::sanitized_clip_duration(item) {
                span = span.max(duration);
            }
        }
        if let Some(video_item) = &inputs.video {
            if let Some(duration) = Self::sanitized_clip_duration(video_item) {
                span = span.max(duration);
            }
        }
        span
    }

    fn sanitized_clip_duration(item: &Item) -> Option<Seconds> {
        let Item::Clip(mut clip) = item.clone() else {
            return None;
        };
        clamp_clip_to_active_available_range(&mut clip);
        let duration = clip.source_range.duration.to_seconds().max(0.0);
        (duration > EPS).then_some(duration)
    }

    fn synced_inputs_match_duration(duration: Seconds, inputs: &SyncedInputs) -> bool {
        for audio_item in &inputs.audio {
            let Some(audio_duration) = Self::sanitized_clip_duration(audio_item) else {
                return false;
            };
            if (audio_duration - duration).abs() > EPS {
                return false;
            }
        }
        if let Some(video_item) = &inputs.video {
            let Some(video_duration) = Self::sanitized_clip_duration(video_item) else {
                return false;
            };
            if (video_duration - duration).abs() > EPS {
                return false;
            }
        }

        true
    }

    fn normalize_synced_inputs(
        synced_audio_clips: Option<Vec<Item>>,
        synced_video_clip: Option<Item>,
    ) -> SyncedInputs {
        let mut inputs = SyncedInputs {
            audio: synced_audio_clips.unwrap_or_default(),
            video: synced_video_clip,
        };
        for item in &mut inputs.audio {
            item.clamp_to_active_available_range();
        }
        if let Some(video) = &mut inputs.video {
            video.clamp_to_active_available_range();
        }
        inputs
    }

    fn destination_move_audio_candidates(
        &self,
        dest_track_index: usize,
        used_audio_indices: &[usize],
    ) -> Vec<usize> {
        let Some(dest_track) = self.children.get(dest_track_index) else {
            return Vec::new();
        };
        match dest_track.kind {
            TrackKind::Video => {
                let mut candidates = Vec::new();
                let mut above = dest_track_index;
                while above > 0 && self.children[above - 1].kind == TrackKind::Audio {
                    above -= 1;
                    if !used_audio_indices.contains(&above) {
                        candidates.push(above);
                    }
                }
                let mut below = dest_track_index + 1;
                while below < self.children.len() && self.children[below].kind == TrackKind::Audio {
                    if !used_audio_indices.contains(&below) {
                        candidates.push(below);
                    }
                    below += 1;
                }
                candidates
            }
            TrackKind::Audio => self
                .boundary_group_indices(dest_track_index)
                .into_iter()
                .filter(|&track_index| {
                    track_index != dest_track_index
                        && self.children.get(track_index).is_some_and(|track| {
                            track.kind == TrackKind::Audio
                                && !used_audio_indices.contains(&track_index)
                        })
                })
                .collect(),
            TrackKind::Other => Vec::new(),
        }
    }

    fn find_usable_destination_move_audio_track(
        &self,
        dest_track_index: usize,
        dest_time: Seconds,
        duration: Seconds,
        used_audio_indices: &[usize],
        exclude_track_indices: &HashSet<usize>,
    ) -> Option<usize> {
        let end_time = dest_time + duration;
        self.destination_move_audio_candidates(dest_track_index, used_audio_indices)
            .into_iter()
            .filter(|track_index| !exclude_track_indices.contains(track_index))
            .find(|&track_index| {
                self.children.get(track_index).is_some_and(|track| {
                    track_is_empty_boundary(track)
                        || range_is_gap_backed(track, dest_time, end_time)
                        || self.track_matches_primary_sync_boundary(dest_track_index, track_index)
                })
            })
    }

    fn has_non_source_destination_audio_tracks(
        &self,
        dest_track_index: usize,
        exclude_track_indices: &HashSet<usize>,
    ) -> bool {
        self.destination_move_audio_candidates(dest_track_index, &[])
            .iter()
            .any(|track_index| !exclude_track_indices.contains(track_index))
    }

    fn preferred_move_audio_track_usable(
        &self,
        dest_track_index: usize,
        candidate_index: usize,
        dest_time: Seconds,
        duration: Seconds,
        used_audio_indices: &[usize],
        exclude_track_indices: &HashSet<usize>,
    ) -> bool {
        if candidate_index == dest_track_index {
            return false;
        }
        let Some(track) = self.children.get(candidate_index) else {
            return false;
        };
        if track.kind != TrackKind::Audio || used_audio_indices.contains(&candidate_index) {
            return false;
        }
        let end_time = dest_time + duration;
        let dest_cluster: HashSet<usize> = self
            .boundary_group_indices(dest_track_index)
            .into_iter()
            .collect();
        if !dest_cluster.contains(&candidate_index)
            && self.has_non_source_destination_audio_tracks(dest_track_index, exclude_track_indices)
        {
            return false;
        }
        if track_is_empty_boundary(track) {
            return true;
        }
        if range_is_gap_backed(track, dest_time, end_time) {
            return true;
        }
        self.track_matches_primary_sync_boundary(dest_track_index, candidate_index)
    }

    fn find_or_create_destination_move_audio_track(
        &mut self,
        dest_track_index: usize,
        dest_time: Seconds,
        duration: Seconds,
        created_track_indices: &mut Vec<usize>,
        used_audio_indices: &[usize],
        exclude_track_indices: &HashSet<usize>,
    ) -> Option<usize> {
        if let Some(track_index) = self.find_usable_destination_move_audio_track(
            dest_track_index,
            dest_time,
            duration,
            used_audio_indices,
            exclude_track_indices,
        ) {
            return Some(track_index);
        }

        let insert_at = match self.children.get(dest_track_index)?.kind {
            TrackKind::Video => {
                let mut above = dest_track_index;
                while above > 0 && self.children[above - 1].kind == TrackKind::Audio {
                    above -= 1;
                }
                if above < dest_track_index {
                    dest_track_index
                } else {
                    dest_track_index + 1
                }
            }
            TrackKind::Audio => dest_track_index + 1,
            TrackKind::Other => return None,
        };
        self.children
            .insert(insert_at, self.new_numbered_track(TrackKind::Audio));
        created_track_indices.push(insert_at);
        Some(insert_at)
    }

    fn assign_move_audio_slots(
        &mut self,
        dest_track_index: &mut usize,
        cluster: &mut Vec<usize>,
        created_track_indices: &mut Vec<usize>,
        start: Seconds,
        audio_durations: &[Seconds],
        preferred_indices: &[usize],
        exclude_track_indices: &HashSet<usize>,
    ) -> Option<Vec<usize>> {
        let needed = audio_durations.len();
        let mut audio_slots = Vec::with_capacity(needed);
        let mut used_audio_indices = Vec::new();
        let mut used_audio_boundary_indices = Vec::new();

        for (audio_index, &duration) in audio_durations.iter().enumerate() {
            let track_count_before = self.children.len();
            let preferred = preferred_indices.get(audio_index).copied();
            let track_index = preferred
                .filter(|&preferred_index| {
                    self.preferred_move_audio_track_usable(
                        *dest_track_index,
                        preferred_index,
                        start,
                        duration,
                        &used_audio_indices,
                        exclude_track_indices,
                    )
                })
                .or_else(|| {
                    self.find_usable_destination_move_audio_track(
                        *dest_track_index,
                        start,
                        duration,
                        &used_audio_indices,
                        exclude_track_indices,
                    )
                })
                .or_else(|| {
                    self.find_or_create_destination_move_audio_track(
                        *dest_track_index,
                        start,
                        duration,
                        created_track_indices,
                        &used_audio_indices,
                        exclude_track_indices,
                    )
                })?;

            if self.children.len() > track_count_before {
                Self::shift_insert_track_indices_after_create(
                    track_index,
                    dest_track_index,
                    cluster,
                    &mut audio_slots,
                    created_track_indices,
                );
            }
            let reused_empty_boundary = self.children.len() == track_count_before
                && track_is_empty_boundary(&self.children[track_index]);
            audio_slots.push(track_index);
            used_audio_indices.push(track_index);
            if reused_empty_boundary {
                used_audio_boundary_indices.push(track_index);
            }
        }

        Some(audio_slots)
    }

    fn shift_insert_track_indices_after_create(
        inserted_at: usize,
        dest_track_index: &mut usize,
        cluster: &mut [usize],
        audio_slots: &mut [usize],
        created_track_indices: &mut [usize],
    ) {
        if inserted_at <= *dest_track_index {
            *dest_track_index += 1;
        }
        for index in cluster.iter_mut() {
            if *index >= inserted_at {
                *index += 1;
            }
        }
        for index in audio_slots.iter_mut() {
            if *index >= inserted_at {
                *index += 1;
            }
        }
        for index in created_track_indices.iter_mut() {
            if *index >= inserted_at {
                *index += 1;
            }
        }
    }

    fn flatten_track_segments(&self, track_index: usize) -> Vec<BoundarySegment> {
        let Some(track) = self.children.get(track_index) else {
            return Vec::new();
        };
        let mut segments = Vec::new();
        let mut start = 0.0;
        for item in &track.items {
            let duration = item.duration().max(0.0);
            let sync_clips_id = match item {
                Item::Clip(clip) => resolve_sync_clips_id(&clip.metadata),
                Item::Gap(_) => None,
            };
            segments.push(BoundarySegment {
                start,
                end: start + duration,
                sync_clips_id,
            });
            start += duration;
        }
        segments
    }

    pub(super) fn track_sync_clips_ids(&self, track_index: usize) -> HashSet<i64> {
        let Some(track) = self.children.get(track_index) else {
            return HashSet::new();
        };
        track
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Clip(clip) => resolve_sync_clips_id(&clip.metadata),
                Item::Gap(_) => None,
            })
            .collect()
    }

    pub(super) fn tracks_share_sync_clips(
        &self,
        left_track_index: usize,
        right_track_index: usize,
    ) -> bool {
        if left_track_index == right_track_index {
            return true;
        }
        let left_groups = self.track_sync_clips_ids(left_track_index);
        !left_groups.is_empty()
            && self
                .track_sync_clips_ids(right_track_index)
                .iter()
                .any(|sync_clips_id| left_groups.contains(sync_clips_id))
    }

    fn flatten_boundary_for_sync_clips(
        &self,
        anchor_track_index: usize,
        sync_clips: &[i64],
    ) -> FlattenedBoundary {
        if anchor_track_index >= self.children.len() {
            return FlattenedBoundary {
                track_indices: Vec::new(),
            };
        }

        let mut track_indices = Vec::new();
        match self.children[anchor_track_index].kind {
            TrackKind::Video => {
                for track_index in (0..anchor_track_index).rev() {
                    if self.children[track_index].kind != TrackKind::Audio {
                        break;
                    }
                    if track_is_empty_boundary(&self.children[track_index]) {
                        track_indices.push(track_index);
                        break;
                    }
                    if !self
                        .track_matches_primary_sync_boundary(anchor_track_index, track_index)
                    {
                        break;
                    }
                    track_indices.push(track_index);
                }
                track_indices.reverse();
                track_indices.push(anchor_track_index);
                if !sync_clips.is_empty() {
                    for track_index in (anchor_track_index + 1)..self.children.len() {
                        if self.children[track_index].kind != TrackKind::Audio {
                            break;
                        }
                        if track_is_empty_boundary(&self.children[track_index]) {
                            track_indices.push(track_index);
                            break;
                        }
                        if !self
                            .track_matches_primary_sync_boundary(anchor_track_index, track_index)
                        {
                            break;
                        }
                        track_indices.push(track_index);
                    }
                }
            }
            TrackKind::Audio => {
                let mut audio_start = anchor_track_index;
                while audio_start > 0 && self.children[audio_start - 1].kind == TrackKind::Audio {
                    let previous = audio_start - 1;
                    if track_is_empty_boundary(&self.children[previous])
                        || !self.track_matches_primary_sync_boundary(anchor_track_index, previous)
                    {
                        break;
                    }
                    audio_start = previous;
                }
                if !sync_clips.is_empty() {
                    let previous_video_index = audio_start.checked_sub(1);
                    if let Some(video_index) = previous_video_index {
                        if self.children[video_index].kind == TrackKind::Video
                            && self
                                .track_matches_primary_sync_boundary(anchor_track_index, video_index)
                        {
                            track_indices.push(video_index);
                        }
                    }
                }
                for track_index in audio_start..self.children.len() {
                    if self.children[track_index].kind != TrackKind::Audio {
                        break;
                    }
                    if track_index != anchor_track_index
                        && !track_is_empty_boundary(&self.children[track_index])
                        && !self.track_matches_primary_sync_boundary(
                            anchor_track_index,
                            track_index,
                        )
                    {
                        break;
                    }
                    track_indices.push(track_index);
                    if track_index != anchor_track_index
                        && track_is_empty_boundary(&self.children[track_index])
                    {
                        break;
                    }
                }
                let video_index = track_indices.last().copied().unwrap_or(anchor_track_index) + 1;
                if video_index < self.children.len()
                    && self.children[video_index].kind == TrackKind::Video
                    && self.track_matches_primary_sync_boundary(anchor_track_index, video_index)
                {
                    track_indices.push(video_index);
                }
            }
            TrackKind::Other => track_indices.push(anchor_track_index),
        }

        FlattenedBoundary { track_indices }
    }

    fn track_matches_primary_sync_boundary(
        &self,
        primary_track_index: usize,
        candidate_track_index: usize,
    ) -> bool {
        self.tracks_share_sync_clips(primary_track_index, candidate_track_index)
    }

    fn boundary_track_indices_for_anchors(
        &self,
        sync_clips: &[i64],
        anchor_track_indices: &[usize],
        excluded_track_indices: &[usize],
    ) -> Vec<usize> {
        let mut track_indices = Vec::new();
        for anchor_track_index in anchor_track_indices {
            for track_index in &self
                .flatten_boundary_for_sync_clips(*anchor_track_index, sync_clips)
                .track_indices
            {
                if excluded_track_indices.contains(track_index) {
                    continue;
                }
                if !track_indices.contains(track_index) {
                    track_indices.push(*track_index);
                }
            }
        }
        track_indices
    }

    fn synced_clips_adjacent_to_time(&self, track_index: usize, time: Seconds) -> Vec<i64> {
        let mut groups = Vec::new();
        for segment in self.flatten_track_segments(track_index) {
            if (segment.start - time).abs() > EPS && (segment.end - time).abs() > EPS {
                continue;
            }
            if let Some(group) = segment.sync_clips_id {
                if !groups.contains(&group) {
                    groups.push(group);
                }
            }
        }
        groups
    }

    fn sync_changed_groups_after_resize(
        &mut self,
        before_states: &HashMap<String, SyncedClipState>,
        modified_track_indices: &[usize],
        excluded_ids: &HashSet<String>,
        overlap_policy: OverlapPolicy,
    ) -> bool {
        let mut synced_clips = HashSet::new();
        let mut changed_groups = Vec::new();
        for (id, before) in before_states {
            if excluded_ids.contains(id) || !modified_track_indices.contains(&before.track_index) {
                continue;
            }
            let Some((track_index, item_index, item)) = self.get_item(id) else {
                continue;
            };
            let start = self.children[track_index].start_time_of_item(item_index);
            let duration = item.duration().max(0.0);
            if ((start - before.start).abs() > EPS || (duration - before.duration).abs() > EPS)
                && synced_clips.insert(before.sync_clips_id)
            {
                changed_groups.push((before.sync_clips_id, start - before.start, duration));
            }
        }

        for (sync_clips_id, start_delta, duration) in changed_groups {
            if !self.shift_sync_clips_by_delta(
                sync_clips_id,
                before_states,
                modified_track_indices,
                start_delta,
                duration,
                overlap_policy,
            ) {
                return false;
            }
        }
        true
    }

    fn shift_sync_clips_by_delta(
        &mut self,
        sync_clips_id: i64,
        before_states: &HashMap<String, SyncedClipState>,
        modified_track_indices: &[usize],
        start_delta: Seconds,
        duration: Seconds,
        overlap_policy: OverlapPolicy,
    ) -> bool {
        let mut items = Vec::new();
        for (track_index, item_index) in self.synced_clips_targets(sync_clips_id) {
            let Some(item) = self
                .children
                .get(track_index)
                .and_then(|track| track.items.get(item_index))
                .cloned()
            else {
                return false;
            };
            let Some(id) = item.get_id() else {
                return false;
            };
            let Some(before) = before_states.get(&id) else {
                continue;
            };
            if modified_track_indices.contains(&before.track_index) {
                continue;
            }
            let mut item = item;
            item.set_duration(duration);
            items.push((id, before.track_index, before.start + start_delta, item));
        }

        for (id, _, _, _) in &items {
            let Some((track_index, item_index, _)) = self.get_item(id) else {
                return false;
            };
            let Some(track) = self.children.get_mut(track_index) else {
                return false;
            };
            if item_index >= track.items.len() {
                return false;
            }
            track.items.remove(item_index);
            track.sanitize_preserving_all_gap_track();
        }

        for (_, track_index, start, item) in items {
            let Some(track) = self.children.get_mut(track_index) else {
                return false;
            };
            track.insert_at_time(start, item, overlap_policy, InsertPolicy::SplitAndInsert);
        }
        true
    }

    fn insert_synced_item_at_time(
        &mut self,
        dest_track_index: usize,
        dest_time: Seconds,
        dest_index: Option<usize>,
        item: Item,
        overlap_policy: OverlapPolicy,
        insert_policy: InsertPolicy,
        synced_audio_clips: Option<Vec<Item>>,
        synced_video_clip: Option<Item>,
        preferred_video_track_id: Option<&str>,
        preferred_audio_track_indices: Option<&[usize]>,
        move_source_track_indices: Option<&[usize]>,
    ) -> Option<InsertItemAtTimeResult> {
        let mut primary_item = item;
        primary_item.clamp_to_active_available_range();
        let synced_inputs =
            Self::normalize_synced_inputs(synced_audio_clips, synced_video_clip);
        let has_synced_clips =
            !synced_inputs.audio.is_empty() || synced_inputs.video.is_some();
        if has_synced_clips && !matches!(primary_item, Item::Clip(_)) {
            return None;
        }
        if synced_inputs.video.is_some()
            && self.children.get(dest_track_index)?.kind == TrackKind::Video
        {
            return None;
        }
        if matches!(synced_inputs.video, Some(Item::Gap(_))) {
            return None;
        }

        let modified_duration = primary_item.duration().max(0.0);
        if modified_duration <= EPS {
            return None;
        }
        let column_span = Self::synced_insert_column_span(modified_duration, &synced_inputs);

        // Resolved start on the primary track; every column member lands here.
        let start = if let Some(dest_index) = dest_index {
            self.children[dest_track_index].start_time_of_item(dest_index)
        } else {
            insertion_start_or_end_for_policy(
                &self.children[dest_track_index],
                dest_time,
                insert_policy,
            )?
        };

        let backup = self.clone();
        let mut used_ids = self.collect_timeline_ids();
        let sync_clips_id = has_synced_clips.then(|| self.next_sync_clips_id());
        let mut created_track_indices = Vec::new();

        // Pick the best sync cluster for the destination: prefer video when inserting
        // on a video track or supplying a synced video clip, otherwise prefer most tracks.
        let prefer_cluster_with_video = self.children.get(dest_track_index).is_some_and(|track| {
            track.kind == TrackKind::Video
        }) || synced_inputs.video.is_some();
        let mut dest_track_index = dest_track_index;
        let mut cluster = self.resolve_insert_cluster(
            dest_track_index,
            prefer_cluster_with_video,
            if synced_inputs.video.is_some() {
                preferred_video_track_id
            } else {
                None
            },
        );

        // Moves pass source audio track indices so partners land back on their original
        // tracks. Empty preferred tracks (no clips, or gaps only) are part of the
        // destination working cluster so insert propagation applies there too.
        if let Some(preferred_indices) = preferred_audio_track_indices {
            for track_index in self.destination_move_audio_candidates(dest_track_index, &[]) {
                if !cluster.contains(&track_index) {
                    cluster.push(track_index);
                }
            }
            for &track_index in preferred_indices {
                if track_index == dest_track_index {
                    continue;
                }
                let Some(track) = self.children.get(track_index) else {
                    continue;
                };
                if track.kind != TrackKind::Audio {
                    continue;
                }
                if !track.items.is_empty() && !track_is_empty_boundary(track) {
                    continue;
                }
                if !cluster.contains(&track_index) {
                    cluster.push(track_index);
                }
            }
            cluster.sort_unstable();
        }

        // Audio targets: reuse existing audio tracks from the cluster, then create
        // new tracks directly below the destination until every clip has a slot.
        let needed = synced_inputs.audio.len();
        let audio_durations: Vec<Seconds> = synced_inputs
            .audio
            .iter()
            .map(|item| Self::sanitized_clip_duration(item).unwrap_or(modified_duration))
            .collect();
        let mut audio_slots: Vec<usize> = if let Some(preferred_indices) =
            preferred_audio_track_indices
        {
            let exclude_track_indices: HashSet<usize> = move_source_track_indices
                .map(|indices| indices.iter().copied().collect())
                .unwrap_or_default();
            self.assign_move_audio_slots(
                &mut dest_track_index,
                &mut cluster,
                &mut created_track_indices,
                start,
                &audio_durations,
                preferred_indices,
                &exclude_track_indices,
            )?
        } else {
            let mut slots: Vec<usize> = cluster
                .iter()
                .copied()
                .filter(|&i| i != dest_track_index && self.children[i].kind == TrackKind::Audio)
                .collect();
            while slots.len() < needed {
                let insert_at = dest_track_index;
                let track = self.new_numbered_track(TrackKind::Audio);
                self.children.insert(insert_at, track);
                for index in created_track_indices.iter_mut() {
                    if *index >= insert_at {
                        *index += 1;
                    }
                }
                created_track_indices.push(insert_at);
                dest_track_index += 1;
                for index in cluster.iter_mut() {
                    if *index >= insert_at {
                        *index += 1;
                    }
                }
                for index in slots.iter_mut() {
                    if *index >= insert_at {
                        *index += 1;
                    }
                }
                slots.push(insert_at);
            }
            slots
        };

        while audio_slots.len() < needed {
            let insert_at = dest_track_index;
            let track = self.new_numbered_track(TrackKind::Audio);
            self.children.insert(insert_at, track);
            for index in created_track_indices.iter_mut() {
                if *index >= insert_at {
                    *index += 1;
                }
            }
            created_track_indices.push(insert_at);
            dest_track_index += 1;
            for index in cluster.iter_mut() {
                if *index >= insert_at {
                    *index += 1;
                }
            }
            for index in audio_slots.iter_mut() {
                if *index >= insert_at {
                    *index += 1;
                }
            }
            audio_slots.push(insert_at);
        }

        let mut synced_video_clip_id = None;
        let mut column_video = None;
        if let Some(video_item) = synced_inputs.video {
            let video_span = Self::sanitized_clip_duration(&video_item).unwrap_or(column_span);
            let track_count_before = self.children.len();
            let video_track_index = preferred_video_track_id
                .and_then(|track_id| self.get_track_by_id(track_id))
                .and_then(|(track_index, _)| {
                    self.try_reuse_video_track_for_audio_move(
                        track_index,
                        dest_track_index,
                        start,
                        start + video_span,
                        sync_clips_id,
                        true,
                        overlap_policy,
                        insert_policy,
                    )
                })
                .or_else(|| {
                    self.find_or_create_video_track_for_audio(
                        dest_track_index,
                        start,
                        video_span,
                        &mut created_track_indices,
                        sync_clips_id,
                        true,
                        overlap_policy,
                        insert_policy,
                    )
                });
            let Some(video_track_index) = video_track_index else {
                *self = backup;
                return None;
            };
            if self.children.len() > track_count_before {
                Self::shift_insert_track_indices_after_create(
                    video_track_index,
                    &mut dest_track_index,
                    &mut cluster,
                    &mut audio_slots,
                    &mut created_track_indices,
                );
            }
            let Some((video_item, video_id)) = Self::prepare_synced_item_preserve_duration(
                video_item,
                sync_clips_id,
                &mut used_ids,
            ) else {
                *self = backup;
                return None;
            };
            synced_video_clip_id = Some(video_id);
            column_video = Some((video_track_index, video_item));
        }

        // Primary clip/gap on the destination track.
        let (primary_item, primary_id) = match primary_item {
            Item::Clip(_) => Self::prepare_synced_item(
                primary_item,
                modified_duration,
                sync_clips_id,
                &mut used_ids,
            )?,
            Item::Gap(mut gap) => {
                gap.source_range.duration.set_from_seconds(modified_duration);
                let mut item = Item::Gap(gap);
                let id = Self::ensure_unique_item_id(&mut item, &mut used_ids);
                (item, id)
            }
        };
        let mut column = Vec::new();

        // Assign the first synced audio to the audio track immediately below the
        // destination (largest index still less than `dest_track_index` in Resolve
        // layout), then the next-nearest, and so on.
        audio_slots.sort_by(|a, b| b.cmp(a));

        // Audio clips onto the nearest cluster audio tracks.
        let mut audio_clips = Vec::new();
        for (audio_item, &audio_track_index) in synced_inputs.audio.into_iter().zip(&audio_slots) {
            let Some((audio_item, audio_id)) = Self::prepare_synced_item_preserve_duration(
                audio_item,
                sync_clips_id,
                &mut used_ids,
            ) else {
                *self = backup;
                return None;
            };
            column.push((audio_track_index, audio_item));
            audio_clips.push((audio_id, audio_track_index));
        }
        if let Some(video_placement) = column_video {
            column.push(video_placement);
        }
        column.sort_by_key(|(track_index, _)| *track_index);

        let mut full_column = vec![(dest_track_index, primary_item)];
        full_column.extend(column);
        if overlap_policy == OverlapPolicy::Push {
            full_column.sort_by_key(|(track_index, _)| *track_index);
        } else {
            let (mut dest_items, mut partners): (Vec<_>, Vec<_>) = full_column
                .into_iter()
                .partition(|(track_index, _)| *track_index == dest_track_index);
            partners.sort_by_key(|(track_index, _)| *track_index);
            dest_items.extend(partners);
            full_column = dest_items;
        }

        let mut insert_updates: Vec<(usize, TrackInsertResult)> = Vec::new();
        for (track_index, item) in full_column {
            let result = self.children[track_index].insert_at_time(
                start,
                item,
                overlap_policy,
                InsertPolicy::SplitAndInsert,
            );
            if !result.success {
                *self = backup;
                return None;
            }
            insert_updates.push((track_index, result));
        }

        let update_refs: Vec<_> = insert_updates
            .iter()
            .map(|(track_index, result)| (*track_index, result))
            .collect();
        if !self.propagate_insert_to_cluster(
            start,
            modified_duration,
            overlap_policy,
            &update_refs,
            &cluster,
            sync_clips_id,
        ) {
            *self = backup;
            return None;
        }
        if overlap_policy == OverlapPolicy::Push
            && !self.propagate_push_to_cluster(
                start,
                start + modified_duration,
                modified_duration,
                &update_refs,
                &cluster,
            )
        {
            *self = backup;
            return None;
        }

        self.sanitize_preserving_all_gap_tracks();

        if sync_clips_id.is_some()
            && matches!(self.get_item(&primary_id), Some((_, _, Item::Clip(_))))
        {
            Some(InsertItemAtTimeResult::Synced(SyncedInsertResult {
                primary_clip_id: primary_id,
                audio_clips,
                synced_video_clip_id,
                sync_clips_id,
                created_track_indices,
            }))
        } else {
            Some(InsertItemAtTimeResult::ItemId(primary_id))
        }
    }

    pub fn resize_item(
        &mut self,
        item_id: &str,
        new_start_time: Seconds,
        new_duration: Seconds,
        overlap_policy: OverlapPolicy,
        clamp_to_media: bool,
    ) -> bool {
        let Some((selected_track_index, selected_item_index, selected_item)) =
            self.get_item(item_id)
        else {
            return false;
        };
        let before_states = self.synced_clip_states();
        let target_ids: Vec<String> = match selected_item {
            Item::Clip(clip) => resolve_sync_clips_id(&clip.metadata)
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
                .unwrap_or_else(|| vec![item_id.to_string()]),
            Item::Gap(_) => vec![item_id.to_string()],
        };
        if target_ids.is_empty() {
            return false;
        }
        if matches!(selected_item, Item::Gap(_)) {
            let backup = self.clone();
            let Some(track) = self.children.get_mut(selected_track_index) else {
                return false;
            };
            let Some(item) = track.items.get_mut(selected_item_index) else {
                return false;
            };
            item.set_duration(new_duration.max(0.0));
            track.sanitize_preserving_all_gap_track();
            if !self.sync_changed_groups_after_resize(
                &before_states,
                &[selected_track_index],
                &HashSet::new(),
                overlap_policy,
            ) {
                *self = backup;
                return false;
            }
            self.sanitize_preserving_all_gap_tracks();
            return true;
        }
        let excluded_ids: HashSet<_> = target_ids.iter().cloned().collect();
        let selected_start =
            self.children[selected_track_index].start_time_of_item(selected_item_index);
        let start_delta = new_start_time - selected_start;

        let effective_duration = target_ids
            .iter()
            .filter_map(|id| {
                self.get_item(id).map(|(_, _, item)| {
                    resize_effective_duration(item, new_duration, clamp_to_media)
                })
            })
            .fold(new_duration.max(0.0), Seconds::min);

        let backup = self.clone();
        if start_delta.abs() <= EPS {
            if overlap_policy != OverlapPolicy::Push {
                let mut resized_items = Vec::new();
                let mut modified_track_indices = Vec::new();

                for id in &target_ids {
                    let Some((track_index, item_index, item)) = self.get_item(id) else {
                        *self = backup;
                        return false;
                    };
                    if matches!(item, Item::Gap(_)) {
                        resized_items.clear();
                        modified_track_indices.clear();
                        break;
                    }

                    let old_start = self.children[track_index].start_time_of_item(item_index);
                    let old_duration = item.duration().max(0.0);
                    if effective_duration <= old_duration + EPS {
                        resized_items.clear();
                        modified_track_indices.clear();
                        break;
                    }

                    let mut item = item.clone();
                    item.set_duration(effective_duration);
                    if clamp_to_media {
                        item.clamp_to_active_available_range();
                    }
                    resized_items.push((
                        track_index,
                        old_start,
                        old_start + effective_duration,
                        item,
                    ));
                    modified_track_indices.push(track_index);
                }

                if !resized_items.is_empty() {
                    for (track_index, range_start, range_end, item) in resized_items {
                        let Some(track) = self.children.get_mut(track_index) else {
                            *self = backup;
                            return false;
                        };
                        replace_track_range_with_item(track, range_start, range_end, item);
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
                    return true;
                }
            }

            let mut modified_track_indices = Vec::new();
            for id in &target_ids {
                let Some((track_index, item_index, _)) = self.get_item(id) else {
                    *self = backup;
                    return false;
                };
                let Some(track) = self.children.get_mut(track_index) else {
                    *self = backup;
                    return false;
                };
                let Some(item) = track.items.get_mut(item_index) else {
                    *self = backup;
                    return false;
                };
                item.set_duration(effective_duration);
                if clamp_to_media {
                    item.clamp_to_active_available_range();
                }
                track.sanitize_preserving_all_gap_track();
                modified_track_indices.push(track_index);
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
            return true;
        }

        if start_delta < -EPS {
            let mut resized_items = Vec::new();
            let mut modified_track_indices = Vec::new();

            for id in &target_ids {
                let Some((track_index, item_index, item)) = self.get_item(id) else {
                    *self = backup;
                    return false;
                };
                let old_start = self.children[track_index].start_time_of_item(item_index);
                let target_start = old_start + start_delta;
                let old_duration = item.duration().max(0.0);
                let old_end = old_start + old_duration;

                modified_track_indices.push(track_index);
                let mut item = item.clone();
                item.set_duration(effective_duration);
                if clamp_to_media {
                    item.clamp_to_active_available_range();
                }
                resized_items.push((
                    track_index,
                    item_index,
                    target_start,
                    old_duration,
                    old_end,
                    item,
                ));
            }

            let mut removals = resized_items
                .iter()
                .map(|(track_index, item_index, _, old_duration, _, _)| {
                    (*track_index, *item_index, *old_duration)
                })
                .collect::<Vec<_>>();
            removals.sort_unstable_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)));

            for (track_index, item_index, old_duration) in removals {
                let Some(track) = self.children.get_mut(track_index) else {
                    *self = backup;
                    return false;
                };
                if item_index >= track.items.len() {
                    *self = backup;
                    return false;
                }
                track.items.remove(item_index);
                track
                    .items
                    .insert(item_index, Item::Gap(Gap::make_gap(old_duration)));
            }

            for (track_index, _, target_start, _, old_end, item) in resized_items {
                let Some(track) = self.children.get_mut(track_index) else {
                    *self = backup;
                    return false;
                };
                replace_track_range_with_item(track, target_start, old_end, item);
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
            return true;
        }

        let mut resized_items = Vec::new();
        let mut modified_track_indices = Vec::new();
        for id in &target_ids {
            let Some((track_index, item_index, item)) = self.get_item(id) else {
                *self = backup;
                return false;
            };
            modified_track_indices.push(track_index);
            let target_start =
                self.children[track_index].start_time_of_item(item_index) + start_delta;
            let mut item = item.clone();
            item.set_duration(effective_duration);
            if clamp_to_media {
                item.clamp_to_active_available_range();
            }
            resized_items.push((track_index, target_start, item));
        }

        for id in &target_ids {
            let Some((track_index, item_index, _)) = self.get_item(id) else {
                *self = backup;
                return false;
            };
            let Some(track) = self.children.get_mut(track_index) else {
                *self = backup;
                return false;
            };
            if item_index >= track.items.len() {
                *self = backup;
                return false;
            }
            track.items.remove(item_index);
            track.sanitize_preserving_all_gap_track();
        }

        for (track_index, target_start, item) in resized_items {
            let Some(track) = self.children.get_mut(track_index) else {
                *self = backup;
                return false;
            };
            track.insert_at_time(
                target_start,
                item,
                overlap_policy,
                InsertPolicy::SplitAndInsert,
            );
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

    pub fn modify_item(
        &mut self,
        item_id: &str,
        source_start_time: Seconds,
        duration: Seconds,
        clamp_to_media: bool,
        resize_from_start: bool,
        push_following: bool,
    ) -> bool {
        let Some((track_index, item_index, _)) = self.get_item(item_id) else {
            return false;
        };

        let mut effective_source_start = source_start_time;
        let mut effective_duration = duration;
        if effective_source_start < 0.0 {
            effective_duration += effective_source_start;
            effective_source_start = 0.0;
        }

        if effective_duration < 0.0 {
            let replace_with_gap =
                !matches!(self.children[track_index].items[item_index], Item::Gap(_));
            self.delete_item(item_id, replace_with_gap);
            self.sanitize_preserving_all_gap_tracks();
            return true;
        }

        let old_timeline_start = self.children[track_index].start_time_of_item(item_index);
        let old_source_start = match &self.children[track_index].items[item_index] {
            Item::Clip(clip) => clip.source_range.start_time.to_seconds(),
            Item::Gap(_) => 0.0,
        };
        let old_duration = self.children[track_index].items[item_index].duration();
        let new_timeline_start =
            (old_timeline_start + effective_source_start - old_source_start).max(0.0);
        let is_gap = matches!(self.children[track_index].items[item_index], Item::Gap(_));
        let is_clip = matches!(self.children[track_index].items[item_index], Item::Clip(_));
        let mut source_delta = effective_source_start - old_source_start;
        if is_clip && resize_from_start && !push_following {
            let unclamped_timeline_start = old_timeline_start + source_delta;
            if unclamped_timeline_start < 0.0 {
                source_delta = -old_timeline_start;
                effective_source_start = old_source_start + source_delta;
                effective_duration = old_duration - source_delta;
            }
        }
        let effective_push_following =
            push_following || (is_gap && effective_duration < old_duration);

        let cluster = self.boundary_group_indices(track_index);
        let is_cluster_column = self.is_cluster_column_at(
            &cluster,
            old_timeline_start,
            old_duration,
        );
        if is_cluster_column
            && (new_timeline_start - old_timeline_start).abs() <= EPS
            && (effective_duration - old_duration).abs() > EPS
        {
            let backup = self.clone();
            let before_states = self.synced_clip_states();
            if !self.apply_cluster_column_duration_change(
                item_id,
                &cluster,
                old_timeline_start,
                old_duration,
                effective_duration,
                effective_source_start,
                clamp_to_media,
            ) {
                return false;
            }
            self.sanitize_preserving_all_gap_tracks();
            let overlap_policy = if effective_push_following {
                OverlapPolicy::Push
            } else {
                OverlapPolicy::Override
            };
            if !self.sync_changed_groups_after_resize(
                &before_states,
                &cluster,
                &HashSet::new(),
                overlap_policy,
            ) {
                *self = backup;
                return false;
            }
            return true;
        }

        if is_gap && effective_duration < old_duration {
            let backup = self.clone();
            let before_states = self.synced_clip_states();
            self.children[track_index].items[item_index].set_duration(effective_duration);
            self.sanitize_preserving_all_gap_tracks();
            if !self.sync_changed_groups_after_resize(
                &before_states,
                &[track_index],
                &HashSet::new(),
                OverlapPolicy::Push,
            ) {
                *self = backup;
                return false;
            }
            return true;
        }

        if is_clip && !effective_push_following {
            if resize_from_start && source_delta > 0.0 && effective_duration < old_duration {
                let targets = self.synced_clip_targets_for_item(item_id);
                self.resize_synced_clips_with_leading_gap(
                    targets,
                    source_delta,
                    effective_duration,
                );
                self.sanitize_preserving_all_gap_tracks();
                return true;
            }

            let duration_delta = old_duration - effective_duration;
            if !resize_from_start && duration_delta > 0.0 {
                let targets = self.synced_clip_targets_for_item(item_id);
                self.resize_synced_clips_with_trailing_gap(targets, effective_duration);
                self.sanitize_preserving_all_gap_tracks();
                return true;
            }
        }

        let resize_timeline_start = if is_clip && effective_push_following && resize_from_start {
            old_timeline_start
        } else {
            new_timeline_start
        };

        let resized = self.resize_item_with_source_start(
            item_id,
            resize_timeline_start,
            effective_source_start,
            effective_duration,
            if effective_push_following {
                OverlapPolicy::Push
            } else {
                OverlapPolicy::Override
            },
            clamp_to_media,
        );
        self.sanitize_preserving_all_gap_tracks();
        resized
    }

    pub fn resize_item_with_source_start(
        &mut self,
        item_id: &str,
        new_start_time: Seconds,
        source_start_time: Seconds,
        new_duration: Seconds,
        overlap_policy: OverlapPolicy,
        clamp_to_media: bool,
    ) -> bool {
        let Some((_, _, item)) = self.get_item(item_id) else {
            return false;
        };
        let source_delta = match item {
            Item::Clip(clip) => source_start_time - clip.source_range.start_time.to_seconds(),
            Item::Gap(_) => 0.0,
        };
        if source_delta.abs() > EPS {
            self.offset_synced_clip_source_starts(item_id, source_delta);
        }
        self.resize_item(
            item_id,
            new_start_time,
            new_duration,
            overlap_policy,
            clamp_to_media,
        )
    }

    fn resize_synced_clips_with_leading_gap(
        &mut self,
        mut targets: Vec<(usize, usize)>,
        source_delta: Seconds,
        duration: Seconds,
    ) {
        targets.sort_unstable_by(|a, b| b.cmp(a));
        for (track_index, item_index) in targets {
            let Some(track) = self.children.get_mut(track_index) else {
                continue;
            };
            if item_index >= track.items.len() {
                continue;
            }
            let source_start_time = item_source_start(&track.items[item_index]) + source_delta;
            set_item_source_start(&mut track.items[item_index], source_start_time);
            track.items[item_index].set_duration(duration);
            track
                .items
                .insert(item_index, Item::Gap(Gap::make_gap(source_delta)));
        }
    }

    fn resize_synced_clips_with_trailing_gap(
        &mut self,
        mut targets: Vec<(usize, usize)>,
        duration: Seconds,
    ) {
        targets.sort_unstable_by(|a, b| b.cmp(a));
        for (track_index, item_index) in targets {
            let Some(track) = self.children.get_mut(track_index) else {
                continue;
            };
            if item_index >= track.items.len() {
                continue;
            }
            let gap_duration = (track.items[item_index].duration() - duration).max(0.0);
            track.items[item_index].set_duration(duration);
            if gap_duration > 0.0 {
                track
                    .items
                    .insert(item_index + 1, Item::Gap(Gap::make_gap(gap_duration)));
            }
        }
    }

    fn synced_clip_targets_for_item(&self, item_id: &str) -> Vec<(usize, usize)> {
        let Some((selected_track_index, selected_item_index, selected_item)) =
            self.get_item(item_id)
        else {
            return Vec::new();
        };
        let Some(sync_clips_id) = (match selected_item {
            Item::Clip(clip) => resolve_sync_clips_id(&clip.metadata),
            Item::Gap(_) => None,
        }) else {
            return vec![(selected_track_index, selected_item_index)];
        };

        let targets = self.synced_clips_targets(sync_clips_id);
        if targets.len() > 1 {
            targets
        } else {
            vec![(selected_track_index, selected_item_index)]
        }
    }

    fn offset_synced_clip_source_starts(&mut self, item_id: &str, source_delta: Seconds) {
        for (track_index, item_index) in self.synced_clip_targets_for_item(item_id) {
            let Some(item) = self
                .children
                .get_mut(track_index)
                .and_then(|track| track.items.get_mut(item_index))
            else {
                continue;
            };
            let source_start_time = (item_source_start(item) + source_delta).max(0.0);
            set_item_source_start(item, source_start_time);
        }
    }

    fn delete_one_item(&mut self, item_id: &str, replace_with_gap: bool) -> Option<(usize, Item)> {
        for ti in 0..self.children.len() {
            if let Some((ii, _)) = self.children[ti].get_item_by_id(item_id) {
                let backup = self.clone();
                let before_states = self.synced_clip_states();
                // Use the track API for deletion and optional gap insertion/merge behavior
                let Some(removed) = self.children[ti].delete_clip(ii, replace_with_gap) else {
                    return None;
                };
                if !replace_with_gap {
                    let excluded_ids = removed.get_id().into_iter().collect();
                    if !self.sync_changed_groups_after_resize(
                        &before_states,
                        &[ti],
                        &excluded_ids,
                        OverlapPolicy::Override,
                    ) {
                        *self = backup;
                        return None;
                    }
                }
                return Some((ti, removed));
            }
        }
        None
    }

    fn remove_item_at_for_move(
        &mut self,
        track_index: usize,
        item_index: usize,
        replace_with_gap: bool,
        used_ids: &mut HashSet<String>,
    ) -> bool {
        let Some(track) = self.children.get_mut(track_index) else {
            return false;
        };
        if item_index >= track.items.len() {
            return false;
        }
        let removed = track.items.remove(item_index);
        let duration = removed.duration().max(0.0);
        if replace_with_gap && duration > EPS {
            let mut gap = Item::Gap(crate::Gap::make_gap(duration));
            Self::ensure_unique_item_id(&mut gap, used_ids);
            track.items.insert(item_index.min(track.items.len()), gap);
            track.merge_adjacent_gaps();
        }
        true
    }

    fn synced_move_items(&self, item_id: &str) -> Option<Vec<SyncedMoveItem>> {
        let (selected_track_index, selected_item_index, selected_item) = self.get_item(item_id)?;
        let sync_clips_id = match selected_item {
            Item::Clip(clip) => resolve_sync_clips_id(&clip.metadata),
            Item::Gap(_) => None,
        }?;
        let selected_start = self.children[selected_track_index]
            .start_time_of_item(selected_item_index);
        let selected_duration = selected_item.duration().max(0.0);
        if selected_duration <= EPS {
            return None;
        }
        let targets =
            self.sync_column_targets_at(sync_clips_id, selected_start, selected_duration);
        if targets.len() <= 1 {
            return None;
        }

        // Resolve exports often misalign video/audio columns. Defer to the
        // linked-move path so each partner keeps its own duration.
        let has_misaligned_video = self
            .synced_clips_targets(sync_clips_id)
            .into_iter()
            .any(|(track_index, item_index)| {
                self.children
                    .get(track_index)
                    .is_some_and(|track| track.kind == TrackKind::Video)
                    && !targets
                        .iter()
                        .any(|(ti, ii)| ti == &track_index && ii == &item_index)
            });
        if has_misaligned_video {
            return None;
        }

        let mut items = Vec::new();
        for (track_index, item_index) in targets {
            let track_kind = self.children.get(track_index)?.kind.clone();
            let item = self
                .children
                .get(track_index)?
                .items
                .get(item_index)?
                .clone();
            let is_selected =
                track_index == selected_track_index && item_index == selected_item_index;
            items.push(SyncedMoveItem {
                track_index,
                item_index,
                track_kind,
                item,
                is_selected,
            });
        }

        Some(items)
    }

    fn is_intra_cluster_sync_move(
        &self,
        dest_track_index: usize,
        items_to_move: &[SyncedMoveItem],
    ) -> bool {
        let cluster: HashSet<usize> = self
            .boundary_group_indices(dest_track_index)
            .into_iter()
            .collect();
        cluster.contains(&dest_track_index)
            && items_to_move
                .iter()
                .all(|item| cluster.contains(&item.track_index))
    }

    fn stack_item_start_time(&self, item_id: &str) -> Option<Seconds> {
        let (track_index, item_index, _) = self.get_item(item_id)?;
        Some(self.children[track_index].start_time_of_item(item_index))
    }

    fn linked_item_ids_for_move(&self, item_timeline_id: &str, item_to_move: &Item) -> Vec<String> {
        let Some((primary_track_index, primary_item_index, _)) = self.get_item(item_timeline_id)
        else {
            return Vec::new();
        };
        let primary_start = self.children[primary_track_index]
            .start_time_of_item(primary_item_index);
        let primary_duration = item_to_move.duration().max(0.0);
        let primary_sync_id = match item_to_move {
            Item::Clip(clip) => resolve_sync_clips_id(&clip.metadata),
            Item::Gap(_) => None,
        };

        let mut selected_ids = HashSet::from([item_timeline_id.to_string()]);
        let mut selected_link_group_ids = HashSet::<i64>::new();
        let mut selected_tellers_group_ids = HashSet::<i64>::new();
        if let Some(link_group_id) = item_link_group_id(item_to_move) {
            selected_link_group_ids.insert(link_group_id);
        }
        if let Some(tellers_group_id) = item_tellers_group_id(item_to_move) {
            selected_tellers_group_ids.insert(tellers_group_id);
        }
        if selected_link_group_ids.is_empty() && selected_tellers_group_ids.is_empty() {
            return Vec::new();
        }

        loop {
            let mut changed = false;
            for (track_index, track) in self.children.iter().enumerate() {
                let mut pos = 0.0;
                for (item_index, item) in track.items.iter().enumerate() {
                    if !matches!(item, Item::Clip(_)) {
                        pos += item.duration().max(0.0);
                        continue;
                    }
                    let linked = item_link_group_id(item)
                        .is_some_and(|id| selected_link_group_ids.contains(&id));
                    let grouped = item_tellers_group_id(item)
                        .is_some_and(|id| selected_tellers_group_ids.contains(&id));
                    if !linked && !grouped {
                        pos += item.duration().max(0.0);
                        continue;
                    }
                    if let Some(sync_id) = primary_sync_id {
                        if resolve_sync_clips_id(match item {
                            Item::Clip(clip) => &clip.metadata,
                            Item::Gap(_) => {
                                pos += item.duration().max(0.0);
                                continue;
                            }
                        }) == Some(sync_id)
                            && !self.item_occupies_column(
                                track_index,
                                item_index,
                                primary_start,
                                primary_duration,
                            )
                            && self.is_independent_sync_column(
                                sync_id,
                                pos,
                                item.duration().max(0.0),
                            )
                        {
                            pos += item.duration().max(0.0);
                            continue;
                        }
                    }
                    let Some(item_id) = item.get_id() else {
                        pos += item.duration().max(0.0);
                        continue;
                    };
                    if selected_ids.insert(item_id) {
                        changed = true;
                    }
                    if let Some(link_group_id) = item_link_group_id(item) {
                        changed |= selected_link_group_ids.insert(link_group_id);
                    }
                    if let Some(tellers_group_id) = item_tellers_group_id(item) {
                        changed |= selected_tellers_group_ids.insert(tellers_group_id);
                    }
                    pos += item.duration().max(0.0);
                }
            }
            if !changed {
                break;
            }
        }

        let mut linked_ids = Vec::new();
        for track in &self.children {
            for item in &track.items {
                let Some(item_id) = item.get_id() else {
                    continue;
                };
                if item_id != item_timeline_id && selected_ids.contains(&item_id) {
                    linked_ids.push(item_id);
                }
            }
        }
        linked_ids
    }

    /// Move a primary clip and its linked / grouped partners while preserving
    /// relative timeline offsets. Uses delete + insert so cluster column padding
    /// applies for unsynced inserts inside a sync cluster.
    fn sync_clips_id_from_insert_result(
        &self,
        result: &InsertItemAtTimeResult,
    ) -> Option<i64> {
        match result {
            InsertItemAtTimeResult::Synced(synced) => synced.sync_clips_id,
            InsertItemAtTimeResult::ItemId(id) => self
                .get_item(id)
                .and_then(|(_, _, item)| item_link_group_id(item)),
        }
    }

    fn place_link_group_video_partner(
        &mut self,
        dest_audio_track_index: usize,
        dest_time: Seconds,
        video_item: Item,
        sync_clips_id: Option<i64>,
        preferred_cluster_video_id: Option<&str>,
        insert_policy: InsertPolicy,
        overlap_policy: OverlapPolicy,
    ) -> bool {
        let video_duration = video_item.duration().max(0.0);
        if video_duration <= EPS {
            return false;
        }
        let Some(start) = insertion_start_or_end_for_policy(
            &self.children[dest_audio_track_index],
            dest_time,
            insert_policy,
        ) else {
            return false;
        };
        let end_time = start + video_duration;

        let mut created_track_indices = Vec::new();
        let video_track_index = if let Some(preferred_id) = preferred_cluster_video_id {
            self.get_track_by_id(preferred_id)
                .map(|(track_index, _)| track_index)
                .and_then(|track_index| {
                    self.try_reuse_video_track_for_audio_move(
                        track_index,
                        dest_audio_track_index,
                        start,
                        end_time,
                        sync_clips_id,
                        true,
                        overlap_policy,
                        insert_policy,
                    )
                })
        } else {
            None
        };
        let Some(video_track_index) = video_track_index.or_else(|| {
            self.find_or_create_video_track_for_audio(
                dest_audio_track_index,
                start,
                video_duration,
                &mut created_track_indices,
                sync_clips_id,
                true,
                overlap_policy,
                insert_policy,
            )
        }) else {
            return false;
        };

        let mut used_ids = self.collect_timeline_ids();
        let mut video_item = video_item;
        Self::set_item_sync_clips(&mut video_item, sync_clips_id);
        Self::ensure_unique_item_id(&mut video_item, &mut used_ids);

        self.children[video_track_index].insert_at_time(
            dest_time,
            video_item,
            overlap_policy,
            insert_policy,
        );
        true
    }

    fn move_linked_items_at_time(
        &mut self,
        item_id: &str,
        dest_track_id: &str,
        dest_time: Seconds,
        replace_with_gap: bool,
        insert_policy: InsertPolicy,
        overlap_policy: OverlapPolicy,
    ) -> bool {
        let item_to_move = match self.get_item(item_id) {
            Some((_ti, _ii, item)) => item.clone(),
            None => return false,
        };
        let dest_track_index = match self.get_track_by_id(dest_track_id) {
            Some((index, _)) => index,
            None => return false,
        };
        let backup = self.clone();
        let primary_start_time = backup
            .stack_item_start_time(item_id)
            .unwrap_or(dest_time);
        let preferred_cluster_video_id = backup
            .get_track_by_id(dest_track_id)
            .map(|(dest_track_index, _)| dest_track_index)
            .and_then(|dest_track_index| {
                backup
                    .boundary_group_indices(dest_track_index)
                    .into_iter()
                    .find_map(|track_index| {
                        let track = backup.children.get(track_index)?;
                        (track.kind == TrackKind::Video)
                            .then(|| track.get_id().clone())
                            .flatten()
                    })
            });
        let linked_item_ids = backup.linked_item_ids_for_move(item_id, &item_to_move);
        let mut ids_to_remove: Vec<String> = std::iter::once(item_id.to_string())
            .chain(linked_item_ids.into_iter())
            .collect();
        ids_to_remove.sort_by(|left, right| {
            let left_pos = backup.get_item(left).map(|(ti, ii, _)| (ti, ii));
            let right_pos = backup.get_item(right).map(|(ti, ii, _)| (ti, ii));
            right_pos.cmp(&left_pos)
        });
        ids_to_remove.dedup();

        let mut removed_items = Vec::new();
        for remove_id in ids_to_remove {
            if let Some(removed) = self.delete_one_item(&remove_id, replace_with_gap) {
                removed_items.push(removed);
            }
        }
        if removed_items.is_empty() {
            return false;
        }

        let mut linked_audio_items = Vec::new();
        let mut preferred_audio_track_indices = Vec::new();
        let mut linked_video_item = None;
        let dest_is_audio = self.children.get(dest_track_index).is_some_and(|track| {
            track.kind == TrackKind::Audio
        });
        for (source_track_index, item) in removed_items {
            if item
                .get_id()
                .as_deref()
                .is_some_and(|id| id == item_id)
            {
                continue;
            }
            let item_destination_time = item
                .get_id()
                .as_deref()
                .and_then(|id| backup.stack_item_start_time(id))
                .map(|start_time| dest_time + start_time - primary_start_time)
                .unwrap_or(dest_time);
            let source_track_kind = backup
                .children
                .get(source_track_index)
                .map(|track| track.kind.clone());
            let is_same_time = (item_destination_time - dest_time).abs() <= EPS;
            let is_same_time_audio_clip = is_same_time
                && matches!(item, Item::Clip(_))
                && matches!(source_track_kind, Some(TrackKind::Audio));
            if is_same_time_audio_clip {
                if let Item::Clip(clip) = item {
                    linked_audio_items.push(Item::Clip(clip));
                    preferred_audio_track_indices.push(source_track_index);
                }
            } else if dest_is_audio
                && matches!(source_track_kind, Some(TrackKind::Video))
                && matches!(item, Item::Clip(_))
            {
                // Resolve link groups often misalign video/audio columns; when
                // moving onto an audio destination, keep the video partner for
                // the synced insert instead of leaving it on the source track.
                if linked_video_item.is_some() {
                    *self = backup;
                    return false;
                }
                linked_video_item = Some(item);
            } else if is_same_time && matches!(item, Item::Gap(_)) {
                // Drop same-time gaps left behind by sync deletion.
            } else if let Some(source_track_id) = backup
                .children
                .get(source_track_index)
                .and_then(|track| track.get_id())
            {
                let Some((target_track_index, _)) = self.get_track_by_id(&source_track_id) else {
                    *self = backup;
                    return false;
                };
                self.insert_at_time_with_sync_splits(
                    target_track_index,
                    item_destination_time,
                    item,
                    overlap_policy,
                    insert_policy,
                );
            }
        }

        let has_synced_partners =
            !linked_audio_items.is_empty() || linked_video_item.is_some();
        if !has_synced_partners {
            self.insert_at_time_with_sync_splits(
                dest_track_index,
                dest_time,
                item_to_move,
                overlap_policy,
                insert_policy,
            );
        } else {
            let Some(insert_result) = self.insert_synced_item_at_time(
                dest_track_index,
                dest_time,
                None,
                item_to_move,
                overlap_policy,
                insert_policy,
                (!linked_audio_items.is_empty()).then_some(linked_audio_items),
                None,
                preferred_cluster_video_id.as_deref(),
                (!preferred_audio_track_indices.is_empty())
                    .then_some(preferred_audio_track_indices.as_slice()),
                None::<&[usize]>,
            ) else {
                *self = backup;
                return false;
            };
            if let Some(video_item) = linked_video_item {
                let sync_clips_id = self.sync_clips_id_from_insert_result(&insert_result);
                let dest_audio_track_index = self
                    .get_track_by_id(dest_track_id)
                    .map(|(index, _)| index)
                    .unwrap_or(dest_track_index);
                if !self.place_link_group_video_partner(
                    dest_audio_track_index,
                    dest_time,
                    video_item,
                    sync_clips_id,
                    preferred_cluster_video_id.as_deref(),
                    insert_policy,
                    overlap_policy,
                ) {
                    *self = backup;
                    return false;
                }
            }
        }

        self.sanitize();
        true
    }

    fn collect_placeholder_gap_ids(&self, targets: &[(usize, usize)]) -> Vec<String> {
        let mut gap_ids = Vec::new();
        for (track_index, item_index) in targets {
            let Some(track) = self.children.get(*track_index) else {
                continue;
            };
            let Some(item) = track.items.get(*item_index) else {
                continue;
            };
            if matches!(item, Item::Gap(_)) {
                if let Some(id) = item.get_id() {
                    gap_ids.push(id);
                }
            }
        }
        gap_ids
    }

    fn remove_gaps_by_id(&mut self, gap_ids: &[String]) {
        let mut gap_ids: Vec<_> = gap_ids.to_vec();
        gap_ids.sort();
        gap_ids.dedup();
        for gap_id in gap_ids {
            let Some((track_index, item_index, item)) = self.get_item(&gap_id) else {
                continue;
            };
            if !matches!(item, Item::Gap(_)) {
                continue;
            }
            let Some(track) = self.children.get_mut(track_index) else {
                continue;
            };
            if item_index < track.items.len() {
                track.items.remove(item_index);
                track.merge_adjacent_gaps();
            }
        }
    }

    fn replace_synced_item_via_insert(
        &mut self,
        items: Vec<SyncedMoveItem>,
        dest_track_index: usize,
        dest_time: Seconds,
        item: Item,
        synced_audio_clips: Option<Vec<Item>>,
    ) -> bool {
        let backup = self.clone();
        if dest_track_index >= self.children.len() {
            return false;
        }

        let Some(selected) = items.iter().find(|item| item.is_selected) else {
            return false;
        };
        let Some(selected_id) = selected.item.get_id() else {
            return false;
        };

        let synced_audio_input_provided = synced_audio_clips.is_some();
        let synced_inputs = Self::normalize_synced_inputs(synced_audio_clips, None);
        if !matches!(item, Item::Clip(_)) {
            return false;
        }
        let mut primary_item = item;
        primary_item.clamp_to_active_available_range();
        primary_item.set_id(Some(selected_id.clone()));
        let replacement_duration = primary_item.duration().max(0.0);
        if replacement_duration <= EPS {
            return false;
        }
        if synced_audio_input_provided
            && !Self::synced_inputs_match_duration(replacement_duration, &synced_inputs)
        {
            return false;
        }

        let mut synced_audio = Vec::new();
        let mut preferred_audio_track_indices = Vec::new();
        let mut gap_only_audio_tracks = Vec::new();
        let mut synced_video = None;
        let mut audio_partners: Vec<_> = items
            .iter()
            .filter(|item| !item.is_selected && item.track_kind == TrackKind::Audio)
            .collect();
        audio_partners.sort_by_key(|item| item.track_index);

        for item in &items {
            if item.is_selected {
                continue;
            }
            if item.track_kind == TrackKind::Video {
                let mut video_item = item.item.clone();
                video_item.set_duration(replacement_duration);
                synced_video = Some(video_item);
            }
        }

        if synced_audio_input_provided {
            let mut synced_audio_inputs = synced_inputs.audio.into_iter();
            for partner in audio_partners {
                if let Some(mut next_audio) = synced_audio_inputs.next() {
                    if let Some(audio_id) = partner.item.get_id() {
                        next_audio.set_id(Some(audio_id));
                    }
                    next_audio.set_duration(replacement_duration);
                    synced_audio.push(next_audio);
                    preferred_audio_track_indices.push(partner.track_index);
                } else {
                    gap_only_audio_tracks.push(partner.track_index);
                }
            }
            for audio_item in synced_audio_inputs {
                synced_audio.push(audio_item);
            }
        } else {
            for item in &items {
                if item.is_selected || item.track_kind != TrackKind::Audio {
                    continue;
                }
                let mut audio_item = item.item.clone();
                audio_item.set_duration(replacement_duration);
                synced_audio.push(audio_item);
                preferred_audio_track_indices.push(item.track_index);
            }
        }

        let Some(targets) = self.delete_item_targets(&selected_id) else {
            return false;
        };
        let removed = self.delete_clips_at_indices(targets.clone(), true);
        if removed.is_empty() {
            return false;
        }
        let placeholder_gap_ids = self.collect_placeholder_gap_ids(&targets);

        let synced_audio_clips = (!synced_audio.is_empty()).then_some(synced_audio);
        let gap_only_exclude = (!gap_only_audio_tracks.is_empty())
            .then_some(gap_only_audio_tracks.as_slice());
        if self
            .insert_synced_item_at_time(
                dest_track_index,
                dest_time,
                None,
                primary_item,
                OverlapPolicy::Override,
                InsertPolicy::SplitAndInsert,
                synced_audio_clips,
                synced_video,
                None,
                (!preferred_audio_track_indices.is_empty())
                    .then_some(preferred_audio_track_indices.as_slice()),
                gap_only_exclude,
            )
            .is_some()
        {
            self.remove_gaps_by_id(&placeholder_gap_ids);
            let mut used_ids = self.collect_timeline_ids();
            for &track_index in &gap_only_audio_tracks {
                let start = dest_time;
                let end = dest_time + replacement_duration;
                let Some(track) = self.children.get_mut(track_index) else {
                    *self = backup;
                    return false;
                };
                track.delete_range(start, end, false);
                let mut gap = Item::Gap(Gap::make_gap(replacement_duration));
                Self::ensure_unique_item_id(&mut gap, &mut used_ids);
                let result = track.insert_at_time(
                    start,
                    gap,
                    OverlapPolicy::Override,
                    InsertPolicy::InsertBefore,
                );
                if !result.success {
                    *self = backup;
                    return false;
                }
            }
            self.sanitize_preserving_all_gap_tracks();
            true
        } else {
            *self = backup;
            false
        }
    }

    fn move_synced_items_at_time_via_insert(
        &mut self,
        items_to_move: Vec<SyncedMoveItem>,
        dest_track_index: usize,
        dest_time: Seconds,
        replace_with_gap: bool,
        insert_policy: InsertPolicy,
        overlap_policy: OverlapPolicy,
    ) -> bool {
        let backup = self.clone();
        if dest_track_index >= self.children.len() {
            return false;
        }

        let Some(selected) = items_to_move.iter().find(|item| item.is_selected) else {
            return false;
        };
        let Some(item_id) = selected.item.get_id() else {
            return false;
        };
        let primary_item = selected.item.clone();

        let mut synced_audio = Vec::new();
        let mut preferred_audio_track_indices = Vec::new();
        let mut synced_video = None;
        let mut audio_partners: Vec<_> = items_to_move
            .iter()
            .filter(|item| !item.is_selected && item.track_kind == TrackKind::Audio)
            .collect();
        audio_partners.sort_by_key(|item| item.track_index);
        for item in &items_to_move {
            if item.is_selected {
                continue;
            }
            match item.track_kind {
                TrackKind::Audio => continue,
                TrackKind::Video => {
                    if synced_video.is_some() {
                        return false;
                    }
                    synced_video = Some(item.item.clone());
                }
                TrackKind::Other => return false,
            }
        }
        for item in audio_partners {
            synced_audio.push(item.item.clone());
            preferred_audio_track_indices.push(item.track_index);
        }
        let move_source_track_indices: Vec<usize> =
            items_to_move.iter().map(|item| item.track_index).collect();

        let Some(targets) = self.delete_item_targets(&item_id) else {
            return false;
        };
        let removed = self.delete_clips_at_indices(targets.clone(), true);
        if removed.is_empty() {
            return false;
        }
        let placeholder_gap_ids = self.collect_placeholder_gap_ids(&targets);

        let synced_audio_clips = (!synced_audio.is_empty()).then_some(synced_audio);
        if self
            .insert_synced_item_at_time(
                dest_track_index,
                dest_time,
                None,
                primary_item,
                overlap_policy,
                insert_policy,
                synced_audio_clips,
                synced_video,
                None,
                (!preferred_audio_track_indices.is_empty())
                    .then_some(preferred_audio_track_indices.as_slice()),
                Some(move_source_track_indices.as_slice()),
            )
            .is_some()
        {
            if !replace_with_gap {
                self.remove_gaps_by_id(&placeholder_gap_ids);
            }
            self.sanitize();
            true
        } else {
            *self = backup;
            false
        }
    }

    fn move_synced_items(
        &mut self,
        items_to_move: Vec<SyncedMoveItem>,
        mut dest_track_index: usize,
        replace_with_gap: bool,
        overlap_policy: OverlapPolicy,
        placement: SyncedMovePlacement,
    ) -> bool {
        let backup = self.clone();
        if dest_track_index >= self.children.len() {
            return false;
        }

        let Some(selected_position) = items_to_move.iter().position(|item| item.is_selected) else {
            return false;
        };
        let selected_item = &items_to_move[selected_position];
        let selected_source_track_index = selected_item.track_index;
        let Some(selected_id) = selected_item.item.get_id() else {
            return false;
        };
        let Some(sync_clips_id) = (match &selected_item.item {
            Item::Clip(clip) => resolve_sync_clips_id(&clip.metadata),
            Item::Gap(_) => None,
        }) else {
            return false;
        };
        let moved_duration = selected_item.item.duration().max(0.0);
        if moved_duration <= EPS
            || items_to_move
                .iter()
                .any(|item| (item.item.duration().max(0.0) - moved_duration).abs() > EPS)
        {
            return false;
        }

        let mut used_ids = self.collect_timeline_ids();
        let mut targets: Vec<_> = items_to_move
            .iter()
            .map(|item| (item.track_index, item.item_index))
            .collect();
        targets.sort_by(|a, b| b.cmp(a));
        for (track_index, item_index) in targets {
            if !self.remove_item_at_for_move(
                track_index,
                item_index,
                replace_with_gap,
                &mut used_ids,
            ) {
                *self = backup;
                return false;
            }
        }

        let insert_policy_for_video = match &placement {
            SyncedMovePlacement::Time { insert_policy, .. } => *insert_policy,
            SyncedMovePlacement::Index { .. } => InsertPolicy::SplitAndInsert,
        };
        let moved_start = match placement {
            SyncedMovePlacement::Time {
                dest_time,
                insert_policy,
            } => {
                let Some(start) = insertion_start_or_end_for_policy(
                    &self.children[dest_track_index],
                    dest_time,
                    insert_policy,
                ) else {
                    *self = backup;
                    return false;
                };
                start
            }
            SyncedMovePlacement::Index { dest_index } => {
                self.children[dest_track_index].start_time_of_item(dest_index)
            }
        };

        let mut placements = vec![(
            dest_track_index,
            items_to_move[selected_position].item.clone(),
            true,
        )];
        let mut created_track_indices = Vec::new();
        let mut used_audio_track_indices = Vec::new();
        let mut used_audio_boundary_indices = Vec::new();
        let mut used_video_track_indices = Vec::new();
        match self.children[dest_track_index].kind {
            TrackKind::Audio => used_audio_track_indices.push(dest_track_index),
            TrackKind::Video => used_video_track_indices.push(dest_track_index),
            TrackKind::Other => {}
        }

        let intra_cluster_move =
            backup.is_intra_cluster_sync_move(dest_track_index, &items_to_move);

        let mut sync_track_items: Vec<_> = items_to_move
            .into_iter()
            .enumerate()
            .filter(|(position, _)| *position != selected_position)
            .collect();
        sync_track_items.sort_by_key(|(_, move_item)| {
            (
                move_item.track_index.abs_diff(selected_source_track_index),
                move_item.track_index,
            )
        });

        for (_, move_item) in sync_track_items {
            if intra_cluster_move {
                let track_index = move_item.track_index;
                match move_item.track_kind {
                    TrackKind::Audio => used_audio_track_indices.push(track_index),
                    TrackKind::Video => used_video_track_indices.push(track_index),
                    TrackKind::Other => {
                        *self = backup;
                        return false;
                    }
                }
                placements.push((track_index, move_item.item, false));
                continue;
            }

            match move_item.track_kind {
                TrackKind::Audio => {
                    let track_count_before = self.children.len();
                    let Some(track_index) = self.find_or_create_move_audio_track(
                        dest_track_index,
                        moved_start,
                        moved_duration,
                        &mut created_track_indices,
                        &used_audio_track_indices,
                        &used_audio_boundary_indices,
                    ) else {
                        *self = backup;
                        return false;
                    };
                    let reused_empty_boundary_track = self.children.len() == track_count_before
                        && track_is_empty_boundary(&self.children[track_index]);
                    if self.children.len() > track_count_before {
                        shift_track_index_after_insert(&mut dest_track_index, track_index);
                        shift_move_placements_after_insert(&mut placements, track_index);
                        shift_track_indices_after_insert(
                            &mut used_audio_track_indices,
                            track_index,
                        );
                        shift_track_indices_after_insert(
                            &mut used_audio_boundary_indices,
                            track_index,
                        );
                        shift_track_indices_after_insert(
                            &mut used_video_track_indices,
                            track_index,
                        );
                    }
                    placements.push((track_index, move_item.item, false));
                    used_audio_track_indices.push(track_index);
                    if reused_empty_boundary_track {
                        used_audio_boundary_indices.push(track_index);
                    }
                }
                TrackKind::Video => {
                    let track_count_before = self.children.len();
                    let Some(mut track_index) = self.find_or_create_video_track_for_audio(
                        dest_track_index,
                        moved_start,
                        moved_duration,
                        &mut created_track_indices,
                        Some(sync_clips_id),
                        true,
                        overlap_policy,
                        insert_policy_for_video,
                    ) else {
                        *self = backup;
                        return false;
                    };
                    if self.children.len() > track_count_before {
                        shift_track_index_after_insert(&mut dest_track_index, track_index);
                        shift_move_placements_after_insert(&mut placements, track_index);
                        shift_track_indices_after_insert(
                            &mut used_audio_track_indices,
                            track_index,
                        );
                        shift_track_indices_after_insert(
                            &mut used_audio_boundary_indices,
                            track_index,
                        );
                        shift_track_indices_after_insert(
                            &mut used_video_track_indices,
                            track_index,
                        );
                    }
                    if used_video_track_indices.contains(&track_index) {
                        let insert_at = track_index;
                        self.children
                            .insert(insert_at, self.new_numbered_track(TrackKind::Video));
                        created_track_indices.push(insert_at);
                        shift_track_index_after_insert(&mut dest_track_index, insert_at);
                        shift_move_placements_after_insert(&mut placements, insert_at);
                        shift_track_indices_after_insert(&mut used_audio_track_indices, insert_at);
                        shift_track_indices_after_insert(
                            &mut used_audio_boundary_indices,
                            insert_at,
                        );
                        shift_track_indices_after_insert(&mut used_video_track_indices, insert_at);
                        track_index = insert_at;
                    }
                    placements.push((track_index, move_item.item, false));
                    used_video_track_indices.push(track_index);
                }
                TrackKind::Other => {
                    *self = backup;
                    return false;
                }
            }
        }

        let mut boundary_track_indices =
            self.boundary_track_indices_for_anchors(&[sync_clips_id], &[dest_track_index], &[]);
        for (track_index, _, _) in &placements {
            if !boundary_track_indices.contains(track_index) {
                boundary_track_indices.push(*track_index);
            }
        }
        boundary_track_indices.sort_unstable();
        boundary_track_indices.dedup();

        for track_index in boundary_track_indices {
            if placements
                .iter()
                .any(|(placement_track_index, _, _)| *placement_track_index == track_index)
            {
                continue;
            }
            let mut gap = Item::Gap(crate::Gap::make_gap(moved_duration));
            Self::ensure_unique_item_id(&mut gap, &mut used_ids);
            placements.push((track_index, gap, false));
        }
        placements.sort_by_key(|(track_index, _, _)| *track_index);

        let insert_policy = match placement {
            SyncedMovePlacement::Time { insert_policy, .. } => insert_policy,
            SyncedMovePlacement::Index { .. } => InsertPolicy::SplitAndInsert,
        };
        let cluster = self.boundary_group_indices(dest_track_index);
        if !self.apply_sync_splits_for_column_insert(
            moved_start,
            moved_duration,
            insert_policy,
            overlap_policy,
            SyncSplitIdPolicy::KeepShared,
            Some(&cluster),
        ) {
            *self = backup;
            return false;
        }

        for (track_index, item, is_selected) in placements {
            if is_selected {
                match placement {
                    SyncedMovePlacement::Time {
                        dest_time,
                        insert_policy,
                    } => {
                        let _ = self.children[track_index].insert_at_time(
                            dest_time,
                            item,
                            overlap_policy,
                            insert_policy,
                        );
                    }
                    SyncedMovePlacement::Index { dest_index } => {
                        let _ = self.children[track_index]
                            .insert_at_index(dest_index, item, overlap_policy);
                    }
                }
            } else {
                let moved_end = moved_start + item.duration().max(0.0);
                if range_is_gap_backed(&self.children[track_index], moved_start, moved_end) {
                    if !self.insert_gap_only(track_index, moved_start, item) {
                        *self = backup;
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
        }
        if self.get_item(&selected_id).is_none() {
            *self = backup;
            return false;
        }

        self.sanitize_preserving_all_gap_tracks();
        true
    }
}

pub(super) fn remove_gap_range(track: &mut Track, start: Seconds, end: Seconds) -> bool {
    if !range_is_gap_backed(track, start, end) {
        return false;
    }

    split_gap_boundary(track, end);
    split_gap_boundary(track, start);

    let mut pos = 0.0;
    let mut index = 0;
    while index < track.items.len() {
        let duration = track.items[index].duration().max(0.0);
        let item_start = pos;
        let item_end = pos + duration;
        if item_start >= start - EPS && item_end <= end + EPS {
            if !matches!(track.items[index], Item::Gap(_)) {
                return false;
            }
            track.items.remove(index);
            pos = item_end;
        } else {
            pos = item_end;
            index += 1;
        }
    }
    true
}

pub(super) fn range_is_gap_backed(track: &Track, start: Seconds, end: Seconds) -> bool {
    if start < -EPS || end < start - EPS {
        return false;
    }
    if end <= start + EPS {
        return true;
    }

    let total = track.total_duration();
    if start >= total - EPS {
        return true;
    }

    let mut pos: Seconds = 0.0;
    for item in &track.items {
        let item_start = pos;
        let item_end = pos + item.duration().max(0.0);
        if item_end > start + EPS && item_start < end - EPS {
            if !matches!(item, Item::Gap(_)) {
                return false;
            }
        }
        pos = item_end;
    }
    true
}

fn track_is_empty_boundary(track: &Track) -> bool {
    track.items.iter().all(|item| matches!(item, Item::Gap(_)))
}

fn range_has_blocking_clip(
    track: &Track,
    start: Seconds,
    end: Seconds,
    sync_clips_id: Option<i64>,
) -> bool {
    if start < -EPS || end < start - EPS {
        return true;
    }
    if end <= start + EPS {
        return false;
    }

    let total = track.total_duration();
    if start >= total - EPS {
        return false;
    }

    let mut pos: Seconds = 0.0;
    for item in &track.items {
        let item_start = pos;
        let item_end = pos + item.duration().max(0.0);
        if item_end > start + EPS && item_start < end - EPS {
            match item {
                Item::Gap(_) => {}
                Item::Clip(clip)
                    if sync_clips_id.is_some()
                        && resolve_sync_clips_id(&clip.metadata) == sync_clips_id => {}
                Item::Clip(_) => return true,
            }
        }
        pos = item_end;
    }
    false
}

pub(super) fn split_gap_boundary(track: &mut Track, time: Seconds) {
    let Some(index) = track.get_item_at_time(time) else {
        return;
    };
    let item_start = track.start_time_of_item(index);
    let local = time - item_start;
    if local <= EPS {
        return;
    }

    let mut gap = match track.items.remove(index) {
        Item::Gap(gap) => gap,
        other => {
            track.items.insert(index, other);
            return;
        }
    };
    let total = gap.source_range.duration.to_seconds().max(0.0);
    if local >= total - EPS {
        track.items.insert(index, Item::Gap(gap));
        return;
    }

    let mut left = gap.clone();
    left.source_range.duration.set_from_seconds(local.max(0.0));
    gap.source_range
        .duration
        .set_from_seconds((total - local).max(0.0));
    gap.set_id(Some(crate::types::gen_hex_id_12()));
    track.items.insert(index, Item::Gap(left));
    track.items.insert(index + 1, Item::Gap(gap));
}

fn clamp_clip_to_active_available_range(clip: &mut Clip) {
    clip.clamp_to_active_available_range();
}

fn resize_effective_duration(
    item: &Item,
    requested_duration: Seconds,
    clamp_to_media: bool,
) -> Seconds {
    let mut item = item.clone();
    item.set_duration(requested_duration.max(0.0));
    if clamp_to_media {
        if let Item::Clip(clip) = &mut item {
            clamp_clip_to_active_available_range(clip);
        }
    }
    item.duration().max(0.0)
}

fn item_source_start(item: &Item) -> Seconds {
    match item {
        Item::Clip(clip) => clip.source_range.start_time.to_seconds(),
        Item::Gap(gap) => gap.source_range.start_time.to_seconds(),
    }
}

fn set_item_source_start(item: &mut Item, source_start_time: Seconds) {
    match item {
        Item::Clip(clip) => {
            clip.source_range
                .start_time
                .set_from_seconds(source_start_time);
        }
        Item::Gap(gap) => {
            gap.source_range
                .start_time
                .set_from_seconds(source_start_time);
        }
    }
}

fn replace_track_range_with_item(
    track: &mut Track,
    range_start: Seconds,
    range_end: Seconds,
    item: Item,
) {
    let start = range_start.max(0.0);
    let end = range_end.max(start);

    track.split_at_time(start);
    track.split_at_time(end);

    let start_index = track.get_item_at_time(start).unwrap_or(track.items.len());
    let end_index = track.get_item_at_time(end).unwrap_or(track.items.len());

    if end_index > start_index {
        track.items.drain(start_index..end_index);
    }
    track.items.insert(start_index, item);
    track.sanitize_preserving_all_gap_track();
}

fn insertion_start_or_end_for_policy(
    track: &Track,
    insert_time: Seconds,
    insert_policy: InsertPolicy,
) -> Option<Seconds> {
    let total = track.total_duration();
    let mut effective_time = insert_time;
    if effective_time < 0.0 {
        effective_time = total - effective_time;
    }
    if effective_time < 0.0 {
        return None;
    }
    if effective_time >= total - EPS {
        return Some(effective_time);
    }

    let Some(item_index) = track.get_item_at_time(effective_time) else {
        return Some(effective_time);
    };
    let item_start = track.start_time_of_item(item_index);
    let item_end = item_start + track.items[item_index].duration().max(0.0);
    let start = match insert_policy {
        InsertPolicy::SplitAndInsert => effective_time,
        InsertPolicy::InsertBefore => item_start,
        InsertPolicy::InsertAfter => item_end,
        InsertPolicy::InsertBeforeOrAfter => {
            let d_start = (effective_time - item_start).abs();
            let d_end = (item_end - effective_time).abs();
            if d_start <= d_end {
                item_start
            } else {
                item_end
            }
        }
    };
    Some(start)
}

fn resolve_sync_clips_id(metadata: &serde_json::Value) -> Option<i64> {
    Clip::resolve_otio_i64(metadata, "Link Group ID")
}

fn resolve_tellers_group_id(metadata: &serde_json::Value) -> Option<i64> {
    Clip::resolve_otio_i64(metadata, "Tellers Group ID")
}

fn item_link_group_id(item: &Item) -> Option<i64> {
    match item {
        Item::Clip(clip) => clip.sync_clips_id(),
        Item::Gap(_) => None,
    }
}

fn item_tellers_group_id(item: &Item) -> Option<i64> {
    match item {
        Item::Clip(clip) => Clip::resolve_otio_i64(&clip.metadata, "Tellers Group ID"),
        Item::Gap(_) => None,
    }
}

pub(super) fn set_resolve_sync_clips_id(metadata: &mut serde_json::Value, sync_clips_id: i64) {
    if metadata.as_object().is_none() {
        *metadata = serde_json::Value::Object(serde_json::Map::new());
    }
    let map = metadata.as_object_mut().unwrap();
    let resolve = map
        .entry("Resolve_OTIO".to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if resolve.as_object().is_none() {
        *resolve = serde_json::Value::Object(serde_json::Map::new());
    }
    resolve.as_object_mut().unwrap().insert(
        "Link Group ID".to_string(),
        serde_json::Value::Number(serde_json::Number::from(sync_clips_id)),
    );
}

fn remove_resolve_sync_clips_id(metadata: &mut serde_json::Value) -> bool {
    let Some(resolve) = metadata
        .get_mut("Resolve_OTIO")
        .and_then(|value| value.as_object_mut())
    else {
        return false;
    };
    resolve.remove("Link Group ID").is_some()
}
