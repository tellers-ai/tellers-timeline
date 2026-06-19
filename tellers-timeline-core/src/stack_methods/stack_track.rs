use super::SyncTrackInfo;
use crate::{IdMetadataExt, Item, Stack, Track, TrackKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TrackBoundaryGroup {
    start: usize,
    end: usize,
}

impl Stack {
    /// Append a track to the stack.
    pub fn add_track(&mut self, track: Track) {
        self.children.push(track);
        self.sanitize();
    }

    /// Insert a track at a specific index. Negative indices behave like Python's.
    pub fn add_track_at(&mut self, track: Track, insertion_index: isize) -> bool {
        let idx = super::clamp_insertion_index(self.children.len(), insertion_index);
        self.children.insert(idx, track);
        self.sanitize();
        true
    }

    /// Move a track to a new insertion index.
    ///
    /// Primary tracks move their whole boundary group to boundary-group edges.
    /// Secondary tracks in a linked boundary can only move inside their current boundary group.
    pub fn reorder_track(&mut self, id: &str, insertion_index: isize) -> bool {
        let Some((track_index, _)) = self.get_track_by_id(id) else {
            return false;
        };
        let dest_index = super::clamp_insertion_index(self.children.len(), insertion_index);
        let Some(group) = self.track_boundary_group_at(track_index) else {
            return false;
        };

        if self.is_primary_track_in_group(track_index, group) {
            if !self.is_track_group_boundary_index(dest_index) {
                return false;
            }
            if dest_index >= group.start && dest_index <= group.end {
                return true;
            }
            let group_len = group.end - group.start;
            let moved_tracks: Vec<_> = self.children.drain(group.start..group.end).collect();
            let adjusted_dest_index = if dest_index > group.start {
                dest_index - group_len
            } else {
                dest_index
            };
            for (offset, track) in moved_tracks.into_iter().enumerate() {
                self.children.insert(adjusted_dest_index + offset, track);
            }
            self.sanitize();
            return true;
        } else if dest_index < group.start || dest_index > group.end {
            return false;
        }

        if dest_index == track_index || dest_index == track_index + 1 {
            return true;
        }

        let track = self.children.remove(track_index);
        let adjusted_dest_index = if dest_index > track_index {
            dest_index - 1
        } else {
            dest_index
        };
        self.children.insert(adjusted_dest_index, track);
        self.sanitize();
        true
    }

    /// Return boundary groups with their primary track and bound tracks.
    pub fn sync_track_info(&self) -> Vec<SyncTrackInfo> {
        self.track_boundary_ranges()
            .into_iter()
            .map(|group| {
                let primary_track_index = self.primary_track_index_in_group(group);
                let track_indices: Vec<_> = (group.start..group.end).collect();
                let track_ids = track_indices
                    .iter()
                    .map(|index| self.children[*index].get_id())
                    .collect();
                let bound_track_indices: Vec<_> = track_indices
                    .iter()
                    .copied()
                    .filter(|index| *index != primary_track_index)
                    .collect();
                let bound_track_ids = bound_track_indices
                    .iter()
                    .map(|index| self.children[*index].get_id())
                    .collect();
                SyncTrackInfo {
                    start_index: group.start,
                    end_index: group.end,
                    track_indices,
                    track_ids,
                    primary_track_index,
                    primary_track_id: self.children[primary_track_index].get_id(),
                    bound_track_indices,
                    bound_track_ids,
                }
            })
            .collect()
    }

    /// Delete a track by id. Returns the removed track on success.
    pub fn delete_track(&mut self, id: &str) -> Option<Track> {
        let (i, track) = self.get_track_by_id(id)?;
        let touched_sync_clips_ids: Vec<_> = track
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Clip(clip) => super::resolve_sync_clips_id(&clip.metadata),
                Item::Gap(_) => None,
            })
            .collect();
        let removed = self.children.remove(i);
        for sync_clips_id in touched_sync_clips_ids {
            self.delete_sync_clips(sync_clips_id, true);
        }
        self.sanitize_preserving_all_gap_tracks();
        Some(removed)
    }

    fn is_track_group_boundary_index(&self, index: usize) -> bool {
        if index > self.children.len() {
            return false;
        }
        index == 0
            || index == self.children.len()
            || self
                .track_boundary_ranges()
                .iter()
                .any(|group| index == group.start || index == group.end)
    }

    fn track_boundary_group_at(&self, track_index: usize) -> Option<TrackBoundaryGroup> {
        self.track_boundary_ranges()
            .into_iter()
            .find(|group| track_index >= group.start && track_index < group.end)
    }

    /// The ascending track indices of the sync group `track_index` belongs to
    /// (the same grouping reported by `sync_track_info`). Falls back to the
    /// track itself if it is not part of any multi-track group.
    pub(super) fn boundary_group_indices(&self, track_index: usize) -> Vec<usize> {
        match self.track_boundary_group_at(track_index) {
            Some(group) => (group.start..group.end).collect(),
            None => vec![track_index],
        }
    }

    /// Build sync boundary groups from the bottom track upward.
    ///
    /// The last track is the initial principal. Each track above it is compared to
    /// the current principal: if every synced clip on the candidate matches a
    /// synced clip on the principal, or the candidate is an empty boundary track
    /// whose first non-empty track above them is already in the cluster,
    /// they share a cluster. Otherwise the candidate becomes the principal of a
    /// new cluster. Repeat until index 0.
    fn track_boundary_ranges(&self) -> Vec<TrackBoundaryGroup> {
        let len = self.children.len();
        if len == 0 {
            return Vec::new();
        }

        let mut groups_bottom_up = Vec::new();
        let mut principal = len - 1;
        let mut cluster_start = principal;
        let mut cluster_end = len;

        for i in 1..len {
            let candidate = len - 1 - i;
            if self.track_matches_principal_cluster(principal, candidate) {
                cluster_start = candidate;
            } else {
                groups_bottom_up.push(TrackBoundaryGroup {
                    start: cluster_start,
                    end: cluster_end,
                });
                principal = candidate;
                cluster_start = candidate;
                cluster_end = candidate + 1;
            }
        }
        groups_bottom_up.push(TrackBoundaryGroup {
            start: cluster_start,
            end: cluster_end,
        });

        groups_bottom_up.reverse();
        groups_bottom_up
    }

    fn is_primary_track_in_group(&self, track_index: usize, group: TrackBoundaryGroup) -> bool {
        track_index == self.primary_track_index_in_group(group)
    }

    fn primary_track_index_in_group(&self, group: TrackBoundaryGroup) -> usize {
        if group.end <= group.start + 1 {
            return group.start;
        }
        if let Some(video_index) = (group.start..group.end)
            .find(|index| self.children[*index].kind == TrackKind::Video)
        {
            return video_index;
        }
        group.start
    }
}
