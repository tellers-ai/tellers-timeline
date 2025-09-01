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
                let active_key = clip
                    .active_media_reference_key
                    .as_deref()
                    .unwrap_or("DEFAULT_MEDIA");
                if let Some(r) = clip.media_references.get(active_key) {
                    if let Some(ar) = &r.available_range {
                        let media_total = ar.duration.value;
                        let start = clip.source_range.start_time.value;
                        let remaining = (media_total - start).max(0.0);
                        effective_duration = effective_duration.min(remaining);
                    }
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
