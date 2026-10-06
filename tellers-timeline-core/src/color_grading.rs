//! `.cube` LUT colour grading attached to a timeline and its clips.
//!
//! The player (`video-player-js`) grades with 3D `.cube` LUTs read from
//! `metadata["tellers.ai"]["color_grading"]`, an ordered list of
//!
//! ```json
//! [
//!   { "name": "slog3_to_709.cube", "cube": "https://cdn.example.test/slog3_to_709.cube" },
//!   { "name": "warm.cube", "cube": "https://cdn.example.test/warm.cube" }
//! ]
//! ```
//!
//! where `cube` is the URL the player downloads the table from and `name` is
//! an optional display label. The list has the same shape at both stages:
//!
//! - on a **clip**, graded on that clip only, in list order (typically a
//!   technical transform such as camera log → Rec.709, then a look);
//! - on the **timeline**, graded over the whole composited image, text
//!   included, after every clip's own LUTs.
//!
//! Tracks are not a grading stage.
//!
//! Reads skip entries the player would ignore (a `cube` that is not a
//! non-empty string) and read a legacy single `{ "cube", "name" }` object as a
//! one-entry list. Indices passed to the list methods count the usable
//! entries only, which are exactly what [`resolve_color_luts`] returns.
//! Writes always store the list form, drop unusable entries, and keep the
//! other keys of untouched entries. The URL is not otherwise restricted: the
//! player fetches it as-is, so a path relative to the page (`/luts/warm.cube`)
//! is valid.

use serde::{Deserialize, Serialize};

use crate::{Clip, Item, Timeline};

/// Metadata key, under the `tellers.ai` namespace, holding the LUT list.
pub const COLOR_GRADING_KEY: &str = "color_grading";

/// Key, inside a list entry, holding the `.cube` URL.
const CUBE_KEY: &str = "cube";
/// Key, inside a list entry, holding the optional display name.
const NAME_KEY: &str = "name";

/// A `.cube` LUT referenced by URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorLut {
    /// Where the player downloads the `.cube` file from. Stored as `cube`.
    #[serde(rename = "cube")]
    pub url: String,
    /// Optional display name, typically the file name (`look.cube`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ColorLut {
    /// A LUT with no display name.
    pub fn new(url: impl Into<String>) -> Self {
        ColorLut {
            url: url.into(),
            name: None,
        }
    }

    /// Set the display name of this LUT.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// This LUT with url and name trimmed and an empty name dropped. `None`
    /// when the player would ignore it: an empty URL.
    pub fn normalized(&self) -> Option<ColorLut> {
        let url = self.url.trim();
        if url.is_empty() {
            return None;
        }
        Some(ColorLut {
            url: url.to_string(),
            name: self
                .name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string),
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
/// Returns `false` — leaving the metadata untouched — when any LUT has an
/// empty URL.
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
/// changing nothing, when the URL is empty.
pub fn push_color_lut(metadata: &mut serde_json::Value, lut: ColorLut) -> bool {
    let len = usable_entries(metadata).len();
    insert_color_lut_at(metadata, len, lut)
}

/// Insert `lut` at `index` (0 grades first; the list length appends).
/// Returns `false`, changing nothing, when `index` is past the end or the
/// URL is empty.
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
/// changing nothing, when there is no LUT at `index` or the URL is empty.
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

impl Timeline {
    /// Every distinct LUT URL the player downloads for this timeline: the
    /// timeline's, then each clip's in track order. Useful to prefetch,
    /// authorize or package the referenced files.
    pub fn color_lut_urls(&self) -> Vec<String> {
        let clip_luts = self
            .tracks
            .children
            .iter()
            .flat_map(|track| track.items.iter())
            .flat_map(Item::get_color_luts);
        let mut urls: Vec<String> = Vec::new();
        for lut in self.get_color_luts().into_iter().chain(clip_luts) {
            if !urls.contains(&lut.url) {
                urls.push(lut.url);
            }
        }
        urls
    }

    /// The LUTs of the clip `item_id`. `None` when no clip has this id.
    pub fn get_item_color_luts(&self, item_id: &str) -> Option<Vec<ColorLut>> {
        Some(self.clip_by_id(item_id)?.get_color_luts())
    }

    /// [`Clip::set_color_luts`] on the clip `item_id`; `false` when no clip
    /// has this id.
    pub fn set_item_color_luts(&mut self, item_id: &str, luts: Vec<ColorLut>) -> bool {
        self.clip_mut_by_id(item_id)
            .is_some_and(|clip| clip.set_color_luts(luts))
    }

    /// [`Clip::push_color_lut`] on the clip `item_id`; `false` when no clip
    /// has this id.
    pub fn push_item_color_lut(&mut self, item_id: &str, lut: ColorLut) -> bool {
        self.clip_mut_by_id(item_id)
            .is_some_and(|clip| clip.push_color_lut(lut))
    }

    /// [`Clip::insert_color_lut_at`] on the clip `item_id`; `false` when no
    /// clip has this id.
    pub fn insert_item_color_lut_at(&mut self, item_id: &str, index: usize, lut: ColorLut) -> bool {
        self.clip_mut_by_id(item_id)
            .is_some_and(|clip| clip.insert_color_lut_at(index, lut))
    }

    /// [`Clip::replace_color_lut_at`] on the clip `item_id`; `None` when no
    /// clip has this id.
    pub fn replace_item_color_lut_at(
        &mut self,
        item_id: &str,
        index: usize,
        lut: ColorLut,
    ) -> Option<ColorLut> {
        self.clip_mut_by_id(item_id)?
            .replace_color_lut_at(index, lut)
    }

    /// [`Clip::remove_color_lut_at`] on the clip `item_id`; `None` when no
    /// clip has this id.
    pub fn remove_item_color_lut_at(&mut self, item_id: &str, index: usize) -> Option<ColorLut> {
        self.clip_mut_by_id(item_id)?.remove_color_lut_at(index)
    }

    /// [`Clip::clear_color_luts`] on the clip `item_id`; `false` when no clip
    /// has this id.
    pub fn clear_item_color_luts(&mut self, item_id: &str) -> bool {
        self.clip_mut_by_id(item_id)
            .is_some_and(|clip| clip.clear_color_luts())
    }

    fn clip_by_id(&self, item_id: &str) -> Option<&Clip> {
        self.tracks
            .children
            .iter()
            .find_map(|track| match track.get_item_by_id(item_id)?.1 {
                Item::Clip(clip) => Some(clip),
                Item::Gap(_) => None,
            })
    }

    fn clip_mut_by_id(&mut self, item_id: &str) -> Option<&mut Clip> {
        self.tracks.children.iter_mut().find_map(|track| {
            let (index, _) = track.get_item_by_id(item_id)?;
            match track.items.get_mut(index)? {
                Item::Clip(clip) => Some(clip),
                Item::Gap(_) => None,
            }
        })
    }
}

impl Item {
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

/// A list entry as a LUT, if the player would use it.
fn lut_from_json(entry: &serde_json::Value) -> Option<ColorLut> {
    ColorLut {
        url: entry.get(CUBE_KEY)?.as_str()?.to_string(),
        name: entry
            .get(NAME_KEY)
            .and_then(|v| v.as_str())
            .map(str::to_string),
    }
    .normalized()
}

/// The raw entries of the LUT list that the player would use, in order. A
/// legacy single object reads as a one-entry list.
fn usable_entries(metadata: &serde_json::Value) -> Vec<serde_json::Value> {
    let Some(grading) = metadata
        .get("tellers.ai")
        .and_then(|ai| ai.get(COLOR_GRADING_KEY))
    else {
        return Vec::new();
    };
    let entries = match grading {
        serde_json::Value::Array(entries) => entries.as_slice(),
        single => std::slice::from_ref(single),
    };
    entries
        .iter()
        .filter(|entry| lut_from_json(entry).is_some())
        .cloned()
        .collect()
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
