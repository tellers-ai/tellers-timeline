use crate::{IdMetadataExt, InsertPolicy, Item, OverlapPolicy, Seconds, Stack, TrackKind};

impl Stack {
    /// Move an item identified by `item_id` to `dest_time` on the track with `dest_track_id`.
    ///
    /// When the selected clip belongs to a Tellers group, every sub-unit of the
    /// group (each sync column, and each standalone clip) shifts by the same time
    /// delta as the selected clip. Only the selected clip changes track; the other
    /// members stay on their own tracks. Otherwise the move falls through to the
    /// regular single-item path.
    ///
    /// A transition attached to a moved clip moves with it, on the same side;
    /// when the clip has one on each side, the first (incoming) one is kept.
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
        let carried = self.attached_transitions(item_id, dest_track_id, dest_time);
        let moved = self.move_item_at_time_inner(
            item_id,
            dest_track_id,
            dest_time,
            replace_with_gap,
            insert_policy,
            overlap_policy,
        );
        if moved {
            self.reattach_transitions(carried);
        }
        moved
    }

    /// Move several items together: `item_id` goes to `dest_time` on the
    /// track with `dest_track_id`, and every item in `other_ids` shifts by the
    /// same time delta on its own track (a multi-selection drag).
    ///
    /// Each moved clip brings its sync partners and its Tellers group, as in
    /// [`Stack::move_item_at_time`]. `other_ids` entries that are missing, not
    /// clips, or `item_id` itself are ignored; with none left this is a plain
    /// [`Stack::move_item_at_time`]. When the delta would put a clip before 0,
    /// it is clamped so the earliest clip lands at 0. Insert-policy snapping
    /// is resolved on `item_id` once and applied to every clip.
    ///
    /// All or nothing: returns false, leaving the stack unchanged, when any
    /// clip cannot be placed.
    #[allow(clippy::too_many_arguments)]
    pub fn move_items_at_time(
        &mut self,
        item_id: &str,
        other_ids: &[String],
        dest_track_id: &str,
        dest_time: Seconds,
        replace_with_gap: bool,
        insert_policy: InsertPolicy,
        overlap_policy: OverlapPolicy,
    ) -> bool {
        let others: Vec<String> = other_ids
            .iter()
            .filter(|id| id.as_str() != item_id)
            .filter(|id| matches!(self.get_item(id), Some((_, _, Item::Clip(_)))))
            .cloned()
            .collect();
        if others.is_empty() {
            return self.move_item_at_time(
                item_id,
                dest_track_id,
                dest_time,
                replace_with_gap,
                insert_policy,
                overlap_policy,
            );
        }
        let Some(mut plan) = self.offset_move_plan(item_id, &others, dest_track_id, dest_time)
        else {
            return false;
        };
        let earliest = plan.iter().map(|(_, _, t)| *t).fold(f64::INFINITY, f64::min);
        if earliest < 0.0 {
            for (_, _, time) in &mut plan {
                *time -= earliest;
            }
        }
        let carried =
            self.attached_transitions_of(plan.iter().map(|(id, _, _)| id.clone()).collect());
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
        self.reattach_transitions(carried);
        true
    }

    fn move_item_at_time_inner(
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
            // Partners in slot order (nearest the owner first), as the
            // single-column move does: insertion assigns slots in list order,
            // so stack order would reverse the channels.
            let member_tracks: Vec<usize> = members.iter().map(|m| m.track_index).collect();
            let owner = snapshot
                .column_owner_track_index(&member_tracks)
                .unwrap_or(selected.track_index);
            let mut audio_partners = Vec::new();
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
                        let slot = snapshot.partner_slot_order(owner, member.track_index);
                        audio_partners.push((slot, member.item.clone(), source));
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
            audio_partners.sort_by_key(|(slot, _, _)| *slot);
            let (audio, audio_tracks): (Vec<_>, Vec<_>) = audio_partners
                .into_iter()
                .map(|(_, item, source)| (item, source))
                .unzip();
            let old_start =
                snapshot.children[selected.track_index].start_time_of_item(selected.item_index);
            if self
                .insert_synced_item_at_time(
                    dest,
                    time,
                    None,
                    selected.item.clone(),
                    Self::effective_move_overlap_policy(
                        overlap_policy,
                        Some(old_start),
                        time,
                        // Every member is already lifted off the stack.
                        self.move_landing_is_free(dest, time, selected.item.duration(), &[]),
                    ),
                    InsertPolicy::SplitAndInsert,
                    (!audio.is_empty()).then_some(audio),
                    video,
                    false,
                    video_track.as_deref(),
                    Some(&audio_tracks),
                    Some(&source_tracks),
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
        if !self.move_keeps_track_kind(item_id, dest_track_id) {
            return false;
        }
        // A ripple move (Push, no gap left behind) only changes the order of
        // clips. Dropping a clip at a time inside its own slot on its own track
        // keeps the order, so nothing moves; placing it would leave the lifted
        // slot's gap behind.
        if overlap_policy == OverlapPolicy::Push && !replace_with_gap {
            if let Some((track_index, item_index, item)) = self.get_item(item_id) {
                let start = self.children[track_index].start_time_of_item(item_index);
                if self.children[track_index].get_id().as_deref() == Some(dest_track_id)
                    && dest_time > start - super::EPS
                    && dest_time < start + item.duration() - super::EPS
                {
                    return true;
                }
            }
        }
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

    /// A clip only ever moves onto a track of its own kind (video to video,
    /// audio to audio), whether or not it is synced.
    fn move_keeps_track_kind(&self, item_id: &str, dest_track_id: &str) -> bool {
        let (Some((source, _, _)), Some((_, dest))) =
            (self.get_item(item_id), self.get_track_by_id(dest_track_id))
        else {
            return false;
        };
        self.children[source].kind == dest.kind
    }

    /// Move an item identified by `item_id` to `dest_index` on the track with `dest_track_id`.
    /// Attached transitions move with it as in [`Stack::move_item_at_time`].
    /// Returns true if the item was successfully moved.
    pub fn move_item_at_index(
        &mut self,
        item_id: &str,
        dest_track_id: &str,
        dest_index: usize,
        replace_with_gap: bool,
        overlap_policy: OverlapPolicy,
    ) -> bool {
        let dest_time = self
            .get_track_by_id(dest_track_id)
            .map(|(_, track)| track.start_time_of_item(dest_index))
            .unwrap_or(0.0);
        let carried = self.attached_transitions(item_id, dest_track_id, dest_time);
        let moved = self.move_item_at_index_inner(
            item_id,
            dest_track_id,
            dest_index,
            replace_with_gap,
            overlap_policy,
        );
        if moved {
            self.reattach_transitions(carried);
        }
        moved
    }

    fn move_item_at_index_inner(
        &mut self,
        item_id: &str,
        dest_track_id: &str,
        dest_index: usize,
        replace_with_gap: bool,
        overlap_policy: OverlapPolicy,
    ) -> bool {
        if !self.move_keeps_track_kind(item_id, dest_track_id) {
            return false;
        }
        if let Some((_, track)) = self.get_track_by_id(dest_track_id) {
            let time = track.start_time_of_item(dest_index);
            if self
                .tellers_group_move_plan(item_id, dest_track_id, time)
                .is_some()
            {
                return self.move_item_at_time_inner(
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

        // `dest_index` refers to the track as it is now. Lifting the item from
        // earlier on the same track shifts the indices after it (its slot goes,
        // or its gap merges with a neighbour), so anchor the destination on the
        // start time `items[dest_index]` has once the item is lifted: the same,
        // or earlier by the item's duration when no gap is left behind. Then
        // find that boundary again, splitting a merged gap if needed.
        let source = self.get_item(item_id).map(|(ti, ii, _)| (ti, ii));
        let anchor_time = match source {
            Some((ti, ii)) if ti == dest_track_index && ii < dest_index => {
                let lifted = if replace_with_gap {
                    0.0
                } else {
                    item_to_move.duration()
                };
                Some(self.children[ti].start_time_of_item(dest_index) - lifted)
            }
            _ => None,
        };

        if self.delete_one_item(item_id, replace_with_gap).is_none() {
            return false;
        }

        let dest_index = match anchor_time {
            Some(time) => {
                let track = &mut self.children[dest_track_index];
                track.split_at_time(time);
                (0..=track.items.len())
                    .find(|&index| track.start_time_of_item(index) >= time - super::EPS)
                    .unwrap_or(track.items.len())
            }
            None => dest_index,
        };

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

    /// The transitions attached to each clip a move of `item_id` lifts (the
    /// clip, its sync partners and its Tellers group). The first one (just
    /// before the clip, else just after) is the one carried along, with `true`
    /// for "before".
    fn attached_transitions(
        &self,
        item_id: &str,
        dest_track_id: &str,
        dest_time: Seconds,
    ) -> Vec<CarriedTransition> {
        let ids: Vec<String> =
            match self.tellers_group_move_plan(item_id, dest_track_id, dest_time) {
                Some(plan) => plan.into_iter().map(|(id, _, _)| id).collect(),
                None => vec![item_id.to_string()],
            };
        self.attached_transitions_of(ids)
    }

    /// [`Stack::attached_transitions`] for the clips `ids` and their sync
    /// partners.
    fn attached_transitions_of(&self, mut ids: Vec<String>) -> Vec<CarriedTransition> {
        for id in ids.clone() {
            for member in self.synced_move_items(&id).unwrap_or_default() {
                if let Some(member_id) = member.item.get_id() {
                    if !ids.contains(&member_id) {
                        ids.push(member_id);
                    }
                }
            }
        }
        ids.into_iter()
            .filter_map(|clip_id| {
                let (ti, ii, item) = self.get_item(&clip_id)?;
                if item.is_transition() {
                    return None;
                }
                let items = &self.children[ti].items;
                let before = ii
                    .checked_sub(1)
                    .and_then(|i| items.get(i))
                    .filter(|it| it.is_transition());
                let after = items.get(ii + 1).filter(|it| it.is_transition());
                let (transition, before_side) = match (before, after) {
                    (Some(t), _) => (t.clone(), true),
                    (None, Some(t)) => (t.clone(), false),
                    (None, None) => return None,
                };
                let attached_ids = [before, after]
                    .into_iter()
                    .flatten()
                    .filter_map(|t| t.get_id())
                    .collect();
                Some(CarriedTransition {
                    clip_id,
                    transition,
                    before: before_side,
                    attached_ids,
                })
            })
            .collect()
    }

    /// Put each transition from [`Stack::attached_transitions`] back on a cut
    /// of its clip: the side it was on, else the other side. A cut takes it
    /// only between the clip and another regular item (clip, compound, text,
    /// shape), never a gap or another transition; with no such cut the
    /// transition is dropped. The clip's other transitions are dropped too.
    fn reattach_transitions(&mut self, carried: Vec<CarriedTransition>) {
        if carried.is_empty() {
            return;
        }
        for c in carried {
            for id in &c.attached_ids {
                if let Some((ti, ii, _)) = self.get_item(id) {
                    self.children[ti].items.remove(ii);
                }
            }
            let Some((ti, ii, _)) = self.get_item(&c.clip_id) else {
                continue;
            };
            let items = &mut self.children[ti].items;
            let joins = |index: Option<usize>| {
                index
                    .and_then(|i| items.get(i))
                    .is_some_and(|it| !matches!(it, Item::Gap(_) | Item::Transition(_)))
            };
            let before = joins(ii.checked_sub(1));
            let after = joins(Some(ii + 1));
            let index = match (c.before, before, after) {
                (true, true, _) | (false, true, false) => ii,
                (false, _, true) | (true, false, true) => ii + 1,
                _ => continue,
            };
            items.insert(index, c.transition);
        }
        self.sanitize();
    }
}

/// A transition that moves with its clip; see [`Stack::attached_transitions`].
struct CarriedTransition {
    clip_id: String,
    transition: Item,
    before: bool,
    attached_ids: Vec<String>,
}
