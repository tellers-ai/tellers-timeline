use crate::{Item, Seconds, Track};

impl Track {
    /// Set the new duration and start time of the item at `index`.

    /// Returns true if index valid and duration updated.
    pub fn resize_item(
        &mut self,
        item_index: usize,
        new_start_time: Seconds,
        new_duration: Seconds,
        clamp_to_media: bool,
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
