use crate::{Item, Stack};
use std::collections::HashSet;

impl Stack {
    pub fn unsync_item(&mut self, item_ids: &[String]) -> usize {
        let mut targets = Vec::new();
        let mut seen_targets = HashSet::new();
        let mut touched_sync_clips = Vec::new();

        for item_id in item_ids {
            let Some((track_index, item_index)) = self.clip_target(item_id) else {
                continue;
            };
            if !seen_targets.insert((track_index, item_index)) {
                continue;
            }
            if let Item::Clip(clip) = &self.children[track_index].items[item_index] {
                if let Some(sync_clips_id) = super::resolve_sync_clips_id(&clip.metadata) {
                    touched_sync_clips.push(sync_clips_id);
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
            if super::remove_resolve_sync_clips_id(&mut clip.metadata) {
                count += 1;
            }
        }
        count += self.cleanup_singleton_sync_clips(&touched_sync_clips);
        count
    }

    /// Group the given clips together under a fresh Tellers group id. Each
    /// clip's sync partners (Resolve "Link Group ID") are pulled into the group
    /// as well, so a group always contains whole sync columns. Any prior group
    /// membership of the selected clips is replaced. Returns the new group id,
    /// or `None` when fewer than two clips would be grouped.
    pub fn group_item(&mut self, item_ids: &[String]) -> Option<i64> {
        let mut targets = Vec::new();
        let mut seen_targets = HashSet::new();
        for item_id in item_ids {
            let Some(target) = self.clip_target(item_id) else {
                continue;
            };
            if seen_targets.insert(target) {
                targets.push(target);
            }
            if let Item::Clip(clip) = &self.children[target.0].items[target.1] {
                if let Some(sync_clips_id) = super::resolve_sync_clips_id(&clip.metadata) {
                    for partner in self.synced_clips_targets(sync_clips_id) {
                        if seen_targets.insert(partner) {
                            targets.push(partner);
                        }
                    }
                }
            }
        }
        if targets.len() < 2 {
            return None;
        }

        let group_id = self.next_tellers_group_id();
        for (track_index, item_index) in targets {
            let Some(Item::Clip(clip)) = self
                .children
                .get_mut(track_index)
                .and_then(|track| track.items.get_mut(item_index))
            else {
                continue;
            };
            crate::set_tellers_group_id(&mut clip.metadata, group_id);
        }
        Some(group_id)
    }

    /// Ungroup the whole Tellers group(s) that the given clips belong to. The
    /// group id is removed from every member, not just the clips passed in.
    /// Sync (Link Group ID) membership is left untouched. Returns the number of
    /// clips that had a group id removed.
    pub fn ungroup_item(&mut self, item_ids: &[String]) -> usize {
        let mut group_ids = HashSet::new();
        for item_id in item_ids {
            let Some((track_index, item_index)) = self.clip_target(item_id) else {
                continue;
            };
            if let Item::Clip(clip) = &self.children[track_index].items[item_index] {
                if let Some(group_id) = crate::resolve_tellers_group_id(&clip.metadata) {
                    group_ids.insert(group_id);
                }
            }
        }

        let mut count = 0;
        for group_id in group_ids {
            for (track_index, item_index) in self.tellers_group_targets(group_id) {
                let Some(Item::Clip(clip)) = self
                    .children
                    .get_mut(track_index)
                    .and_then(|track| track.items.get_mut(item_index))
                else {
                    continue;
                };
                if crate::remove_tellers_group_id(&mut clip.metadata) {
                    count += 1;
                }
            }
        }
        count
    }

    pub fn sync_item(&mut self, item_ids: &[String]) -> Option<i64> {
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

        let backup = self.clone();
        let mut touched_sync_clips = Vec::new();
        for (track_index, item_index) in &targets {
            let Some(Item::Clip(clip)) = self
                .children
                .get_mut(*track_index)
                .and_then(|track| track.items.get_mut(*item_index))
            else {
                *self = backup;
                return None;
            };
            if let Some(sync_clips_id) = super::resolve_sync_clips_id(&clip.metadata) {
                touched_sync_clips.push(sync_clips_id);
                super::remove_resolve_sync_clips_id(&mut clip.metadata);
            }
        }
        self.cleanup_singleton_sync_clips(&touched_sync_clips);

        let sync_clips_id = self.next_sync_clips_id();
        for (track_index, item_index) in targets {
            let Some(Item::Clip(clip)) = self
                .children
                .get_mut(track_index)
                .and_then(|track| track.items.get_mut(item_index))
            else {
                *self = backup;
                return None;
            };
            super::set_resolve_sync_clips_id(&mut clip.metadata, sync_clips_id);
        }

        Some(sync_clips_id)
    }
}
