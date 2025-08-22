use crate::{Item, Track};

impl Track {
    pub fn sanitize(&mut self) {
        self.remove_zero_length_items();
        self.merge_adjacent_gaps();
    }

    pub fn remove_zero_length_items(&mut self) {
        self.items.retain(|it| it.duration() > 0.0);
    }

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
}
