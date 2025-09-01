use crate::{Item, Seconds, Track};

impl Track {
    /// Find an item by id stored at `metadata["tellers.ai"]["timeline_id"]`.
    /// Returns the index and a non-mut reference to the item Some((index, item)).
    pub fn get_item_by_id(&self, id: &str) -> Option<(usize, &Item)> {
        for (i, it) in self.items.iter().enumerate() {
            if crate::metadata::IdMetadataExt::get_id(it).as_deref() == Some(id) {
                return Some((i, it));
            }
        }
        None
    }

    /// Find the index of the item containing the given time.
    pub fn get_item_at_time(&self, time: Seconds) -> Option<usize> {
        let mut pos: Seconds = 0.0;
        for (i, it) in self.items.iter().enumerate() {
            let end = pos + it.duration().max(0.0);
            if time >= pos && time < end {
                return Some(i);
            }
            pos = end;
        }
        None
    }
}
