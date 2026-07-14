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

    /// Sanitize only the tracks at the given indices, running the per-track
    /// passes (clamp to available range, clamp negative durations, drop
    /// zero-length items, merge adjacent gaps, drop a trailing gap), then the
    /// stack-wide `ensure_unique_timeline_ids` pass so a split introduced by an
    /// override move/insert can't leave two items sharing a timeline id.
    ///
    /// Unlike [`Self::sanitize`] this keeps the heavier per-track passes scoped
    /// to the given tracks and skips `cleanup_dangling_sync_clips`, so it costs
    /// O(items in the given tracks) plus a cheap id-hash pass rather than the
    /// full per-track work over every track. It is meant for interactive
    /// previews that touch only a couple of tracks and re-run the full
    /// [`Self::sanitize`] when the edit is committed. Out-of-range indices are
    /// ignored.
    pub fn sanitize_tracks(&mut self, track_indices: &[usize]) {
        for &index in track_indices {
            if let Some(track) = self.children.get_mut(index) {
                track.sanitize();
            }
        }
        self.ensure_unique_timeline_ids();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Clip, Gap, MediaReference, RationalTime, TimeRange, TrackKind};
    use std::collections::HashMap;

    fn range(duration: f64) -> TimeRange {
        TimeRange {
            otio_schema: "TimeRange.1".to_string(),
            start_time: RationalTime {
                otio_schema: "RationalTime.1".to_string(),
                rate: 1.0,
                value: 0.0,
            },
            duration: RationalTime {
                otio_schema: "RationalTime.1".to_string(),
                rate: 1.0,
                value: duration,
            },
        }
    }

    fn clip(duration: f64) -> Item {
        let mut refs = HashMap::new();
        refs.insert(
            "DEFAULT_MEDIA".to_string(),
            MediaReference::ExternalReference {
                target_url: "media://dummy".to_string(),
                available_range: None,
                name: None,
                available_image_bounds: None,
                metadata: serde_json::Value::Null,
            },
        );
        Item::Clip(Clip::new(
            range(duration),
            refs,
            Some("DEFAULT_MEDIA".to_string()),
            None,
            None,
        ))
    }

    fn track_with_mergeable_gaps() -> Track {
        let mut t = Track::new(TrackKind::Video, None);
        // clip | gap | gap : sanitize merges the gaps and drops the trailing one,
        // leaving just the clip.
        t.items = vec![
            clip(2.0),
            Item::Gap(Gap::new(1.0, None)),
            Item::Gap(Gap::new(1.0, None)),
        ];
        t
    }

    #[test]
    fn sanitize_tracks_only_touches_given_tracks() {
        let mut stack = Stack::default();
        stack.children = vec![track_with_mergeable_gaps(), track_with_mergeable_gaps()];

        stack.sanitize_tracks(&[0]);

        // Track 0 was sanitized: gaps merged + trailing gap removed -> clip only.
        assert_eq!(stack.children[0].items.len(), 1);
        // Track 1 was left untouched.
        assert_eq!(stack.children[1].items.len(), 3);

        // An out-of-range index is ignored (no panic, no effect).
        stack.sanitize_tracks(&[99]);
        assert_eq!(stack.children[1].items.len(), 3);
    }
}
