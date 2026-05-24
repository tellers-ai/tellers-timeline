use crate::{Item, Seconds, Stack};
use std::collections::HashSet;

const EPS: Seconds = super::EPS;

impl Stack {
    pub fn unlink_item(&mut self, item_ids: &[String]) -> usize {
        let mut targets = Vec::new();
        let mut seen_targets = HashSet::new();
        let mut touched_link_groups = Vec::new();

        for item_id in item_ids {
            let Some((track_index, item_index)) = self.clip_target(item_id) else {
                continue;
            };
            if !seen_targets.insert((track_index, item_index)) {
                continue;
            }
            if let Item::Clip(clip) = &self.children[track_index].items[item_index] {
                if let Some(link_group_id) = super::resolve_link_group_id(&clip.metadata) {
                    touched_link_groups.push(link_group_id);
                    targets.push((track_index, item_index));
                }
            }
        }

        let mut count = 0;
        for (track_index, item_index) in targets {
            let Some(Item::Clip(clip)) = self
                .children
                .get_mut(track_index)
                .and_then(|track| track.items.get_mut(item_index))
            else {
                continue;
            };
            if super::remove_resolve_link_group_id(&mut clip.metadata) {
                count += 1;
            }
        }
        count += self.cleanup_singleton_link_groups(&touched_link_groups);
        count
    }

    pub fn link_item(&mut self, item_ids: &[String]) -> Option<i64> {
        let mut targets = Vec::new();
        let mut seen_targets = HashSet::new();
        for item_id in item_ids {
            let target = self.clip_target(item_id)?;
            if seen_targets.insert(target) {
                targets.push(target);
            }
        }
        if targets.len() < 2 {
            return None;
        }

        let (first_track_index, first_item_index) = targets[0];
        let first_start = self.children[first_track_index].start_time_of_item(first_item_index);
        let first_duration = self.children[first_track_index].items[first_item_index].duration();
        for (track_index, item_index) in targets.iter().skip(1) {
            let start = self.children[*track_index].start_time_of_item(*item_index);
            let duration = self.children[*track_index].items[*item_index].duration();
            if (start - first_start).abs() > EPS || (duration - first_duration).abs() > EPS {
                return None;
            }
        }

        let backup = self.clone();
        let mut touched_link_groups = Vec::new();
        for (track_index, item_index) in &targets {
            let Some(Item::Clip(clip)) = self
                .children
                .get_mut(*track_index)
                .and_then(|track| track.items.get_mut(*item_index))
            else {
                *self = backup;
                return None;
            };
            if let Some(link_group_id) = super::resolve_link_group_id(&clip.metadata) {
                touched_link_groups.push(link_group_id);
                super::remove_resolve_link_group_id(&mut clip.metadata);
            }
        }
        self.cleanup_singleton_link_groups(&touched_link_groups);

        let link_group_id = self.next_link_group_id();
        for (track_index, item_index) in targets {
            let Some(Item::Clip(clip)) = self
                .children
                .get_mut(track_index)
                .and_then(|track| track.items.get_mut(item_index))
            else {
                *self = backup;
                return None;
            };
            super::set_resolve_link_group_id(&mut clip.metadata, link_group_id);
        }

        Some(link_group_id)
    }
}
