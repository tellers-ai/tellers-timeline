//! Trimming keeps a clip's transitions on its cuts.

use super::{item_source_start, reversed_tail_trim, EPS};
use crate::{IdMetadataExt, Item, OverlapPolicy, Seconds, Stack};

/// A transition on a track a trim touches, and the two items it joined.
struct EdgeTransition {
    transition: Item,
    left: Option<Side>,
    right: Option<Side>,
}

/// One neighbour of an [`EdgeTransition`].
struct Side {
    id: String,
    is_gap: bool,
    trimmed: bool,
}

impl Stack {
    /// Resize an item to `new_start_time` / `new_duration`. A transition on
    /// a trimmed edge follows the cut; see [`Stack::modify_item`].
    pub fn resize_item(
        &mut self,
        item_id: &str,
        new_start_time: Seconds,
        new_duration: Seconds,
        overlap_policy: OverlapPolicy,
        clamp_to_media: bool,
    ) -> bool {
        // A reversed clip resized from its tail keeps its first frame, the
        // top of its source range.
        if let Some((start, Item::Clip(clip))) = self
            .get_item(item_id)
            .map(|(ti, ii, item)| (self.children[ti].start_time_of_item(ii), item))
        {
            if clip.is_reversed() && (new_start_time - start).abs() <= EPS {
                let (source_start, duration) =
                    reversed_tail_trim(clip, new_duration, clamp_to_media);
                return self.resize_item_with_source_start(
                    item_id,
                    new_start_time,
                    source_start,
                    duration,
                    overlap_policy,
                    clamp_to_media,
                );
            }
        }
        self.resize_item_keeping_source_start(
            item_id,
            new_start_time,
            new_duration,
            overlap_policy,
            clamp_to_media,
        )
    }

    /// [`Self::resize_item`] that leaves every source start as it is.
    pub(super) fn resize_item_keeping_source_start(
        &mut self,
        item_id: &str,
        new_start_time: Seconds,
        new_duration: Seconds,
        overlap_policy: OverlapPolicy,
        clamp_to_media: bool,
    ) -> bool {
        self.keeping_sync_columns(false, |stack| {
            stack.resize_item_unchecked(
                item_id,
                new_start_time,
                new_duration,
                overlap_policy,
                clamp_to_media,
            )
        })
    }

    fn resize_item_unchecked(
        &mut self,
        item_id: &str,
        new_start_time: Seconds,
        new_duration: Seconds,
        overlap_policy: OverlapPolicy,
        clamp_to_media: bool,
    ) -> bool {
        let edges = self.edge_transitions(item_id);
        let resized = self.resize_item_inner(
            item_id,
            new_start_time,
            new_duration,
            overlap_policy,
            clamp_to_media,
        );
        if resized {
            self.restore_edge_transitions(edges);
        }
        resized
    }

    /// Trim an item by its source range. Transitions follow their cuts: one
    /// sits between the same two items when they still touch (the cut moved),
    /// else on the edge of the clip it was on that is left, the trimmed one
    /// first. One whose clips are both gone is dropped.
    ///
    /// `trim_sync_partners`: a synced clip's sync partners (same Link Group
    /// ID) take the same trim when true. When false only this clip is
    /// trimmed and it stays synced: that is a split edit. Pulling an audio
    /// head before its video is a J cut, running its tail past the video an L
    /// cut. The clip records how far its start now is from the column as its
    /// Sync Offset ([`crate::metadata::resolve_sync_offset`]), so the column
    /// still moves, splits and trims as one. A trim that would push other
    /// sync columns out of step is refused.
    #[allow(clippy::too_many_arguments)]
    pub fn modify_item(
        &mut self,
        item_id: &str,
        source_start_time: Seconds,
        duration: Seconds,
        clamp_to_media: bool,
        resize_from_start: bool,
        push_following: bool,
        trim_sync_partners: bool,
    ) -> bool {
        self.keeping_sync_columns(false, |stack| {
            if !trim_sync_partners {
                if let Some(modified) = stack.modify_synced_clip_alone(
                    item_id,
                    source_start_time,
                    duration,
                    clamp_to_media,
                    resize_from_start,
                    push_following,
                ) {
                    return modified;
                }
            }
            stack.modify_item_unchecked(
                item_id,
                source_start_time,
                duration,
                clamp_to_media,
                resize_from_start,
                push_following,
            )
        })
    }

    /// The split edit of [`Self::modify_item`]: trim the synced clip
    /// `item_id` as if no clip were synced, so neither it nor a clip it
    /// overwrites on its track drags its partners along, then put every
    /// Link Group ID back. The clip's start move is added to its Sync
    /// Offset, and so is that of a clip whose head it overwrote (an L cut
    /// into the next clip). A clip that only shifted keeps its offset, so a
    /// ripple that leaves later columns out of step is refused. `None` when
    /// `item_id` is not a clip with sync partners (an ordinary trim).
    fn modify_synced_clip_alone(
        &mut self,
        item_id: &str,
        source_start_time: Seconds,
        duration: Seconds,
        clamp_to_media: bool,
        resize_from_start: bool,
        push_following: bool,
    ) -> Option<bool> {
        let (_, _, Item::Clip(clip)) = self.get_item(item_id)? else {
            return None;
        };
        let sync_clips_id = super::resolve_sync_clips_id(&clip.metadata)?;
        if self.synced_clips_targets(sync_clips_id).len() <= 1 {
            return None;
        }
        let backup = self.clone();

        // Every synced clip is set aside while the clip is trimmed (a column
        // left with one clip loses its id on sanitize): its id, sync id,
        // start and source start.
        let mut synced = Vec::new();
        for track in &mut self.children {
            let mut start = 0.0;
            for item in &mut track.items {
                let item_start = start;
                start += item.duration().max(0.0);
                let Some(id) = item.get_id() else {
                    continue;
                };
                let source = item_source_start(item);
                let Item::Clip(clip) = item else {
                    continue;
                };
                let Some(sync_id) = super::resolve_sync_clips_id(&clip.metadata) else {
                    continue;
                };
                super::remove_resolve_sync_clips_id(&mut clip.metadata);
                synced.push((id, sync_id, item_start, source));
            }
        }
        let modified = self.modify_item_unchecked(
            item_id,
            source_start_time,
            duration,
            clamp_to_media,
            resize_from_start,
            push_following,
        );
        if !modified {
            *self = backup;
            return Some(false);
        }
        let mut sync_ids = Vec::new();
        for (id, sync_id, old_start, old_source) in synced {
            sync_ids.push(sync_id);
            let Some((ti, ii, item)) = self.get_item(&id) else {
                continue;
            };
            let start_delta = self.children[ti].start_time_of_item(ii) - old_start;
            let head_trimmed = id == item_id
                || (start_delta.abs() > EPS
                    && (item_source_start(item) - old_source - start_delta).abs() <= EPS);
            if let Item::Clip(clip) = &mut self.children[ti].items[ii] {
                super::set_resolve_sync_clips_id(&mut clip.metadata, sync_id);
                if head_trimmed {
                    let offset = crate::metadata::resolve_sync_offset(&clip.metadata);
                    crate::metadata::set_sync_offset(&mut clip.metadata, offset + start_delta);
                }
            }
        }
        // Trimmed away: a partner left on its own is no longer synced.
        sync_ids.sort_unstable();
        sync_ids.dedup();
        self.cleanup_singleton_sync_clips(&sync_ids);
        Some(true)
    }

    fn modify_item_unchecked(
        &mut self,
        item_id: &str,
        source_start_time: Seconds,
        duration: Seconds,
        clamp_to_media: bool,
        resize_from_start: bool,
        push_following: bool,
    ) -> bool {
        let edges = self.edge_transitions(item_id);
        let modified = self.modify_item_inner(
            item_id,
            source_start_time,
            duration,
            clamp_to_media,
            resize_from_start,
            push_following,
        );
        if modified {
            self.restore_edge_transitions(edges);
        }
        modified
    }

    /// Every transition on the tracks of `item_id` and of its sync partners.
    fn edge_transitions(&self, item_id: &str) -> Vec<EdgeTransition> {
        let mut ids = vec![item_id.to_string()];
        for member in self.synced_move_items(item_id).unwrap_or_default() {
            if let Some(id) = member.item.get_id() {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        let mut tracks: Vec<usize> = ids
            .iter()
            .filter_map(|id| self.get_item(id).map(|(ti, _, _)| ti))
            .collect();
        tracks.sort_unstable();
        tracks.dedup();
        let mut edges = Vec::new();
        for ti in tracks {
            let items = &self.children[ti].items;
            let side = |i: Option<usize>| {
                let item = i.and_then(|i| items.get(i))?;
                let id = item.get_id()?;
                Some(Side {
                    trimmed: ids.contains(&id),
                    is_gap: matches!(item, Item::Gap(_)),
                    id,
                })
            };
            for (ii, item) in items.iter().enumerate() {
                if item.is_transition() {
                    edges.push(EdgeTransition {
                        transition: item.clone(),
                        left: side(ii.checked_sub(1)),
                        right: side(Some(ii + 1)),
                    });
                }
            }
        }
        edges
    }

    /// Put each transition from [`Stack::edge_transitions`] back on its cut.
    fn restore_edge_transitions(&mut self, edges: Vec<EdgeTransition>) {
        if edges.is_empty() {
            return;
        }
        for edge in edges {
            if let Some((ti, ii, _)) = edge.transition.get_id().and_then(|id| self.get_item(&id)) {
                self.children[ti].items.remove(ii);
            }
            let between = match (&edge.left, &edge.right) {
                (Some(left), Some(right)) => match (self.get_item(&left.id), self.get_item(&right.id)) {
                    (Some((lt, li, _)), Some((rt, ri, _))) if lt == rt && li + 1 == ri => Some((lt, ri)),
                    _ => None,
                },
                _ => None,
            };
            // Else the head of the clip after it or the tail of the clip
            // before it: a trimmed one first, never a gap.
            let mut owners: Vec<(&Side, bool)> = [(&edge.right, true), (&edge.left, false)]
                .into_iter()
                .filter_map(|(side, before)| side.as_ref().map(|s| (s, before)))
                .filter(|(side, _)| !side.is_gap)
                .collect();
            owners.sort_by_key(|(side, _)| !side.trimmed);
            let place = between.or_else(|| {
                owners.iter().find_map(|(side, before)| {
                    let (ti, ii, _) = self.get_item(&side.id)?;
                    let items = &self.children[ti].items;
                    let index = if *before { ii } else { ii + 1 };
                    let left_free = index == 0 || !items[index - 1].is_transition();
                    let right_free = items.get(index).is_none_or(|it| !it.is_transition());
                    (left_free && right_free).then_some((ti, index))
                })
            });
            if let Some((ti, index)) = place {
                self.children[ti].items.insert(index, edge.transition);
            }
        }
        self.sanitize_preserving_all_gap_tracks();
    }
}
