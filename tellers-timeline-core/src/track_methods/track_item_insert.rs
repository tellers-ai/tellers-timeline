use crate::{Item, Seconds, Track};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlapPolicy {
    Override,
    Push,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertPolicy {
    /// If inserting inside a gap, split the gap into left-gap/item/right-gap.
    /// If inside a non-gap item, falls back to BeforeOrAfter.
    SplitAndInsert,
    /// If inserting inside an item, adjust start to the item's start.
    InsertBefore,
    /// If inserting inside an item, adjust start to the item's end.
    InsertAfter,
    /// If inserting inside an item, choose the closer boundary (start or end).
    InsertBeforeOrAfter,
}

impl Track {
    pub fn append(&mut self, item: Item) {
        self.items.push(item);
    }

    pub fn insert_at_index(&mut self, index: usize, item: Item, overlap_policy: OverlapPolicy) {
        if overlap_policy == OverlapPolicy::Push {
            self.insert_and_push(index, item);
            return;
        }

        self.insert_and_override(index, item);
    }

    pub fn insert_and_push(&mut self, index: usize, item: Item) {
        self.items.insert(index, item);
    }

    pub fn insert_and_override(&mut self, index: usize, item: Item) {
        const EPS: Seconds = 1e-9;

        let mut insert_index = index.min(self.items.len());
        let insert_start = self.start_time_of_item(insert_index);
        let insert_end = insert_start + item.duration().max(0.0);

        if item.duration() <= EPS {
            self.items.insert(insert_index, item);
            self.sanitize();
            return;
        }

        // If the insertion start falls within an item at insert_index, split at start
        if let Some(containing_idx) = self.get_item_at_time(insert_start) {
            if containing_idx <= insert_index {
                self.split_at_time(insert_start);
                // After split, the right piece is at containing_idx + 1; our insertion point is after the left piece
                if insert_index <= containing_idx {
                    insert_index = containing_idx + 1;
                } else {
                    insert_index += 1;
                }
            }
        }

        // Split at end boundary if it falls inside an item
        if self.get_item_at_time(insert_end).is_some() {
            self.split_at_time(insert_end);
        }

        // Remove any items that start before insert_end and at/after insert_start
        let cur_index = insert_index;
        while cur_index < self.items.len() {
            let cur_start = self.start_time_of_item(cur_index);
            if cur_start < insert_end - EPS {
                self.items.remove(cur_index);
            } else {
                break;
            }
        }

        self.items.insert(insert_index, item);
        self.sanitize();
    }

    /// Insert an item at a timeline time, controlling how overlaps are handled
    /// and how to place the item relative to neighbors.
    pub fn insert_at_time(
        &mut self,
        insert_time: Seconds,
        item: Item,
        overlap_policy: OverlapPolicy,
        insert_policy: InsertPolicy,
    ) {
        let mut effective_insert_time = insert_time;
        let total_track_duration = self.total_duration();

        if effective_insert_time < 0.0 {
            effective_insert_time = total_track_duration - effective_insert_time;
        }

        if effective_insert_time < 0.0 {
            panic!(
                "Negative insert start time ({}) is bigger than track duration ({})",
                insert_time, total_track_duration
            );
        }

        if effective_insert_time > total_track_duration {
            let gap_duration: Seconds = (effective_insert_time - total_track_duration).max(0.0);
            self.items
                .push(Item::Gap(crate::types::Gap::make_gap(gap_duration)));
            self.items.push(item);
            self.sanitize();
            return;
        }

        let containing_index = self.get_item_at_time(effective_insert_time);

        let insert_index = self.get_insertion_index(effective_insert_time, insert_policy);

        if let (InsertPolicy::SplitAndInsert, Some(_i)) = (insert_policy, containing_index) {
            // Create the boundary at the insertion time before inserting.
            self.split_at_time(effective_insert_time);
        }

        self.insert_at_index(insert_index, item, overlap_policy);
    }

    /// Compute the insertion index according to the policy without.
    fn get_insertion_index(&self, t: Seconds, policy: InsertPolicy) -> usize {
        let i = self.get_item_at_time(t).unwrap_or(self.items.len());

        match policy {
            InsertPolicy::InsertBefore => i,
            InsertPolicy::InsertAfter => i + 1,
            InsertPolicy::InsertBeforeOrAfter => {
                let start = self.start_time_of_item(i);
                let end = start + self.items[i].duration().max(0.0);
                let d_start = (t - start).abs();
                let d_end = (end - t).abs();
                if d_start <= d_end {
                    i
                } else {
                    i + 1
                }
            }
            InsertPolicy::SplitAndInsert => i + 1,
        }
    }
}
