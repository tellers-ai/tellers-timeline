//! Trimming keeps a clip's transitions on its cuts.

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
    pub fn modify_item(
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
