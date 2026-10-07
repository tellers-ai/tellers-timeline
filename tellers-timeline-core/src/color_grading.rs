//! `.cube` LUT colour grading attached to a timeline and its clips.
//!
//! LUTs live at `metadata["tellers.ai"]["color_grading"]`, an ordered list of
//!
//! ```json
//! [
//!   { "asset_id": "a1b2", "name": "slog3_to_709.cube", "cube": "https://cdn.example.test/…" },
//!   { "asset_id": "c3d4", "name": "warm.cube" }
//! ]
//! ```
//!
//! - `asset_id` identifies the `.cube` file. It is what the timeline
//!   persists and what the list methods take.
//! - `cube` is the URL the player (`video-player-js`) downloads the table
//!   from. It is resolved from the asset id, like a media `target_url`: set it
//!   with [`Timeline::set_color_lut_url`] / [`Timeline::set_color_lut_urls`]
//!   before handing the timeline to the player, and strip it with
//!   [`Timeline::clear_color_lut_urls`] so ephemeral (e.g. presigned) URLs are
//!   not persisted. The player skips an entry without one.
//! - `name` is an optional display label.
//!
//! The list has the same shape at both stages:
//!
//! - on a **clip**, graded on that clip only, in list order (typically a
//!   technical transform such as camera log → Rec.709, then a look);
//! - on the **timeline**, graded over the whole composited image, text
//!   included, after every clip's own LUTs.
//!
//! Tracks are not a grading stage. The player grades a clip's LUTs only for
//! video and image clips on **video** tracks: clips on audio or other tracks
//! and text (or other generator) clips are not graded, so the timeline's
//! by-id writers refuse them and its URL resolution skips them (see
//! [`Timeline::is_item_color_gradable`]). The `Clip` and `Item` methods do
//! not know their track and stay unrestricted.
//!
//! An entry is usable when its `asset_id` is a non-empty string; reads skip
//! the others. Indices passed to the list methods count usable entries only,
//! which are exactly what [`resolve_color_luts`] returns. List writes drop
//! unusable entries and keep the other keys of untouched entries.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{Clip, Item, MediaReference, Timeline, TrackKind};

/// Metadata key, under the `tellers.ai` namespace, holding the LUT list.
pub const COLOR_GRADING_KEY: &str = "color_grading";

/// Key, inside a list entry, holding the asset id of the `.cube` file.
const ASSET_ID_KEY: &str = "asset_id";
/// Key, inside a list entry, holding the resolved `.cube` URL.
const CUBE_KEY: &str = "cube";
/// Key, inside a list entry, holding the optional display name.
const NAME_KEY: &str = "name";

/// A `.cube` LUT referenced by asset id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorLut {
    /// Asset id of the `.cube` file.
    pub asset_id: String,
    /// Optional display name, typically the file name (`look.cube`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The URL the player downloads the file from, once resolved from the
    /// asset id. Stored as `cube`.
    #[serde(rename = "cube", default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl ColorLut {
    /// A LUT with no display name and no resolved URL.
    pub fn new(asset_id: impl Into<String>) -> Self {
        ColorLut {
            asset_id: asset_id.into(),
            name: None,
            url: None,
        }
    }

    /// Set the display name of this LUT.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the resolved URL of this LUT.
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// This LUT with every field trimmed and an empty name or URL dropped.
    /// `None` when the asset id is empty.
    pub fn normalized(&self) -> Option<ColorLut> {
        let asset_id = self.asset_id.trim();
        if asset_id.is_empty() {
            return None;
        }
        Some(ColorLut {
            asset_id: asset_id.to_string(),
            name: non_empty(self.name.as_deref()),
            url: non_empty(self.url.as_deref()),
        })
    }

    /// Whether this LUT would be kept by [`ColorLut::normalized`].
    pub fn is_valid(&self) -> bool {
        self.normalized().is_some()
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("ColorLut serializes to JSON")
    }
}

/// The LUTs in `metadata["tellers.ai"]["color_grading"]`, in grading order,
/// normalized and with unusable entries skipped.
pub fn resolve_color_luts(metadata: &serde_json::Value) -> Vec<ColorLut> {
    usable_entries(metadata)
        .iter()
        .filter_map(lut_from_json)
        .collect()
}

/// Replace the whole LUT list with `luts`. An empty list removes the key.
/// Returns `false` — leaving the metadata untouched — when any asset id is
/// empty.
pub fn set_color_luts(metadata: &mut serde_json::Value, luts: Vec<ColorLut>) -> bool {
    let Some(luts) = luts
        .iter()
        .map(ColorLut::normalized)
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    write_entries(metadata, luts.iter().map(ColorLut::to_json).collect());
    true
}

/// Append `lut` to the end of the list (graded last). Returns `false`,
/// changing nothing, when the asset id is empty.
pub fn push_color_lut(metadata: &mut serde_json::Value, lut: ColorLut) -> bool {
    let len = usable_entries(metadata).len();
    insert_color_lut_at(metadata, len, lut)
}

/// Insert `lut` at `index` (0 grades first; the list length appends).
/// Returns `false`, changing nothing, when `index` is past the end or the
/// asset id is empty.
pub fn insert_color_lut_at(metadata: &mut serde_json::Value, index: usize, lut: ColorLut) -> bool {
    let Some(lut) = lut.normalized() else {
        return false;
    };
    let mut entries = usable_entries(metadata);
    if index > entries.len() {
        return false;
    }
    entries.insert(index, lut.to_json());
    write_entries(metadata, entries);
    true
}

/// Replace the LUT at `index` with `lut`, returning the previous one. `None`,
/// changing nothing, when there is no LUT at `index` or the asset id is
/// empty.
pub fn replace_color_lut_at(
    metadata: &mut serde_json::Value,
    index: usize,
    lut: ColorLut,
) -> Option<ColorLut> {
    let lut = lut.normalized()?;
    let mut entries = usable_entries(metadata);
    let previous = lut_from_json(entries.get(index)?)?;
    entries[index] = lut.to_json();
    write_entries(metadata, entries);
    Some(previous)
}

/// Remove and return the LUT at `index`. `None`, changing nothing, when there
/// is no LUT at `index`. The key is dropped once the list is empty.
pub fn remove_color_lut_at(metadata: &mut serde_json::Value, index: usize) -> Option<ColorLut> {
    let mut entries = usable_entries(metadata);
    if index >= entries.len() {
        return None;
    }
    let removed = lut_from_json(&entries.remove(index));
    write_entries(metadata, entries);
    removed
}

/// Remove every LUT, returning whether any usable one was present.
pub fn clear_color_luts(metadata: &mut serde_json::Value) -> bool {
    let had_luts = !usable_entries(metadata).is_empty();
    if let Some(ai) = metadata
        .get_mut("tellers.ai")
        .and_then(|value| value.as_object_mut())
    {
        ai.remove(COLOR_GRADING_KEY);
    }
    had_luts
}

/// Set the resolved URL of every LUT whose asset id is in `urls` (asset id →
/// URL), returning how many entries were updated. Other entries are left as
/// they are. An empty URL is ignored; use [`clear_color_lut_urls`] to strip
/// URLs.
pub fn set_color_lut_urls(
    metadata: &mut serde_json::Value,
    urls: &HashMap<String, String>,
) -> usize {
    let mut updated = 0;
    for entry in entries_mut(metadata) {
        let Some(url) = entry
            .get(ASSET_ID_KEY)
            .and_then(|id| id.as_str())
            .and_then(|id| urls.get(id.trim()))
            .and_then(|url| non_empty(Some(url)))
        else {
            continue;
        };
        if let Some(entry) = entry.as_object_mut() {
            entry.insert(CUBE_KEY.to_string(), serde_json::Value::String(url));
            updated += 1;
        }
    }
    updated
}

/// Remove the resolved URL from every LUT, keeping its asset id, and return
/// how many entries had one.
pub fn clear_color_lut_urls(metadata: &mut serde_json::Value) -> usize {
    entries_mut(metadata)
        .filter_map(|entry| entry.as_object_mut()?.remove(CUBE_KEY))
        .count()
}

/// Generates the LUT list accessors for a type holding its metadata in
/// `self.metadata`, so the timeline and clip stages stay identical.
macro_rules! impl_color_luts {
    ($ty:ty, $stage:literal) => {
        impl $ty {
            #[doc = concat!("The LUTs graded ", $stage, ", in order.")]
            pub fn get_color_luts(&self) -> Vec<ColorLut> {
                resolve_color_luts(&self.metadata)
            }

            #[doc = concat!("Replace every LUT graded ", $stage, " with `luts`; see [`set_color_luts`].")]
            pub fn set_color_luts(&mut self, luts: Vec<ColorLut>) -> bool {
                set_color_luts(&mut self.metadata, luts)
            }

            #[doc = concat!("Append a LUT graded ", $stage, "; see [`push_color_lut`].")]
            pub fn push_color_lut(&mut self, lut: ColorLut) -> bool {
                push_color_lut(&mut self.metadata, lut)
            }

            #[doc = concat!("Insert a LUT graded ", $stage, " at `index`; see [`insert_color_lut_at`].")]
            pub fn insert_color_lut_at(&mut self, index: usize, lut: ColorLut) -> bool {
                insert_color_lut_at(&mut self.metadata, index, lut)
            }

            #[doc = concat!("Replace the LUT graded ", $stage, " at `index`; see [`replace_color_lut_at`].")]
            pub fn replace_color_lut_at(&mut self, index: usize, lut: ColorLut) -> Option<ColorLut> {
                replace_color_lut_at(&mut self.metadata, index, lut)
            }

            #[doc = concat!("Remove the LUT graded ", $stage, " at `index`; see [`remove_color_lut_at`].")]
            pub fn remove_color_lut_at(&mut self, index: usize) -> Option<ColorLut> {
                remove_color_lut_at(&mut self.metadata, index)
            }

            #[doc = concat!("Remove every LUT graded ", $stage, "; see [`clear_color_luts`].")]
            pub fn clear_color_luts(&mut self) -> bool {
                clear_color_luts(&mut self.metadata)
            }
        }
    };
}

impl_color_luts!(Timeline, "over the whole composited image");
impl_color_luts!(Clip, "on this clip");

impl Clip {
    /// Set the resolved URL of this clip's LUTs from `urls` (asset id → URL);
    /// see [`set_color_lut_urls`].
    pub fn set_color_lut_urls(&mut self, urls: &HashMap<String, String>) -> usize {
        set_color_lut_urls(&mut self.metadata, urls)
    }

    /// Strip the resolved URL from this clip's LUTs; see
    /// [`clear_color_lut_urls`].
    pub fn clear_color_lut_urls(&mut self) -> usize {
        clear_color_lut_urls(&mut self.metadata)
    }

    /// Whether the player grades this clip's LUTs when it sits on a video
    /// track: its active media reference (`DEFAULT_MEDIA` when unset) is an
    /// external reference, i.e. video or image media. Text and other
    /// generator references are not graded. An empty `target_url` still
    /// counts, since media URLs are resolved later.
    pub fn is_color_gradable(&self) -> bool {
        let key = self
            .active_media_reference_key
            .as_deref()
            .unwrap_or("DEFAULT_MEDIA");
        matches!(
            self.media_references.get(key),
            Some(MediaReference::ExternalReference { .. })
        )
    }
}

impl Timeline {
    /// Every distinct LUT asset id the player grades with: the timeline's,
    /// then each graded clip's (see [`Timeline::is_item_color_gradable`]) in
    /// track order. These are the ids to resolve into URLs for
    /// [`Timeline::set_color_lut_urls`].
    pub fn color_lut_asset_ids(&self) -> Vec<String> {
        let clip_luts = self.graded_clips().flat_map(Clip::get_color_luts);
        let mut ids: Vec<String> = Vec::new();
        for lut in self.get_color_luts().into_iter().chain(clip_luts) {
            if !ids.contains(&lut.asset_id) {
                ids.push(lut.asset_id);
            }
        }
        ids
    }

    /// Set the resolved URL of every LUT, on the timeline and on every graded
    /// clip, whose asset id is `asset_id`. Returns how many entries were
    /// updated; 0 when the URL is empty or no LUT uses this id.
    pub fn set_color_lut_url(&mut self, asset_id: &str, url: &str) -> usize {
        let urls = HashMap::from([(asset_id.trim().to_string(), url.to_string())]);
        self.set_color_lut_urls(&urls)
    }

    /// Set the resolved URL of every LUT, on the timeline and on every graded
    /// clip, whose asset id is in `urls` (asset id → URL). Unknown ids are
    /// ignored. Returns how many entries were updated.
    pub fn set_color_lut_urls(&mut self, urls: &HashMap<String, String>) -> usize {
        set_color_lut_urls(&mut self.metadata, urls)
            + self
                .graded_clips_mut()
                .map(|clip| clip.set_color_lut_urls(urls))
                .sum::<usize>()
    }

    /// Strip the resolved URL from every LUT on the timeline and on every
    /// clip (graded or not), keeping the asset ids, like
    /// [`Timeline::clear_target_urls`] does for media. Returns how many
    /// entries had one.
    pub fn clear_color_lut_urls(&mut self) -> usize {
        clear_color_lut_urls(&mut self.metadata)
            + self
                .tracks
                .children
                .iter_mut()
                .flat_map(|track| track.items.iter_mut())
                .filter_map(Item::clip_mut)
                .map(Clip::clear_color_lut_urls)
                .sum::<usize>()
    }

    /// Whether the player grades the LUTs of the item `item_id`: a clip on a
    /// video track whose media is video or an image (see
    /// [`Clip::is_color_gradable`]). `false` for gaps, text clips, clips on
    /// audio or other tracks, and unknown ids.
    pub fn is_item_color_gradable(&self, item_id: &str) -> bool {
        self.find_clip(item_id)
            .is_some_and(|(kind, clip)| kind == TrackKind::Video && clip.is_color_gradable())
    }

    /// The LUTs stored on the clip `item_id`, graded or not. `None` when no
    /// clip has this id.
    pub fn get_item_color_luts(&self, item_id: &str) -> Option<Vec<ColorLut>> {
        Some(self.find_clip(item_id)?.1.get_color_luts())
    }

    /// [`Clip::set_color_luts`] on the clip `item_id`; `false` when it is not
    /// a graded clip (see [`Timeline::is_item_color_gradable`]).
    pub fn set_item_color_luts(&mut self, item_id: &str, luts: Vec<ColorLut>) -> bool {
        self.graded_clip_mut(item_id)
            .is_some_and(|clip| clip.set_color_luts(luts))
    }

    /// [`Clip::push_color_lut`] on the clip `item_id`; `false` when it is not
    /// a graded clip.
    pub fn push_item_color_lut(&mut self, item_id: &str, lut: ColorLut) -> bool {
        self.graded_clip_mut(item_id)
            .is_some_and(|clip| clip.push_color_lut(lut))
    }

    /// [`Clip::insert_color_lut_at`] on the clip `item_id`; `false` when it
    /// is not a graded clip.
    pub fn insert_item_color_lut_at(&mut self, item_id: &str, index: usize, lut: ColorLut) -> bool {
        self.graded_clip_mut(item_id)
            .is_some_and(|clip| clip.insert_color_lut_at(index, lut))
    }

    /// [`Clip::replace_color_lut_at`] on the clip `item_id`; `None` when it
    /// is not a graded clip.
    pub fn replace_item_color_lut_at(
        &mut self,
        item_id: &str,
        index: usize,
        lut: ColorLut,
    ) -> Option<ColorLut> {
        self.graded_clip_mut(item_id)?
            .replace_color_lut_at(index, lut)
    }

    /// [`Clip::remove_color_lut_at`] on the clip `item_id`, graded or not, so
    /// stale LUTs can always be cleaned up; `None` when no clip has this id.
    pub fn remove_item_color_lut_at(&mut self, item_id: &str, index: usize) -> Option<ColorLut> {
        self.find_clip_mut(item_id)?.1.remove_color_lut_at(index)
    }

    /// [`Clip::clear_color_luts`] on the clip `item_id`, graded or not;
    /// `false` when no clip has this id.
    pub fn clear_item_color_luts(&mut self, item_id: &str) -> bool {
        self.find_clip_mut(item_id)
            .is_some_and(|(_, clip)| clip.clear_color_luts())
    }

    /// Clips whose LUTs the player grades: on video tracks, with video or
    /// image media.
    fn graded_clips(&self) -> impl Iterator<Item = &Clip> {
        self.tracks
            .children
            .iter()
            .filter(|track| track.kind == TrackKind::Video)
            .flat_map(|track| track.items.iter())
            .filter_map(|item| match item {
                Item::Clip(clip) if clip.is_color_gradable() => Some(clip),
                _ => None,
            })
    }

    fn graded_clips_mut(&mut self) -> impl Iterator<Item = &mut Clip> {
        self.tracks
            .children
            .iter_mut()
            .filter(|track| track.kind == TrackKind::Video)
            .flat_map(|track| track.items.iter_mut())
            .filter_map(Item::clip_mut)
            .filter(|clip| clip.is_color_gradable())
    }

    fn graded_clip_mut(&mut self, item_id: &str) -> Option<&mut Clip> {
        match self.find_clip_mut(item_id)? {
            (TrackKind::Video, clip) if clip.is_color_gradable() => Some(clip),
            _ => None,
        }
    }

    /// The clip `item_id` and the kind of its track.
    fn find_clip(&self, item_id: &str) -> Option<(TrackKind, &Clip)> {
        self.tracks
            .children
            .iter()
            .find_map(|track| match track.get_item_by_id(item_id)?.1 {
                Item::Clip(clip) => Some((track.kind.clone(), clip)),
                Item::Gap(_) => None,
            })
    }

    fn find_clip_mut(&mut self, item_id: &str) -> Option<(TrackKind, &mut Clip)> {
        self.tracks.children.iter_mut().find_map(|track| {
            let (index, _) = track.get_item_by_id(item_id)?;
            let kind = track.kind.clone();
            Some((kind, track.items.get_mut(index)?.clip_mut()?))
        })
    }
}

impl Item {
    /// [`Clip::is_color_gradable`]; `false` for gaps.
    pub fn is_color_gradable(&self) -> bool {
        match self {
            Item::Clip(clip) => clip.is_color_gradable(),
            Item::Gap(_) => false,
        }
    }

    /// The clip's LUTs; always empty for gaps.
    pub fn get_color_luts(&self) -> Vec<ColorLut> {
        match self {
            Item::Clip(clip) => clip.get_color_luts(),
            Item::Gap(_) => Vec::new(),
        }
    }

    /// [`Clip::set_color_luts`]; `false` for gaps.
    pub fn set_color_luts(&mut self, luts: Vec<ColorLut>) -> bool {
        self.clip_mut()
            .is_some_and(|clip| clip.set_color_luts(luts))
    }

    /// [`Clip::push_color_lut`]; `false` for gaps.
    pub fn push_color_lut(&mut self, lut: ColorLut) -> bool {
        self.clip_mut().is_some_and(|clip| clip.push_color_lut(lut))
    }

    /// [`Clip::insert_color_lut_at`]; `false` for gaps.
    pub fn insert_color_lut_at(&mut self, index: usize, lut: ColorLut) -> bool {
        self.clip_mut()
            .is_some_and(|clip| clip.insert_color_lut_at(index, lut))
    }

    /// [`Clip::replace_color_lut_at`]; `None` for gaps.
    pub fn replace_color_lut_at(&mut self, index: usize, lut: ColorLut) -> Option<ColorLut> {
        self.clip_mut()?.replace_color_lut_at(index, lut)
    }

    /// [`Clip::remove_color_lut_at`]; `None` for gaps.
    pub fn remove_color_lut_at(&mut self, index: usize) -> Option<ColorLut> {
        self.clip_mut()?.remove_color_lut_at(index)
    }

    /// [`Clip::clear_color_luts`]; `false` for gaps.
    pub fn clear_color_luts(&mut self) -> bool {
        self.clip_mut().is_some_and(|clip| clip.clear_color_luts())
    }

    fn clip_mut(&mut self) -> Option<&mut Clip> {
        match self {
            Item::Clip(clip) => Some(clip),
            Item::Gap(_) => None,
        }
    }
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// A list entry as a LUT, if usable.
fn lut_from_json(entry: &serde_json::Value) -> Option<ColorLut> {
    let text = |key| entry.get(key).and_then(|v| v.as_str()).map(str::to_string);
    ColorLut {
        asset_id: text(ASSET_ID_KEY)?,
        name: text(NAME_KEY),
        url: text(CUBE_KEY),
    }
    .normalized()
}

/// The raw entries of the LUT list that are usable, in order.
fn usable_entries(metadata: &serde_json::Value) -> Vec<serde_json::Value> {
    metadata
        .get("tellers.ai")
        .and_then(|ai| ai.get(COLOR_GRADING_KEY))
        .and_then(|grading| grading.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| lut_from_json(entry).is_some())
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// Every raw entry of the LUT list, for in-place updates.
fn entries_mut(metadata: &mut serde_json::Value) -> impl Iterator<Item = &mut serde_json::Value> {
    metadata
        .get_mut("tellers.ai")
        .and_then(|ai| ai.get_mut(COLOR_GRADING_KEY))
        .and_then(|grading| grading.as_array_mut())
        .into_iter()
        .flatten()
}

/// Store `entries` as the LUT list, creating the `tellers.ai` object as
/// needed, or drop the key when `entries` is empty.
fn write_entries(metadata: &mut serde_json::Value, entries: Vec<serde_json::Value>) {
    if entries.is_empty() {
        clear_color_luts(metadata);
        return;
    }
    if metadata.as_object().is_none() {
        *metadata = serde_json::Value::Object(serde_json::Map::new());
    }
    let ai = metadata
        .as_object_mut()
        .unwrap()
        .entry("tellers.ai".to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if ai.as_object().is_none() {
        *ai = serde_json::Value::Object(serde_json::Map::new());
    }
    ai.as_object_mut().unwrap().insert(
        COLOR_GRADING_KEY.to_string(),
        serde_json::Value::Array(entries),
    );
}
