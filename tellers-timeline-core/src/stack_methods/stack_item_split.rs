use crate::{Item, Seconds, Stack};

const EPS: Seconds = super::EPS;

#[derive(Copy, Clone)]
pub(super) enum SyncSplitIdPolicy {
    KeepShared,
    AssignNewIdToRight,
}

impl Stack {
    pub fn split_item_at_time(&mut self, item_id: &str, split_time: Seconds) -> bool {
        let Some((selected_track_index, selected_item_index, selected_item)) =
            self.get_item(item_id)
        else {
            return false;
        };
        let Item::Clip(selected_clip) = selected_item else {
            return false;
        };
        let selected_start =
            self.children[selected_track_index].start_time_of_item(selected_item_index);
        let selected_end =
            selected_start + selected_clip.source_range.duration.to_seconds().max(0.0);
        if split_time < selected_start - EPS || split_time > selected_end + EPS {
            return false;
        }
        if split_time <= selected_start + EPS || split_time >= selected_end - EPS {
            return true;
        }

        let selected_sync_clips_id = super::resolve_sync_clips_id(&selected_clip.metadata);
        if let Some(sync_clips_id) = selected_sync_clips_id {
            let targets = self.synced_clips_targets(sync_clips_id);
            if targets.len() > 1 {
                let ok = self.split_sync_clips_group_at_time(
                    sync_clips_id,
                    split_time,
                    SyncSplitIdPolicy::AssignNewIdToRight,
                    true,
                );
                if ok {
                    self.sanitize();
                }
                return ok;
            }
        }

        self.children[selected_track_index].split_at_time(split_time);
        self.sanitize();
        true
    }

    /// Split every synced clip in a link group that strictly contains `split_time`.
    pub(super) fn split_sync_clips_at_time(
        &mut self,
        split_time: Seconds,
        id_policy: SyncSplitIdPolicy,
    ) -> bool {
        let mut sync_ids = std::collections::HashSet::new();
        for track in &self.children {
            let Some(item_index) = track.get_item_at_time(split_time) else {
                continue;
            };
            let item_start = track.start_time_of_item(item_index);
            let item = &track.items[item_index];
            let Item::Clip(clip) = item else {
                continue;
            };
            if split_time <= item_start + EPS || split_time >= item_start + item.duration() - EPS {
                continue;
            }
            if let Some(sync_id) = super::resolve_sync_clips_id(&clip.metadata) {
                sync_ids.insert(sync_id);
            }
        }
        if sync_ids.is_empty() {
            return true;
        }

        let backup = self.clone();
        for sync_clips_id in sync_ids {
            if !self.split_sync_clips_group_at_time(
                sync_clips_id,
                split_time,
                id_policy,
                false,
            ) {
                *self = backup;
                return false;
            }
        }
        true
    }

    fn split_sync_clips_group_at_time(
        &mut self,
        sync_clips_id: i64,
        split_time: Seconds,
        id_policy: SyncSplitIdPolicy,
        require_all_targets: bool,
    ) -> bool {
        let targets = self.synced_clips_targets(sync_clips_id);
        if targets.len() <= 1 {
            return true;
        }

        let mut splittable = Vec::new();
        for (track_index, item_index) in &targets {
            let Some(item) = self
                .children
                .get(*track_index)
                .and_then(|track| track.items.get(*item_index))
            else {
                return false;
            };
            let Item::Clip(clip) = item else {
                if require_all_targets {
                    return false;
                }
                continue;
            };
            let item_start = self.children[*track_index].start_time_of_item(*item_index);
            let item_end = item_start + clip.source_range.duration.to_seconds().max(0.0);
            if split_time <= item_start + EPS || split_time >= item_end - EPS {
                if require_all_targets {
                    return false;
                }
                continue;
            }
            splittable.push((*track_index, *item_index));
        }

        if splittable.is_empty() {
            return true;
        }
        if require_all_targets && splittable.len() != targets.len() {
            return false;
        }

        let right_sync_clips_id = matches!(id_policy, SyncSplitIdPolicy::AssignNewIdToRight)
            .then(|| self.next_sync_clips_id());
        let mut target_tracks: Vec<_> = splittable
            .iter()
            .map(|(track_index, _)| *track_index)
            .collect();
        target_tracks.sort_unstable();
        target_tracks.dedup();
        for track_index in target_tracks {
            self.children[track_index].split_at_time(split_time);
        }
        if let Some(sync_clips_id) = right_sync_clips_id {
            for (track_index, item_index) in &splittable {
                let Some(Item::Clip(clip)) = self
                    .children
                    .get_mut(*track_index)
                    .and_then(|track| track.items.get_mut(item_index + 1))
                else {
                    return false;
                };
                super::set_resolve_sync_clips_id(&mut clip.metadata, sync_clips_id);
            }
        }
        true
    }
}
