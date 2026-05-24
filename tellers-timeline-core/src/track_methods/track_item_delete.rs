use crate::{Item, Track};

impl Track {
    /// Delete the clip at a given index. If `replace_with_gap` is true, insert a gap of the
    /// same duration at that position and merge adjacent gaps.
    /// Returns whether a deletion occurred.
    pub(crate) fn delete_clip(&mut self, index: usize, replace_with_gap: bool) -> bool {
        if index >= self.items.len() {
            return false;
        }
        match &self.items[index] {
            Item::Clip(c) => {
                let removed_duration = c.source_range.duration.value.max(0.0);
                self.items.remove(index);
                if replace_with_gap && removed_duration > 0.0 {
                    self.items.insert(
                        index.min(self.items.len()),
                        Item::Gap(crate::types::Gap::make_gap(removed_duration)),
                    );
                }
                self.sanitize();
                true
            }
            Item::Gap(_) => false,
        }
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

        assert!(track.delete_clip(1, false));

        assert_eq!(track.items.len(), 1);
        assert!(matches!(track.items[0], Item::Clip(_)));
    }
}
