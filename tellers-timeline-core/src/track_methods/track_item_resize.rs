use crate::{InsertPolicy, Item, OverlapPolicy, Seconds, Track};

impl Track {
    /// Returns true if index valid and duration updated.
    pub fn resize_item(
        &mut self,
        item_index: usize,
        new_start_time: Seconds,
        new_duration: Seconds,
        overlap_policy: OverlapPolicy,
        clamp_to_media: bool,
    ) -> bool {
        if item_index >= self.items.len() {
            return false;
        }

        let mut item = self.items.remove(item_index);

        let mut effective_duration = new_duration.max(0.0);
        if clamp_to_media {
            if let Item::Clip(ref clip) = item {
                if let Some(media_total) = clip.source.media_duration {
                    let remaining = (media_total - clip.source.media_start).max(0.0);
                    effective_duration = effective_duration.min(remaining);
                }
            }
        }

        item.set_duration(effective_duration);

        self.insert_at_time(
            new_start_time,
            item,
            overlap_policy,
            InsertPolicy::SplitAndInsert,
        );
        self.sanitize();
        true
    }
}
