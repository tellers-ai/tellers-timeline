use crate::{
    Gap, IdMetadataExt, InsertPolicy, Item, OverlapPolicy, Seconds, SplitClipInfo, Stack, TrackInsertResult,
};
use std::collections::{HashMap, HashSet};

use super::{resolve_sync_clips_id, EPS};

fn is_split_fragment(clip_id: &str, splits: &[SplitClipInfo]) -> bool {
    splits.iter().any(|split| {
        split.left_clip_id.as_deref() == Some(clip_id)
            || split.right_clip_id.as_deref() == Some(clip_id)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplitOutcome {
    BothSides,
    LeftOnly,
    RightOnly,
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

    /// Among sync clusters containing `track_index`, prefer one with a video track
    /// when `prefer_cluster_with_video` is true (insert on a video track or using a
    /// synced video clip), otherwise prefer the cluster with the most tracks.
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

    /// Resolve the working sync cluster for an insert. When a synced video clip is
    /// used but the destination cluster has no video track, merge in the best cluster
    /// that contains `preferred_video_track_id`.
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

    /// Apply primary-track insert mutations to synced clips on partner tracks in `cluster`.
    pub(super) fn propagate_track_insert_to_cluster(
        &mut self,
        dest_track_index: usize,
        insert_start: Seconds,
        insert_duration: Seconds,
        overlap_policy: OverlapPolicy,
        track_result: &TrackInsertResult,
        cluster: &[usize],
        insert_sync_clips_id: Option<i64>,
        column_tracks: &[usize],
    ) -> bool {
        if insert_duration <= EPS {
            return true;
        }
        let insert_end = insert_start + insert_duration;

        let mut deleted_sync_ids = HashSet::new();
        for deleted in &track_result.deleted_clips {
            if is_split_fragment(&deleted.clip_id, &track_result.split_clips) {
                continue;
            }
            if let Some(sync_id) = deleted.sync_clips_id {
                deleted_sync_ids.insert(sync_id);
            }
        }
        for sync_id in deleted_sync_ids {
            self.delete_sync_clips(sync_id, true);
        }

        let split_outcomes = self.classify_split_outcomes(
            track_result,
            dest_track_index,
            insert_start,
            insert_end,
        );
        if overlap_policy != OverlapPolicy::Push {
            for (split, outcome) in split_outcomes {
                let Some(sync_clips_id) = split.sync_clips_id else {
                    continue;
                };
                match outcome {
                    SplitOutcome::BothSides => {
                        let right_sync_clips_id = match insert_sync_clips_id {
                            Some(id) => id,
                            None if overlap_policy == OverlapPolicy::Override => {
                                self.next_sync_clips_id()
                            }
                            None => sync_clips_id,
                        };
                        if !self.propagate_full_split_to_sync_group(
                            sync_clips_id,
                            split.split_time,
                            insert_start,
                            insert_duration,
                            overlap_policy,
                            cluster,
                            dest_track_index,
                            right_sync_clips_id,
                        ) {
                            return false;
                        }
                    }
                    SplitOutcome::LeftOnly | SplitOutcome::RightOnly => {
                        if !self.propagate_partial_split_to_sync_group(
                            sync_clips_id,
                            insert_start,
                            insert_end,
                            overlap_policy,
                            cluster,
                            dest_track_index,
                        ) {
                            return false;
                        }
                    }
                }
            }
        }

        if overlap_policy == OverlapPolicy::Push {
            return self.propagate_push_insert_to_cluster(
                dest_track_index,
                insert_start,
                insert_duration,
                cluster,
                column_tracks,
            );
        }

        true
    }

    fn classify_split_outcomes(
        &self,
        track_result: &TrackInsertResult,
        dest_track_index: usize,
        insert_start: Seconds,
        insert_end: Seconds,
    ) -> Vec<(SplitClipInfo, SplitOutcome)> {
        let mut by_sync_id: HashMap<i64, SplitClipInfo> = HashMap::new();
        for split in &track_result.split_clips {
            let Some(sync_id) = split.sync_clips_id else {
                continue;
            };
            by_sync_id
                .entry(sync_id)
                .and_modify(|existing| {
                    if existing.split_time > split.split_time {
                        existing.split_time = split.split_time;
                    }
                    if existing.old_clip_id.is_empty() {
                        existing.old_clip_id = split.old_clip_id.clone();
                    }
                })
                .or_insert_with(|| split.clone());
        }

        by_sync_id
            .into_values()
            .filter_map(|split| {
                let sync_id = split.sync_clips_id?;
                let mut has_left = false;
                let mut has_right = false;
                for (track_index, item_index) in self.synced_clips_targets(sync_id) {
                    if track_index != dest_track_index {
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
                let outcome = match (has_left, has_right) {
                    (true, true) => SplitOutcome::BothSides,
                    (true, false) => SplitOutcome::LeftOnly,
                    (false, true) => SplitOutcome::RightOnly,
                    (false, false) => return None,
                };
                Some((split, outcome))
            })
            .collect()
    }

    fn propagate_full_split_to_sync_group(
        &mut self,
        sync_clips_id: i64,
        split_time: Seconds,
        insert_start: Seconds,
        insert_duration: Seconds,
        overlap_policy: OverlapPolicy,
        cluster: &[usize],
        dest_track_index: usize,
        right_sync_clips_id: i64,
    ) -> bool {
        let partner_tracks: Vec<usize> = cluster
            .iter()
            .copied()
            .filter(|&track_index| track_index != dest_track_index)
            .collect();

        let mut used_ids = self.collect_timeline_ids();

        for track_index in partner_tracks {
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

            if self.track_has_spacer_at(track_index, insert_start, insert_duration) {
                continue;
            }

            let mut gap = Item::Gap(Gap::make_gap(insert_duration));
            Self::ensure_unique_item_id(&mut gap, &mut used_ids);
            let gap_result = self.children[track_index].insert_at_time(
                insert_start,
                gap,
                overlap_policy,
                InsertPolicy::SplitAndInsert,
            );
            if !gap_result.success {
                return false;
            }
        }

        let right_start_threshold = split_time + insert_duration - EPS;
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
            super::set_resolve_sync_clips_id(&mut clip.metadata, right_sync_clips_id);
        }

        true
    }

    fn propagate_partial_split_to_sync_group(
        &mut self,
        sync_clips_id: i64,
        insert_start: Seconds,
        insert_end: Seconds,
        overlap_policy: OverlapPolicy,
        cluster: &[usize],
        dest_track_index: usize,
    ) -> bool {
        if overlap_policy == OverlapPolicy::Push {
            let insert_duration = insert_end - insert_start;
            if insert_duration <= EPS {
                return true;
            }
            let mut used_ids = self.collect_timeline_ids();
            for track_index in cluster {
                if *track_index == dest_track_index {
                    continue;
                }
                let Some(track) = self.children.get(*track_index) else {
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
                if self.track_has_spacer_at(*track_index, insert_start, insert_duration) {
                    continue;
                }
                let mut gap = Item::Gap(Gap::make_gap(insert_duration));
                Self::ensure_unique_item_id(&mut gap, &mut used_ids);
                let result = self.children[*track_index].insert_at_time(
                    insert_start,
                    gap,
                    OverlapPolicy::Push,
                    InsertPolicy::SplitAndInsert,
                );
                if !result.success {
                    return false;
                }
            }
            return true;
        }

        for track_index in cluster {
            if *track_index == dest_track_index {
                continue;
            }
            let Some(track) = self.children.get(*track_index) else {
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
            self.children[*track_index].delete_range(insert_start, insert_end, true);
        }
        true
    }

    fn propagate_push_insert_to_cluster(
        &mut self,
        dest_track_index: usize,
        insert_start: Seconds,
        insert_duration: Seconds,
        cluster: &[usize],
        column_tracks: &[usize],
    ) -> bool {
        let column_track_set: HashSet<usize> = column_tracks.iter().copied().collect();
        let push_anchor = insert_start + insert_duration;
        let dest_right_start = self.children.get(dest_track_index).and_then(|track| {
            let mut pos = 0.0;
            let mut min_start: Option<Seconds> = None;
            for item in &track.items {
                if pos >= push_anchor - EPS {
                    if let Item::Clip(clip) = item {
                        if resolve_sync_clips_id(&clip.metadata).is_some() {
                            min_start = Some(match min_start {
                                Some(current) => current.min(pos),
                                None => pos,
                            });
                        }
                    }
                }
                pos += item.duration().max(0.0);
            }
            min_start
        });

        let dest_right_start = dest_right_start.unwrap_or(push_anchor);
        let mut used_ids = self.collect_timeline_ids();
        for &track_index in cluster {
            if track_index == dest_track_index || column_track_set.contains(&track_index) {
                continue;
            }
            let Some(partner_sync_start) =
                self.first_cluster_sync_clip_start_at_or_after(track_index, insert_start, cluster)
            else {
                continue;
            };
            let (gap_at, gap_duration) = if partner_sync_start <= insert_start + EPS {
                (insert_start, dest_right_start - insert_start)
            } else {
                (insert_start, dest_right_start - partner_sync_start)
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
        true
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
                Item::Clip(clip) => resolve_sync_clips_id(&clip.metadata),
                Item::Gap(_) => None,
            })
            .collect();
        let mut pos = 0.0;
        let mut any_sync_start: Option<Seconds> = None;
        for item in &track.items {
            if pos + item.duration().max(0.0) > insert_start + EPS {
                if let Item::Clip(clip) = item {
                    if let Some(sync_id) = resolve_sync_clips_id(&clip.metadata) {
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
        (track.items[item_index].duration() - duration).abs() <= EPS
    }

    /// Sync clips on partner tracks that start at or after `insert_start`.
    pub(super) fn sync_clips_after_time_in_cluster(
        &self,
        insert_start: Seconds,
        cluster: &[usize],
        dest_track_index: usize,
    ) -> Vec<(usize, String, i64)> {
        let mut clips = Vec::new();
        for &track_index in cluster {
            if track_index == dest_track_index {
                continue;
            }
            let Some(track) = self.children.get(track_index) else {
                continue;
            };
            let mut pos = 0.0;
            for item in &track.items {
                if pos >= insert_start - EPS {
                    if let Item::Clip(clip) = item {
                        if let (Some(id), Some(sync_id)) = (
                            item.get_id(),
                            resolve_sync_clips_id(&clip.metadata),
                        ) {
                            clips.push((track_index, id, sync_id));
                        }
                    }
                }
                pos += item.duration().max(0.0);
            }
        }
        clips
    }
}
