use crate::{IdMetadataExt, Item, Stack, Timeline, Track};
use std::collections::{HashMap, HashSet};

impl Timeline {
    pub fn sanitize(&mut self) {
        self.tracks.sanitize();
    }
}

impl Track {
    pub(crate) fn sanitize(&mut self) {
        self.clamp_clips_to_available_ranges();
        self.clamp_negative_durations();
        self.remove_zero_length_items();
        self.merge_adjacent_gaps();
        self.remove_trailing_gap();
    }

    pub(crate) fn sanitize_preserving_all_gap_track(&mut self) {
        self.clamp_clips_to_available_ranges();
        self.clamp_negative_durations();
        self.remove_zero_length_items();
        self.merge_adjacent_gaps();
        if !self.items.iter().all(|item| matches!(item, Item::Gap(_))) {
            self.remove_trailing_gap();
        }
    }

    pub(crate) fn clamp_clips_to_available_ranges(&mut self) {
        for it in &mut self.items {
            it.clamp_to_active_available_range();
        }
    }

    pub(crate) fn clamp_negative_durations(&mut self) {
        for it in &mut self.items {
            if it.duration() < 0.0 {
                it.set_duration(0.0);
            }
        }
    }

    pub(crate) fn remove_zero_length_items(&mut self) {
        self.items.retain(|it| it.duration() > 0.0);
    }

    pub(crate) fn merge_adjacent_gaps(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let mut merged: Vec<Item> = Vec::with_capacity(self.items.len());
        for item in self.items.drain(..) {
            match (merged.last_mut(), &item) {
                (Some(Item::Gap(prev)), Item::Gap(next)) => {
                    let duration =
                        prev.source_range.duration.to_seconds() + next.source_range.duration.to_seconds();
                    prev.source_range.duration.set_from_seconds(duration);
                }
                _ => merged.push(item),
            }
        }
        self.items = merged;
    }

    pub(crate) fn remove_trailing_gap(&mut self) {
        if self
            .items
            .last()
            .is_some_and(|item| matches!(item, Item::Gap(_)))
        {
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
        self.cleanup_dangling_sync_clips();
    }

    pub(crate) fn sanitize_preserving_all_gap_tracks(&mut self) {
        for t in &mut self.children {
            t.sanitize_preserving_all_gap_track();
        }
        self.ensure_unique_timeline_ids();
        self.cleanup_dangling_sync_clips();
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

    fn cleanup_dangling_sync_clips(&mut self) {
        let mut counts: HashMap<i64, usize> = HashMap::new();
        for track in &self.children {
            for item in &track.items {
                let Item::Clip(clip) = item else {
                    continue;
                };
                if let Some(sync_clips_id) = clip.sync_clips_id() {
                    *counts.entry(sync_clips_id).or_default() += 1;
                }
            }
        }

        for track in &mut self.children {
            for item in &mut track.items {
                let Item::Clip(clip) = item else {
                    continue;
                };
                let Some(sync_clips_id) = clip.sync_clips_id() else {
                    continue;
                };
                if counts.get(&sync_clips_id).copied().unwrap_or_default() < 2 {
                    remove_resolve_sync_clips_id(&mut clip.metadata);
                }
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

fn remove_resolve_sync_clips_id(metadata: &mut serde_json::Value) -> bool {
    let Some(resolve) = metadata
        .get_mut("Resolve_OTIO")
        .and_then(|value| value.as_object_mut())
    else {
        return false;
    };
    resolve.remove("Link Group ID").is_some()
}
