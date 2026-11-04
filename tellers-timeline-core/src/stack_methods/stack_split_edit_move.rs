//! Moving a sync column that holds a split edit (J / L cut).
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
    /// Its track, by id when the track has one (a move may add tracks).
    track_id: Option<String>,
    track_index: usize,
    start: Seconds,
    item: Item,
}

impl Stack {
    /// [`Stack::move_item_at_time`] for a column holding a clip with a Sync
    /// Offset; `None` when `item_id`'s column has none (the move is an
    /// ordinary one). The column's other clips move by the drop, led by
    /// `item_id` or, when `item_id` is the offset clip, by a partner moved by
    /// the same time; the offset clips follow by that time, `item_id` onto
    /// `dest_track_id`.
    pub(super) fn move_split_edit_column(
        &mut self,
        item_id: &str,
        dest_track_id: &str,
        dest_time: Seconds,
        replace_with_gap: bool,
        insert_policy: InsertPolicy,
        overlap_policy: OverlapPolicy,
    ) -> Option<bool> {
        let (selected_track, selected_index, Item::Clip(selected)) = self.get_item(item_id)? else {
            return None;
        };
        let sync_clips_id = resolve_sync_clips_id(&selected.metadata)?;
        let selected_start = self.children[selected_track].start_time_of_item(selected_index);

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
        let selected_is_offset = offset_clips
            .iter()
            .any(|&(ti, ii, _)| (ti, ii) == (selected_track, selected_index));
        // The clip that leads the move: the dragged one, else a partner in
        // step with the column, a video first.
        let (anchor_id, anchor_track, anchor_start) = if selected_is_offset {
            anchors.sort_by_key(|(_, _, _, is_video)| !is_video);
            let (id, track_index, start, _) = anchors.into_iter().next()?;
            (id, track_index, start)
        } else {
            (item_id.to_string(), selected_track, selected_start)
        };
        let dest_track_index = self
            .get_track_by_id(dest_track_id)
            .map(|(index, _)| index)?;
        if selected_is_offset
            && self.children[dest_track_index].kind != self.children[selected_track].kind
        {
            return Some(false);
        }

        let backup = self.clone();
        offset_clips.sort_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)));
        let mut lifted = Vec::new();
        for (track_index, item_index, start) in offset_clips {
            let track = &mut self.children[track_index];
            let duration = track.items[item_index].duration().max(0.0);
            let item = std::mem::replace(
                &mut track.items[item_index],
                Item::Gap(Gap::make_gap(duration)),
            );
            lifted.push(Lifted {
                track_id: track.get_id(),
                track_index,
                start,
                item,
            });
        }

        let (anchor_dest_track, anchor_dest_time) = if selected_is_offset {
            let Some(track_id) = self.children[anchor_track].get_id() else {
                *self = backup;
                return Some(false);
            };
            (track_id, anchor_start + dest_time - selected_start)
        } else {
            (dest_track_id.to_string(), dest_time)
        };
        if !self.move_item_at_time_unchecked(
            &anchor_id,
            &anchor_dest_track,
            anchor_dest_time,
            replace_with_gap,
            insert_policy,
            overlap_policy,
        ) {
            *self = backup;
            return Some(false);
        }

        // The move may snap the drop and renumbers the column.
        let Some((track_index, item_index, Item::Clip(anchor))) = self.get_item(&anchor_id) else {
            *self = backup;
            return Some(false);
        };
        // An anchor that moved alone (its partners were all lifted) comes out
        // unsynced: the column takes a fresh id.
        let new_sync_clips_id = match resolve_sync_clips_id(&anchor.metadata) {
            Some(id) => id,
            None => {
                let id = self.next_sync_clips_id();
                if let Item::Clip(clip) = &mut self.children[track_index].items[item_index] {
                    set_resolve_sync_clips_id(&mut clip.metadata, id);
                }
                id
            }
        };
        let delta = self.children[track_index].start_time_of_item(item_index) - anchor_start;
        let dest_track_index = self.get_track_by_id(dest_track_id).map(|(index, _)| index);

        for Lifted {
            track_id,
            track_index,
            start,
            mut item,
        } in lifted
        {
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
                set_resolve_sync_clips_id(&mut clip.metadata, new_sync_clips_id);
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
}
