use super::SyncTrackInfo;
use crate::{IdMetadataExt, Item, Stack, Track};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrackBoundaryGroup {
    track_indices: Vec<usize>,
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
    pub fn reorder_track(&mut self, id: &str, insertion_index: isize) -> bool {
        let Some((track_index, _)) = self.get_track_by_id(id) else {
            return false;
        };
        let dest_index = super::clamp_insertion_index(self.children.len(), insertion_index);
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

    /// Return sync groups of tracks that share a link group.
    pub fn sync_track_info(&self) -> Vec<SyncTrackInfo> {
        self.track_boundary_ranges()
            .into_iter()
            .map(|group| {
                let track_indices = group.track_indices.clone();
                let track_ids = track_indices
                    .iter()
                    .map(|index| self.children[*index].get_id())
                    .collect();
                SyncTrackInfo {
                    track_indices,
                    track_ids,
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

    fn track_boundary_group_at(&self, track_index: usize) -> Option<TrackBoundaryGroup> {
        self.track_boundary_ranges()
            .into_iter()
            .find(|group| group.track_indices.contains(&track_index))
    }

    /// The ascending track indices of the sync group `track_index` belongs to
    /// (the same grouping reported by `sync_track_info`). Falls back to the
    /// track itself if it is not part of any multi-track group.
    pub(super) fn boundary_group_indices(&self, track_index: usize) -> Vec<usize> {
        match self.track_boundary_group_at(track_index) {
            Some(group) => group.track_indices,
            None => vec![track_index],
        }
    }

    /// Build sync boundary groups.
    ///
    /// Tracks share a sync group when they share any link group. Timing, track
    /// kind, and empty boundary tracks do not affect membership.
    fn track_boundary_ranges(&self) -> Vec<TrackBoundaryGroup> {
        let len = self.children.len();
        if len == 0 {
            return Vec::new();
        }

        let mut parents: Vec<_> = (0..len).collect();
        let mut first_track_for_sync_clips = HashMap::new();

        fn find_root(parents: &mut [usize], index: usize) -> usize {
            if parents[index] != index {
                parents[index] = find_root(parents, parents[index]);
            }
            parents[index]
        }

        for track_index in 0..len {
            for sync_clips_id in self.track_sync_clips_ids(track_index) {
                if let Some(previous_track_index) =
                    first_track_for_sync_clips.insert(sync_clips_id, track_index)
                {
                    let previous_root = find_root(&mut parents, previous_track_index);
                    let current_root = find_root(&mut parents, track_index);
                    if previous_root != current_root {
                        parents[current_root] = previous_root;
                    }
                }
            }
        }

        let mut groups_by_root: HashMap<usize, Vec<usize>> = HashMap::new();
        for track_index in 0..len {
            let root = find_root(&mut parents, track_index);
            groups_by_root.entry(root).or_default().push(track_index);
        }

        let mut groups: Vec<_> = groups_by_root
            .into_values()
            .map(|mut track_indices| {
                track_indices.sort_unstable();
                TrackBoundaryGroup { track_indices }
            })
            .collect();
        groups.sort_by_key(|group| group.track_indices[0]);
        groups
    }
}
