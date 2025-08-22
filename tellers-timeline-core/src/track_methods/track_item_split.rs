use crate::{Seconds, Track};

impl Track {
    pub fn split_at_time(&mut self, split_time: Seconds) {
        const EPS: Seconds = 1e-9;

        let Some(item_index) = self.get_item_at_time(split_time) else {
            return;
        };

        // Compute the offset from the start of the item
        let item_start_time = self.start_time_of_item(item_index);
        let local_offset = split_time - item_start_time;

        if local_offset <= EPS {
            return;
        }

        // Move the item out to avoid borrow issues
        let original = self.items.remove(item_index);

        match original {
            crate::Item::Clip(mut clip) => {
                let total = clip.duration.max(0.0);
                if local_offset >= total - EPS {
                    // Nothing to split, put the original back
                    self.items.insert(item_index, crate::Item::Clip(clip));
                    return;
                }

                let left_duration = local_offset.max(0.0);
                let right_duration = (total - left_duration).max(0.0);

                let mut left_clip = clip.clone();
                left_clip.duration = left_duration;

                // Right clip keeps the rest, media_start advances by left_duration
                clip.duration = right_duration;
                clip.source.media_start += left_duration;

                self.items.insert(item_index, crate::Item::Clip(left_clip));
                self.items.insert(item_index + 1, crate::Item::Clip(clip));
            }
            crate::Item::Gap(mut gap) => {
                let total = gap.duration.max(0.0);
                if local_offset >= total - EPS {
                    // Nothing to split, put the original back
                    self.items.insert(item_index, crate::Item::Gap(gap));
                    return;
                }

                let left_duration = local_offset.max(0.0);
                let right_duration = (total - local_offset).max(0.0);

                let left_gap = crate::types::Gap {
                    otio_schema: gap.otio_schema.clone(),
                    duration: left_duration,
                    metadata: gap.metadata.clone(),
                };

                gap.duration = right_duration;

                self.items.insert(item_index, crate::Item::Gap(left_gap));
                self.items.insert(item_index + 1, crate::Item::Gap(gap));
            }
        }
    }
}
