use crate::{IdMetadataExt, InsertPolicy, Item, OverlapPolicy, Stack, TrackKind};
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
        let Some(targets) = self.gather_sync_targets_into_one_group(targets) else {
            *self = backup;
            return None;
        };
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

    /// Clips synced by hand must end up in one group. The column's owner is
    /// its video track (two video clips cannot be synced: that would need a
    /// second video track), or the owner of the group of the highest audio
    /// track among the clips. A clip on a track of that group stays; one on a
    /// free track makes the owner adopt the track; one on a track of another
    /// group is moved, at the same time, to a new partner track created for
    /// the owner. Returns the updated targets, or `None` when the clips
    /// cannot be grouped.
    fn gather_sync_targets_into_one_group(
        &mut self,
        targets: Vec<(usize, usize)>,
    ) -> Option<Vec<(usize, usize)>> {
        let clip_ids: Vec<String> = targets
            .iter()
            .map(|&(track_index, item_index)| self.children[track_index].items[item_index].get_id())
            .collect::<Option<Vec<_>>>()?;
        let video_tracks: Vec<usize> = targets
            .iter()
            .map(|&(track_index, _)| track_index)
            .filter(|&track_index| self.children[track_index].kind == TrackKind::Video)
            .collect();
        if video_tracks.len() > 1 {
            return None;
        }
        let owner_index = match video_tracks.first() {
            Some(&video) => video,
            None => {
                let highest = targets.iter().map(|&(track_index, _)| track_index).max()?;
                self.group_owner_of(highest)
            }
        };
        let owner_id = self.children[owner_index].get_id()?;

        let mut adopted_ids: Vec<String> = Vec::new();
        for clip_id in &clip_ids {
            let (track_index, item_index) = self.clip_target(clip_id)?;
            let (owner_index, _) = self.get_track_by_id(&owner_id)?;
            if track_index == owner_index || self.group_owner_of(track_index) == owner_index {
                continue;
            }
            if self.track_is_free(track_index) {
                adopted_ids.push(self.children[track_index].get_id()?);
                continue;
            }
            // Move the clip to a new partner track of the owner, at the same time.
            let start = self.children[track_index].start_time_of_item(item_index);
            let item = self.children[track_index].delete_clip(item_index, true)?;
            let kind = self.children[track_index].kind.clone();
            let mut created = Vec::new();
            let new_index = self.create_partner_track(owner_index, kind, &mut created);
            let inserted = self.children[new_index].insert_at_time(
                start,
                item,
                OverlapPolicy::Override,
                InsertPolicy::InsertBefore,
            );
            if !inserted.success {
                return None;
            }
            let (owner_index, _) = self.get_track_by_id(&owner_id)?;
            self.record_associated_tracks(owner_index, &[new_index]);
        }
        let (owner_index, _) = self.get_track_by_id(&owner_id)?;
        let adopted: Vec<usize> = adopted_ids
            .iter()
            .filter_map(|id| self.get_track_by_id(id).map(|(index, _)| index))
            .collect();
        if !adopted.is_empty() {
            self.record_associated_tracks(owner_index, &adopted);
        }
        clip_ids.iter().map(|id| self.clip_target(id)).collect()
    }
}
