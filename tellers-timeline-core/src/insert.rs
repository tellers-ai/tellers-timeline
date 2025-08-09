use crate::{Item, Seconds, Track};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverridePolicy {
    Override,
    Naive,
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
        mut item: Item,
        overlap_policy: OverridePolicy,
        insert_policy: InsertPolicy,
    ) {
        let duration = item.duration();
        let containing_index = self.find_containing_index(start_time);
        let effective_start = self.compute_effective_start(start_time, containing_index, insert_policy);

        item.set_start(effective_start);
        item.set_duration(duration);

        match overlap_policy {
            OverridePolicy::Naive | OverridePolicy::Keep => {
                // Do not split; but place at effective_start computed from insert policy
                self.add_trailing_gap_if_needed(effective_start);
                self.items.push(item);
                self.sort_children_by_start();
            }
            OverridePolicy::Override => {
                if matches!(insert_policy, InsertPolicy::SplitAndInsert) {
                    self.do_override_split_and_insert(effective_start, duration, item);
                } else {
                    self.do_override_insert(effective_start, duration, item);
                }
            }
            OverridePolicy::Push => {
                if matches!(insert_policy, InsertPolicy::SplitAndInsert) {
                    self.do_push_split_and_insert(effective_start, duration, item);
                } else {
                    self.do_push_insert(effective_start, duration, item);
                }
            }
        }
    }

    fn find_containing_index(&self, time: Seconds) -> Option<usize> {
        self.items
            .iter()
            .position(|it| it.start() < time && (it.start() + it.duration().max(0.0)) > time)
    }

    fn compute_effective_start(
        &self,
        requested_start: Seconds,
        containing_index: Option<usize>,
        insert_policy: InsertPolicy,
    ) -> Seconds {
        let mut effective_start = requested_start;
        if let Some(idx) = containing_index {
            let ex = &self.items[idx];
            let ex_start = ex.start();
            let ex_end = ex.start() + ex.duration().max(0.0);
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
        if self.items.is_empty() {
            return;
        }
        let track_end = self
            .items
            .iter()
            .map(|it| it.start() + it.duration().max(0.0))
            .fold(f64::NEG_INFINITY, f64::max);
        if insert_start > track_end + 1e-9 {
            let gap_dur = (insert_start - track_end).max(0.0);
            self.items.push(make_gap(track_end, gap_dur));
        }
    }

    // Split the item containing [start, end) into left/right pieces and replace it in-place.
    // Returns (original_end_bound, original_clip_start_if_clip) on success.
    fn split_containing_item(&mut self, start: Seconds, end: Seconds) -> Option<(Seconds, Option<Seconds>)> {
        let idx = self.find_containing_index(start)?;
        let ins_start = start;
        let ins_end = end;
        let mut new_items: Vec<Item> = Vec::with_capacity(self.items.len() + 2);
        let original_end_bound: Seconds;
        let mut original_clip_start: Option<Seconds> = None;
        match &self.items[idx] {
            Item::Gap(g) => {
                let gap_start = g.start;
                let gap_end = g.start + g.duration.max(0.0);
                if !(ins_start >= gap_start && ins_end <= gap_end) {
                    return None;
                }
                original_end_bound = gap_end;
                for (i, existing) in self.items.drain(..).enumerate() {
                    if i != idx {
                        new_items.push(existing);
                        continue;
                    }
                    let left_dur = (ins_start - gap_start).max(0.0);
                    if left_dur > 0.0 { new_items.push(make_gap(gap_start, left_dur)); }
                    // middle is removed; inserted item will be added by caller
                    let right_dur = (gap_end - ins_end).max(0.0);
                    if right_dur > 0.0 { new_items.push(make_gap(ins_end, right_dur)); }
                }
            }
            Item::Clip(c) => {
                let clip_start = c.start;
                let clip_end = c.start + c.duration.max(0.0);
                if !(ins_start >= clip_start && ins_end <= clip_end) {
                    return None;
                }
                original_end_bound = clip_end;
                original_clip_start = Some(clip_start);
                for (i, existing) in self.items.drain(..).enumerate() {
                    if i != idx {
                        new_items.push(existing);
                        continue;
                    }
                    let left_dur = (ins_start - clip_start).max(0.0);
                    if left_dur > 0.0 {
                        if let Item::Clip(orig) = &existing {
                            let mut left = orig.clone();
                            left.duration = left_dur;
                            new_items.push(Item::Clip(left));
                        }
                    }
                    let right_dur = (clip_end - ins_end).max(0.0);
                    if right_dur > 0.0 {
                        if let Item::Clip(orig) = &existing {
                            let mut right = orig.clone();
                            right.start = ins_end;
                            right.duration = right_dur;
                            // Do not adjust media_start here; callers will adjust as needed
                            new_items.push(Item::Clip(right));
                        }
                    }
                }
            }
        }
        new_items.sort_by(|a, b| a.start().partial_cmp(&b.start()).unwrap());
        self.items = new_items;
        Some((original_end_bound, original_clip_start))
    }

    fn do_override_insert(&mut self, new_start: Seconds, duration: Seconds, item: Item) {
        let new_end = new_start + duration.max(0.0);
        let original_len = self.items.len();
        let mut new_items: Vec<Item> = Vec::with_capacity(original_len + 2);
        for existing in self.items.drain(..) {
            let ex_start = existing.start();
            let ex_end = existing.start() + existing.duration().max(0.0);
            if ex_end <= new_start {
                new_items.push(existing);
                continue;
            }
            if ex_start >= new_end {
                new_items.push(existing);
                continue;
            }
            if ex_start < new_start && ex_end > new_start {
                let mut left = existing.clone();
                left.set_duration((new_start - ex_start).max(0.0));
                new_items.push(left);
            }
            if ex_end > new_end && ex_start < new_end {
                let mut right = existing;
                right.set_start(new_end);
                right.set_duration((ex_end - new_end).max(0.0));
                new_items.push(right);
            }
        }
        if original_len > 0 {
            let track_end = new_items
                .iter()
                .map(|it| it.start() + it.duration().max(0.0))
                .fold(f64::NEG_INFINITY, f64::max);
            if new_start > track_end + 1e-9 {
                let gap_dur = (new_start - track_end).max(0.0);
                new_items.push(make_gap(track_end, gap_dur));
            }
        }
        new_items.push(item);
        new_items.sort_by(|a, b| a.start().partial_cmp(&b.start()).unwrap());
        self.items = new_items;
    }

    fn do_push_insert(&mut self, split_time: Seconds, shift_by: Seconds, item: Item) {
        let original_len = self.items.len();
        let mut new_items: Vec<Item> = Vec::with_capacity(original_len + 2);
        for existing in self.items.drain(..) {
            let ex_start = existing.start();
            let ex_end = existing.start() + existing.duration().max(0.0);
            if ex_end <= split_time {
                new_items.push(existing);
            } else if ex_start >= split_time {
                let mut shifted = existing.clone();
                shifted.set_start(ex_start + shift_by);
                new_items.push(shifted);
            } else {
                match existing {
                    Item::Gap(g) => {
                        let left_dur = (split_time - g.start).max(0.0);
                        if left_dur > 0.0 {
                            new_items.push(make_gap(g.start, left_dur));
                        }
                        let right_dur = (g.start + g.duration.max(0.0) - split_time).max(0.0);
                        if right_dur > 0.0 {
                            new_items.push(make_gap(split_time + shift_by, right_dur));
                        }
                    }
                    Item::Clip(c) => {
                        let left_dur = (split_time - c.start).max(0.0);
                        if left_dur > 0.0 {
                            let mut left = c.clone();
                            left.duration = left_dur;
                            new_items.push(Item::Clip(left));
                        }
                        let right_dur = (c.start + c.duration.max(0.0) - split_time).max(0.0);
                        if right_dur > 0.0 {
                            let mut right = c.clone();
                            right.start = split_time + shift_by;
                            right.duration = right_dur;
                            right.source.media_start = c.source.media_start + left_dur;
                            new_items.push(Item::Clip(right));
                        }
                    }
                }
            }
        }
        if original_len > 0 {
            let track_end = new_items
                .iter()
                .map(|it| it.start() + it.duration().max(0.0))
                .fold(f64::NEG_INFINITY, f64::max);
            if split_time > track_end + 1e-9 {
                let gap_dur = (split_time - track_end).max(0.0);
                new_items.push(make_gap(track_end, gap_dur));
            }
        }
        new_items.push(item);
        new_items.sort_by(|a, b| a.start().partial_cmp(&b.start()).unwrap());
        self.items = new_items;
    }

    // Override + SplitAndInsert: split containing item, remove middle region globally, insert item
    fn do_override_split_and_insert(&mut self, start: Seconds, duration: Seconds, item: Item) {
        let end = start + duration.max(0.0);
        if let Some((_original_end, original_clip_start)) = self.split_containing_item(start, end) {
            // Remove any items overlapping [start, end)
            self.items.retain(|it| {
                let s = it.start();
                let e = it.start() + it.duration().max(0.0);
                e <= start || s >= end
            });
            // If the right piece is a clip starting at `end`, adjust its media_start by consumed = end - clip_start
            if let Some(clip_start) = original_clip_start {
                let consumed = (end - clip_start).max(0.0);
                for it in &mut self.items {
                    if let Item::Clip(c) = it {
                        if (c.start - end).abs() < 1e-9 {
                            c.source.media_start += consumed;
                            break;
                        }
                    }
                }
            }
            // Insert the new item
            self.items.push(item);
            self.sort_children_by_start();
            return;
        }
        // Fallback: same as override insert
        self.do_override_insert(start, duration, item);
    }

    // Push + SplitAndInsert: split the containing item and insert; shift subsequent items to the right by duration
    fn do_push_split_and_insert(&mut self, start: Seconds, duration: Seconds, item: Item) {
        let end = start + duration.max(0.0);
        if let Some((original_end_bound, original_clip_start)) = self.split_containing_item(start, end) {
            // Insert the new item between left and right pieces
            self.items.push(item);
            // If the right piece is a clip starting at `end`, adjust its media_start by left_dur = start - clip_start
            if let Some(clip_start) = original_clip_start {
                let left_dur = (start - clip_start).max(0.0);
                for it in &mut self.items {
                    if let Item::Clip(c) = it {
                        if (c.start - end).abs() < 1e-9 {
                            c.source.media_start += left_dur;
                            break;
                        }
                    }
                }
            }
            // Shift items whose start was at/after the original containing end bound
            for it in &mut self.items {
                let s = it.start();
                if s >= original_end_bound {
                    it.set_start(s + duration.max(0.0));
                }
            }
            self.sort_children_by_start();
            return;
        }
        // Fallback to push insert if not inside an item
        self.do_push_insert(start, duration, item);
    }
}

/// Helper to create a gap item
pub fn make_gap(start: Seconds, duration: Seconds) -> Item {
    Item::Gap(crate::types::Gap { otio_schema: "Gap.1".to_string(), start, duration, metadata: serde_json::Value::Object(serde_json::Map::new()) })
}
