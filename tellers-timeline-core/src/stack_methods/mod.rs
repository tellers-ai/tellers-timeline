use crate::{
    Clip, Gap, IdMetadataExt, InsertPolicy, Item, OverlapPolicy, Seconds, Stack, Track, TrackKind,
};
use std::collections::{HashMap, HashSet};

mod stack_item_delete;
mod stack_item_get;
mod stack_item_insert;
mod stack_item_link;
mod stack_item_move;
mod stack_item_replace;
mod stack_item_split;
mod stack_track;

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
pub struct TrackBoundaryGroupInfo {
    pub start_index: usize,
    pub end_index: usize,
    pub track_indices: Vec<usize>,
    pub track_ids: Vec<Option<String>>,
    pub primary_track_index: usize,
    pub primary_track_id: Option<String>,
    pub bound_track_indices: Vec<usize>,
    pub bound_track_ids: Vec<Option<String>>,
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

    fn find_or_create_video_track_for_audio(
        &mut self,
        audio_track_index: usize,
        dest_time: Seconds,
        duration: Seconds,
        created_track_indices: &mut Vec<usize>,
        sync_clips_id: Option<i64>,
        use_sync_backed_track: bool,
        overlap_policy: OverlapPolicy,
    ) -> Option<usize> {
        let end_time = dest_time + duration;
        let mut group_start = audio_track_index;
        while group_start > 0 && self.children[group_start - 1].kind == TrackKind::Audio {
            group_start -= 1;
            if track_is_empty_boundary(&self.children[group_start]) {
                break;
            }
        }
        if group_start > 0
            && !track_is_empty_boundary(&self.children[group_start])
            && self.children[group_start - 1].kind == TrackKind::Video
        {
            group_start -= 1;
        }

        if self.children.get(group_start)?.kind == TrackKind::Video {
            if range_is_gap_backed(&self.children[group_start], dest_time, end_time) {
                return Some(group_start);
            }
            let has_blocking_clip = range_has_blocking_clip(
                &self.children[group_start],
                dest_time,
                end_time,
                sync_clips_id,
            );
            let can_push_existing_boundary = overlap_policy == OverlapPolicy::Push
                && self.track_matches_primary_sync_boundary(audio_track_index, group_start);
            if !has_blocking_clip || can_push_existing_boundary {
                return use_sync_backed_track.then_some(group_start);
            }
        }

        let track = self.new_numbered_track(TrackKind::Video);
        self.children.insert(group_start, track);
        created_track_indices.push(group_start);
        Some(group_start)
    }

    fn delete_sync_clips(
        &mut self,
        sync_clips_id: i64,
        replace_with_gap: bool,
    ) -> Vec<(usize, Item)> {
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

        targets.sort_by(|a, b| b.cmp(a));
        let mut removed = Vec::new();
        let mut used_ids = self.collect_timeline_ids();
        for (ti, ii) in targets {
            if ti >= self.children.len() || ii >= self.children[ti].items.len() {
                continue;
            }
            let removed_item = self.children[ti].items.remove(ii);
            let duration = removed_item.duration().max(0.0);
            if replace_with_gap && duration > EPS {
                let mut gap = Item::Gap(crate::Gap::make_gap(duration));
                Self::ensure_unique_item_id(&mut gap, &mut used_ids);
                self.children[ti].items.insert(ii, gap);
                self.children[ti].merge_adjacent_gaps();
            }
            removed.push((ti, removed_item));
        }
        removed.reverse();
        removed
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

    fn sanitized_clip_duration(item: &Item) -> Option<Seconds> {
        let Item::Clip(mut clip) = item.clone() else {
            return None;
        };
        clamp_clip_to_active_available_range(&mut clip);
        let duration = clip.source_range.duration.to_seconds().max(0.0);
        (duration > EPS).then_some(duration)
    }

    fn synced_inputs_match_duration(duration: Seconds, inputs: &SyncedInputs) -> bool {
        if let Some(video_item) = &inputs.video {
            let Some(video_duration) = Self::sanitized_clip_duration(video_item) else {
                return false;
            };
            if (video_duration - duration).abs() > EPS {
                return false;
            }
        }

        for audio_item in &inputs.audio {
            let Some(audio_duration) = Self::sanitized_clip_duration(audio_item) else {
                return false;
            };
            if (audio_duration - duration).abs() > EPS {
                return false;
            }
        }

        true
    }

    fn same_item_content_ignoring_timeline_id(left: &Item, right: &Item) -> bool {
        let mut left = left.clone();
        let mut right = right.clone();
        left.set_id(None);
        right.set_id(None);
        left == right
    }

    fn remove_item_from_synced_video_input(item: &Item, synced_video_clip: &mut Option<Item>) {
        if synced_video_clip.as_ref().is_some_and(|synced_item| {
            Self::same_item_content_ignoring_timeline_id(item, synced_item)
        }) {
            *synced_video_clip = None;
        }
    }

    fn normalize_synced_inputs(
        item: &Item,
        synced_audio_clips: Option<Vec<Item>>,
        synced_video_clip: Option<Item>,
    ) -> SyncedInputs {
        let mut inputs = SyncedInputs {
            audio: synced_audio_clips.unwrap_or_default(),
            video: synced_video_clip,
        };
        Self::remove_item_from_synced_video_input(item, &mut inputs.video);
        for item in &mut inputs.audio {
            item.clamp_to_active_available_range();
        }
        if let Some(video) = &mut inputs.video {
            video.clamp_to_active_available_range();
        }
        inputs
    }

    fn has_synced_inputs(
        synced_audio_clips: &Option<Vec<Item>>,
        synced_video_clip: &Option<Item>,
    ) -> bool {
        synced_audio_clips.is_some() || synced_video_clip.is_some()
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
        if primary_track_index == candidate_track_index {
            return true;
        }
        let Some(candidate_track) = self.children.get(candidate_track_index) else {
            return false;
        };
        let primary_segments = self.flatten_track_segments(primary_track_index);
        let mut start = 0.0;
        for item in &candidate_track.items {
            let duration = item.duration().max(0.0);
            let end = start + duration;
            match item {
                Item::Gap(_) => {}
                Item::Clip(clip) => {
                    let Some(sync_clips_id) = resolve_sync_clips_id(&clip.metadata) else {
                        return false;
                    };
                    // Match by sync group + overlapping position rather than exact
                    // start/end. Synced members may differ slightly in start/duration
                    // (the model is delta-based), so requiring exact equality wrongly
                    // treats a synced partner's track as outside the boundary — which
                    // makes inserts create new audio tracks instead of reusing it.
                    if !primary_segments.iter().any(|segment| {
                        segment.sync_clips_id == Some(sync_clips_id)
                            && start < segment.end - EPS
                            && end > segment.start + EPS
                    }) {
                        return false;
                    }
                }
            }
            start = end;
        }
        true
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

    fn synced_clips_overlapping_range(
        &self,
        track_index: usize,
        start: Seconds,
        duration: Seconds,
    ) -> Vec<i64> {
        let end = start + duration.max(0.0);
        let mut groups = Vec::new();
        for segment in self.flatten_track_segments(track_index) {
            if segment.end > start + EPS && segment.start < end - EPS {
                if let Some(group) = segment.sync_clips_id {
                    if !groups.contains(&group) {
                        groups.push(group);
                    }
                }
            }
        }
        groups
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

    fn synced_clips_touched_by_insert_at_time(
        &self,
        track_index: usize,
        dest_time: Seconds,
        duration: Seconds,
        overlap_policy: OverlapPolicy,
        insert_policy: InsertPolicy,
    ) -> Vec<i64> {
        let Some(track) = self.children.get(track_index) else {
            return Vec::new();
        };
        let Some(start) = insertion_start_for_policy(track, dest_time, insert_policy) else {
            return Vec::new();
        };
        let total = track.total_duration();
        let affected_duration = if overlap_policy == OverlapPolicy::Push {
            total - start
        } else {
            duration
        };
        self.synced_clips_overlapping_range(track_index, start, affected_duration)
    }

    fn synced_clips_touched_by_insert_at_index(
        &self,
        track_index: usize,
        dest_index: usize,
        duration: Seconds,
        overlap_policy: OverlapPolicy,
    ) -> Vec<i64> {
        let Some(track) = self.children.get(track_index) else {
            return Vec::new();
        };
        if dest_index >= track.items.len() {
            return Vec::new();
        }
        let start = track.start_time_of_item(dest_index);
        let affected_duration = if overlap_policy == OverlapPolicy::Push {
            track.total_duration() - start
        } else {
            duration
        };
        self.synced_clips_overlapping_range(track_index, start, affected_duration)
    }

    fn synced_clips_for_insert_at_time_boundary(
        &self,
        track_index: usize,
        dest_time: Seconds,
        duration: Seconds,
        overlap_policy: OverlapPolicy,
        insert_policy: InsertPolicy,
    ) -> Vec<i64> {
        let Some(track) = self.children.get(track_index) else {
            return Vec::new();
        };
        let Some(start) = insertion_start_or_end_for_policy(track, dest_time, insert_policy) else {
            return Vec::new();
        };
        let mut groups = self.synced_clips_touched_by_insert_at_time(
            track_index,
            dest_time,
            duration,
            overlap_policy,
            insert_policy,
        );
        for group in self.synced_clips_adjacent_to_time(track_index, start) {
            if !groups.contains(&group) {
                groups.push(group);
            }
        }
        for group in self.synced_clips_adjacent_to_time(track_index, start + duration.max(0.0)) {
            if !groups.contains(&group) {
                groups.push(group);
            }
        }
        groups
    }

    fn synced_clips_for_insert_at_index_boundary(
        &self,
        track_index: usize,
        dest_index: usize,
        duration: Seconds,
        overlap_policy: OverlapPolicy,
    ) -> Vec<i64> {
        let Some(track) = self.children.get(track_index) else {
            return Vec::new();
        };
        let start = track.start_time_of_item(dest_index.min(track.items.len()));
        let mut groups = self.synced_clips_touched_by_insert_at_index(
            track_index,
            dest_index,
            duration,
            overlap_policy,
        );
        for group in self.synced_clips_adjacent_to_time(track_index, start) {
            if !groups.contains(&group) {
                groups.push(group);
            }
        }
        for group in self.synced_clips_adjacent_to_time(track_index, start + duration.max(0.0)) {
            if !groups.contains(&group) {
                groups.push(group);
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
    ) -> Option<InsertItemAtTimeResult> {
        let mut primary_item = item;
        primary_item.clamp_to_active_available_range();
        let synced_inputs =
            Self::normalize_synced_inputs(&primary_item, synced_audio_clips, synced_video_clip);
        let has_synced_clips = !synced_inputs.audio.is_empty() || synced_inputs.video.is_some();
        if has_synced_clips && !matches!(primary_item, Item::Clip(_)) {
            return None;
        }
        if synced_inputs.video.is_some() && self.children[dest_track_index].kind != TrackKind::Audio
        {
            return None;
        }

        let modified_duration = primary_item.duration().max(0.0);
        if modified_duration <= EPS
            || !Self::synced_inputs_match_duration(modified_duration, &synced_inputs)
        {
            return None;
        }
        let boundary_sync_clips = if let Some(dest_index) = dest_index {
            self.synced_clips_for_insert_at_index_boundary(
                dest_track_index,
                dest_index,
                modified_duration,
                overlap_policy,
            )
        } else {
            self.synced_clips_for_insert_at_time_boundary(
                dest_track_index,
                dest_time,
                modified_duration,
                overlap_policy,
                insert_policy,
            )
        };

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

        let mut primary_track_index = dest_track_index;
        let mut created_track_indices = Vec::new();
        let mut audio_track_indices = Vec::new();
        let mut video_track_index = None;

        if synced_inputs.video.is_some() {
            let track_count_before = self.children.len();
            let Some(track_index) = self.find_or_create_video_track_for_audio(
                primary_track_index,
                start,
                modified_duration,
                &mut created_track_indices,
                sync_clips_id,
                true,
                overlap_policy,
            ) else {
                *self = backup;
                return None;
            };
            if self.children.len() > track_count_before {
                if track_index <= primary_track_index {
                    primary_track_index += 1;
                }
                for audio_track_index in &mut audio_track_indices {
                    if track_index <= *audio_track_index {
                        *audio_track_index += 1;
                    }
                }
            }
            video_track_index = Some(track_index);
        }

        let mut used_audio_boundary_indices = Vec::new();
        for _ in &synced_inputs.audio {
            let track_count_before = self.children.len();
            let mut unavailable_audio_track_indices = audio_track_indices.clone();
            if self.children[primary_track_index].kind == TrackKind::Audio
                && !unavailable_audio_track_indices.contains(&primary_track_index)
            {
                unavailable_audio_track_indices.push(primary_track_index);
            }
            let Some(track_index) = self.find_or_create_audio_track(
                primary_track_index,
                start,
                modified_duration,
                &mut created_track_indices,
                &unavailable_audio_track_indices,
                &used_audio_boundary_indices,
                sync_clips_id,
                true,
                overlap_policy,
            ) else {
                *self = backup;
                return None;
            };
            let reused_empty_boundary_track = self.children.len() == track_count_before
                && track_is_empty_boundary(&self.children[track_index]);
            if self.children.len() > track_count_before {
                if track_index <= primary_track_index {
                    primary_track_index += 1;
                }
                if let Some(video_index) = &mut video_track_index {
                    if track_index <= *video_index {
                        *video_index += 1;
                    }
                }
                for audio_track_index in &mut audio_track_indices {
                    if track_index <= *audio_track_index {
                        *audio_track_index += 1;
                    }
                }
            }
            audio_track_indices.push(track_index);
            if reused_empty_boundary_track {
                used_audio_boundary_indices.push(track_index);
            }
        }

        let boundary_groups = if boundary_sync_clips.is_empty() && has_synced_clips {
            self.synced_clips_adjacent_to_time(primary_track_index, start)
        } else {
            boundary_sync_clips.clone()
        };
        let mut boundary_track_indices =
            self.boundary_track_indices_for_anchors(&boundary_groups, &[primary_track_index], &[]);
        // A sync clips must keep one shared duration across its tracks, so the spacer
        // has to reach every track that holds a member of an affected group — even when
        // an empty track separates the group. Without this, splitting one member (e.g.
        // the video) would leave the across-the-gap member (the audio) unsplit and the
        // group's durations would diverge.
        for sync_clips in &boundary_groups {
            for (track_index, _) in self.synced_clips_targets(*sync_clips) {
                if !boundary_track_indices.contains(&track_index) {
                    boundary_track_indices.push(track_index);
                }
            }
        }
        if !boundary_track_indices.contains(&primary_track_index) {
            boundary_track_indices.push(primary_track_index);
        }
        if let Some(video_track_index) = video_track_index {
            if !boundary_track_indices.contains(&video_track_index) {
                boundary_track_indices.push(video_track_index);
            }
        }
        for audio_track_index in &audio_track_indices {
            if !boundary_track_indices.contains(audio_track_index) {
                boundary_track_indices.push(*audio_track_index);
            }
        }
        boundary_track_indices.sort_unstable();
        boundary_track_indices.dedup();

        let mut ordered_audio_track_indices: Vec<usize> = boundary_track_indices
            .iter()
            .copied()
            .filter(|track_index| {
                *track_index != primary_track_index
                    && self.children[*track_index].kind == TrackKind::Audio
            })
            .collect();
        ordered_audio_track_indices.sort_by_key(|track_index| {
            (track_index.abs_diff(primary_track_index), *track_index)
        });
        ordered_audio_track_indices.truncate(audio_track_indices.len());
        for track_index in audio_track_indices {
            if ordered_audio_track_indices.len() >= synced_inputs.audio.len() {
                break;
            }
            if !ordered_audio_track_indices.contains(&track_index) {
                ordered_audio_track_indices.push(track_index);
            }
        }
        let audio_track_indices = ordered_audio_track_indices;

        let (primary_item, primary_id) = match primary_item {
            Item::Clip(_) => {
                let (item, id) = Self::prepare_synced_item(
                    primary_item,
                    modified_duration,
                    sync_clips_id,
                    &mut used_ids,
                )?;
                (item, id)
            }
            Item::Gap(mut gap) => {
                gap.source_range.duration.set_from_seconds(modified_duration);
                let mut item = Item::Gap(gap);
                let id = Self::ensure_unique_item_id(&mut item, &mut used_ids);
                (item, id)
            }
        };

        // ---------------------------------------------------------------------
        // Build the synced "column": exactly one item per track in the sync
        // boundary. The asset supplies the primary clip and its video/audio
        // companions; every remaining boundary track is padded with a gap
        // "spacer" of the same duration. So the column is always as wide as the
        // boundary (pad with gaps when the asset has fewer clips than there are
        // sync tracks; extra tracks were already created above when it has more).
        // The whole column is then inserted at one position with one policy, so
        // every track splits/pushes identically and the group stays aligned.
        // ---------------------------------------------------------------------
        let mut column = vec![(primary_track_index, primary_item)];
        let mut synced_video_clip_id = None;
        if let (Some(video_track_index), Some(video_item)) =
            (video_track_index, synced_inputs.video)
        {
            let Some((video_item, video_id)) = Self::prepare_synced_item(
                video_item,
                modified_duration,
                sync_clips_id,
                &mut used_ids,
            ) else {
                *self = backup;
                return None;
            };
            column.push((video_track_index, video_item));
            synced_video_clip_id = Some(video_id);
        }

        let mut audio_clips = Vec::new();
        for (audio_item, audio_track_index) in
            synced_inputs.audio.into_iter().zip(audio_track_indices)
        {
            let Some((audio_item, audio_id)) = Self::prepare_synced_item(
                audio_item,
                modified_duration,
                sync_clips_id,
                &mut used_ids,
            ) else {
                *self = backup;
                return None;
            };
            column.push((audio_track_index, audio_item));
            audio_clips.push((audio_id, audio_track_index));
        }

        // Pad every boundary track that has no clip in the column with a gap.
        for track_index in boundary_track_indices {
            if column
                .iter()
                .any(|(column_track_index, _)| *column_track_index == track_index)
            {
                continue;
            }
            let mut gap = Item::Gap(Gap::make_gap(modified_duration));
            Self::ensure_unique_item_id(&mut gap, &mut used_ids);
            column.push((track_index, gap));
        }
        column.sort_by_key(|(track_index, _)| *track_index);

        // Insert the whole column. The primary lands at the requested position
        // with the caller's insert policy; every other track receives its item
        // at the same resolved start with the same overlap policy, splitting at
        // that exact time, so all tracks are edited identically.
        for (track_index, item) in column {
            if track_index == primary_track_index {
                if let Some(dest_index) = dest_index {
                    self.children[track_index].insert_at_index(dest_index, item, overlap_policy);
                } else {
                    self.children[track_index].insert_at_time(
                        dest_time,
                        item,
                        overlap_policy,
                        insert_policy,
                    );
                }
                continue;
            }

            // A gap spacer that lands at or past the end of an Override track has
            // nothing to overwrite, so push it to extend the track instead.
            let sync_track_overlap_policy = if matches!(item, Item::Gap(_))
                && overlap_policy == OverlapPolicy::Override
                && start >= self.children[track_index].total_duration() - EPS
            {
                OverlapPolicy::Push
            } else {
                overlap_policy
            };
            self.children[track_index].insert_at_time(
                start,
                item,
                sync_track_overlap_policy,
                InsertPolicy::SplitAndInsert,
            );
        }

        if !matches!(self.get_item(&primary_id), Some((_, _, Item::Clip(_)))) {
            return Some(InsertItemAtTimeResult::ItemId(primary_id));
        }
        Some(InsertItemAtTimeResult::Synced(SyncedInsertResult {
            primary_clip_id: primary_id,
            audio_clips,
            synced_video_clip_id,
            sync_clips_id,
            created_track_indices,
        }))
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
                let removed = self.children[ti].items[ii].clone();
                // Use the track API for deletion and optional gap insertion/merge behavior
                let deleted = self.children[ti].delete_clip(ii, replace_with_gap);
                if deleted {
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
                } else {
                    return None;
                }
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
        let targets = self.synced_clips_targets(sync_clips_id);
        if targets.len() <= 1 {
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

        for (track_index, item, is_selected) in placements {
            if is_selected {
                match placement {
                    SyncedMovePlacement::Time {
                        dest_time,
                        insert_policy,
                    } => self.children[track_index].insert_at_time(
                        dest_time,
                        item,
                        overlap_policy,
                        insert_policy,
                    ),
                    SyncedMovePlacement::Index { dest_index } => {
                        self.children[track_index].insert_at_index(dest_index, item, overlap_policy)
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

fn insertion_start_for_policy(
    track: &Track,
    insert_time: Seconds,
    insert_policy: InsertPolicy,
) -> Option<Seconds> {
    let total = track.total_duration();
    let mut effective_time = insert_time;
    if effective_time < 0.0 {
        effective_time = total - effective_time;
    }
    if effective_time < 0.0 || effective_time >= total - EPS {
        return None;
    }

    let item_index = track.get_item_at_time(effective_time)?;
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
    (start < total - EPS).then_some(start)
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
    metadata
        .get("Resolve_OTIO")
        .and_then(|v| v.get("Link Group ID"))
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_u64().and_then(|n| i64::try_from(n).ok()))
                .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
        })
}

fn set_resolve_sync_clips_id(metadata: &mut serde_json::Value, sync_clips_id: i64) {
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
