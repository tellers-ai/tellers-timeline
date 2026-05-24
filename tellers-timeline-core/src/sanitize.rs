use crate::{IdMetadataExt, Item, Stack, Timeline, Track};
use std::collections::HashSet;

impl Timeline {
    pub fn sanitize(&mut self) {
        self.tracks.sanitize();
    }
}

impl Track {
    pub fn sanitize(&mut self) {
        self.clamp_clips_to_available_ranges();
        self.clamp_negative_durations();
        self.remove_zero_length_items();
        self.merge_adjacent_gaps();
        self.remove_trailing_gap();
    }

    pub fn clamp_clips_to_available_ranges(&mut self) {
        for it in &mut self.items {
            it.clamp_to_active_available_range();
        }
    }

    pub fn clamp_negative_durations(&mut self) {
        for it in &mut self.items {
            if it.duration() < 0.0 {
                it.set_duration(0.0);
            }
        }
    }

    pub fn remove_zero_length_items(&mut self) {
        self.items.retain(|it| it.duration() > 0.0);
    }

    pub fn merge_adjacent_gaps(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let mut merged: Vec<Item> = Vec::with_capacity(self.items.len());
        for item in self.items.drain(..) {
            match (merged.last_mut(), &item) {
                (Some(Item::Gap(prev)), Item::Gap(next)) => {
                    prev.source_range.duration.value += next.source_range.duration.value;
                }
                _ => merged.push(item),
            }
        }
        self.items = merged;
    }

    pub fn remove_trailing_gap(&mut self) {
        if self.items.last().is_some_and(|item| matches!(item, Item::Gap(_))) {
            self.items.pop();
        }
    }
}

impl Stack {
    pub fn sanitize(&mut self) {
        for t in &mut self.children {
            t.sanitize();
        }
        self.ensure_unique_timeline_ids();
    }

    fn ensure_unique_timeline_ids(&mut self) {
        let mut used_ids = HashSet::new();
        for track in &mut self.children {
            ensure_unique_timeline_id(track, &mut used_ids);
            for item in &mut track.items {
                ensure_unique_timeline_id(item, &mut used_ids);
            }
        }
    }
}

fn ensure_unique_timeline_id<T: IdMetadataExt>(value: &mut T, used_ids: &mut HashSet<String>) {
    if let Some(id) = value.get_id().filter(|id| !id.is_empty()) {
        if used_ids.insert(id) {
            return;
        }
    }

    value.set_id(Some(new_unused_timeline_id(used_ids)));
}

fn new_unused_timeline_id(used_ids: &mut HashSet<String>) -> String {
    loop {
        let id = crate::types::gen_hex_id_12();
        if used_ids.insert(id.clone()) {
            return id;
        }
    }
}
