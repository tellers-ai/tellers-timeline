use crate::{IdMetadataExt, InsertPolicy, OverlapPolicy, Seconds, Stack, TrackKind};

impl Stack {
    /// Move an item identified by `item_id` to `dest_time` on the track with `dest_track_id`.
    ///
    /// When the selected clip belongs to a Tellers group, every sub-unit of the
    /// group (each sync column, and each standalone clip) shifts by the same time
    /// delta as the selected clip. Only the selected clip changes track; the other
    /// members stay on their own tracks. Otherwise the move falls through to the
    /// regular single-item path.
    ///
    /// Returns true if the item was successfully moved.
    pub fn move_item_at_time(
        &mut self,
        item_id: &str,
        dest_track_id: &str,
        dest_time: Seconds,
        replace_with_gap: bool,
        insert_policy: InsertPolicy,
        overlap_policy: OverlapPolicy,
    ) -> bool {
        if let Some(plan) = self.tellers_group_move_plan(item_id, dest_track_id, dest_time) {
            let backup = self.clone();
            if !self.move_group_plan(
                &backup,
                item_id,
                plan,
                replace_with_gap,
                insert_policy,
                overlap_policy,
            ) {
                *self = backup;
                return false;
            }
            return true;
        }

        self.move_item_at_time_single(
            item_id,
            dest_track_id,
            dest_time,
            replace_with_gap,
            insert_policy,
            overlap_policy,
        )
    }

    /// Lift the entire selection before placing any member. Ripple deletion must
    /// not shift a group member that has already reached its destination.
    fn move_group_plan(
        &mut self,
        snapshot: &Stack,
        selected_id: &str,
        plan: Vec<(String, String, Seconds)>,
        replace_with_gap: bool,
        insert_policy: InsertPolicy,
        overlap_policy: OverlapPolicy,
    ) -> bool {
        let mut columns = Vec::new();
        let mut targets = Vec::new();
        for (id, track_id, time) in plan {
            if !time.is_finite() || time < 0.0 {
                return false;
            }
            let Some((ti, ii, item)) = snapshot.get_item(&id) else {
                return false;
            };
            let Some((_, destination)) = snapshot.get_track_by_id(&track_id) else {
                return false;
            };
            if snapshot.children[ti].kind != destination.kind {
                return false;
            }
            let members = snapshot.synced_move_items(&id).unwrap_or_else(|| {
                vec![super::SyncedMoveItem {
                    track_index: ti,
                    item_index: ii,
                    track_kind: snapshot.children[ti].kind.clone(),
                    item: item.clone(),
                    is_selected: true,
                }]
            });
            targets.extend(members.iter().map(|m| (m.track_index, m.item_index)));
            columns.push((track_id, time, members));
        }
        self.delete_clips_at_indices(targets, replace_with_gap);
        // Snap the selected anchor once, never each group member independently.
        let Some((track_id, requested, _)) = columns.iter().find(|(_, _, members)| {
            members
                .iter()
                .any(|m| m.is_selected && m.item.get_id().as_deref() == Some(selected_id))
        }) else {
            return false;
        };
        let Some((_, track)) = self.get_track_by_id(track_id) else {
            return false;
        };
        let Some(resolved) =
            super::insertion_start_or_end_for_policy(track, *requested, insert_policy)
        else {
            return false;
        };
        let adjustment = resolved - requested;
        for (_, time, _) in &mut columns {
            *time += adjustment;
            if *time < 0.0 {
                return false;
            }
        }
        // All sources are lifted: place earliest first so later Push inserts
        // cannot ripple a member that is already at its planned position.
        columns.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut expected = Vec::new();
        for (track_id, time, members) in columns {
            let Some((dest, _)) = self.get_track_by_id(&track_id) else {
                return false;
            };
            let Some(selected) = members.iter().find(|m| m.is_selected) else {
                return false;
            };
            let mut audio = Vec::new();
            let mut audio_tracks = Vec::new();
            let mut video = None;
            let mut video_track = None;
            let mut source_tracks = Vec::new();
            for member in &members {
                let Some(source_id) = snapshot.children[member.track_index].get_id() else {
                    return false;
                };
                let Some((source, _)) = self.get_track_by_id(&source_id) else {
                    return false;
                };
                source_tracks.push(source);
                if let Some(id) = member.item.get_id() {
                    expected.push((
                        id,
                        time,
                        member.item.duration(),
                        crate::item_link_group_id(&member.item),
                    ));
                }
                if member.is_selected {
                    continue;
                }
                match member.track_kind {
                    TrackKind::Audio => {
                        audio.push(member.item.clone());
                        audio_tracks.push(source);
                    }
                    TrackKind::Video => {
                        if video.is_some() {
                            return false;
                        }
                        video = Some(member.item.clone());
                        if snapshot.children[selected.track_index].get_id().as_deref()
                            == Some(&track_id)
                        {
                            video_track = Some(source_id);
                        }
                    }
                    TrackKind::Other => return false,
                }
            }
            let old_start =
                snapshot.children[selected.track_index].start_time_of_item(selected.item_index);
            if self
                .insert_synced_item_at_time(
                    dest,
                    time,
                    None,
                    selected.item.clone(),
                    Self::effective_move_overlap_policy(overlap_policy, Some(old_start), time),
                    InsertPolicy::SplitAndInsert,
                    (!audio.is_empty()).then_some(audio),
                    video,
                    video_track.as_deref(),
                    Some(&audio_tracks),
                    Some(&source_tracks),
                    Some(old_start),
                )
                .is_none()
            {
                return false;
            }
        }
        // Never report success with clipped, lost, or independently rippled members.
        if expected.iter().any(|(id, time, duration, _)| {
            self.get_item(id).is_none_or(|(ti, ii, item)| {
                (self.children[ti].start_time_of_item(ii) - time).abs() > super::EPS
                    || (item.duration() - duration).abs() > super::EPS
            })
        }) {
            return false;
        }
        // Reinsertions sanitize intermediate singletons. Restore the original
        // links only after every member is present, including offset partners.
        // Any new split columns that reused a temporarily vacant link ID must
        // receive another ID before the original relationship is restored.
        let moved_ids: std::collections::HashSet<_> =
            expected.iter().map(|e| e.0.as_str()).collect();
        let original_links: std::collections::BTreeSet<_> =
            expected.iter().filter_map(|e| e.3).collect();
        let mut next_link = self.next_sync_clips_id().max(snapshot.next_sync_clips_id());
        for link in original_links {
            let mut reassigned = false;
            for track in &mut self.children {
                for item in &mut track.items {
                    if crate::item_link_group_id(item) == Some(link)
                        && !item
                            .get_id()
                            .as_deref()
                            .is_some_and(|id| moved_ids.contains(id))
                    {
                        Self::set_item_sync_clips(item, Some(next_link));
                        reassigned = true;
                    }
                }
            }
            if reassigned {
                next_link += 1;
            }
        }
        for (id, _, _, link) in &expected {
            if let Some((ti, ii, _)) = self.get_item(id) {
                Self::set_item_sync_clips(&mut self.children[ti].items[ii], *link);
            }
        }
        self.sanitize();
        true
    }

    /// Move a single sub-unit: a sync column (aligned video/audio partners) moves
    /// as a unit; all other clips, including link groups and unsynced items, share
    /// the same delete + insert path with cluster propagation. This is the
    /// group-unaware move used as a building block by `move_item_at_time`.
    fn move_item_at_time_single(
        &mut self,
        item_id: &str,
        dest_track_id: &str,
        dest_time: Seconds,
        replace_with_gap: bool,
        insert_policy: InsertPolicy,
        overlap_policy: OverlapPolicy,
    ) -> bool {
        if let Some(items_to_move) = self.synced_move_items(item_id) {
            let dest_track_index = match self.get_track_by_id(dest_track_id) {
                Some((index, _)) => index,
                None => return false,
            };
            return self.move_synced_items_at_time_via_insert(
                items_to_move,
                dest_track_index,
                dest_time,
                replace_with_gap,
                insert_policy,
                overlap_policy,
            );
        }

        self.move_linked_items_at_time(
            item_id,
            dest_track_id,
            dest_time,
            replace_with_gap,
            insert_policy,
            overlap_policy,
        )
    }

    /// Move an item identified by `item_id` to `dest_index` on the track with `dest_track_id`.
    /// Returns true if the item was successfully moved.
    pub fn move_item_at_index(
        &mut self,
        item_id: &str,
        dest_track_id: &str,
        dest_index: usize,
        replace_with_gap: bool,
        overlap_policy: OverlapPolicy,
    ) -> bool {
        if let Some((_, track)) = self.get_track_by_id(dest_track_id) {
            let time = track.start_time_of_item(dest_index);
            if self
                .tellers_group_move_plan(item_id, dest_track_id, time)
                .is_some()
            {
                return self.move_item_at_time(
                    item_id,
                    dest_track_id,
                    time,
                    replace_with_gap,
                    InsertPolicy::InsertBefore,
                    overlap_policy,
                );
            }
        }
        let item_to_move = match self.get_item(item_id) {
            Some((_ti, _ii, it)) => it.clone(),
            None => return false,
        };

        let backup = self.clone();
        let dest_track_index = match self.get_track_by_id(dest_track_id) {
            Some((i, _)) => i,
            None => return false,
        };
        if let Some(items_to_move) = self.synced_move_items(item_id) {
            return self.move_synced_items(
                items_to_move,
                dest_track_index,
                replace_with_gap,
                overlap_policy,
                dest_index,
            );
        }

        if self.delete_one_item(item_id, replace_with_gap).is_none() {
            return false;
        }

        if self
            .insert_item_at_index(
                dest_track_id,
                dest_index,
                item_to_move,
                overlap_policy,
                None,
                None,
            )
            .is_some()
        {
            self.sanitize();
            true
        } else {
            *self = backup;
            false
        }
    }
}
