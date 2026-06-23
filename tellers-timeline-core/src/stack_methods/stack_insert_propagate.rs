use crate::{
    Gap, InsertPolicy, Item, OverlapPolicy, Seconds, SplitClipInfo, Stack,
    TrackInsertResult,
};
use std::collections::{HashMap, HashSet};

use super::{resolve_sync_clips_id, set_resolve_sync_clips_id, EPS};

pub(super) type TrackInsertUpdate<'a> = (usize, &'a TrackInsertResult);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplitOutcome {
    BothSides,
    LeftOnly,
    RightOnly,
}

fn is_split_fragment(clip_id: &str, splits: &[SplitClipInfo]) -> bool {
    splits.iter().any(|split| {
        split.left_clip_id.as_deref() == Some(clip_id)
            || split.right_clip_id.as_deref() == Some(clip_id)
    })
}

fn updated_track_set(updates: &[TrackInsertUpdate<'_>]) -> HashSet<usize> {
    updates.iter().map(|(track_index, _)| *track_index).collect()
}

fn all_split_clips(updates: &[TrackInsertUpdate<'_>]) -> Vec<SplitClipInfo> {
    updates
        .iter()
        .flat_map(|(_, result)| result.split_clips.iter())
        .cloned()
        .collect()
}

impl Stack {
    fn pick_best_sync_cluster(
        &self,
        groups: Vec<super::SyncTrackInfo>,
        prefer_cluster_with_video: bool,
    ) -> Option<Vec<usize>> {
        if groups.is_empty() {
            return None;
        }
        groups
            .into_iter()
            .max_by(|left, right| {
                if prefer_cluster_with_video {
                    use crate::TrackKind;
                    let left_has_video = left.track_indices.iter().any(|&i| {
                        self.children
                            .get(i)
                            .is_some_and(|track| track.kind == TrackKind::Video)
                    });
                    let right_has_video = right.track_indices.iter().any(|&i| {
                        self.children
                            .get(i)
                            .is_some_and(|track| track.kind == TrackKind::Video)
                    });
                    match (left_has_video, right_has_video) {
                        (true, false) => std::cmp::Ordering::Greater,
                        (false, true) => std::cmp::Ordering::Less,
                        _ => left
                            .track_indices
                            .len()
                            .cmp(&right.track_indices.len()),
                    }
                } else {
                    left.track_indices
                        .len()
                        .cmp(&right.track_indices.len())
                }
            })
            .map(|group| group.track_indices)
    }

    pub(super) fn preferred_cluster_indices(
        &self,
        track_index: usize,
        prefer_cluster_with_video: bool,
    ) -> Vec<usize> {
        let groups: Vec<_> = self
            .sync_track_info()
            .into_iter()
            .filter(|group| group.track_indices.contains(&track_index))
            .collect();

        self.pick_best_sync_cluster(groups, prefer_cluster_with_video)
            .unwrap_or_else(|| vec![track_index])
    }

    pub(super) fn resolve_insert_cluster(
        &self,
        dest_track_index: usize,
        prefer_cluster_with_video: bool,
        preferred_video_track_id: Option<&str>,
    ) -> Vec<usize> {
        let mut cluster =
            self.preferred_cluster_indices(dest_track_index, prefer_cluster_with_video);

        if !prefer_cluster_with_video {
            return cluster;
        }

        let cluster_has_video = cluster.iter().any(|&track_index| {
            self.children
                .get(track_index)
                .is_some_and(|track| track.kind == crate::TrackKind::Video)
        });
        if cluster_has_video {
            return cluster;
        }

        let Some(preferred_video_track_id) = preferred_video_track_id else {
            return cluster;
        };
        let Some((preferred_video_index, preferred_track)) =
            self.get_track_by_id(preferred_video_track_id)
        else {
            return cluster;
        };
        if preferred_track.kind != crate::TrackKind::Video {
            return cluster;
        }

        let video_groups: Vec<_> = self
            .sync_track_info()
            .into_iter()
            .filter(|group| group.track_indices.contains(&preferred_video_index))
            .collect();

        if let Some(video_cluster) = self.pick_best_sync_cluster(video_groups, true) {
            for track_index in video_cluster {
                if !cluster.contains(&track_index) {
                    cluster.push(track_index);
                }
            }
        } else if !cluster.contains(&preferred_video_index) {
            cluster.push(preferred_video_index);
        }

        cluster.sort_unstable();
        cluster
    }

    /// Always run after a column insert: deleted sync clips, partner splits/gaps, then
    /// right-fragment link-group ids for splits that cut through a sync group.
    pub(super) fn propagate_insert_to_cluster(
        &mut self,
        insert_start: Seconds,
        insert_duration: Seconds,
        overlap_policy: OverlapPolicy,
        updates: &[TrackInsertUpdate<'_>],
        cluster: &[usize],
        insert_sync_clips_id: Option<i64>,
    ) -> bool {
        if insert_duration <= EPS || updates.is_empty() {
            return true;
        }
        let insert_end = insert_start + insert_duration;
        let updated_tracks = updated_track_set(updates);
        let all_splits = all_split_clips(updates);

        if !self.propagate_deleted_sync_clips(updates, &all_splits) {
            return false;
        }
        if !self.propagate_splits_to_cluster(
            updates,
            insert_start,
            insert_end,
            insert_duration,
            overlap_policy,
            cluster,
            &updated_tracks,
            insert_sync_clips_id,
        ) {
            return false;
        }
        true
    }

    /// Push-only: for sync groups present after the insert on updated tracks, push a gap
    /// on cluster partners that were not part of the column insert.
    pub(super) fn propagate_push_to_cluster(
        &mut self,
        insert_start: Seconds,
        insert_end: Seconds,
        insert_duration: Seconds,
        updates: &[TrackInsertUpdate<'_>],
        cluster: &[usize],
    ) -> bool {
        if insert_duration <= EPS {
            return true;
        }
        let updated_tracks = updated_track_set(updates);
        let sync_groups =
            self.sync_groups_after_insert_on_tracks(&updated_tracks, insert_end, updates);
        let aligned_start = self
            .min_sync_clip_start_at_or_after(&updated_tracks, insert_end)
            .unwrap_or(insert_end);
        let cluster_set: HashSet<usize> = cluster.iter().copied().collect();
        let mut used_ids = self.collect_timeline_ids();

        if let Some(updated_leading) =
            self.min_item_start_before_on_tracks(&updated_tracks, insert_start)
        {
            for &track_index in cluster {
                if updated_tracks.contains(&track_index) {
                    continue;
                }
                let Some(partner_first_sync) = self.first_sync_clip_start_on_track(track_index)
                else {
                    continue;
                };
                if partner_first_sync <= updated_leading + EPS {
                    continue;
                }
                let gap_duration = partner_first_sync - updated_leading;
                if gap_duration <= EPS
                    || self.track_has_spacer_at(track_index, updated_leading, gap_duration)
                {
                    continue;
                }
                let mut gap = Item::Gap(Gap::make_gap(gap_duration));
                Self::ensure_unique_item_id(&mut gap, &mut used_ids);
                let result = self.children[track_index].insert_at_time(
                    updated_leading,
                    gap,
                    OverlapPolicy::Push,
                    InsertPolicy::SplitAndInsert,
                );
                if !result.success {
                    return false;
                }
            }
        }

        for sync_id in sync_groups {
            for (track_index, _) in self.synced_clips_targets(sync_id) {
                if !cluster_set.contains(&track_index) || updated_tracks.contains(&track_index) {
                    continue;
                }
                let Some(partner_sync_start) = self.first_cluster_sync_clip_start_at_or_after(
                    track_index,
                    insert_start,
                    cluster,
                ) else {
                    continue;
                };
                let (gap_at, gap_duration) = if partner_sync_start <= insert_start + EPS {
                    (insert_start, aligned_start - insert_start)
                } else {
                    (insert_start, aligned_start - partner_sync_start)
                };
                if gap_duration <= EPS {
                    continue;
                }
                if partner_sync_start <= insert_start + EPS
                    && self.track_has_spacer_at(track_index, gap_at, gap_duration)
                {
                    continue;
                }
                let mut gap = Item::Gap(Gap::make_gap(gap_duration));
                Self::ensure_unique_item_id(&mut gap, &mut used_ids);
                let result = self.children[track_index].insert_at_time(
                    gap_at,
                    gap,
                    OverlapPolicy::Push,
                    InsertPolicy::SplitAndInsert,
                );
                if !result.success {
                    return false;
                }
            }
        }
        true
    }

    fn min_sync_clip_start_at_or_after(
        &self,
        track_indices: &HashSet<usize>,
        threshold: Seconds,
    ) -> Option<Seconds> {
        let mut min_start: Option<Seconds> = None;
        for &track_index in track_indices {
            let Some(track) = self.children.get(track_index) else {
                continue;
            };
            let mut pos = 0.0;
            for item in &track.items {
                if pos >= threshold - EPS {
                    if let Item::Clip(clip) = item {
                        if clip.sync_clips_id().is_some() {
                            min_start = Some(match min_start {
                                Some(current) => current.min(pos),
                                None => pos,
                            });
                        }
                    }
                }
                pos += item.duration().max(0.0);
            }
        }
        min_start
    }

    fn min_item_start_before_on_tracks(
        &self,
        track_indices: &HashSet<usize>,
        insert_start: Seconds,
    ) -> Option<Seconds> {
        let mut min_start: Option<Seconds> = None;
        for &track_index in track_indices {
            let Some(track) = self.children.get(track_index) else {
                continue;
            };
            let mut pos = 0.0;
            for item in &track.items {
                if pos >= insert_start - EPS {
                    break;
                }
                min_start = Some(match min_start {
                    Some(current) => current.min(pos),
                    None => pos,
                });
                pos += item.duration().max(0.0);
            }
        }
        min_start
    }

    fn first_sync_clip_start_on_track(&self, track_index: usize) -> Option<Seconds> {
        let track = self.children.get(track_index)?;
        let mut pos = 0.0;
        for item in &track.items {
            if let Item::Clip(clip) = item {
                if clip.sync_clips_id().is_some() {
                    return Some(pos);
                }
            }
            pos += item.duration().max(0.0);
        }
        None
    }

    fn first_cluster_sync_clip_start_at_or_after(
        &self,
        track_index: usize,
        insert_start: Seconds,
        cluster: &[usize],
    ) -> Option<Seconds> {
        let track = self.children.get(track_index)?;
        let cluster_sync_ids: HashSet<i64> = cluster
            .iter()
            .filter_map(|&index| self.children.get(index))
            .flat_map(|track| track.items.iter())
            .filter_map(|item| match item {
                Item::Clip(clip) => clip.sync_clips_id(),
                Item::Gap(_) => None,
            })
            .collect();
        let mut pos = 0.0;
        let mut any_sync_start: Option<Seconds> = None;
        for item in &track.items {
            if pos + item.duration().max(0.0) > insert_start + EPS {
                if let Item::Clip(clip) = item {
                    if let Some(sync_id) = clip.sync_clips_id() {
                        if cluster_sync_ids.is_empty() || cluster_sync_ids.contains(&sync_id) {
                            return Some(pos);
                        }
                        any_sync_start = Some(any_sync_start.map_or(pos, |current| current.min(pos)));
                    }
                }
            }
            pos += item.duration().max(0.0);
        }
        any_sync_start
    }

    fn propagate_deleted_sync_clips(
        &mut self,
        updates: &[TrackInsertUpdate<'_>],
        all_splits: &[SplitClipInfo],
    ) -> bool {
        let mut deleted_sync_ids = HashSet::new();
        for (_, result) in updates {
            for deleted in &result.deleted_clips {
                if is_split_fragment(&deleted.clip_id, all_splits) {
                    continue;
                }
                if let Some(sync_id) = deleted.sync_clips_id {
                    deleted_sync_ids.insert(sync_id);
                }
            }
        }
        for sync_id in deleted_sync_ids {
            self.delete_sync_clips(sync_id, true);
        }
        true
    }

    fn propagate_splits_to_cluster(
        &mut self,
        updates: &[TrackInsertUpdate<'_>],
        insert_start: Seconds,
        insert_end: Seconds,
        insert_duration: Seconds,
        overlap_policy: OverlapPolicy,
        cluster: &[usize],
        updated_tracks: &HashSet<usize>,
        insert_sync_clips_id: Option<i64>,
    ) -> bool {
        let mut splits_by_source: Vec<(usize, SplitClipInfo)> = Vec::new();
        for &(track_index, result) in updates {
            for split in &result.split_clips {
                splits_by_source.push((track_index, split.clone()));
            }
        }

        let mut by_sync_id: HashMap<i64, (usize, SplitClipInfo)> = HashMap::new();
        for (source_track, split) in splits_by_source {
            let Some(sync_id) = split.sync_clips_id else {
                continue;
            };
            by_sync_id
                .entry(sync_id)
                .and_modify(|(existing_source, existing)| {
                    if split.split_time < existing.split_time {
                        existing.split_time = split.split_time;
                    }
                    if existing.old_clip_id.is_empty() {
                        existing.old_clip_id = split.old_clip_id.clone();
                    }
                    if split.split_time < existing.split_time {
                        *existing_source = source_track;
                    }
                })
                .or_insert_with(|| (source_track, split));
        }

        for (sync_id, (source_track, split)) in by_sync_id {
            let outcome = self.classify_split_outcome(
                &split,
                source_track,
                insert_start,
                insert_end,
            );

            if !self.split_sync_partners_at_time(
                sync_id,
                split.split_time,
                cluster,
                updated_tracks,
            ) {
                return false;
            }

            let Some(outcome) = outcome else {
                continue;
            };

            let right_sync_clips_id = match (insert_sync_clips_id, overlap_policy) {
                (Some(insert_id), OverlapPolicy::Override) => {
                    let right_id = sync_id + 1;
                    if right_id == insert_id {
                        insert_id + 1
                    } else {
                        right_id
                    }
                }
                (Some(_), OverlapPolicy::Push) => self.next_sync_clips_id(),
                (None, OverlapPolicy::Override) => self.next_sync_clips_id(),
                (None, OverlapPolicy::Push) => sync_id,
            };

            match (overlap_policy, outcome) {
                (OverlapPolicy::Override, SplitOutcome::BothSides) => {
                    if !self.insert_override_gap_on_cluster_partners(
                        insert_start,
                        insert_duration,
                        cluster,
                        updated_tracks,
                    ) {
                        return false;
                    }
                    let right_start_threshold = split.split_time + insert_duration - EPS;
                    self.reassign_right_sync_group_ids(
                        sync_id,
                        right_start_threshold,
                        right_sync_clips_id,
                    );
                }
                (OverlapPolicy::Override, SplitOutcome::LeftOnly | SplitOutcome::RightOnly) => {
                    if !self.delete_sync_range_on_cluster_partners(
                        sync_id,
                        insert_start,
                        insert_end,
                        cluster,
                        updated_tracks,
                    ) {
                        return false;
                    }
                }
                (OverlapPolicy::Push, SplitOutcome::BothSides) => {
                    let right_start_threshold = split.split_time + insert_duration - EPS;
                    self.reassign_right_sync_group_ids(
                        sync_id,
                        right_start_threshold,
                        right_sync_clips_id,
                    );
                }
                _ => {}
            }
        }
        true
    }

    fn classify_split_outcome(
        &self,
        split: &SplitClipInfo,
        source_track_index: usize,
        insert_start: Seconds,
        insert_end: Seconds,
    ) -> Option<SplitOutcome> {
        let sync_id = split.sync_clips_id?;
        let mut has_left = false;
        let mut has_right = false;
        for (track_index, item_index) in self.synced_clips_targets(sync_id) {
            if track_index != source_track_index {
                continue;
            }
            let start = self.children[track_index].start_time_of_item(item_index);
            if start + EPS < insert_start {
                has_left = true;
            }
            if start >= insert_end - EPS {
                has_right = true;
            }
        }
        match (has_left, has_right) {
            (true, true) => Some(SplitOutcome::BothSides),
            (true, false) => Some(SplitOutcome::LeftOnly),
            (false, true) => Some(SplitOutcome::RightOnly),
            (false, false) => None,
        }
    }

    fn split_sync_partners_at_time(
        &mut self,
        sync_clips_id: i64,
        split_time: Seconds,
        cluster: &[usize],
        updated_tracks: &HashSet<usize>,
    ) -> bool {
        for &track_index in cluster {
            if updated_tracks.contains(&track_index) {
                continue;
            }
            let needs_split = self.children.get(track_index).is_some_and(|track| {
                track
                    .get_item_at_time(split_time)
                    .and_then(|item_index| track.items.get(item_index))
                    .and_then(|item| match item {
                        Item::Clip(clip) => resolve_sync_clips_id(&clip.metadata),
                        Item::Gap(_) => None,
                    })
                    .is_some_and(|id| id == sync_clips_id)
            });
            if needs_split {
                self.children[track_index].split_at_time(split_time);
            }
        }
        true
    }

    fn insert_override_gap_on_cluster_partners(
        &mut self,
        insert_start: Seconds,
        insert_duration: Seconds,
        cluster: &[usize],
        updated_tracks: &HashSet<usize>,
    ) -> bool {
        let mut used_ids = self.collect_timeline_ids();
        for &track_index in cluster {
            if updated_tracks.contains(&track_index) {
                continue;
            }
            if self.track_has_spacer_at(track_index, insert_start, insert_duration) {
                continue;
            }
            let mut gap = Item::Gap(Gap::make_gap(insert_duration));
            Self::ensure_unique_item_id(&mut gap, &mut used_ids);
            let result = self.children[track_index].insert_at_time(
                insert_start,
                gap,
                OverlapPolicy::Override,
                InsertPolicy::SplitAndInsert,
            );
            if !result.success {
                return false;
            }
        }
        true
    }

    fn delete_sync_range_on_cluster_partners(
        &mut self,
        sync_clips_id: i64,
        insert_start: Seconds,
        insert_end: Seconds,
        cluster: &[usize],
        updated_tracks: &HashSet<usize>,
    ) -> bool {
        for &track_index in cluster {
            if updated_tracks.contains(&track_index) {
                continue;
            }
            let Some(track) = self.children.get(track_index) else {
                continue;
            };
            let has_sync_clip_in_range = {
                let mut pos = 0.0;
                let mut found = false;
                for item in &track.items {
                    let item_start = pos;
                    let item_end = pos + item.duration().max(0.0);
                    if item_end > insert_start + EPS && item_start < insert_end - EPS {
                        if let Item::Clip(clip) = item {
                            if resolve_sync_clips_id(&clip.metadata) == Some(sync_clips_id) {
                                found = true;
                                break;
                            }
                        }
                    }
                    pos = item_end;
                }
                found
            };
            if !has_sync_clip_in_range {
                continue;
            }
            self.children[track_index].delete_range(insert_start, insert_end, true);
        }
        true
    }

    fn reassign_right_sync_group_ids(
        &mut self,
        sync_clips_id: i64,
        right_start_threshold: Seconds,
        right_sync_clips_id: i64,
    ) {
        for (track_index, item_index) in self.synced_clips_targets(sync_clips_id) {
            let item_start = self.children[track_index].start_time_of_item(item_index);
            if item_start < right_start_threshold {
                continue;
            }
            let Some(Item::Clip(clip)) = self
                .children
                .get_mut(track_index)
                .and_then(|track| track.items.get_mut(item_index))
            else {
                continue;
            };
            if resolve_sync_clips_id(&clip.metadata) != Some(sync_clips_id) {
                continue;
            }
            set_resolve_sync_clips_id(&mut clip.metadata, right_sync_clips_id);
        }
    }

    fn sync_groups_after_insert_on_tracks(
        &self,
        updated_tracks: &HashSet<usize>,
        insert_end: Seconds,
        updates: &[TrackInsertUpdate<'_>],
    ) -> HashSet<i64> {
        let mut sync_ids = HashSet::new();
        for &track_index in updated_tracks {
            let Some(track) = self.children.get(track_index) else {
                continue;
            };
            let mut pos = 0.0;
            for item in &track.items {
                if pos >= insert_end - EPS {
                    if let Item::Clip(clip) = item {
                        if let Some(sync_id) = clip.sync_clips_id() {
                            sync_ids.insert(sync_id);
                        }
                    }
                }
                pos += item.duration().max(0.0);
            }
        }
        for (_, result) in updates {
            for split in &result.split_clips {
                if let Some(sync_id) = split.sync_clips_id {
                    sync_ids.insert(sync_id);
                }
            }
        }
        sync_ids
    }

    pub(super) fn track_has_spacer_at(
        &self,
        track_index: usize,
        start: Seconds,
        duration: Seconds,
    ) -> bool {
        let Some(track) = self.children.get(track_index) else {
            return false;
        };
        let Some(item_index) = track.get_item_at_time(start) else {
            return false;
        };
        if (track.start_time_of_item(item_index) - start).abs() > EPS {
            return false;
        }
        matches!(track.items[item_index], Item::Gap(_))
            && (track.items[item_index].duration() - duration).abs() <= EPS
    }
}
