use crate::{Item, Seconds, Track};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlapPolicy {
    Override,
    Keep,
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

// no SplitMode needed; callers adjust right piece media_start after split

impl Track {
    pub fn append(&mut self, item: Item) {
        self.items.push(item);
    }

    pub fn insert_at_index(&mut self, index: usize, item: Item) {
        let idx = index.min(self.items.len());
        self.items.insert(idx, item);
    }

    /// Insert an item at a timeline time, controlling how overlaps are handled
    /// and how to place the item relative to neighbors.
    pub fn insert_at_time_with(
        &mut self,
        start_time: Seconds,
        item: Item,
        overlap_policy: OverlapPolicy,
        insert_policy: InsertPolicy,
    ) {
        let duration = item.duration();
        let containing_index = self.find_containing_index(start_time);
        let effective_start = self.compute_effective_start(start_time, containing_index, insert_policy);

        match overlap_policy {
            OverlapPolicy::Keep => {
                self.do_boundary_insert(effective_start, containing_index, insert_policy, item);
            }
            OverlapPolicy::Override => {
                if matches!(insert_policy, InsertPolicy::SplitAndInsert) {
                    self.do_override_split_and_insert(effective_start, duration, item);
                } else {
                    self.do_override_insert(effective_start, duration, item);
                }
            }
            OverlapPolicy::Push => {
                // Push semantics are equivalent to placing the item at that time in a sequential model
                // because subsequent items will naturally be shifted later by the inserted duration.
                if matches!(insert_policy, InsertPolicy::SplitAndInsert) {
                    self.do_push_split_and_insert(effective_start, duration, item);
                } else {
                    self.do_place_insert(effective_start, item);
                }
            }
        }
    }

    fn find_containing_index(&self, time: Seconds) -> Option<usize> {
        let mut pos: Seconds = 0.0;
        for (i, it) in self.items.iter().enumerate() {
            let end = pos + it.duration().max(0.0);
            if time > pos && time < end {
                return Some(i);
            }
            pos = end;
        }
        None
    }

    fn compute_effective_start(
        &self,
        requested_start: Seconds,
        containing_index: Option<usize>,
        insert_policy: InsertPolicy,
    ) -> Seconds {
        let mut effective_start = requested_start;
        if let Some(idx) = containing_index {
            let ex_start = self.start_time_of_item(idx);
            let ex_end = ex_start + self.items[idx].duration().max(0.0);
            match insert_policy {
                InsertPolicy::InsertBefore => effective_start = ex_start,
                InsertPolicy::InsertAfter => effective_start = ex_end,
                InsertPolicy::InsertBeforeOrAfter => {
                    let dist_to_start = (requested_start - ex_start).abs();
                    let dist_to_end = (ex_end - requested_start).abs();
                    effective_start = if dist_to_start <= dist_to_end { ex_start } else { ex_end };
                }
                InsertPolicy::SplitAndInsert => {
                    // Always use the requested start for explicit split insert
                    effective_start = requested_start;
                }
            }
        }
        effective_start
    }

    fn add_trailing_gap_if_needed(&mut self, insert_start: Seconds) {
        let track_end = self.total_duration();
        if insert_start > track_end + 1e-9 {
            let gap_dur = (insert_start - track_end).max(0.0);
            self.items.push(make_gap(gap_dur));
        }
    }

    // Insert item at a boundary without splitting the containing element.
    fn do_boundary_insert(
        &mut self,
        requested_start: Seconds,
        containing_index: Option<usize>,
        insert_policy: InsertPolicy,
        item: Item,
    ) {
        if let Some(idx) = containing_index {
            // Choose before/after boundary
            let ex_start = self.start_time_of_item(idx);
            let ex_end = ex_start + self.items[idx].duration().max(0.0);
            let insert_after = match insert_policy {
                InsertPolicy::InsertBefore => false,
                InsertPolicy::InsertAfter => true,
                InsertPolicy::InsertBeforeOrAfter => {
                    let dist_to_start = (requested_start - ex_start).abs();
                    let dist_to_end = (ex_end - requested_start).abs();
                    dist_to_end < dist_to_start
                }
                InsertPolicy::SplitAndInsert => true,
            };
            let insert_idx = if insert_after { idx + 1 } else { idx };
            self.items.insert(insert_idx.min(self.items.len()), item);
            return;
        }
        // Not contained: insert at an existing boundary, or after end
        let mut acc: Seconds = 0.0;
        for (i, it) in self.items.iter().enumerate() {
            if (requested_start - acc).abs() < 1e-9 {
                self.items.insert(i, item);
                return;
            }
            acc += it.duration().max(0.0);
        }
        // After end
        self.add_trailing_gap_if_needed(requested_start);
        self.items.push(item);
    }

    // Place: split containing element (gap or clip) at `start` and insert `item` between left/right.
    fn do_place_insert(&mut self, start: Seconds, item: Item) {
        // Find if inside an item
        if let Some(idx) = self.find_containing_index(start) {
            let seg_start = self.start_time_of_item(idx);
            let seg_dur = self.items[idx].duration().max(0.0);
            let offset = (start - seg_start).max(0.0);
            let mut new_items: Vec<Item> = Vec::with_capacity(self.items.len() + 2);
            for (i, existing) in self.items.drain(..).enumerate() {
                if i != idx {
                    new_items.push(existing);
                    continue;
                }
                match existing {
                    Item::Gap(_) => {
                        if offset > 0.0 { new_items.push(make_gap(offset)); }
                        new_items.push(item.clone());
                        let right_dur = (seg_dur - offset).max(0.0);
                        if right_dur > 0.0 { new_items.push(make_gap(right_dur)); }
                    }
                    Item::Clip(c) => {
                        if offset > 0.0 {
                            let mut left = c.clone();
                            left.duration = offset;
                            new_items.push(Item::Clip(left));
                        }
                        new_items.push(item.clone());
                        let right_dur = (seg_dur - offset).max(0.0);
                        if right_dur > 0.0 {
                            let mut right = c.clone();
                            right.duration = right_dur;
                            right.source.media_start = c.source.media_start + offset;
                            new_items.push(Item::Clip(right));
                        }
                    }
                }
            }
            self.items = new_items;
            return;
        }
        // Not inside an item: insert at boundary or after end
        let mut acc: Seconds = 0.0;
        for (i, it) in self.items.iter().enumerate() {
            if (start - acc).abs() < 1e-9 { self.items.insert(i, item.clone()); return; }
            acc += it.duration().max(0.0);
        }
        // After end
        self.add_trailing_gap_if_needed(start);
        self.items.push(item);
    }

    // Split the item containing [start, end) into left/right pieces and replace it in-place.
    // Returns (original_end_bound, original_item_start_if_clip) on success.
    fn split_containing_item(&mut self, start: Seconds, end: Seconds) -> Option<(Seconds, Option<Seconds>)> {
        let idx = self.find_containing_index(start)?;
        let seg_start = self.start_time_of_item(idx);
        let seg_dur = self.items[idx].duration().max(0.0);
        let seg_end = seg_start + seg_dur;
        if !(start >= seg_start && end <= seg_end) {
            return None;
        }
        let mut new_items: Vec<Item> = Vec::with_capacity(self.items.len() + 2);
        let mut original_clip_start: Option<Seconds> = None;
        for (i, existing) in self.items.drain(..).enumerate() {
            if i != idx {
                new_items.push(existing);
                continue;
            }
            match existing {
                Item::Gap(_) => {
                    let left_dur = (start - seg_start).max(0.0);
                    if left_dur > 0.0 { new_items.push(make_gap(left_dur)); }
                    let right_dur = (seg_end - end).max(0.0);
                    if right_dur > 0.0 { new_items.push(make_gap(right_dur)); }
                }
                Item::Clip(c) => {
                    original_clip_start = Some(seg_start);
                    let left_dur = (start - seg_start).max(0.0);
                    if left_dur > 0.0 {
                        let mut left = c.clone();
                        left.duration = left_dur;
                        new_items.push(Item::Clip(left));
                    }
                    let right_dur = (seg_end - end).max(0.0);
                    if right_dur > 0.0 {
                        let mut right = c.clone();
                        right.duration = right_dur;
                        // Right piece begins at `end` relative to original clip start
                        right.source.media_start = c.source.media_start + (end - seg_start).max(0.0);
                        new_items.push(Item::Clip(right));
                    }
                }
            }
        }
        self.items = new_items;
        Some((seg_end, original_clip_start))
    }

    fn do_override_insert(&mut self, new_start: Seconds, duration: Seconds, item: Item) {
        let new_end = new_start + duration.max(0.0);
        let original_len = self.items.len();
        let mut new_items: Vec<Item> = Vec::with_capacity(original_len + 2);
        let mut pos: Seconds = 0.0;
        for existing in self.items.drain(..) {
            let ex_dur = existing.duration().max(0.0);
            let ex_start = pos;
            let ex_end = pos + ex_dur;
            if ex_end <= new_start {
                pos = ex_end;
                new_items.push(existing);
                continue;
            }
            if ex_start >= new_end {
                // Insert before this item if not inserted yet
                pos = ex_end;
                new_items.push(existing);
                continue;
            }
            // Overlap: keep left and/or right parts
            if ex_start < new_start {
                let left_dur = (new_start - ex_start).max(0.0);
                if left_dur > 0.0 {
                    match &existing {
                        Item::Gap(_) => new_items.push(make_gap(left_dur)),
                        Item::Clip(c) => {
                            let mut left = c.clone();
                            left.duration = left_dur;
                            new_items.push(Item::Clip(left));
                        }
                    }
                }
            }
            if ex_end > new_end {
                let right_dur = (ex_end - new_end).max(0.0);
                if right_dur > 0.0 {
                    match existing {
                        Item::Gap(_) => new_items.push(make_gap(right_dur)),
                        Item::Clip(mut c) => {
                            // Right piece's media starts at offset new_end - ex_start
                            c.source.media_start = c.source.media_start + (new_end - ex_start).max(0.0);
                            c.duration = right_dur;
                            new_items.push(Item::Clip(c));
                        }
                    }
                }
            }
            pos = ex_end;
        }
        // Add trailing gap if inserting beyond end
        let current_end: Seconds = new_items.iter().map(|it| it.duration().max(0.0)).sum();
        if new_start > current_end + 1e-9 {
            new_items.push(make_gap((new_start - current_end).max(0.0)));
        }
        // Insert the new item at the correct index
        let mut acc: Seconds = 0.0;
        let mut insert_idx = new_items.len();
        for (i, it) in new_items.iter().enumerate() {
            if new_start <= acc + 1e-9 { insert_idx = i; break; }
            acc += it.duration().max(0.0);
        }
        if insert_idx >= new_items.len() { new_items.push(item); } else { new_items.insert(insert_idx, item); }
        self.items = new_items;
    }

    // fn do_push_insert(&mut self, split_time: Seconds, shift_by: Seconds, item: Item) { /* unused */ }

    // Override + SplitAndInsert: split containing item, remove middle region globally, insert item
    fn do_override_split_and_insert(&mut self, start: Seconds, duration: Seconds, item: Item) {
        let end = start + duration.max(0.0);
        if let Some((_original_end, _original_clip_start)) = self.split_containing_item(start, end) {
            // Insert the new item at the boundary `start`
            let mut acc: Seconds = 0.0;
            let mut insert_idx = self.items.len();
            for (i, it) in self.items.iter().enumerate() {
                if (start - acc).abs() < 1e-9 { insert_idx = i; break; }
                acc += it.duration().max(0.0);
            }
            if insert_idx > self.items.len() { insert_idx = self.items.len(); }
            self.items.insert(insert_idx, item);
            return;
        }
        // Fallback: same as override insert
        self.do_override_insert(start, duration, item);
    }

    // Push + SplitAndInsert: split the containing item and insert; shift subsequent items to the right by duration
    fn do_push_split_and_insert(&mut self, start: Seconds, duration: Seconds, item: Item) {
        let end = start + duration.max(0.0);
        if let Some((original_end_bound, _original_clip_start)) = self.split_containing_item(start, end) {
            // Insert the new item at boundary `start`
            let mut acc: Seconds = 0.0;
            let mut insert_idx = self.items.len();
            for (i, it) in self.items.iter().enumerate() {
                if (start - acc).abs() < 1e-9 { insert_idx = i; break; }
                acc += it.duration().max(0.0);
            }
            if insert_idx > self.items.len() { insert_idx = self.items.len(); }
            self.items.insert(insert_idx, item);
            // Shift subsequent items at/after original end bound by inserting a gap there
            let mut acc2: Seconds = 0.0;
            let mut gap_idx = self.items.len();
            for (i, it) in self.items.iter().enumerate() {
                if acc2 >= original_end_bound - 1e-9 { gap_idx = i; break; }
                acc2 += it.duration().max(0.0);
            }
            if gap_idx > self.items.len() { gap_idx = self.items.len(); }
            self.items.insert(gap_idx, make_gap(duration.max(0.0)));
            #[cfg(test)]
            {
                eprintln!(
                    "after push split: {:?}",
                    self.items
                        .iter()
                        .map(|it| match it { Item::Gap(g) => format!("gap({})", g.duration), Item::Clip(c) => format!("clip({})", c.duration) })
                        .collect::<Vec<_>>()
                );
            }
            return;
        }
        // Fallback: same as place insert if not inside an item
        self.do_place_insert(start, item);
    }
}

/// Helper to create a gap item
pub fn make_gap(duration: Seconds) -> Item {
    Item::Gap(crate::types::Gap { otio_schema: "Gap.1".to_string(), duration, metadata: serde_json::Value::Object(serde_json::Map::new()) })
}
