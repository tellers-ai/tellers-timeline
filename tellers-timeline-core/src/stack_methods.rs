use crate::{
    Clip, IdMetadataExt, InsertPolicy, Item, OverlapPolicy, Seconds, Stack, Track, TrackKind,
};
use std::collections::HashSet;

const EPS: Seconds = 1e-9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedInsertResult {
    pub primary_clip_id: String,
    pub audio_clips: Vec<(String, usize)>,
    pub linked_video_clip_id: Option<String>,
    pub link_group_id: Option<i64>,
    pub created_track_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertItemAtTimeResult {
    ItemId(String),
    Linked(LinkedInsertResult),
}

#[derive(Debug, Clone)]
struct LinkedInputs {
    audio: Vec<Item>,
    video: Option<Item>,
}

#[derive(Debug, Clone)]
struct LinkedMoveItem {
    track_index: usize,
    item_index: usize,
    item: Item,
    is_selected: bool,
}

enum LinkedMovePlacement {
    Time {
        dest_time: Seconds,
        insert_policy: InsertPolicy,
    },
    Index {
        dest_index: usize,
    },
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

    fn next_link_group_id(&self) -> i64 {
        self.children
            .iter()
            .flat_map(|track| track.items.iter())
            .filter_map(|item| match item {
                Item::Clip(clip) => resolve_link_group_id(&clip.metadata),
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
            return self
                .insert_item_at_time(
                    track_index,
                    start,
                    item,
                    OverlapPolicy::Override,
                    InsertPolicy::InsertBefore,
                    None,
                    None,
                )
                .is_some();
        }

        if !range_is_gap_backed(&self.children[track_index], start, end) {
            return false;
        }

        let track = &mut self.children[track_index];
        split_gap_boundary(track, end);
        split_gap_boundary(track, start);

        self.insert_item_at_time(
            track_index,
            start,
            item,
            OverlapPolicy::Override,
            InsertPolicy::InsertBefore,
            None,
            None,
        )
        .is_some()
    }

    fn find_or_create_audio_track(
        &mut self,
        track_index: usize,
        dest_time: Seconds,
        duration: Seconds,
        created_track_indices: &mut Vec<usize>,
        used_audio_indices: &[usize],
        link_group_id: Option<i64>,
        use_link_backed_track: bool,
    ) -> Option<usize> {
        let end_time = dest_time + duration;
        if self.children.get(track_index)?.kind == TrackKind::Video {
            let mut index = track_index;
            while index > 0 && self.children[index - 1].kind == TrackKind::Audio {
                index -= 1;
            }
            for audio_index in (index..track_index).rev() {
                if used_audio_indices.contains(&audio_index) {
                    continue;
                }
                if range_is_gap_backed(&self.children[audio_index], dest_time, end_time) {
                    return Some(audio_index);
                }
                let has_blocking_clip = range_has_blocking_clip(
                    &self.children[audio_index],
                    dest_time,
                    end_time,
                    link_group_id,
                );
                if !has_blocking_clip {
                    if use_link_backed_track {
                        return Some(audio_index);
                    }
                    continue;
                }
                let insert_at = audio_index + 1;
                self.children
                    .insert(insert_at, Track::new(TrackKind::Audio, None));
                created_track_indices.push(insert_at);
                return Some(insert_at);
            }

            self.children
                .insert(track_index, Track::new(TrackKind::Audio, None));
            created_track_indices.push(track_index);
            return Some(track_index);
        }

        let mut index = match self.children.get(track_index)?.kind {
            TrackKind::Audio => {
                let mut audio_start = track_index;
                while audio_start > 0 && self.children[audio_start - 1].kind == TrackKind::Audio {
                    audio_start -= 1;
                }
                audio_start
            }
            TrackKind::Video | TrackKind::Other => return None,
        };

        while index < self.children.len() && self.children[index].kind == TrackKind::Audio {
            if used_audio_indices.contains(&index) {
                index += 1;
                continue;
            }
            if range_is_gap_backed(&self.children[index], dest_time, end_time) {
                return Some(index);
            }
            let has_blocking_clip = range_has_blocking_clip(
                &self.children[index],
                dest_time,
                end_time,
                link_group_id,
            );
            if !has_blocking_clip {
                if use_link_backed_track {
                    return Some(index);
                }
                index += 1;
                continue;
            }
            self.children
                .insert(index, Track::new(TrackKind::Audio, None));
            created_track_indices.push(index);
            return Some(index);
        }

        let insert_at = index;
        self.children
            .insert(insert_at, Track::new(TrackKind::Audio, None));
        created_track_indices.push(insert_at);
        Some(insert_at)
    }

    fn find_or_create_video_track_for_audio(
        &mut self,
        audio_track_index: usize,
        dest_time: Seconds,
        duration: Seconds,
        created_track_indices: &mut Vec<usize>,
        link_group_id: Option<i64>,
        use_link_backed_track: bool,
    ) -> Option<usize> {
        let end_time = dest_time + duration;
        let mut group_start = audio_track_index;
        while group_start > 0 && self.children[group_start - 1].kind == TrackKind::Audio {
            group_start -= 1;
        }
        if group_start > 0 && self.children[group_start - 1].kind == TrackKind::Video {
            group_start -= 1;
        }

        if self.children.get(group_start)?.kind == TrackKind::Video {
            if range_is_gap_backed(&self.children[group_start], dest_time, end_time) {
                return Some(group_start);
            }
            if !range_has_blocking_clip(
                &self.children[group_start],
                dest_time,
                end_time,
                link_group_id,
            ) {
                return use_link_backed_track.then_some(group_start);
            }
        }

        self.children
            .insert(group_start, Track::new(TrackKind::Video, None));
        created_track_indices.push(group_start);
        Some(group_start)
    }

    /// Find an item by id across all tracks. Returns (track_index, item_index, &Item).
    pub fn get_item(&self, item_id: &str) -> Option<(usize, usize, &Item)> {
        for (ti, track) in self.children.iter().enumerate() {
            if let Some((ii, item)) = track.get_item_by_id(item_id) {
                return Some((ti, ii, item));
            }
        }
        None
    }

    /// Delete an item by id across all tracks. Linked clips with the same Resolve
    /// link group are deleted too. If replace_with_gap is true and a removed item
    /// has a positive duration, a gap of equal duration is inserted.
    /// Returns removed items with their source track indices.
    pub fn delete_item(&mut self, item_id: &str, replace_with_gap: bool) -> Vec<(usize, Item)> {
        let link_group_id = match self.get_item(item_id).and_then(|(_, _, item)| match item {
            Item::Clip(clip) => resolve_link_group_id(&clip.metadata),
            Item::Gap(_) => None,
        }) {
            Some(id) => id,
            None => {
                return self
                    .delete_one_item(item_id, replace_with_gap)
                    .into_iter()
                    .collect();
            }
        };

        let mut targets = Vec::new();
        for (ti, track) in self.children.iter().enumerate() {
            for (ii, item) in track.items.iter().enumerate() {
                if let Item::Clip(clip) = item {
                    if resolve_link_group_id(&clip.metadata) == Some(link_group_id) {
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

    fn linked_group_targets(&self, link_group_id: i64) -> Vec<(usize, usize)> {
        let mut targets = Vec::new();
        for (ti, track) in self.children.iter().enumerate() {
            for (ii, item) in track.items.iter().enumerate() {
                if let Item::Clip(clip) = item {
                    if resolve_link_group_id(&clip.metadata) == Some(link_group_id) {
                        targets.push((ti, ii));
                    }
                }
            }
        }
        targets
    }

    fn cleanup_singleton_link_groups(&mut self, link_group_ids: &[i64]) -> usize {
        let mut count = 0;
        let mut seen = HashSet::new();
        for link_group_id in link_group_ids {
            if !seen.insert(*link_group_id) {
                continue;
            }
            let targets = self.linked_group_targets(*link_group_id);
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
                if remove_resolve_link_group_id(&mut clip.metadata) {
                    count += 1;
                }
            }
        }
        count
    }

    fn set_item_link_group(item: &mut Item, link_group_id: Option<i64>) {
        let Item::Clip(clip) = item else {
            return;
        };
        if let Some(link_group_id) = link_group_id {
            set_resolve_link_group_id(&mut clip.metadata, link_group_id);
        } else {
            remove_resolve_link_group_id(&mut clip.metadata);
        }
    }

    fn prepare_linked_item(
        item: Item,
        duration: Seconds,
        link_group_id: Option<i64>,
        used_ids: &mut HashSet<String>,
    ) -> Option<(Item, String)> {
        let Item::Clip(mut clip) = item else {
            return None;
        };
        clip.source_range.duration.value = duration;
        clamp_clip_to_active_available_range(&mut clip);
        if clip.source_range.duration.value + EPS < duration
            || clip.source_range.duration.value <= EPS
        {
            return None;
        }

        let mut item = Item::Clip(clip);
        Self::set_item_link_group(&mut item, link_group_id);
        let id = Self::ensure_unique_item_id(&mut item, used_ids);
        Some((item, id))
    }

    fn sanitized_clip_duration(item: &Item) -> Option<Seconds> {
        let Item::Clip(mut clip) = item.clone() else {
            return None;
        };
        clamp_clip_to_active_available_range(&mut clip);
        let duration = clip.source_range.duration.value.max(0.0);
        (duration > EPS).then_some(duration)
    }

    fn linked_inputs_match_duration(duration: Seconds, inputs: &LinkedInputs) -> bool {
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

    fn remove_item_from_linked_video_input(item: &Item, linked_video_clip: &mut Option<Item>) {
        if linked_video_clip
            .as_ref()
            .is_some_and(|linked_item| Self::same_item_content_ignoring_timeline_id(item, linked_item))
        {
            *linked_video_clip = None;
        }
    }

    fn normalize_linked_inputs(
        item: &Item,
        linked_audio_clips: Option<Vec<Item>>,
        linked_video_clip: Option<Item>,
    ) -> LinkedInputs {
        let mut inputs = LinkedInputs {
            audio: linked_audio_clips.unwrap_or_default(),
            video: linked_video_clip,
        };
        Self::remove_item_from_linked_video_input(item, &mut inputs.video);
        inputs
    }

    fn has_linked_inputs(
        linked_audio_clips: &Option<Vec<Item>>,
        linked_video_clip: &Option<Item>,
    ) -> bool {
        linked_audio_clips.is_some() || linked_video_clip.is_some()
    }

    fn linked_groups_overlapping_range(
        &self,
        track_index: usize,
        start: Seconds,
        duration: Seconds,
    ) -> Vec<i64> {
        let Some(track) = self.children.get(track_index) else {
            return Vec::new();
        };
        let end = start + duration.max(0.0);
        let mut groups = Vec::new();
        let mut pos = 0.0;
        for item in &track.items {
            let item_start = pos;
            let item_end = pos + item.duration().max(0.0);
            if item_end > start + EPS && item_start < end - EPS {
                if let Item::Clip(clip) = item {
                    if let Some(group) = resolve_link_group_id(&clip.metadata) {
                        if !groups.contains(&group) {
                            groups.push(group);
                        }
                    }
                }
            }
            pos = item_end;
        }
        groups
    }

    fn linked_groups_touched_by_insert_at_time(
        &self,
        track_index: usize,
        dest_time: Seconds,
        duration: Seconds,
    ) -> Vec<i64> {
        let Some(track) = self.children.get(track_index) else {
            return Vec::new();
        };
        let total = track.total_duration();
        let mut start = dest_time;
        if start < 0.0 {
            start = total - start;
        }
        if start < 0.0 || start >= total - EPS {
            return Vec::new();
        }
        self.linked_groups_overlapping_range(track_index, start, duration)
    }

    fn linked_groups_touched_by_insert_at_index(
        &self,
        track_index: usize,
        dest_index: usize,
        duration: Seconds,
    ) -> Vec<i64> {
        let Some(track) = self.children.get(track_index) else {
            return Vec::new();
        };
        if dest_index >= track.items.len() {
            return Vec::new();
        }
        self.linked_groups_overlapping_range(
            track_index,
            track.start_time_of_item(dest_index),
            duration,
        )
    }

    fn track_indices_for_link_groups(
        &self,
        link_groups: &[i64],
        excluded_track_index: usize,
    ) -> Vec<usize> {
        let mut track_indices = Vec::new();
        for (track_index, track) in self.children.iter().enumerate() {
            if track_index == excluded_track_index {
                continue;
            }
            if track.items.iter().any(|item| match item {
                Item::Clip(clip) => resolve_link_group_id(&clip.metadata)
                    .is_some_and(|group| link_groups.contains(&group)),
                Item::Gap(_) => false,
            }) {
                track_indices.push(track_index);
            }
        }
        track_indices
    }

    fn insert_spacer_gap(
        &mut self,
        track_index: usize,
        start: Seconds,
        duration: Seconds,
        overlap_policy: OverlapPolicy,
        insert_policy: InsertPolicy,
        used_ids: &mut HashSet<String>,
    ) {
        let mut gap = Item::Gap(crate::Gap::make_gap(duration));
        Self::ensure_unique_item_id(&mut gap, used_ids);
        self.children[track_index].insert_at_time(
            start,
            gap,
            overlap_policy,
            insert_policy,
        );
    }

    fn insert_gap_then_linked_item(
        &mut self,
        track_index: usize,
        start: Seconds,
        duration: Seconds,
        item: Item,
        overlap_policy: OverlapPolicy,
        insert_policy: InsertPolicy,
        used_ids: &mut HashSet<String>,
    ) {
        self.insert_spacer_gap(
            track_index,
            start,
            duration,
            overlap_policy,
            insert_policy,
            used_ids,
        );
        self.children[track_index].insert_at_time(
            start,
            item,
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
        );
    }

    fn insert_linked_item_at_time(
        &mut self,
        dest_track_index: usize,
        dest_time: Seconds,
        dest_index: Option<usize>,
        item: Item,
        overlap_policy: OverlapPolicy,
        insert_policy: InsertPolicy,
        linked_audio_clips: Option<Vec<Item>>,
        linked_video_clip: Option<Item>,
    ) -> Option<InsertItemAtTimeResult> {
        let primary_item = item.clone();
        let Item::Clip(mut primary_clip) = item else {
            return None;
        };
        let linked_inputs =
            Self::normalize_linked_inputs(&primary_item, linked_audio_clips, linked_video_clip);
        clamp_clip_to_active_available_range(&mut primary_clip);
        let modified_duration = primary_clip.source_range.duration.value.max(0.0);
        if modified_duration <= EPS {
            return None;
        }

        if !Self::linked_inputs_match_duration(modified_duration, &linked_inputs) {
            return None;
        }

        if linked_inputs.video.is_some()
            && self.children[dest_track_index].kind != TrackKind::Audio
        {
            return None;
        }
        let touched_link_groups = if let Some(dest_index) = dest_index {
            self.linked_groups_touched_by_insert_at_index(
                dest_track_index,
                dest_index,
                modified_duration,
            )
        } else {
            self.linked_groups_touched_by_insert_at_time(
                dest_track_index,
                dest_time,
                modified_duration,
            )
        };

        let backup = self.clone();
        let mut used_ids = self.collect_timeline_ids();
        let has_linked_clips = !linked_inputs.audio.is_empty() || linked_inputs.video.is_some();
        let link_group_id = has_linked_clips.then(|| self.next_link_group_id());
        primary_clip.source_range.duration.value = modified_duration;
        let (primary_item, primary_clip_id) = Self::prepare_linked_item(
            Item::Clip(primary_clip),
            modified_duration,
            link_group_id,
            &mut used_ids,
        )?;

        if let Some(dest_index) = dest_index {
            self.children[dest_track_index].insert_at_index(
                dest_index,
                primary_item,
                overlap_policy,
            );
        } else {
            self.children[dest_track_index].insert_at_time(
                dest_time,
                primary_item,
                overlap_policy,
                insert_policy,
            );
        }
        let Some((mut modified_track_index, modified_item_index, _)) =
            self.get_item(&primary_clip_id)
        else {
            *self = backup;
            return None;
        };
        let modified_start =
            self.children[modified_track_index].start_time_of_item(modified_item_index);

        let mut audio_clips = Vec::new();
        let mut created_track_indices = Vec::new();
        let mut spacer_track_indices =
            self.track_indices_for_link_groups(&touched_link_groups, modified_track_index);
        for track_index in spacer_track_indices.iter().copied() {
            self.insert_spacer_gap(
                track_index,
                modified_start,
                modified_duration,
                overlap_policy,
                insert_policy,
                &mut used_ids,
            );
        }
        let mut linked_video_clip_id = None;
        if let Some(video_item) = linked_inputs.video {
            let track_count_before_video = self.children.len();
            let mut used_existing_spacer = false;
            let video_track_index = if let Some(position) = spacer_track_indices
                .iter()
                .position(|index| self.children[*index].kind == TrackKind::Video)
            {
                used_existing_spacer = true;
                spacer_track_indices.remove(position)
            } else {
                let Some(video_track_index) = self.find_or_create_video_track_for_audio(
                    modified_track_index,
                    modified_start,
                    modified_duration,
                    &mut created_track_indices,
                    link_group_id,
                    true,
                ) else {
                    *self = backup;
                    return None;
                };
                video_track_index
            };
            if self.children.len() > track_count_before_video
                && video_track_index <= modified_track_index
            {
                modified_track_index += 1;
            }
            let Some((video_item, _video_id)) = Self::prepare_linked_item(
                video_item,
                modified_duration,
                link_group_id,
                &mut used_ids,
            ) else {
                *self = backup;
                return None;
            };
            if used_existing_spacer {
                self.children[video_track_index].insert_at_time(
                    modified_start,
                    video_item,
                    OverlapPolicy::Override,
                    InsertPolicy::SplitAndInsert,
                );
            } else {
                self.insert_gap_then_linked_item(
                    video_track_index,
                    modified_start,
                    modified_duration,
                    video_item,
                    overlap_policy,
                    insert_policy,
                    &mut used_ids,
                );
            }
            linked_video_clip_id = Some(_video_id);
        }

        for audio_item in linked_inputs.audio {
            let used_audio_track_indices: Vec<_> = audio_clips
                .iter()
                .map(|(_, track_index)| *track_index)
                .collect();
            let track_count_before_audio = self.children.len();
            let mut used_existing_spacer = false;
            let audio_track_index = if let Some(position) = spacer_track_indices
                .iter()
                .position(|index| self.children[*index].kind == TrackKind::Audio)
            {
                used_existing_spacer = true;
                spacer_track_indices.remove(position)
            } else {
                let Some(audio_track_index) = self.find_or_create_audio_track(
                    modified_track_index,
                    modified_start,
                    modified_duration,
                    &mut created_track_indices,
                    &used_audio_track_indices,
                    link_group_id,
                    true,
                ) else {
                    *self = backup;
                    return None;
                };
                audio_track_index
            };
            if self.children.len() > track_count_before_audio
                && audio_track_index <= modified_track_index
            {
                modified_track_index += 1;
            }

            let Some((audio_item, audio_id)) =
                Self::prepare_linked_item(audio_item, modified_duration, link_group_id, &mut used_ids)
            else {
                *self = backup;
                return None;
            };

            if used_existing_spacer {
                self.children[audio_track_index].insert_at_time(
                    modified_start,
                    audio_item,
                    OverlapPolicy::Override,
                    InsertPolicy::SplitAndInsert,
                );
            } else {
                self.insert_gap_then_linked_item(
                    audio_track_index,
                    modified_start,
                    modified_duration,
                    audio_item,
                    overlap_policy,
                    insert_policy,
                    &mut used_ids,
                );
            }
            audio_clips.push((audio_id, audio_track_index));
        }

        Some(InsertItemAtTimeResult::Linked(LinkedInsertResult {
            primary_clip_id,
            audio_clips,
            linked_video_clip_id,
            link_group_id,
            created_track_indices,
        }))
    }

    pub fn unlink_item(&mut self, item_ids: &[String]) -> usize {
        let mut targets = Vec::new();
        let mut seen_targets = HashSet::new();
        let mut touched_link_groups = Vec::new();

        for item_id in item_ids {
            let Some((track_index, item_index)) = self.clip_target(item_id) else {
                continue;
            };
            if !seen_targets.insert((track_index, item_index)) {
                continue;
            }
            if let Item::Clip(clip) = &self.children[track_index].items[item_index] {
                if let Some(link_group_id) = resolve_link_group_id(&clip.metadata) {
                    touched_link_groups.push(link_group_id);
                    targets.push((track_index, item_index));
                }
            }
        }

        let mut count = 0;
        for (track_index, item_index) in targets {
            let Some(Item::Clip(clip)) = self
                .children
                .get_mut(track_index)
                .and_then(|track| track.items.get_mut(item_index))
            else {
                continue;
            };
            if remove_resolve_link_group_id(&mut clip.metadata) {
                count += 1;
            }
        }
        count += self.cleanup_singleton_link_groups(&touched_link_groups);
        count
    }

    pub fn link_item(&mut self, item_ids: &[String]) -> Option<i64> {
        let mut targets = Vec::new();
        let mut seen_targets = HashSet::new();
        for item_id in item_ids {
            let target = self.clip_target(item_id)?;
            if seen_targets.insert(target) {
                targets.push(target);
            }
        }
        if targets.len() < 2 {
            return None;
        }

        let (first_track_index, first_item_index) = targets[0];
        let first_start = self.children[first_track_index].start_time_of_item(first_item_index);
        let first_duration = self.children[first_track_index].items[first_item_index].duration();
        for (track_index, item_index) in targets.iter().skip(1) {
            let start = self.children[*track_index].start_time_of_item(*item_index);
            let duration = self.children[*track_index].items[*item_index].duration();
            if (start - first_start).abs() > EPS || (duration - first_duration).abs() > EPS {
                return None;
            }
        }

        let backup = self.clone();
        let mut touched_link_groups = Vec::new();
        for (track_index, item_index) in &targets {
            let Some(Item::Clip(clip)) = self
                .children
                .get_mut(*track_index)
                .and_then(|track| track.items.get_mut(*item_index))
            else {
                *self = backup;
                return None;
            };
            if let Some(link_group_id) = resolve_link_group_id(&clip.metadata) {
                touched_link_groups.push(link_group_id);
                remove_resolve_link_group_id(&mut clip.metadata);
            }
        }
        self.cleanup_singleton_link_groups(&touched_link_groups);

        let link_group_id = self.next_link_group_id();
        for (track_index, item_index) in targets {
            let Some(Item::Clip(clip)) = self
                .children
                .get_mut(track_index)
                .and_then(|track| track.items.get_mut(item_index))
            else {
                *self = backup;
                return None;
            };
            set_resolve_link_group_id(&mut clip.metadata, link_group_id);
        }

        Some(link_group_id)
    }

    pub fn replace_item(
        &mut self,
        item_id: &str,
        item: Item,
        linked_audio_clips: Option<Vec<Item>>,
        linked_video_clip: Option<Item>,
    ) -> bool {
        let Some((selected_track_index, selected_item_index, selected_item)) =
            self.get_item(item_id)
        else {
            return false;
        };
        let selected_start =
            self.children[selected_track_index].start_time_of_item(selected_item_index);
        let selected_link_group = match selected_item {
            Item::Clip(clip) => resolve_link_group_id(&clip.metadata),
            Item::Gap(_) => None,
        };
        let linked_inputs =
            Self::normalize_linked_inputs(&item, linked_audio_clips, linked_video_clip);
        if linked_inputs.video.is_some()
            && self.children[selected_track_index].kind != TrackKind::Audio
        {
            return false;
        }
        let should_link = selected_link_group.is_some()
            || !linked_inputs.audio.is_empty()
            || linked_inputs.video.is_some();
        let link_group =
            selected_link_group.or_else(|| should_link.then(|| self.next_link_group_id()));
        let targets = selected_link_group
            .map(|link_group_id| self.linked_group_targets(link_group_id))
            .unwrap_or_else(|| vec![(selected_track_index, selected_item_index)]);
        if targets.is_empty() {
            return false;
        }
        if should_link && !matches!(item, Item::Clip(_)) {
            return false;
        }

        let backup = self.clone();
        let mut replacement_item = item;
        let replacement_duration = if should_link {
            let Item::Clip(clip) = &mut replacement_item else {
                return false;
            };
            clamp_clip_to_active_available_range(clip);
            let duration = clip.source_range.duration.value.max(0.0);
            if duration <= EPS {
                return false;
            }
            duration
        } else {
            replacement_item.duration().max(0.0)
        };
        if !Self::linked_inputs_match_duration(replacement_duration, &linked_inputs) {
            return false;
        }

        for (track_index, item_index) in targets {
            let Some(existing) = self
                .children
                .get(track_index)
                .and_then(|track| track.items.get(item_index))
            else {
                *self = backup;
                return false;
            };
            let existing_id = existing.get_id();

            let mut next =
                if track_index == selected_track_index && item_index == selected_item_index {
                    replacement_item.clone()
                } else {
                    existing.clone()
                };
            next.set_id(existing_id);
            next.set_duration(replacement_duration);
            Self::set_item_link_group(&mut next, link_group);

            let Some(track) = self.children.get_mut(track_index) else {
                *self = backup;
                return false;
            };
            if !track.replace_item_by_index(item_index, next) {
                *self = backup;
                return false;
            }
        }

        let Some(link_group_id) = link_group else {
            return true;
        };

        let mut used_ids = self.collect_timeline_ids();
        let mut primary_track_index = selected_track_index;
        let mut created_track_indices = Vec::new();
        if let Some(video_item) = linked_inputs.video {
            let track_count_before_video = self.children.len();
            let Some(video_track_index) = self.find_or_create_video_track_for_audio(
                primary_track_index,
                selected_start,
                replacement_duration,
                &mut created_track_indices,
                Some(link_group_id),
                false,
            ) else {
                *self = backup;
                return false;
            };
            if self.children.len() > track_count_before_video
                && video_track_index <= primary_track_index
            {
                primary_track_index += 1;
            }
            let Some((video_item, _video_id)) = Self::prepare_linked_item(
                video_item,
                replacement_duration,
                Some(link_group_id),
                &mut used_ids,
            ) else {
                *self = backup;
                return false;
            };
            if !self.insert_gap_only(video_track_index, selected_start, video_item) {
                *self = backup;
                return false;
            }
        }

        let mut inserted_audio_tracks = Vec::new();
        for audio_item in linked_inputs.audio {
            let track_count_before_audio = self.children.len();
            let Some(audio_track_index) = self.find_or_create_audio_track(
                primary_track_index,
                selected_start,
                replacement_duration,
                &mut created_track_indices,
                &inserted_audio_tracks,
                Some(link_group_id),
                false,
            ) else {
                *self = backup;
                return false;
            };
            if self.children.len() > track_count_before_audio
                && audio_track_index <= primary_track_index
            {
                primary_track_index += 1;
            }
            let Some((audio_item, _audio_id)) = Self::prepare_linked_item(
                audio_item,
                replacement_duration,
                Some(link_group_id),
                &mut used_ids,
            ) else {
                *self = backup;
                return false;
            };
            if !self.insert_gap_only(audio_track_index, selected_start, audio_item) {
                *self = backup;
                return false;
            }
            inserted_audio_tracks.push(audio_track_index);
        }

        true
    }

    pub fn split_item_at_time(&mut self, item_id: &str, split_time: Seconds) -> bool {
        let Some((selected_track_index, selected_item_index, selected_item)) =
            self.get_item(item_id)
        else {
            return false;
        };
        let Item::Clip(selected_clip) = selected_item else {
            return false;
        };
        let selected_start =
            self.children[selected_track_index].start_time_of_item(selected_item_index);
        let selected_end = selected_start + selected_clip.source_range.duration.value.max(0.0);
        if split_time < selected_start - EPS || split_time > selected_end + EPS {
            return false;
        }
        if split_time <= selected_start + EPS || split_time >= selected_end - EPS {
            return true;
        }

        let targets = resolve_link_group_id(&selected_clip.metadata)
            .map(|link_group_id| self.linked_group_targets(link_group_id))
            .filter(|targets| targets.len() > 1)
            .unwrap_or_else(|| vec![(selected_track_index, selected_item_index)]);
        let backup = self.clone();
        for (track_index, item_index) in &targets {
            let Some(item) = self
                .children
                .get(*track_index)
                .and_then(|track| track.items.get(*item_index))
            else {
                *self = backup;
                return false;
            };
            let Item::Clip(clip) = item else {
                *self = backup;
                return false;
            };
            let item_start = self.children[*track_index].start_time_of_item(*item_index);
            let item_end = item_start + clip.source_range.duration.value.max(0.0);
            if split_time <= item_start + EPS || split_time >= item_end - EPS {
                *self = backup;
                return false;
            }
        }

        let mut target_tracks: Vec<_> = targets
            .into_iter()
            .map(|(track_index, _)| track_index)
            .collect();
        target_tracks.sort_unstable();
        target_tracks.dedup();
        for track_index in target_tracks {
            self.children[track_index].split_at_time(split_time);
        }
        true
    }

    fn delete_one_item(&mut self, item_id: &str, replace_with_gap: bool) -> Option<(usize, Item)> {
        for ti in 0..self.children.len() {
            if let Some((ii, _)) = self.children[ti].get_item_by_id(item_id) {
                let removed = self.children[ti].items[ii].clone();
                // Use the track API for deletion and optional gap insertion/merge behavior
                let deleted = self.children[ti].delete_clip(ii, replace_with_gap);
                if deleted {
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

    fn linked_move_items(&self, item_id: &str) -> Option<Vec<LinkedMoveItem>> {
        let (selected_track_index, selected_item_index, selected_item) = self.get_item(item_id)?;
        let link_group_id = match selected_item {
            Item::Clip(clip) => resolve_link_group_id(&clip.metadata),
            Item::Gap(_) => None,
        }?;
        let targets = self.linked_group_targets(link_group_id);
        if targets.len() <= 1 {
            return None;
        }

        let mut items = Vec::new();
        for (track_index, item_index) in targets {
            let item = self.children.get(track_index)?.items.get(item_index)?.clone();
            let is_selected =
                track_index == selected_track_index && item_index == selected_item_index;
            items.push(LinkedMoveItem {
                track_index,
                item_index,
                item,
                is_selected,
            });
        }
        Some(items)
    }

    fn move_linked_items(
        &mut self,
        items_to_move: Vec<LinkedMoveItem>,
        dest_track_index: usize,
        replace_with_gap: bool,
        overlap_policy: OverlapPolicy,
        placement: LinkedMovePlacement,
    ) -> bool {
        let backup = self.clone();
        let Some(selected_item) = items_to_move.iter().find(|item| item.is_selected) else {
            return false;
        };
        let Some(selected_id) = selected_item.item.get_id() else {
            return false;
        };

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

        match placement {
            LinkedMovePlacement::Time {
                dest_time,
                insert_policy,
            } => self.children[dest_track_index].insert_at_time(
                dest_time,
                selected_item.item.clone(),
                overlap_policy,
                insert_policy,
            ),
            LinkedMovePlacement::Index { dest_index } => self.children[dest_track_index]
                .insert_at_index(dest_index, selected_item.item.clone(), overlap_policy),
        }
        let Some((selected_track_index, selected_item_index, _)) = self.get_item(&selected_id)
        else {
            *self = backup;
            return false;
        };
        let moved_start =
            self.children[selected_track_index].start_time_of_item(selected_item_index);

        for move_item in items_to_move {
            if move_item.is_selected {
                continue;
            }
            let Some(track) = self.children.get_mut(move_item.track_index) else {
                *self = backup;
                return false;
            };
            track.insert_at_time(
                moved_start,
                move_item.item,
                overlap_policy,
                InsertPolicy::SplitAndInsert,
            );
        }

        true
    }

    /// Insert an item at a given time into the track at `dest_track_index`.
    /// Returns the inserted item's id if insertion occurred.
    pub fn insert_item_at_time(
        &mut self,
        dest_track_index: usize,
        dest_time: Seconds,
        item: Item,
        overlap_policy: OverlapPolicy,
        insert_policy: InsertPolicy,
        linked_audio_clips: Option<Vec<Item>>,
        linked_video_clip: Option<Item>,
    ) -> Option<InsertItemAtTimeResult> {
        if dest_track_index >= self.children.len() {
            return None;
        }
        let touches_linked_group = matches!(item, Item::Clip(_))
            && !self
                .linked_groups_touched_by_insert_at_time(
                    dest_track_index,
                    dest_time,
                    item.duration(),
                )
                .is_empty();
        if Self::has_linked_inputs(&linked_audio_clips, &linked_video_clip)
            || touches_linked_group
        {
            return self.insert_linked_item_at_time(
                dest_track_index,
                dest_time,
                None,
                item,
                overlap_policy,
                insert_policy,
                linked_audio_clips,
                linked_video_clip,
            );
        }

        let inserted_id = crate::metadata::IdMetadataExt::get_id(&item);
        self.children[dest_track_index].insert_at_time(
            dest_time,
            item,
            overlap_policy,
            insert_policy,
        );
        inserted_id.map(InsertItemAtTimeResult::ItemId)
    }

    /// Insert an item at an index into the track with `dest_track_id`.
    /// Returns the inserted item's id if insertion occurred.
    pub fn insert_item_at_index(
        &mut self,
        dest_track_id: &str,
        dest_index: usize,
        item: Item,
        overlap_policy: OverlapPolicy,
        linked_audio_clips: Option<Vec<Item>>,
        linked_video_clip: Option<Item>,
    ) -> Option<InsertItemAtTimeResult> {
        let dest_track_index = match self.get_track_by_id(dest_track_id) {
            Some((i, _)) => i,
            None => return None,
        };
        if dest_track_index >= self.children.len() {
            return None;
        }

        let touches_linked_group = matches!(item, Item::Clip(_))
            && !self
                .linked_groups_touched_by_insert_at_index(
                    dest_track_index,
                    dest_index,
                    item.duration(),
                )
                .is_empty();
        if Self::has_linked_inputs(&linked_audio_clips, &linked_video_clip)
            || touches_linked_group
        {
            return self.insert_linked_item_at_time(
                dest_track_index,
                0.0,
                Some(dest_index),
                item,
                overlap_policy,
                InsertPolicy::InsertBefore,
                linked_audio_clips,
                linked_video_clip,
            );
        }

        let inserted_id = crate::metadata::IdMetadataExt::get_id(&item);
        self.children[dest_track_index].insert_at_index(dest_index, item, overlap_policy);
        inserted_id.map(InsertItemAtTimeResult::ItemId)
    }

    /// Move an item identified by `item_id` to `dest_time` on the track with `dest_track_id`.
    /// Returns true if the item was successfully moved.
    pub fn move_item_at_time(
        &mut self,
        item_id: &str,
        dest_track_id: &str,
        dest_time: Seconds,
        replace_with_gap: bool,
        insert_policy: InsertPolicy,
        overlap_policy: OverlapPolicy,
    ) -> bool {
        let item_to_move = match self.get_item(item_id) {
            Some((_ti, _ii, it)) => it.clone(),
            None => return false,
        };
        let dest_track_index = match self.get_track_by_id(dest_track_id) {
            Some((i, _)) => i,
            None => return false,
        };
        if let Some(items_to_move) = self.linked_move_items(item_id) {
            return self.move_linked_items(
                items_to_move,
                dest_track_index,
                replace_with_gap,
                overlap_policy,
                LinkedMovePlacement::Time {
                    dest_time,
                    insert_policy,
                },
            );
        }

        let backup = self.clone();
        if self.delete_one_item(item_id, replace_with_gap).is_none() {
            return false;
        }

        if self
            .insert_item_at_time(
                dest_track_index,
                dest_time,
                item_to_move,
                overlap_policy,
                insert_policy,
                None,
                None,
            )
            .is_some()
        {
            true
        } else {
            *self = backup;
            false
        }
    }

    /// Move an item identified by `item_id` to `dest_index` on the track with `dest_track_id`.
    /// Returns true if the item was successfully moved.
    pub fn move_item_at_index(
        &mut self,
        item_id: &str,
        dest_track_id: &str,
        dest_index: usize,
        replace_with_gap: bool,
        overlap_policy: OverlapPolicy,
    ) -> bool {
        let item_to_move = match self.get_item(item_id) {
            Some((_ti, _ii, it)) => it.clone(),
            None => return false,
        };

        let backup = self.clone();
        let dest_track_index = match self.get_track_by_id(dest_track_id) {
            Some((i, _)) => i,
            None => return false,
        };
        if let Some(items_to_move) = self.linked_move_items(item_id) {
            return self.move_linked_items(
                items_to_move,
                dest_track_index,
                replace_with_gap,
                overlap_policy,
                LinkedMovePlacement::Index { dest_index },
            );
        }

        if self.delete_one_item(item_id, replace_with_gap).is_none() {
            return false;
        }

        if self
            .insert_item_at_index(dest_track_id, dest_index, item_to_move, overlap_policy, None, None)
            .is_some()
        {
            true
        } else {
            *self = backup;
            false
        }
    }

    /// Append a track to the stack.
    pub fn add_track(&mut self, track: Track) {
        self.children.push(track);
    }

    /// Insert a track at a specific index. Negative indices behave like Python's.
    pub fn add_track_at(&mut self, track: Track, insertion_index: isize) {
        let idx = clamp_insertion_index(self.children.len(), insertion_index);
        self.children.insert(idx, track);
    }

    /// Delete a track by id. Returns the removed track on success.
    pub fn delete_track(&mut self, id: &str) -> Option<Track> {
        let (i, track) = self.get_track_by_id(id)?;
        let touched_link_groups: Vec<_> = track
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Clip(clip) => resolve_link_group_id(&clip.metadata),
                Item::Gap(_) => None,
            })
            .collect();
        let removed = self.children.remove(i);
        self.cleanup_singleton_link_groups(&touched_link_groups);
        Some(removed)
    }
}

fn range_is_gap_backed(track: &Track, start: Seconds, end: Seconds) -> bool {
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

fn range_has_blocking_clip(
    track: &Track,
    start: Seconds,
    end: Seconds,
    link_group_id: Option<i64>,
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
                    if link_group_id.is_some()
                        && resolve_link_group_id(&clip.metadata) == link_group_id => {}
                Item::Clip(_) => return true,
            }
        }
        pos = item_end;
    }
    false
}

fn split_gap_boundary(track: &mut Track, time: Seconds) {
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
    let total = gap.source_range.duration.value.max(0.0);
    if local >= total - EPS {
        track.items.insert(index, Item::Gap(gap));
        return;
    }

    let mut left = gap.clone();
    left.source_range.duration.value = local.max(0.0);
    gap.source_range.duration.value = (total - local).max(0.0);
    gap.set_id(Some(crate::types::gen_hex_id_12()));
    track.items.insert(index, Item::Gap(left));
    track.items.insert(index + 1, Item::Gap(gap));
}

fn clamp_clip_to_active_available_range(clip: &mut Clip) {
    let active_key = clip
        .active_media_reference_key
        .as_deref()
        .filter(|key| clip.media_references.contains_key(*key))
        .or_else(|| {
            clip.media_references
                .contains_key("DEFAULT_MEDIA")
                .then_some("DEFAULT_MEDIA")
        })
        .or_else(|| clip.media_references.keys().next().map(String::as_str));

    let Some(active_key) = active_key else {
        clip.source_range.duration.value = clip.source_range.duration.value.max(0.0);
        return;
    };

    let Some(available_range) = clip
        .media_references
        .get(active_key)
        .and_then(|reference| reference.available_range().as_ref())
    else {
        clip.source_range.duration.value = clip.source_range.duration.value.max(0.0);
        return;
    };

    let media_start = available_range.start_time.value.max(0.0);
    let media_duration = available_range.duration.value.max(0.0);
    let media_end = media_start + media_duration;
    let source_start = clip.source_range.start_time.value.max(media_start);
    let requested_end =
        (clip.source_range.start_time.value + clip.source_range.duration.value).max(source_start);
    let clamped_end = requested_end.min(media_end);

    clip.source_range.start_time.value = source_start;
    clip.source_range.duration.value = (clamped_end - source_start).max(0.0);
}

fn resolve_link_group_id(metadata: &serde_json::Value) -> Option<i64> {
    metadata
        .get("Resolve_OTIO")
        .and_then(|v| v.get("Link Group ID"))
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_u64().and_then(|n| i64::try_from(n).ok()))
                .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
        })
}

fn set_resolve_link_group_id(metadata: &mut serde_json::Value, link_group_id: i64) {
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
        serde_json::Value::Number(serde_json::Number::from(link_group_id)),
    );
}

fn remove_resolve_link_group_id(metadata: &mut serde_json::Value) -> bool {
    let Some(resolve) = metadata
        .get_mut("Resolve_OTIO")
        .and_then(|value| value.as_object_mut())
    else {
        return false;
    };
    resolve.remove("Link Group ID").is_some()
}
