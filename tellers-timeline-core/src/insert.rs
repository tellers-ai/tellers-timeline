use crate::{Item, Seconds, Track};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverridePolicy {
    Override,
    Keep,
}

impl Track {
    pub fn append(&mut self, item: Item) {
        self.items.push(item);
    }

    pub fn insert_at_index(&mut self, index: usize, item: Item) {
        let idx = index.min(self.items.len());
        self.items.insert(idx, item);
    }

    /// Insert an item at a timeline time. If `policy` is Override, any overlapping
    /// items will be trimmed or removed to avoid overlap.
    pub fn insert_at_time(&mut self, start_time: Seconds, mut item: Item, policy: OverridePolicy) {
        let duration = item.duration();
        item.set_start(start_time);
        item.set_duration(duration);

        if policy == OverridePolicy::Override {
            let new_start = start_time;
            let new_end = start_time + duration.max(0.0);
            let mut new_items: Vec<Item> = Vec::with_capacity(self.items.len() + 1);
            for existing in self.items.drain(..) {
                let ex_start = existing.start();
                let ex_end = existing.start() + existing.duration().max(0.0);

                // Completely before
                if ex_end <= new_start {
                    new_items.push(existing);
                    continue;
                }
                // Completely after
                if ex_start >= new_end {
                    new_items.push(existing);
                    continue;
                }
                // Overlaps: consider trimming leading and trailing pieces
                if ex_start < new_start && ex_end > new_start {
                    // keep leading
                    let mut left = existing.clone();
                    left.set_duration((new_start - ex_start).max(0.0));
                    new_items.push(left);
                }
                if ex_end > new_end && ex_start < new_end {
                    // keep trailing
                    let mut right = existing;
                    right.set_start(new_end);
                    right.set_duration((ex_end - new_end).max(0.0));
                    new_items.push(right);
                }
            }
            // insert the new item, then sort by start
            new_items.push(item);
            new_items.sort_by(|a, b| a.start().partial_cmp(&b.start()).unwrap());
            self.items = new_items;
        } else {
            // Keep existing: just insert and resort, may overlap until sanitized
            self.items.push(item);
            self.items.sort_by(|a, b| a.start().partial_cmp(&b.start()).unwrap());
        }
    }
}

/// Helper to create a gap item
pub fn make_gap(start: Seconds, duration: Seconds) -> Item {
    Item::Gap(crate::types::Gap { otio_schema: "Gap.1".to_string(), start, duration, metadata: serde_json::Value::Object(serde_json::Map::new()) })
}
