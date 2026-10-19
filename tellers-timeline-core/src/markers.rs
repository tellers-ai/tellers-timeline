//! OTIO markers (`Marker.2`) on the stack, on tracks and on clips.
//!
//! A marker flags a point (or a range) of the edit with a name, a comment and
//! a color, so users and the agent can leave notes on the timeline. It is the
//! plain OTIO schema, so markers survive a round trip through other OTIO tools
//! (Resolve imports them as timeline / clip markers):
//!
//! ```json
//! {
//!   "OTIO_SCHEMA": "Marker.2",
//!   "name": "Fix color",
//!   "comment": "Too warm compared to the previous shot",
//!   "color": "RED",
//!   "marked_range": { "OTIO_SCHEMA": "TimeRange.1", "start_time": {...}, "duration": {...} },
//!   "metadata": { "tellers.ai": { "timeline_id": "a1b2c3d4e5f6", "created_by": "<user id>" } }
//! }
//! ```
//!
//! As in OTIO, `marked_range` is expressed in the time of the object holding
//! the marker:
//!
//! - on the stack, in timeline time;
//! - on a track, in that track's time (which is timeline time, tracks all start
//!   at 0 in the stack);
//! - on a clip, in the clip's source (media) time, the same space as its
//!   `source_range`. The marker therefore follows its clip when the clip is
//!   moved, and is hidden (not deleted) when the clip is trimmed past it.
//!
//! The `Stack` / `Timeline` methods below take and return timeline seconds and
//! do the conversion, so callers never deal with source time. Every marker has
//! an id at `metadata["tellers.ai"]["timeline_id"]`, like items and tracks, and
//! the id of the user that created it at `metadata["tellers.ai"]["created_by"]`.
//!
//! Gaps also carry a `markers` list so OTIO input round-trips, but the API does
//! not place markers on gaps: gaps are merged, split and dropped freely while
//! editing, which would lose them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{IdMetadataExt, Item, Seconds, Stack, TimeRange, Timeline, Track};

/// Metadata key, under the `tellers.ai` namespace, holding the creator user id.
pub const MARKER_CREATED_BY_KEY: &str = "created_by";

/// Default marker color, as in OTIO.
pub const DEFAULT_MARKER_COLOR: &str = "RED";

/// The colors OTIO defines for markers (`otio.schema.MarkerColor`), which
/// other tools (Resolve, Premiere adapters) understand. `color` is a free
/// string, so other values round-trip, but these are the ones to offer.
pub const MARKER_COLORS: [&str; 11] = [
    "PINK", "RED", "ORANGE", "YELLOW", "GREEN", "CYAN", "BLUE", "PURPLE", "MAGENTA", "BLACK",
    "WHITE",
];

fn default_marker_schema() -> String {
    "Marker.2".to_string()
}

/// Metadata of a marker read without one: OTIO writers that do not know about
/// our ids still get a marker that can be addressed by id.
fn default_marker_metadata() -> serde_json::Value {
    crate::types::ensure_tellers_ai_with_id(serde_json::Value::Null)
}

fn default_marker_color() -> String {
    DEFAULT_MARKER_COLOR.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Marker {
    #[serde(rename = "OTIO_SCHEMA", default = "default_marker_schema")]
    pub otio_schema: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub comment: String,
    #[serde(default = "default_marker_color")]
    pub color: String,
    #[serde(default)]
    pub marked_range: TimeRange,
    #[serde(
        default = "default_marker_metadata",
        deserialize_with = "crate::types::deserialize_metadata_with_id"
    )]
    pub metadata: serde_json::Value,
}

impl Default for Marker {
    fn default() -> Self {
        Marker::new(0.0, 0.0, "", "", None, None, None)
    }
}

impl Marker {
    /// A marker at `start` (in the time of its owner) lasting `duration`
    /// seconds (0 for a point marker). `color` defaults to [`DEFAULT_MARKER_COLOR`]
    /// and `id` to a fresh one.
    pub fn new(
        start: Seconds,
        duration: Seconds,
        name: impl Into<String>,
        comment: impl Into<String>,
        color: Option<String>,
        created_by: Option<String>,
        id: Option<String>,
    ) -> Self {
        let mut m = Marker {
            otio_schema: default_marker_schema(),
            name: name.into(),
            comment: comment.into(),
            color: color.unwrap_or_else(default_marker_color),
            marked_range: TimeRange::new(duration.max(0.0), start),
            metadata: serde_json::Value::Object(serde_json::Map::new()),
        };
        m.set_id(Some(id.unwrap_or_else(crate::types::gen_hex_id_12)));
        m.set_created_by(created_by);
        m
    }

    pub fn start(&self) -> Seconds {
        self.marked_range.get_start_time()
    }

    pub fn duration(&self) -> Seconds {
        self.marked_range.get_duration().max(0.0)
    }

    /// The id of the user that created this marker, if recorded.
    pub fn get_created_by(&self) -> Option<String> {
        self.metadata
            .get("tellers.ai")
            .and_then(|v| v.get(MARKER_CREATED_BY_KEY))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    pub fn set_created_by(&mut self, user_id: Option<String>) {
        if self.metadata.as_object().is_none() {
            self.metadata = serde_json::Value::Object(serde_json::Map::new());
        }
        let ai = self
            .metadata
            .as_object_mut()
            .unwrap()
            .entry("tellers.ai".to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if ai.as_object().is_none() {
            *ai = serde_json::Value::Object(serde_json::Map::new());
        }
        let ai = ai.as_object_mut().unwrap();
        match user_id {
            Some(id) => {
                ai.insert(
                    MARKER_CREATED_BY_KEY.to_string(),
                    serde_json::Value::String(id),
                );
            }
            None => {
                ai.remove(MARKER_CREATED_BY_KEY);
            }
        }
    }
}

impl IdMetadataExt for Marker {
    fn get_id(&self) -> Option<String> {
        self.metadata
            .get("tellers.ai")
            .and_then(|v| v.get("timeline_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
    fn set_id(&mut self, id: Option<String>) {
        crate::metadata::write_id_to_metadata(&mut self.metadata, id)
    }
}

/// What a marker is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarkerOwnerKind {
    Stack,
    Track,
    Clip,
    Gap,
    Composite,
}

/// A marker together with where it sits in the timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MarkerInfo {
    pub marker: Marker,
    pub owner_kind: MarkerOwnerKind,
    /// Id of the track or item holding the marker; `None` for the stack.
    pub owner_id: Option<String>,
    /// Id of the track the marker shows on: the owner track, or the track of
    /// the owner item. `None` for stack markers.
    pub track_id: Option<String>,
    /// Start of the marker in timeline seconds.
    pub timeline_start: Seconds,
    /// Duration of the marker in seconds (0 for a point marker).
    pub duration: Seconds,
    /// False when the marker starts outside the visible part of its clip
    /// (the clip was trimmed past it). Always true for stack and track markers.
    pub visible: bool,
}

/// Changes to apply to a marker; `None` fields are left as they are.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MarkerUpdate {
    pub name: Option<String>,
    pub comment: Option<String>,
    pub color: Option<String>,
    /// New start, in timeline seconds.
    pub timeline_start: Option<Seconds>,
    pub duration: Option<Seconds>,
}

/// Offset to add to a time in item source space to get timeline time.
fn item_source_to_timeline_offset(track: &Track, index: usize) -> Seconds {
    let item = &track.items[index];
    track.start_time_of_item(index) - item.get_source_range().get_start_time()
}

/// No markers: what a transition holds (OTIO transitions carry none).
static NO_MARKERS: Vec<Marker> = Vec::new();

fn item_markers(item: &Item) -> &Vec<Marker> {
    match item {
        Item::Clip(c) => &c.markers,
        Item::Gap(g) => &g.markers,
        Item::Composite(s) => &s.markers,
        Item::Transition(_) => &NO_MARKERS,
    }
}

/// Callers look a marker up with [`item_markers`] first, which finds none on
/// a transition, so they never reach one here.
fn item_markers_mut(item: &mut Item) -> &mut Vec<Marker> {
    match item {
        Item::Clip(c) => &mut c.markers,
        Item::Gap(g) => &mut g.markers,
        Item::Composite(s) => &mut s.markers,
        Item::Transition(_) => unreachable!("transitions hold no markers"),
    }
}

fn find_marker_index(markers: &[Marker], marker_id: &str) -> Option<usize> {
    markers
        .iter()
        .position(|m| m.get_id().as_deref() == Some(marker_id))
}

/// Where a marker lives, and the offset from its owner's time to timeline time.
enum MarkerLocation {
    Stack(usize),
    Track(usize, usize),
    Item(usize, usize, usize, Seconds),
}

impl Stack {
    /// Every marker of the timeline (stack, then each track and its items in
    /// stack order), with timeline times.
    pub fn get_markers(&self) -> Vec<MarkerInfo> {
        let mut out = Vec::new();
        for m in &self.markers {
            out.push(MarkerInfo {
                marker: m.clone(),
                owner_kind: MarkerOwnerKind::Stack,
                owner_id: None,
                track_id: None,
                timeline_start: m.start(),
                duration: m.duration(),
                visible: true,
            });
        }
        for track in &self.children {
            let track_id = track.get_id();
            for m in &track.markers {
                out.push(MarkerInfo {
                    marker: m.clone(),
                    owner_kind: MarkerOwnerKind::Track,
                    owner_id: track_id.clone(),
                    track_id: track_id.clone(),
                    timeline_start: m.start(),
                    duration: m.duration(),
                    visible: true,
                });
            }
            for (ii, item) in track.items.iter().enumerate() {
                let markers = item_markers(item);
                if markers.is_empty() {
                    continue;
                }
                let offset = item_source_to_timeline_offset(track, ii);
                let range = item.get_source_range();
                let (src_start, src_end) = (
                    range.get_start_time(),
                    range.get_start_time() + range.get_duration(),
                );
                let owner_kind = match item {
                    Item::Clip(_) => MarkerOwnerKind::Clip,
                    Item::Gap(_) => MarkerOwnerKind::Gap,
                    Item::Composite(_) => MarkerOwnerKind::Composite,
                    // Unreachable: a transition has no markers (skipped above).
                    Item::Transition(_) => continue,
                };
                for m in markers {
                    const EPS: Seconds = 1e-9;
                    out.push(MarkerInfo {
                        marker: m.clone(),
                        owner_kind,
                        owner_id: item.get_id(),
                        track_id: track_id.clone(),
                        timeline_start: m.start() + offset,
                        duration: m.duration(),
                        visible: m.start() >= src_start - EPS && m.start() <= src_end + EPS,
                    });
                }
            }
        }
        out
    }

    /// The marker with id `marker_id`, wherever it is attached.
    pub fn get_marker(&self, marker_id: &str) -> Option<MarkerInfo> {
        self.get_markers()
            .into_iter()
            .find(|info| info.marker.get_id().as_deref() == Some(marker_id))
    }

    fn locate_marker(&self, marker_id: &str) -> Option<MarkerLocation> {
        if let Some(mi) = find_marker_index(&self.markers, marker_id) {
            return Some(MarkerLocation::Stack(mi));
        }
        for (ti, track) in self.children.iter().enumerate() {
            if let Some(mi) = find_marker_index(&track.markers, marker_id) {
                return Some(MarkerLocation::Track(ti, mi));
            }
            for (ii, item) in track.items.iter().enumerate() {
                if let Some(mi) = find_marker_index(item_markers(item), marker_id) {
                    let offset = item_source_to_timeline_offset(track, ii);
                    return Some(MarkerLocation::Item(ti, ii, mi, offset));
                }
            }
        }
        None
    }

    fn marker_mut(&mut self, location: &MarkerLocation) -> (&mut Marker, Seconds) {
        match *location {
            MarkerLocation::Stack(mi) => (&mut self.markers[mi], 0.0),
            MarkerLocation::Track(ti, mi) => (&mut self.children[ti].markers[mi], 0.0),
            MarkerLocation::Item(ti, ii, mi, offset) => (
                &mut item_markers_mut(&mut self.children[ti].items[ii])[mi],
                offset,
            ),
        }
    }

    /// Attach `marker` to the stack (`owner_id` `None`, or the stack's own id),
    /// to the track with id `owner_id`, or to the clip with id `owner_id`.
    ///
    /// `marker.marked_range.start_time` is read as timeline seconds and stored
    /// in the owner's time. A marker without an id gets a fresh one, and one
    /// whose id is already used in the timeline is refused. Returns the
    /// marker id, or `None` when the owner is not found (or is a gap).
    pub fn add_marker(&mut self, owner_id: Option<&str>, mut marker: Marker) -> Option<String> {
        let marker_id = match marker.get_id() {
            Some(id) if !id.is_empty() => id,
            _ => {
                let id = crate::types::gen_hex_id_12();
                marker.set_id(Some(id.clone()));
                id
            }
        };
        if self.locate_marker(&marker_id).is_some() {
            return None;
        }
        let timeline_start = marker.start();
        let stack_id = self.metadata.get("tellers.ai")
            .and_then(|v| v.get("timeline_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        match owner_id {
            None => {
                self.markers.push(marker);
                return Some(marker_id);
            }
            Some(id) if stack_id.as_deref() == Some(id) => {
                self.markers.push(marker);
                return Some(marker_id);
            }
            Some(id) => {
                if let Some(ti) = self
                    .children
                    .iter()
                    .position(|t| t.get_id().as_deref() == Some(id))
                {
                    self.children[ti].markers.push(marker);
                    return Some(marker_id);
                }
                for track in &mut self.children {
                    let Some(ii) = track
                        .items
                        .iter()
                        .position(|it| it.get_id().as_deref() == Some(id))
                    else {
                        continue;
                    };
                    if matches!(track.items[ii], Item::Gap(_) | Item::Transition(_)) {
                        return None;
                    }
                    let offset = item_source_to_timeline_offset(track, ii);
                    marker
                        .marked_range
                        .set_start_time(timeline_start - offset);
                    item_markers_mut(&mut track.items[ii]).push(marker);
                    return Some(marker_id);
                }
                None
            }
        }
    }

    /// Apply `update` to the marker with id `marker_id`. Returns false when
    /// no such marker exists.
    pub fn update_marker(&mut self, marker_id: &str, update: MarkerUpdate) -> bool {
        let Some(location) = self.locate_marker(marker_id) else {
            return false;
        };
        let (marker, offset) = self.marker_mut(&location);
        if let Some(name) = update.name {
            marker.name = name;
        }
        if let Some(comment) = update.comment {
            marker.comment = comment;
        }
        if let Some(color) = update.color {
            marker.color = color;
        }
        if let Some(start) = update.timeline_start {
            marker.marked_range.set_start_time(start - offset);
        }
        if let Some(duration) = update.duration {
            marker.marked_range.set_duration(duration.max(0.0));
        }
        true
    }

    /// Remove the marker with id `marker_id` and return it (with its range
    /// still in its owner's time), or `None` when no such marker exists.
    pub fn delete_marker(&mut self, marker_id: &str) -> Option<Marker> {
        Some(match self.locate_marker(marker_id)? {
            MarkerLocation::Stack(mi) => self.markers.remove(mi),
            MarkerLocation::Track(ti, mi) => self.children[ti].markers.remove(mi),
            MarkerLocation::Item(ti, ii, mi, _) => {
                item_markers_mut(&mut self.children[ti].items[ii]).remove(mi)
            }
        })
    }
}

impl Timeline {
    /// See [`Stack::get_markers`].
    pub fn get_markers(&self) -> Vec<MarkerInfo> {
        self.tracks.get_markers()
    }
    /// See [`Stack::get_marker`].
    pub fn get_marker(&self, marker_id: &str) -> Option<MarkerInfo> {
        self.tracks.get_marker(marker_id)
    }
    /// See [`Stack::add_marker`].
    pub fn add_marker(&mut self, owner_id: Option<&str>, marker: Marker) -> Option<String> {
        self.tracks.add_marker(owner_id, marker)
    }
    /// See [`Stack::update_marker`].
    pub fn update_marker(&mut self, marker_id: &str, update: MarkerUpdate) -> bool {
        self.tracks.update_marker(marker_id, update)
    }
    /// See [`Stack::delete_marker`].
    pub fn delete_marker(&mut self, marker_id: &str) -> Option<Marker> {
        self.tracks.delete_marker(marker_id)
    }
}
