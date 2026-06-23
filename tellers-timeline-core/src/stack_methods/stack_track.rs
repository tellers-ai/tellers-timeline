use super::SyncTrackInfo;
use crate::{IdMetadataExt, Item, Stack, Track};
use std::collections::HashSet;

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

    /// The ascending track indices of every sync group `track_index` belongs to,
    /// merged for operations that need the full partner set (insert padding, etc.).
    pub(super) fn boundary_group_indices(&self, track_index: usize) -> Vec<usize> {
        let mut indices = HashSet::new();
        for group in self.track_boundary_ranges() {
            if group.track_indices.contains(&track_index) {
                indices.extend(&group.track_indices);
            }
        }
        if indices.is_empty() {
            return vec![track_index];
        }
        let mut result: Vec<_> = indices.into_iter().collect();
        result.sort_unstable();
        result
    }

    /// Build sync groups by expanding `tracks_share_sync_clips` from each track.
    ///
    /// A track can appear in multiple groups when it shares different link
    /// groups with different partners. Identical track sets are reported once.
    fn track_boundary_ranges(&self) -> Vec<TrackBoundaryGroup> {
        let len = self.children.len();
        if len == 0 {
            return Vec::new();
        }

        let mut groups = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for anchor in 0..len {
            let mut track_indices: Vec<usize> = (0..len)
                .filter(|&candidate| self.tracks_share_sync_clips(anchor, candidate))
                .collect();
            track_indices.sort_unstable();
            if seen.insert(track_indices.clone()) {
                groups.push(TrackBoundaryGroup { track_indices });
            }
        }
        groups.sort_by(|left, right| left.track_indices.cmp(&right.track_indices));
        groups
    }
}
