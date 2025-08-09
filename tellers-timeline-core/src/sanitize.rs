use crate::{Item, Seconds, Timeline, Track};

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
        self.sort_children_by_start();
        self.merge_adjacent_gaps();
        self.ensure_non_overlap();
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

    pub fn sort_children_by_start(&mut self) {
        self.items
            .sort_by(|a, b| a.start().partial_cmp(&b.start()).unwrap_or(std::cmp::Ordering::Equal));
    }

    pub fn merge_adjacent_gaps(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let mut merged: Vec<Item> = Vec::with_capacity(self.items.len());
        for item in self.items.drain(..) {
            match (merged.last_mut(), &item) {
                (Some(Item::Gap(prev)), Item::Gap(next)) => {
                    let prev_end = prev.start + prev.duration;
                    if (prev_end - next.start).abs() < 1e-9 {
                        prev.duration += next.duration;
                    } else {
                        merged.push(item);
                    }
                }
                _ => merged.push(item),
            }
        }
        self.items = merged;
    }

    pub fn ensure_non_overlap(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.sort_children_by_start();
        let mut last_end: Seconds = f64::NEG_INFINITY;
        for it in &mut self.items {
            let start = it.start();
            let duration = it.duration();
            let mut new_start = start;
            if start < last_end {
                new_start = last_end;
                it.set_start(new_start);
            }
            let end = new_start + duration.max(0.0);
            last_end = end;
        }
    }
}
