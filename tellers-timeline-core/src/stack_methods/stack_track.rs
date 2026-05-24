use super::TrackBoundaryGroupInfo;
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
        if !self.is_track_group_boundary_index(idx) {
            return false;
        }
        self.children.insert(idx, track);
        self.sanitize();
        true
    }

    /// Move a track to a new insertion index.
    ///
    /// Primary tracks move their whole boundary group to the same positions accepted by
    /// `add_track_at`.
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
    pub fn track_boundary_group_info(&self) -> Vec<TrackBoundaryGroupInfo> {
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
                TrackBoundaryGroupInfo {
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
        let touched_link_groups: Vec<_> = track
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Clip(clip) => super::resolve_link_group_id(&clip.metadata),
                Item::Gap(_) => None,
            })
            .collect();
        let removed = self.children.remove(i);
        for link_group_id in touched_link_groups {
            self.delete_link_group(link_group_id, true);
        }
        self.sanitize();
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

    fn track_boundary_ranges(&self) -> Vec<TrackBoundaryGroup> {
        let mut groups = Vec::new();
        let mut start = 0;
        while start < self.children.len() {
            let mut end = start + 1;
            while end < self.children.len() && self.tracks_share_boundary_group(end - 1, end) {
                end += 1;
            }
            groups.push(TrackBoundaryGroup { start, end });
            start = end;
        }
        groups
    }

    fn tracks_share_boundary_group(&self, left: usize, right: usize) -> bool {
        let Some(left_track) = self.children.get(left) else {
            return false;
        };
        let Some(right_track) = self.children.get(right) else {
            return false;
        };
        if !track_has_linked_clip(left_track)
            || !track_has_linked_clip(right_track)
            || track_has_unlinked_clip(left_track)
            || track_has_unlinked_clip(right_track)
        {
            return false;
        }
        self.track_matches_primary_link_boundary(left, right)
            || self.track_matches_primary_link_boundary(right, left)
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

fn track_has_linked_clip(track: &Track) -> bool {
    track.items.iter().any(|item| match item {
        Item::Clip(clip) => super::resolve_link_group_id(&clip.metadata).is_some(),
        Item::Gap(_) => false,
    })
}

fn track_has_unlinked_clip(track: &Track) -> bool {
    track.items.iter().any(|item| match item {
        Item::Clip(clip) => super::resolve_link_group_id(&clip.metadata).is_none(),
        Item::Gap(_) => false,
    })
}
