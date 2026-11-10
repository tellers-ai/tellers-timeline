//! Copy and paste: [`Timeline::copy_items`] takes clips out as a
//! [`TimelineClipboard`], [`Timeline::paste_items`] puts a copy of them back
//! at another time, in the same timeline or another one.
//!
//! - **What is copied.** The given clips and composite clips, each with its
//!   sync partners (a video clip brings its linked audio). Gaps and
//!   transitions are not copied. Each sync group is one column: its video
//!   clip leads, its audio clips follow as the linked audio, as in a drop
//!   ([`Stack::insert_item_at_time`]).
//! - **Where it lands.** Every column keeps its source track and its start
//!   relative to the earliest copied clip. Pasting at `t` puts that earliest
//!   clip at `t` and the others at the same distance from it, each on the
//!   track it came from.
//! - **Nothing is overwritten.** When a source track holds anything where its
//!   clips would land, or no longer exists (another timeline), all of that
//!   track's clips go to a new track instead: right above a video track,
//!   right below an audio one. Linked audio takes free partner slots, as in
//!   any synced insert.
//! - **The copy is independent.** Pasted items get new `timeline_id`s
//!   (with the tracks, items and markers inside a composite, and clip
//!   markers), new sync links, new edit groups (`Tellers Group ID`) and new
//!   clip groups, each with a copy of its transform. Pasting twice makes two
//!   independent copies.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    item_clip_group_id, item_link_group_id, item_tellers_group_id, set_item_clip_group_id,
    set_item_tellers_group_id, ClipGroup, IdMetadataExt, InsertItemAtTimeResult, InsertPolicy,
    Item, Marker, OverlapPolicy, Seconds, Timeline, Track, TrackKind,
};

const EPS: Seconds = 1e-6;

/// Clips taken out of a timeline by [`Timeline::copy_items`], ready for
/// [`Timeline::paste_items`]. Serializable, so a host can keep it as JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TimelineClipboard {
    /// One entry per sync group or unsynced clip, in track order then time.
    pub columns: Vec<ClipboardColumn>,
    /// The transform (`parameters`) of every clip group a copied item belongs
    /// to, by group id.
    #[serde(default)]
    pub clip_groups: Map<String, Value>,
}

/// One clip (with its linked audio) of a [`TimelineClipboard`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipboardColumn {
    /// The `timeline_id` of the track it was copied from.
    pub track_id: String,
    pub track_kind: TrackKind,
    /// Seconds after the earliest copied clip.
    pub offset: Seconds,
    /// The clip or composite clip that leads the column.
    pub item: Item,
    /// Its sync partners on audio tracks.
    #[serde(default)]
    pub linked_audio: Vec<Item>,
}

impl TimelineClipboard {
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("a clipboard serializes")
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|error| error.to_string())
    }
}

impl Timeline {
    /// Copy the clips and composite clips `item_ids` with their sync partners;
    /// see the [module docs](self). `None` when none of them can be copied.
    pub fn copy_items(&self, item_ids: &[String]) -> Option<TimelineClipboard> {
        let requested: HashSet<&str> = item_ids.iter().map(String::as_str).collect();
        let copyable = |item: &Item| matches!(item, Item::Clip(_) | Item::Composite(_));
        let links: HashSet<i64> = self
            .tracks
            .children
            .iter()
            .flat_map(|track| &track.items)
            .filter(|item| copyable(item))
            .filter(|item| {
                item.get_id()
                    .is_some_and(|id| requested.contains(id.as_str()))
            })
            .filter_map(item_link_group_id)
            .collect();

        struct Picked<'a> {
            track: &'a Track,
            track_id: String,
            start: Seconds,
            item: &'a Item,
        }
        let mut picked: Vec<Picked> = Vec::new();
        for track in &self.tracks.children {
            let Some(track_id) = track.get_id() else {
                continue;
            };
            for (index, item) in track.items.iter().enumerate() {
                if !copyable(item) {
                    continue;
                }
                let wanted = item
                    .get_id()
                    .is_some_and(|id| requested.contains(id.as_str()))
                    || item_link_group_id(item).is_some_and(|link| links.contains(&link));
                if wanted {
                    picked.push(Picked {
                        track,
                        track_id: track_id.clone(),
                        start: track.start_time_of_item(index),
                        item,
                    });
                }
            }
        }
        let origin = picked.iter().map(|p| p.start).reduce(f64::min)?;

        // One column per sync group; an unsynced clip is a column of its own.
        let mut groups: Vec<Vec<&Picked>> = Vec::new();
        let mut group_of_link: HashMap<i64, usize> = HashMap::new();
        for p in &picked {
            match item_link_group_id(p.item) {
                Some(link) => match group_of_link.get(&link) {
                    Some(&index) => groups[index].push(p),
                    None => {
                        group_of_link.insert(link, groups.len());
                        groups.push(vec![p]);
                    }
                },
                None => groups.push(vec![p]),
            }
        }

        let column = |p: &Picked, linked_audio: Vec<Item>| ClipboardColumn {
            track_id: p.track_id.clone(),
            track_kind: p.track.kind.clone(),
            offset: p.start - origin,
            item: p.item.clone(),
            linked_audio,
        };
        let mut columns = Vec::new();
        for group in groups {
            // The video clip leads a synced column; an audio-only one is led
            // by its first clip. A second video clip in one sync group is
            // pasted on its own (and loses the link, like any lone clip).
            let lead = group
                .iter()
                .position(|p| p.track.kind == TrackKind::Video)
                .unwrap_or(0);
            let mut linked_audio = Vec::new();
            let mut alone = Vec::new();
            for (index, p) in group.iter().enumerate() {
                if index == lead {
                    continue;
                }
                if p.track.kind == TrackKind::Audio {
                    linked_audio.push(p.item.clone());
                } else {
                    alone.push(*p);
                }
            }
            columns.push(column(group[lead], linked_audio));
            columns.extend(alone.into_iter().map(|p| column(p, Vec::new())));
        }

        let mut clip_groups = Map::new();
        for p in &picked {
            if let Some(group) = item_clip_group_id(p.item).and_then(|id| self.get_clip_group(&id))
            {
                clip_groups.insert(group.id, Value::Object(group.parameters));
            }
        }
        Some(TimelineClipboard {
            columns,
            clip_groups,
        })
    }

    /// Paste a copy of `clipboard` with its earliest clip at `time`; see the
    /// [module docs](self). Returns the `timeline_id`s of every item placed
    /// (leads and linked audio), for the host's selection.
    pub fn paste_items(&mut self, clipboard: &TimelineClipboard, time: Seconds) -> Vec<String> {
        if clipboard.is_empty() {
            return Vec::new();
        }
        let time = time.max(0.0);
        let mut columns = clipboard.columns.clone();

        // New identity: ids, edit groups and clip groups.
        let mut next_edit_group = self.tracks.next_tellers_group_id();
        let mut edit_groups: HashMap<i64, i64> = HashMap::new();
        let mut clip_groups: HashMap<String, Option<String>> = HashMap::new();
        for column in &mut columns {
            for item in std::iter::once(&mut column.item).chain(column.linked_audio.iter_mut()) {
                reassign_ids(item);
                if let Some(group) = item_tellers_group_id(item) {
                    let fresh = *edit_groups.entry(group).or_insert_with(|| {
                        next_edit_group += 1;
                        next_edit_group - 1
                    });
                    set_item_tellers_group_id(item, Some(fresh));
                }
                if let Some(group) = item_clip_group_id(item) {
                    let fresh = clip_groups
                        .entry(group.clone())
                        .or_insert_with(|| {
                            let parameters = clipboard.clip_groups.get(&group)?.as_object()?;
                            let copy = ClipGroup {
                                id: crate::types::gen_hex_id_12(),
                                parameters: parameters.clone(),
                            };
                            self.set_clip_group(&copy).ok()?;
                            Some(copy.id)
                        })
                        .clone();
                    set_item_clip_group_id(item, fresh.as_deref());
                }
            }
        }

        // Where each source track's clips land.
        let mut destinations: HashMap<&str, String> = HashMap::new();
        for column in &columns {
            if destinations.contains_key(column.track_id.as_str()) {
                continue;
            }
            let free = |track: &Track| {
                columns
                    .iter()
                    .filter(|other| other.track_id == column.track_id)
                    .all(|other| {
                        let start = time + other.offset;
                        track_is_free(track, start, start + other.item.duration())
                    })
            };
            let source = self.tracks.get_track_by_id(&column.track_id);
            let destination = match source {
                Some((_, track)) if track.kind == column.track_kind && free(track) => {
                    column.track_id.clone()
                }
                _ => {
                    let index = match (source.map(|(index, _)| index), &column.track_kind) {
                        (Some(index), TrackKind::Audio) => index,
                        (Some(index), _) => index + 1,
                        (None, TrackKind::Audio) => 0,
                        (None, _) => self.tracks.children.len(),
                    };
                    let track = self.tracks.new_numbered_track(column.track_kind.clone());
                    let id = track.get_id().expect("a numbered track has an id");
                    self.tracks.add_track_at(track, index as isize);
                    id
                }
            };
            destinations.insert(&column.track_id, destination);
        }

        let mut pasted = Vec::new();
        for column in &columns {
            let Some((track_index, _)) = self
                .tracks
                .get_track_by_id(&destinations[column.track_id.as_str()])
            else {
                continue;
            };
            let linked_audio =
                (!column.linked_audio.is_empty()).then(|| column.linked_audio.clone());
            match self.tracks.insert_item_at_time(
                track_index,
                time + column.offset,
                column.item.clone(),
                OverlapPolicy::Override,
                InsertPolicy::SplitAndInsert,
                linked_audio,
                None,
            ) {
                Some(InsertItemAtTimeResult::ItemId(id)) => pasted.push(id),
                Some(InsertItemAtTimeResult::Synced(synced)) => {
                    pasted.push(synced.primary_clip_id);
                    pasted.extend(synced.audio_clips.into_iter().map(|(id, _)| id));
                    pasted.extend(synced.synced_video_clip_id);
                }
                None => {}
            }
        }
        pasted
    }
}

/// Whether `track` holds nothing but gaps and transitions between `from`
/// and `to`; past its end a track is free.
fn track_is_free(track: &Track, from: Seconds, to: Seconds) -> bool {
    let mut time = 0.0;
    track.items.iter().all(|item| {
        let start = time;
        time += item.duration();
        matches!(item, Item::Gap(_) | Item::Transition(_))
            || start >= to - EPS
            || time <= from + EPS
    })
}

/// New ids for a pasted item and everything it holds.
fn reassign_ids(item: &mut Item) {
    item.set_id(Some(crate::types::gen_hex_id_12()));
    match item {
        Item::Clip(clip) => reassign_marker_ids(&mut clip.markers),
        Item::Composite(stack) => {
            for track in &mut stack.children {
                track.set_id(Some(crate::types::gen_hex_id_12()));
                reassign_marker_ids(&mut track.markers);
                for child in &mut track.items {
                    reassign_ids(child);
                }
            }
        }
        Item::Gap(_) | Item::Transition(_) => {}
    }
}

fn reassign_marker_ids(markers: &mut [Marker]) {
    for marker in markers {
        if marker.get_id().is_some() {
            marker.set_id(Some(crate::types::gen_hex_id_12()));
        }
    }
}
