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
                let total = clip.source_range.duration.value.max(0.0);
                if local_offset >= total - EPS {
                    // Nothing to split, put the original back
                    self.items.insert(item_index, crate::Item::Clip(clip));
                    return;
                }

                let left_duration = local_offset.max(0.0);
                let right_duration = (total - left_duration).max(0.0);

                let mut left_clip = clip.clone();
                left_clip.source_range.duration.value = left_duration;

                // Right clip keeps the rest, media_start advances by left_duration
                clip.source_range.duration.value = right_duration;
                clip.source_range.start_time.value += left_duration;

                // Ensure the right-hand piece receives a fresh unique id
                crate::metadata::IdMetadataExt::set_id(
                    &mut clip,
                    Some(crate::types::gen_hex_id_12()),
                );

                self.items.insert(item_index, crate::Item::Clip(left_clip));
                self.items.insert(item_index + 1, crate::Item::Clip(clip));
            }
            crate::Item::Gap(mut gap) => {
                let total = gap.source_range.duration.value.max(0.0);
                if local_offset >= total - EPS {
                    // Nothing to split, put the original back
                    self.items.insert(item_index, crate::Item::Gap(gap));
                    return;
                }

                let left_duration = local_offset.max(0.0);
                let right_duration = (total - local_offset).max(0.0);

                let left_gap = crate::types::Gap {
                    otio_schema: gap.otio_schema.clone(),
                    name: gap.name.clone(),
                    source_range: crate::types::TimeRange {
                        otio_schema: gap.source_range.otio_schema.clone(),
                        duration: crate::types::RationalTime {
                            otio_schema: gap.source_range.duration.otio_schema.clone(),
                            rate: gap.source_range.duration.rate,
                            value: left_duration,
                        },
                        start_time: crate::types::RationalTime {
                            otio_schema: gap.source_range.start_time.otio_schema.clone(),
                            rate: gap.source_range.start_time.rate,
                            value: gap.source_range.start_time.value,
                        },
                    },
                    metadata: gap.metadata.clone(),
                };

                gap.source_range.duration.value = right_duration;
                gap.source_range.start_time.value += left_duration;

                // Ensure the right-hand piece receives a fresh unique id
                crate::metadata::IdMetadataExt::set_id(
                    &mut gap,
                    Some(crate::types::gen_hex_id_12()),
                );

                self.items.insert(item_index, crate::Item::Gap(left_gap));
                self.items.insert(item_index + 1, crate::Item::Gap(gap));
            }
        }
    }
}
