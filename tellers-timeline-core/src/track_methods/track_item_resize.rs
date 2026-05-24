use crate::{InsertPolicy, OverlapPolicy, Seconds, Track};

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

        let effective_duration = new_duration.max(0.0);
        item.set_duration(effective_duration);
        if clamp_to_media {
            item.clamp_to_active_available_range();
        }

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
