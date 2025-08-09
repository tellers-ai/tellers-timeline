use crate::{Item, Timeline, Track};

impl Timeline {
    pub fn sanitize(&mut self) {
        for track in &mut self.tracks {
            track.sanitize();
        }
    }
}

impl Track {
    pub fn sanitize(&mut self) {
        self.clamp_negative_durations();
        self.remove_zero_length_items();
        self.merge_adjacent_gaps();
    }

    pub fn clamp_negative_durations(&mut self) {
        for it in &mut self.items {
            if it.duration() < 0.0 {
                it.set_duration(0.0);
            }
        }
    }

    pub fn remove_zero_length_items(&mut self) {
        self.items.retain(|it| it.duration() > 0.0);
    }

    // Sorting by start is meaningless now; order is authoritative.

    pub fn merge_adjacent_gaps(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let mut merged: Vec<Item> = Vec::with_capacity(self.items.len());
        for item in self.items.drain(..) {
            match (merged.last_mut(), &item) {
                (Some(Item::Gap(prev)), Item::Gap(next)) => {
                    prev.duration += next.duration;
                }
                _ => merged.push(item),
            }
        }
        self.items = merged;
    }

    // Overlaps cannot exist when starts are derived; nothing to do.
}
