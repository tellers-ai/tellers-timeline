use crate::{Item, Stack, Track};

impl Stack {
    /// Append a track to the stack.
    pub fn add_track(&mut self, track: Track) {
        self.children.push(track);
        self.sanitize();
    }

    /// Insert a track at a specific index. Negative indices behave like Python's.
    pub fn add_track_at(&mut self, track: Track, insertion_index: isize) {
        let idx = super::clamp_insertion_index(self.children.len(), insertion_index);
        self.children.insert(idx, track);
        self.sanitize();
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
}
