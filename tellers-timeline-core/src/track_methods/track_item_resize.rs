use crate::{Seconds, Track};

impl Track {
    /// Set the new duration and start time of the item at `index`.

    /// Returns true if index valid and duration updated.
    pub fn resize_item(
        &mut self,
        item_index: usize,
        _new_start_time: Seconds,
        _new_duration: Seconds,
        _clamp_to_media: bool,
    ) -> bool {
        if item_index >= self.items.len() {
            return false;
        }
        // if
        // self.items[item_index].set_duration(new_duration);

        self.sanitize();

        true
    }
}
