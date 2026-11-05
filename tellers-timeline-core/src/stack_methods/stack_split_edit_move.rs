//! Moving sync columns that hold a split edit (J / L cut).
//!
//! A clip trimmed on its own starts its Sync Offset away from its column
//! (see [`Stack::modify_item`]). The column moves look for partners at the
//! dragged clip's start and length, so such a clip would be left behind and
//! lose its sync. Here it is lifted out first, the rest of the column moves
//! as usual, and it lands the same distance from the column on its track.

use super::{resolve_sync_clips_id, set_resolve_sync_clips_id, EPS};
use crate::metadata::resolve_sync_offset;
use crate::{Gap, IdMetadataExt, InsertPolicy, Item, OverlapPolicy, Seconds, Stack, TrackKind};

/// A clip set aside while its column moves.
struct Lifted {
    /// The column's anchor, by id.
    anchor_id: String,
    /// Its track, by id when the track has one (a move may add tracks).
    track_id: Option<String>,
    track_index: usize,
    start: Seconds,
    item: Item,
}

/// A column with offset clips: the partner that leads its move and the
/// offset clips that follow it.
struct SplitColumn {
    anchor_id: String,
    anchor_track: usize,
    anchor_start: Seconds,
    /// (track index, item index, start).
    offset_clips: Vec<(usize, usize, Seconds)>,
}

/// What the wrapped move is asked to move: `item_id`'s stand-in, where it
/// goes, and the other selected clips' stand-ins.
struct AnchorMove {
    item_id: String,
    dest_track_id: String,
    dest_time: Seconds,
    others: Vec<String>,
}

impl Stack {
    /// [`Stack::move_item_at_time`] for a column holding a clip with a Sync
    /// Offset; `None` when `item_id`'s column has none (the move is an
    /// ordinary one). See [`Self::moving_split_edit_columns`].
    pub(super) fn move_split_edit_column(
        &mut self,
        item_id: &str,
        dest_track_id: &str,
        dest_time: Seconds,
        replace_with_gap: bool,
        insert_policy: InsertPolicy,
        overlap_policy: OverlapPolicy,
    ) -> Option<bool> {
        self.moving_split_edit_columns(item_id, &[], dest_track_id, dest_time, |stack, mv| {
            stack.move_item_at_time_unchecked(
                &mv.item_id,
                &mv.dest_track_id,
                mv.dest_time,
                replace_with_gap,
                insert_policy,
                overlap_policy,
            )
        })
    }

    /// [`Stack::move_items_at_time`] when a selected clip's column holds a
    /// clip with a Sync Offset; `None` when none does.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn move_split_edit_columns(
        &mut self,
        item_id: &str,
        other_ids: &[String],
        dest_track_id: &str,
        dest_time: Seconds,
        replace_with_gap: bool,
        insert_policy: InsertPolicy,
        overlap_policy: OverlapPolicy,
    ) -> Option<bool> {
        self.moving_split_edit_columns(item_id, other_ids, dest_track_id, dest_time, |stack, mv| {
            stack.move_items_at_time_unchecked(
                &mv.item_id,
                &mv.others,
                &mv.dest_track_id,
                mv.dest_time,
                replace_with_gap,
                insert_policy,
                overlap_policy,
            )
        })
    }

    /// [`Stack::move_item_at_index`] for a column holding a clip with a Sync
    /// Offset, as a move to the start of the item now at `dest_index`;
    /// `None` when `item_id`'s column has none.
    pub(super) fn move_split_edit_column_at_index(
        &mut self,
        item_id: &str,
        dest_track_id: &str,
        dest_index: usize,
        replace_with_gap: bool,
        overlap_policy: OverlapPolicy,
    ) -> Option<bool> {
        let (track_index, item_index, item) = self.get_item(item_id)?;
        let (dest_track_index, dest_track) = self.get_track_by_id(dest_track_id)?;
        let mut dest_time = dest_track.start_time_of_item(dest_index);
        // Later in its own track: the lift closes up before the drop.
        if !replace_with_gap && dest_track_index == track_index && dest_index > item_index {
            dest_time -= item.duration().max(0.0);
        }
        self.move_split_edit_column(
            item_id,
            dest_track_id,
            dest_time.max(0.0),
            replace_with_gap,
            InsertPolicy::InsertBefore,
            overlap_policy,
        )
    }

    /// Lift every offset clip of the selected clips' columns, let `move_rest`
    /// move the rest (an offset clip selected is stood in for by its
    /// column's anchor, a partner in step with the column, a video first),
    /// then put each lifted clip back by its anchor's move, under the
    /// column's new id. `item_id` itself lands on `dest_track_id`. `None`
    /// when no selected column holds an offset clip.
    fn moving_split_edit_columns(
        &mut self,
        item_id: &str,
        other_ids: &[String],
        dest_track_id: &str,
        dest_time: Seconds,
        move_rest: impl FnOnce(&mut Stack, &AnchorMove) -> bool,
    ) -> Option<bool> {
        let (selected_track, selected_index, _) = self.get_item(item_id)?;
        let selected_start = self.children[selected_track].start_time_of_item(selected_index);

        let mut columns: Vec<(i64, SplitColumn)> = Vec::new();
        for id in std::iter::once(item_id).chain(other_ids.iter().map(String::as_str)) {
            let Some((_, _, Item::Clip(clip))) = self.get_item(id) else {
                continue;
            };
            let Some(sync_clips_id) = resolve_sync_clips_id(&clip.metadata) else {
                continue;
            };
            if columns.iter().any(|(sid, _)| *sid == sync_clips_id) {
                continue;
            }
            match self.split_column(sync_clips_id) {
                Some(Ok(column)) => columns.push((sync_clips_id, column)),
                // Every clip is offset: nothing to lead the move.
                Some(Err(())) => return Some(false),
                None => {}
            }
        }
        if columns.is_empty() {
            return None;
        }

        // A selected offset clip is moved through its anchor.
        let stand_in = |id: &str, stack: &Stack| -> String {
            let Some((ti, ii, _)) = stack.get_item(id) else {
                return id.to_string();
            };
            columns
                .iter()
                .find(|(_, c)| c.offset_clips.iter().any(|&(t, i, _)| (t, i) == (ti, ii)))
                .map(|(_, c)| c.anchor_id.clone())
                .unwrap_or_else(|| id.to_string())
        };
        let anchor_id = stand_in(item_id, self);
        let selected_is_offset = anchor_id != item_id;
        if selected_is_offset {
            let dest_kind = &self.get_track_by_id(dest_track_id)?.1.kind;
            if *dest_kind != self.children[selected_track].kind {
                return Some(false);
            }
        }
        let mut others: Vec<String> = Vec::new();
        for id in other_ids {
            let id = stand_in(id, self);
            if id != anchor_id && !others.contains(&id) {
                others.push(id);
            }
        }
        let (anchor_dest_track, anchor_dest_time) = if selected_is_offset {
            let column = &columns.iter().find(|(_, c)| c.anchor_id == anchor_id)?.1;
            let Some(track_id) = self.children[column.anchor_track].get_id() else {
                return Some(false);
            };
            (track_id, column.anchor_start + dest_time - selected_start)
        } else {
            (dest_track_id.to_string(), dest_time)
        };

        let backup = self.clone();
        let mut lifted = Vec::new();
        let mut offset_clips: Vec<(usize, usize, Seconds, String)> = columns
            .iter()
            .flat_map(|(_, c)| {
                c.offset_clips
                    .iter()
                    .map(|&(ti, ii, start)| (ti, ii, start, c.anchor_id.clone()))
            })
            .collect();
        // Back to front, so the indices still to lift stay valid.
        offset_clips.sort_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)));
        for (track_index, item_index, start, anchor_id) in offset_clips {
            let track = &mut self.children[track_index];
            let duration = track.items[item_index].duration().max(0.0);
            let item = std::mem::replace(
                &mut track.items[item_index],
                Item::Gap(Gap::make_gap(duration)),
            );
            lifted.push(Lifted {
                anchor_id,
                track_id: track.get_id(),
                track_index,
                start,
                item,
            });
        }

        let mv = AnchorMove {
            item_id: anchor_id,
            dest_track_id: anchor_dest_track,
            dest_time: anchor_dest_time,
            others,
        };
        if !move_rest(self, &mv) {
            *self = backup;
            return Some(false);
        }

        // Each column's anchor: its move (it may have snapped) and its id
        // (renumbered by the move, or gone if it moved alone).
        let mut landed: Vec<(String, i64, Seconds)> = Vec::new();
        for (_, column) in &columns {
            let Some((ti, ii, Item::Clip(anchor))) = self.get_item(&column.anchor_id) else {
                *self = backup;
                return Some(false);
            };
            let sync_clips_id = match resolve_sync_clips_id(&anchor.metadata) {
                Some(id) => id,
                None => {
                    let id = self.next_sync_clips_id();
                    if let Item::Clip(clip) = &mut self.children[ti].items[ii] {
                        set_resolve_sync_clips_id(&mut clip.metadata, id);
                    }
                    id
                }
            };
            let delta = self.children[ti].start_time_of_item(ii) - column.anchor_start;
            landed.push((column.anchor_id.clone(), sync_clips_id, delta));
        }
        let dest_track_index = self.get_track_by_id(dest_track_id).map(|(index, _)| index);

        for Lifted {
            anchor_id,
            track_id,
            track_index,
            start,
            mut item,
        } in lifted
        {
            let Some(&(_, sync_clips_id, delta)) =
                landed.iter().find(|(id, _, _)| *id == anchor_id)
            else {
                *self = backup;
                return Some(false);
            };
            let is_selected = item.get_id().as_deref() == Some(item_id);
            let target = if is_selected {
                dest_track_index
            } else {
                track_id
                    .as_deref()
                    .and_then(|id| self.get_track_by_id(id).map(|(index, _)| index))
                    .or(Some(track_index))
            };
            let Some(target) = target.filter(|&index| index < self.children.len()) else {
                *self = backup;
                return Some(false);
            };
            if let Item::Clip(clip) = &mut item {
                set_resolve_sync_clips_id(&mut clip.metadata, sync_clips_id);
            }
            // Over what is there, as the split edit itself was: a J cut's
            // lead-in covers the tail of the audio before it.
            let _ = self.children[target].insert_at_time(
                (start + delta).max(0.0),
                item,
                OverlapPolicy::Override,
                InsertPolicy::SplitAndInsert,
            );
        }
        self.sanitize_preserving_all_gap_tracks();
        Some(true)
    }

    /// Column `sync_clips_id` split into its anchor and its offset clips;
    /// `None` when it has no offset clip, `Err` when it has nothing else.
    fn split_column(&self, sync_clips_id: i64) -> Option<Result<SplitColumn, ()>> {
        let mut offset_clips = Vec::new();
        let mut anchors = Vec::new();
        for (track_index, item_index) in self.synced_clips_targets(sync_clips_id) {
            let track = &self.children[track_index];
            let Item::Clip(clip) = &track.items[item_index] else {
                continue;
            };
            let start = track.start_time_of_item(item_index);
            if resolve_sync_offset(&clip.metadata).abs() > EPS {
                offset_clips.push((track_index, item_index, start));
            } else if let Some(id) = clip.get_id() {
                anchors.push((id, track_index, start, track.kind == TrackKind::Video));
            }
        }
        if offset_clips.is_empty() {
            return None;
        }
        anchors.sort_by_key(|(_, _, _, is_video)| !is_video);
        let Some((anchor_id, anchor_track, anchor_start, _)) = anchors.into_iter().next() else {
            return Some(Err(()));
        };
        Some(Ok(SplitColumn {
            anchor_id,
            anchor_track,
            anchor_start,
            offset_clips,
        }))
    }
}
