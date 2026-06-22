use crate::{Item, Track};

impl Track {
    /// Remove the clip or gap at `index`, optionally inserting a gap of the same
    /// duration. Does not run track sanitize; callers batch sanitize at the stack level.
    /// Returns the removed item on success.
    pub(crate) fn delete_clip_at(&mut self, index: usize, replace_with_gap: bool) -> Option<Item> {
        if index >= self.items.len() {
            return None;
        }
        match &self.items[index] {
            Item::Clip(c) => {
                let removed_duration = c.source_range.duration.to_seconds().max(0.0);
                let removed = self.items.remove(index);
                if replace_with_gap && removed_duration > 0.0 {
                    self.items.insert(
                        index.min(self.items.len()),
                        Item::Gap(crate::types::Gap::make_gap(removed_duration)),
                    );
                    self.merge_adjacent_gaps();
                }
                Some(removed)
            }
            Item::Gap(_) if !replace_with_gap => Some(self.items.remove(index)),
            Item::Gap(_) => None,
        }
    }

    /// Delete the clip at a given index. If `replace_with_gap` is true, insert a gap of the
    /// same duration at that position and merge adjacent gaps.
    /// Returns the removed item on success.
    pub(crate) fn delete_clip(&mut self, index: usize, replace_with_gap: bool) -> Option<Item> {
        let removed = self.delete_clip_at(index, replace_with_gap)?;
        self.sanitize();
        Some(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Clip, Gap, MediaReference, RationalTime, TimeRange};
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

    #[test]
    fn delete_clip_sanitizes_track_after_successful_delete() {
        let mut track = Track::default();
        track.items.push(clip(2.0));
        track.items.push(clip(3.0));
        track.items.push(Item::Gap(Gap::make_gap(1.0)));

        assert!(matches!(track.delete_clip(1, false), Some(Item::Clip(_))));

        assert_eq!(track.items.len(), 1);
        assert!(matches!(track.items[0], Item::Clip(_)));
    }
}
