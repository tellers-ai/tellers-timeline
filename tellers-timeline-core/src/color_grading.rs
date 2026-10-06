//! `.cube` LUT colour grading attached to a timeline, its clips and their media.
//!
//! The player (`video-player-js`) grades with 3D `.cube` LUTs read from
//! `metadata["tellers.ai"]["color_grading"]`, an object of the shape
//!
//! ```json
//! { "name": "teal_orange.cube", "cube": "https://cdn.example.test/teal_orange.cube" }
//! ```
//!
//! where `cube` is the URL the player downloads the table from and `name` is
//! an optional display label. LUTs are read at three stages and applied in
//! DaVinci Resolve's order:
//!
//! 1. **Asset** — the clip's active media reference: a technical transform,
//!    e.g. camera log → Rec.709.
//! 2. **Clip** — the clip itself: shot correction or a creative look.
//! 3. **Timeline** — the timeline: a look over the whole composited image,
//!    text included.
//!
//! Tracks are not a grading stage; the player ignores a LUT on a track.
//!
//! Reads drop a LUT the player would ignore (a `cube` that is not a non-empty
//! string), and writes store the normalized (trimmed) form, so anything this
//! module returns is a LUT the player will try to apply. The URL is not
//! otherwise restricted: the player fetches it as-is, so a path relative to
//! the page (`/luts/warm.cube`) is valid.

use serde::{Deserialize, Serialize};

use crate::{Clip, Item, MediaReference, Timeline};

/// Metadata key, under the `tellers.ai` namespace, holding the grading object.
pub const COLOR_GRADING_KEY: &str = "color_grading";

/// Key, inside the grading object, holding the `.cube` URL.
const CUBE_KEY: &str = "cube";
/// Key, inside the grading object, holding the optional display name.
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
}

/// The LUT declared in `metadata["tellers.ai"]["color_grading"]`, if the
/// player would use it.
pub fn resolve_color_lut(metadata: &serde_json::Value) -> Option<ColorLut> {
    let grading = metadata.get("tellers.ai")?.get(COLOR_GRADING_KEY)?;
    let url = grading.get(CUBE_KEY)?.as_str()?;
    let name = grading.get(NAME_KEY).and_then(|v| v.as_str());
    ColorLut {
        url: url.to_string(),
        name: name.map(str::to_string),
    }
    .normalized()
}

/// Write `lut` to `metadata["tellers.ai"]["color_grading"]`, creating the
/// `tellers.ai` and grading objects as needed and replacing any previous LUT.
/// Other keys of the grading object are preserved. Returns `false` — leaving
/// the metadata untouched — when the URL is empty.
pub fn set_color_lut(metadata: &mut serde_json::Value, lut: ColorLut) -> bool {
    let Some(lut) = lut.normalized() else {
        return false;
    };
    let grading = color_grading_object_mut(metadata);
    grading.insert(CUBE_KEY.to_string(), serde_json::Value::String(lut.url));
    match lut.name {
        Some(name) => {
            grading.insert(NAME_KEY.to_string(), serde_json::Value::String(name));
        }
        None => {
            grading.remove(NAME_KEY);
        }
    }
    true
}

/// Remove the grading object from `metadata["tellers.ai"]`, returning whether
/// a usable LUT was present.
pub fn remove_color_lut(metadata: &mut serde_json::Value) -> bool {
    let had_lut = resolve_color_lut(metadata).is_some();
    if let Some(ai) = metadata
        .get_mut("tellers.ai")
        .and_then(|value| value.as_object_mut())
    {
        ai.remove(COLOR_GRADING_KEY);
    }
    had_lut
}

impl Timeline {
    /// The timeline-stage LUT, graded over the whole composited image.
    pub fn get_color_lut(&self) -> Option<ColorLut> {
        resolve_color_lut(&self.metadata)
    }

    /// Set (or replace) the timeline-stage LUT. Returns `false`, changing
    /// nothing, when the URL is empty.
    pub fn set_color_lut(&mut self, lut: ColorLut) -> bool {
        set_color_lut(&mut self.metadata, lut)
    }

    /// Remove the timeline-stage LUT, returning whether one was present.
    pub fn remove_color_lut(&mut self) -> bool {
        remove_color_lut(&mut self.metadata)
    }

    /// Every distinct LUT URL the player downloads for this timeline: the
    /// timeline's, then each clip's asset and clip LUTs in track order.
    /// Useful to prefetch, authorize or package the referenced files.
    pub fn color_lut_urls(&self) -> Vec<String> {
        let mut urls: Vec<String> = Vec::new();
        let candidates = self.get_color_lut().into_iter().map(|lut| lut.url).chain(
            self.tracks
                .children
                .iter()
                .flat_map(|track| track.items.iter())
                .flat_map(|item| match item {
                    Item::Clip(clip) => clip.color_lut_urls(),
                    Item::Gap(_) => Vec::new(),
                }),
        );
        for url in candidates {
            if !urls.contains(&url) {
                urls.push(url);
            }
        }
        urls
    }

    /// The clip-stage LUT of the item `item_id`. `None` when there is no LUT
    /// or no clip with this id.
    pub fn get_item_color_lut(&self, item_id: &str) -> Option<ColorLut> {
        self.clip_by_id(item_id)?.get_color_lut()
    }

    /// Set the clip-stage LUT of the item `item_id`. Returns `false`, changing
    /// nothing, when no clip has this id or the URL is empty.
    pub fn set_item_color_lut(&mut self, item_id: &str, lut: ColorLut) -> bool {
        self.clip_mut_by_id(item_id)
            .is_some_and(|clip| clip.set_color_lut(lut))
    }

    /// Remove the clip-stage LUT of the item `item_id`, returning whether one
    /// was present.
    pub fn remove_item_color_lut(&mut self, item_id: &str) -> bool {
        self.clip_mut_by_id(item_id)
            .is_some_and(|clip| clip.remove_color_lut())
    }

    /// The asset-stage LUT of the item `item_id` (on its active media
    /// reference).
    pub fn get_item_asset_color_lut(&self, item_id: &str) -> Option<ColorLut> {
        self.clip_by_id(item_id)?.get_asset_color_lut()
    }

    /// Set the asset-stage LUT of the item `item_id` on its active media
    /// reference. Returns `false`, changing nothing, when no clip has this id,
    /// it has no active media reference, or the URL is empty.
    pub fn set_item_asset_color_lut(&mut self, item_id: &str, lut: ColorLut) -> bool {
        self.clip_mut_by_id(item_id)
            .is_some_and(|clip| clip.set_asset_color_lut(lut))
    }

    /// Remove the asset-stage LUT of the item `item_id`, returning whether one
    /// was present.
    pub fn remove_item_asset_color_lut(&mut self, item_id: &str) -> bool {
        self.clip_mut_by_id(item_id)
            .is_some_and(|clip| clip.remove_asset_color_lut())
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

impl Clip {
    /// The clip-stage LUT, graded after the asset LUT.
    pub fn get_color_lut(&self) -> Option<ColorLut> {
        resolve_color_lut(&self.metadata)
    }

    /// Set (or replace) the clip-stage LUT. Returns `false`, changing nothing,
    /// when the URL is empty.
    pub fn set_color_lut(&mut self, lut: ColorLut) -> bool {
        set_color_lut(&mut self.metadata, lut)
    }

    /// Remove the clip-stage LUT, returning whether one was present.
    pub fn remove_color_lut(&mut self) -> bool {
        remove_color_lut(&mut self.metadata)
    }

    /// The asset-stage LUT, read from the active media reference.
    pub fn get_asset_color_lut(&self) -> Option<ColorLut> {
        self.active_media_reference()?.get_color_lut()
    }

    /// Set (or replace) the asset-stage LUT on the active media reference.
    /// Returns `false`, changing nothing, when there is no active media
    /// reference or the URL is empty.
    pub fn set_asset_color_lut(&mut self, lut: ColorLut) -> bool {
        self.active_media_reference_mut()
            .is_some_and(|reference| reference.set_color_lut(lut))
    }

    /// Remove the asset-stage LUT from the active media reference, returning
    /// whether one was present.
    pub fn remove_asset_color_lut(&mut self) -> bool {
        self.active_media_reference_mut()
            .is_some_and(|reference| reference.remove_color_lut())
    }

    /// The LUT URLs the player applies to this clip, in order: the asset's,
    /// then the clip's.
    pub fn color_lut_urls(&self) -> Vec<String> {
        self.get_asset_color_lut()
            .into_iter()
            .chain(self.get_color_lut())
            .map(|lut| lut.url)
            .collect()
    }

    // The player falls back to DEFAULT_MEDIA when no active key is set.
    fn active_media_reference(&self) -> Option<&MediaReference> {
        let key = self
            .active_media_reference_key
            .as_deref()
            .unwrap_or("DEFAULT_MEDIA");
        self.media_references.get(key)
    }

    fn active_media_reference_mut(&mut self) -> Option<&mut MediaReference> {
        let key = self
            .active_media_reference_key
            .as_deref()
            .unwrap_or("DEFAULT_MEDIA");
        self.media_references.get_mut(key)
    }
}

impl MediaReference {
    /// The asset-stage LUT declared on this media reference.
    pub fn get_color_lut(&self) -> Option<ColorLut> {
        resolve_color_lut(self.metadata())
    }

    /// Set (or replace) this media reference's LUT. Returns `false`, changing
    /// nothing, when the URL is empty.
    pub fn set_color_lut(&mut self, lut: ColorLut) -> bool {
        set_color_lut(self.metadata_mut(), lut)
    }

    /// Remove this media reference's LUT, returning whether one was present.
    pub fn remove_color_lut(&mut self) -> bool {
        remove_color_lut(self.metadata_mut())
    }
}

impl Item {
    /// The clip-stage LUT; always `None` for gaps.
    pub fn get_color_lut(&self) -> Option<ColorLut> {
        match self {
            Item::Clip(clip) => clip.get_color_lut(),
            Item::Gap(_) => None,
        }
    }

    /// Set the clip-stage LUT. Returns `false` for gaps.
    pub fn set_color_lut(&mut self, lut: ColorLut) -> bool {
        match self {
            Item::Clip(clip) => clip.set_color_lut(lut),
            Item::Gap(_) => false,
        }
    }

    /// Remove the clip-stage LUT, returning whether one was present.
    pub fn remove_color_lut(&mut self) -> bool {
        match self {
            Item::Clip(clip) => clip.remove_color_lut(),
            Item::Gap(_) => false,
        }
    }

    /// The asset-stage LUT; always `None` for gaps.
    pub fn get_asset_color_lut(&self) -> Option<ColorLut> {
        match self {
            Item::Clip(clip) => clip.get_asset_color_lut(),
            Item::Gap(_) => None,
        }
    }

    /// Set the asset-stage LUT. Returns `false` for gaps.
    pub fn set_asset_color_lut(&mut self, lut: ColorLut) -> bool {
        match self {
            Item::Clip(clip) => clip.set_asset_color_lut(lut),
            Item::Gap(_) => false,
        }
    }

    /// Remove the asset-stage LUT, returning whether one was present.
    pub fn remove_asset_color_lut(&mut self) -> bool {
        match self {
            Item::Clip(clip) => clip.remove_asset_color_lut(),
            Item::Gap(_) => false,
        }
    }
}

/// The grading object at `metadata["tellers.ai"]["color_grading"]`, creating
/// the `tellers.ai` and grading objects when they are missing or the wrong
/// type.
fn color_grading_object_mut(
    metadata: &mut serde_json::Value,
) -> &mut serde_json::Map<String, serde_json::Value> {
    if metadata.as_object().is_none() {
        *metadata = serde_json::Value::Object(serde_json::Map::new());
    }
    let map = metadata.as_object_mut().unwrap();
    let ai = map
        .entry("tellers.ai".to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if ai.as_object().is_none() {
        *ai = serde_json::Value::Object(serde_json::Map::new());
    }
    let grading = ai
        .as_object_mut()
        .unwrap()
        .entry(COLOR_GRADING_KEY.to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if grading.as_object().is_none() {
        *grading = serde_json::Value::Object(serde_json::Map::new());
    }
    grading.as_object_mut().unwrap()
}
