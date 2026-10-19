//! `.cube` LUT colour grading attached to a timeline, its tracks and its
//! clips.
//!
//! Each LUT is an OTIO effect in the `effects` list of the clip, the track,
//! or — for the timeline — its root stack (`timeline.tracks`):
//!
//! ```json
//! {
//!   "OTIO_SCHEMA": "Effect.1",
//!   "effect_name": "tellers.ai.ColorLut",
//!   "name": "slog3_to_709.cube",
//!   "metadata": { "tellers.ai": { "asset_id": "a1b2", "parameters": { "cube_url": "https://cdn.example.test/…" } } }
//! }
//! ```
//!
//! - `asset_id` identifies the `.cube` file. It is what the timeline
//!   persists and what the list methods take.
//! - `cube_url` is the URL the player (`video-player-js`) downloads the table
//!   from. It is resolved from the asset id, like a media `target_url`: set it
//!   with [`Timeline::set_color_lut_url`] / [`Timeline::set_color_lut_urls`]
//!   before handing the timeline to the player, and strip it with
//!   [`Timeline::clear_color_lut_urls`] so ephemeral (e.g. presigned) URLs are
//!   not persisted. The player skips an effect without one. Like every
//!   effect parameter it lives in `parameters`; a `cube_url` beside
//!   `parameters`, where it used to be written, is still read and is moved
//!   there by the URL setters.
//! - `name` is an optional display label.
//!
//! The `ColorLut` effects of a stage, in list order, are its LUT list. Other
//! effects are left where they are; writes keep the LUT effects together, at
//! the position of the first one (appended when there was none).
//!
//! The list has the same shape at all three stages, graded in this order:
//!
//! 1. on a **clip**, graded on that clip only, in list order (typically a
//!    technical transform such as camera log → Rec.709, then a look);
//! 2. on a **video track**, graded on every video and image clip of that
//!    track, after each clip's own LUTs. A track's clips never overlap, so
//!    this is the same as grading the track image; text clips on the track
//!    are not graded;
//! 3. on the **timeline**, graded over the whole composited image, text
//!    included, after every clip's and track's LUTs.
//!
//! The player grades a clip's LUTs only for video and image clips on
//! **video** tracks: clips on audio or other tracks and text (or other
//! generator) clips are not graded, so the timeline's by-id writers refuse
//! them and its URL resolution skips them (see
//! [`Timeline::is_item_color_gradable`]). Likewise only video tracks are
//! graded (see [`Timeline::is_track_color_gradable`]). The `Clip`, `Item` and
//! `Track` methods do not know their context and stay unrestricted.
//!
//! A LUT is usable when its `asset_id` is a non-empty string; reads skip the
//! others. Indices passed to the list methods count usable LUTs only, which
//! are exactly what [`resolve_color_luts`] returns. List writes drop unusable
//! LUTs and keep the other keys of untouched ones.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{Clip, Effect, EffectMetadata, Item, MediaReference, Timeline, Track, TrackKind};

/// `effect_name` of a LUT effect.
pub const COLOR_LUT_EFFECT_NAME: &str = "tellers.ai.ColorLut";
/// `effect_name` of a LUT effect written before tellers effects were prefixed, still read.
const LEGACY_COLOR_LUT_EFFECT_NAME: &str = "ColorLut";

/// Key holding the asset id of the `.cube` file.
const ASSET_ID_KEY: &str = "asset_id";
/// Key, inside a LUT effect's `parameters`, holding the resolved `.cube` URL.
const CUBE_URL_KEY: &str = "cube_url";
/// Key of the effect parameters inside `tellers.ai`.
const PARAMETERS_KEY: &str = "parameters";

/// A `.cube` LUT referenced by asset id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorLut {
    /// Asset id of the `.cube` file.
    pub asset_id: String,
    /// Optional display name, typically the file name (`look.cube`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The URL the player downloads the file from, once resolved from the
    /// asset id. Stored as `cube_url`.
    #[serde(rename = "cube_url", default, skip_serializing_if = "Option::is_none")]
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

    /// This LUT as a `ColorLut` effect.
    pub fn to_effect(&self) -> Effect {
        let mut ai = serde_json::Map::new();
        ai.insert(ASSET_ID_KEY.to_string(), self.asset_id.clone().into());
        if let Some(url) = &self.url {
            let mut parameters = serde_json::Map::new();
            parameters.insert(CUBE_URL_KEY.to_string(), url.clone().into());
            ai.insert(
                PARAMETERS_KEY.to_string(),
                serde_json::Value::Object(parameters),
            );
        }
        let mut other = serde_json::Map::new();
        other.insert("tellers.ai".to_string(), serde_json::Value::Object(ai));
        Effect {
            otio_schema: "Effect.1".to_string(),
            name: self.name.clone().unwrap_or_default(),
            effect_name: COLOR_LUT_EFFECT_NAME.to_string(),
            metadata: EffectMetadata {
                resolve_otio: None,
                other,
            },
        }
    }

    /// The LUT of a `ColorLut` effect, if it is one and is usable.
    pub fn from_effect(effect: &Effect) -> Option<ColorLut> {
        if !is_lut_effect(effect) {
            return None;
        }
        let ai = effect.metadata.other.get("tellers.ai");
        let text = |value: Option<&serde_json::Value>| value?.as_str().map(str::to_string);
        let url = text(ai.and_then(|ai| ai.get(PARAMETERS_KEY)?.get(CUBE_URL_KEY)))
            .or_else(|| text(ai.and_then(|ai| ai.get(CUBE_URL_KEY))));
        ColorLut {
            asset_id: text(ai.and_then(|ai| ai.get(ASSET_ID_KEY)))?,
            name: Some(effect.name.clone()),
            url,
        }
        .normalized()
    }
}

/// Whether `effect` is a `ColorLut` effect, usable or not.
pub fn is_lut_effect(effect: &Effect) -> bool {
    effect.effect_name == COLOR_LUT_EFFECT_NAME
        || effect.effect_name == LEGACY_COLOR_LUT_EFFECT_NAME
}

/// Give every LUT effect in `effects` written under the legacy `ColorLut`
/// name the current `tellers.ai.ColorLut` one, returning how many were
/// renamed. Run by [`crate::Stack::sanitize`], so stored timelines converge
/// on the new name without a migration.
pub fn rename_legacy_color_lut_effects(effects: &mut [Effect]) -> usize {
    effects
        .iter_mut()
        .filter(|effect| effect.effect_name == LEGACY_COLOR_LUT_EFFECT_NAME)
        .map(|effect| effect.effect_name = COLOR_LUT_EFFECT_NAME.to_string())
        .count()
}

/// The LUTs of a stage, in grading order, normalized and with unusable ones
/// skipped.
pub fn resolve_color_luts(effects: &[Effect]) -> Vec<ColorLut> {
    effects.iter().filter_map(ColorLut::from_effect).collect()
}

/// Replace the whole LUT list with `luts`. An empty list removes every LUT
/// effect. Returns `false` — leaving the stage untouched — when any asset id
/// is empty.
pub fn set_color_luts(effects: &mut Vec<Effect>, luts: Vec<ColorLut>) -> bool {
    let Some(luts) = luts
        .iter()
        .map(ColorLut::normalized)
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    write_lut_effects(effects, luts.iter().map(ColorLut::to_effect).collect());
    true
}

/// Append `lut` to the end of the list (graded last). Returns `false`,
/// changing nothing, when the asset id is empty.
pub fn push_color_lut(effects: &mut Vec<Effect>, lut: ColorLut) -> bool {
    let len = usable_lut_effects(effects).len();
    insert_color_lut_at(effects, len, lut)
}

/// Insert `lut` at `index` (0 grades first; the list length appends).
/// Returns `false`, changing nothing, when `index` is past the end or the
/// asset id is empty.
pub fn insert_color_lut_at(effects: &mut Vec<Effect>, index: usize, lut: ColorLut) -> bool {
    let Some(lut) = lut.normalized() else {
        return false;
    };
    let mut luts = usable_lut_effects(effects);
    if index > luts.len() {
        return false;
    }
    luts.insert(index, lut.to_effect());
    write_lut_effects(effects, luts);
    true
}

/// Replace the LUT at `index` with `lut`, returning the previous one. `None`,
/// changing nothing, when there is no LUT at `index` or the asset id is
/// empty.
pub fn replace_color_lut_at(
    effects: &mut Vec<Effect>,
    index: usize,
    lut: ColorLut,
) -> Option<ColorLut> {
    let lut = lut.normalized()?;
    let mut luts = usable_lut_effects(effects);
    let previous = ColorLut::from_effect(luts.get(index)?)?;
    luts[index] = lut.to_effect();
    write_lut_effects(effects, luts);
    Some(previous)
}

/// Remove and return the LUT at `index`. `None`, changing nothing, when there
/// is no LUT at `index`.
pub fn remove_color_lut_at(effects: &mut Vec<Effect>, index: usize) -> Option<ColorLut> {
    let mut luts = usable_lut_effects(effects);
    if index >= luts.len() {
        return None;
    }
    let removed = ColorLut::from_effect(&luts.remove(index));
    write_lut_effects(effects, luts);
    removed
}

/// Remove every LUT, returning whether any usable one was present.
pub fn clear_color_luts(effects: &mut Vec<Effect>) -> bool {
    let had_luts = !usable_lut_effects(effects).is_empty();
    write_lut_effects(effects, Vec::new());
    had_luts
}

/// Set the resolved URL of every LUT whose asset id is in `urls` (asset id →
/// URL), returning how many were updated. Other LUTs are left as they are.
/// An empty URL is ignored; use [`clear_color_lut_urls`] to strip URLs.
pub fn set_color_lut_urls(effects: &mut [Effect], urls: &HashMap<String, String>) -> usize {
    let mut updated = 0;
    for ai in lut_objects_mut(effects) {
        let Some(url) = ai
            .get(ASSET_ID_KEY)
            .and_then(|id| id.as_str())
            .and_then(|id| urls.get(id.trim()))
            .and_then(|url| non_empty(Some(url)))
        else {
            continue;
        };
        ai.remove(CUBE_URL_KEY);
        let parameters = ai
            .entry(PARAMETERS_KEY)
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if !parameters.is_object() {
            *parameters = serde_json::Value::Object(serde_json::Map::new());
        }
        if let Some(parameters) = parameters.as_object_mut() {
            parameters.insert(CUBE_URL_KEY.to_string(), serde_json::Value::String(url));
        }
        updated += 1;
    }
    updated
}

/// Remove the resolved URL from every LUT, keeping its asset id, and return
/// how many had one.
pub fn clear_color_lut_urls(effects: &mut [Effect]) -> usize {
    lut_objects_mut(effects)
        .map(|ai| {
            let legacy = ai.remove(CUBE_URL_KEY).is_some();
            let current = ai
                .get_mut(PARAMETERS_KEY)
                .and_then(|parameters| parameters.as_object_mut())
                .and_then(|parameters| parameters.remove(CUBE_URL_KEY))
                .is_some();
            if ai
                .get(PARAMETERS_KEY)
                .and_then(|p| p.as_object())
                .is_some_and(|p| p.is_empty())
            {
                ai.remove(PARAMETERS_KEY);
            }
            legacy || current
        })
        .filter(|had_url| *had_url)
        .count()
}

/// A stage holding a LUT list in its effects.
trait LutStage {
    fn lut_effects(&self) -> &[Effect];
    fn lut_effects_mut(&mut self) -> &mut Vec<Effect>;
}

impl LutStage for Clip {
    fn lut_effects(&self) -> &[Effect] {
        &self.effects
    }
    fn lut_effects_mut(&mut self) -> &mut Vec<Effect> {
        &mut self.effects
    }
}

impl LutStage for Track {
    fn lut_effects(&self) -> &[Effect] {
        &self.effects
    }
    fn lut_effects_mut(&mut self) -> &mut Vec<Effect> {
        &mut self.effects
    }
}

/// OTIO timelines have no effects of their own; the root stack holds them.
impl LutStage for Timeline {
    fn lut_effects(&self) -> &[Effect] {
        &self.tracks.effects
    }
    fn lut_effects_mut(&mut self) -> &mut Vec<Effect> {
        &mut self.tracks.effects
    }
}

/// Generates the LUT list accessors for a [`LutStage`], so the timeline,
/// track and clip stages stay identical.
macro_rules! impl_color_luts {
    ($ty:ty, $stage:literal) => {
        impl $ty {
            #[doc = concat!("The LUTs graded ", $stage, ", in order.")]
            pub fn get_color_luts(&self) -> Vec<ColorLut> {
                resolve_color_luts(self.lut_effects())
            }

            #[doc = concat!("Replace every LUT graded ", $stage, " with `luts`; see [`set_color_luts`].")]
            pub fn set_color_luts(&mut self, luts: Vec<ColorLut>) -> bool {
                set_color_luts(self.lut_effects_mut(), luts)
            }

            #[doc = concat!("Append a LUT graded ", $stage, "; see [`push_color_lut`].")]
            pub fn push_color_lut(&mut self, lut: ColorLut) -> bool {
                push_color_lut(self.lut_effects_mut(), lut)
            }

            #[doc = concat!("Insert a LUT graded ", $stage, " at `index`; see [`insert_color_lut_at`].")]
            pub fn insert_color_lut_at(&mut self, index: usize, lut: ColorLut) -> bool {
                insert_color_lut_at(self.lut_effects_mut(), index, lut)
            }

            #[doc = concat!("Replace the LUT graded ", $stage, " at `index`; see [`replace_color_lut_at`].")]
            pub fn replace_color_lut_at(&mut self, index: usize, lut: ColorLut) -> Option<ColorLut> {
                replace_color_lut_at(self.lut_effects_mut(), index, lut)
            }

            #[doc = concat!("Remove the LUT graded ", $stage, " at `index`; see [`remove_color_lut_at`].")]
            pub fn remove_color_lut_at(&mut self, index: usize) -> Option<ColorLut> {
                remove_color_lut_at(self.lut_effects_mut(), index)
            }

            #[doc = concat!("Remove every LUT graded ", $stage, "; see [`clear_color_luts`].")]
            pub fn clear_color_luts(&mut self) -> bool {
                clear_color_luts(self.lut_effects_mut())
            }
        }
    };
}

impl_color_luts!(Timeline, "over the whole composited image");
impl_color_luts!(
    Track,
    "on every video and image clip of this track, after each clip's own"
);
impl_color_luts!(Clip, "on this clip");

impl Track {
    /// Whether the player grades this track's LUTs: it is a video track.
    pub fn is_color_gradable(&self) -> bool {
        self.kind == TrackKind::Video
    }

    /// Set the resolved URL of this track's LUTs from `urls` (asset id →
    /// URL); see [`set_color_lut_urls`].
    pub fn set_color_lut_urls(&mut self, urls: &HashMap<String, String>) -> usize {
        set_color_lut_urls(&mut self.effects, urls)
    }

    /// Strip the resolved URL from this track's LUTs; see
    /// [`clear_color_lut_urls`].
    pub fn clear_color_lut_urls(&mut self) -> usize {
        clear_color_lut_urls(&mut self.effects)
    }
}

impl Clip {
    /// Set the resolved URL of this clip's LUTs from `urls` (asset id → URL);
    /// see [`set_color_lut_urls`].
    pub fn set_color_lut_urls(&mut self, urls: &HashMap<String, String>) -> usize {
        set_color_lut_urls(&mut self.effects, urls)
    }

    /// Strip the resolved URL from this clip's LUTs; see
    /// [`clear_color_lut_urls`].
    pub fn clear_color_lut_urls(&mut self) -> usize {
        clear_color_lut_urls(&mut self.effects)
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
    /// then, for each video track in order, its graded clips' (see
    /// [`Timeline::is_item_color_gradable`]) and the track's own. These are
    /// the ids to resolve into URLs for [`Timeline::set_color_lut_urls`].
    pub fn color_lut_asset_ids(&self) -> Vec<String> {
        let track_luts = self.graded_tracks().flat_map(|track| {
            graded_clips_of(track)
                .flat_map(Clip::get_color_luts)
                .chain(track.get_color_luts())
        });
        let mut ids: Vec<String> = Vec::new();
        for lut in self.get_color_luts().into_iter().chain(track_luts) {
            if !ids.contains(&lut.asset_id) {
                ids.push(lut.asset_id);
            }
        }
        ids
    }

    /// Set the resolved URL of every LUT, on the timeline, every video track
    /// and every graded clip, whose asset id is `asset_id`. Returns how many
    /// entries were updated; 0 when the URL is empty or no LUT uses this id.
    pub fn set_color_lut_url(&mut self, asset_id: &str, url: &str) -> usize {
        let urls = HashMap::from([(asset_id.trim().to_string(), url.to_string())]);
        self.set_color_lut_urls(&urls)
    }

    /// Set the resolved URL of every LUT, on the timeline, every video track
    /// and every graded clip, whose asset id is in `urls` (asset id → URL).
    /// Unknown ids are ignored. Returns how many entries were updated.
    pub fn set_color_lut_urls(&mut self, urls: &HashMap<String, String>) -> usize {
        let mut updated = set_color_lut_urls(&mut self.tracks.effects, urls);
        for track in self
            .tracks
            .children
            .iter_mut()
            .filter(|track| track.is_color_gradable())
        {
            updated += track.set_color_lut_urls(urls);
            updated += track
                .items
                .iter_mut()
                .filter_map(Item::clip_mut)
                .filter(|clip| clip.is_color_gradable())
                .map(|clip| clip.set_color_lut_urls(urls))
                .sum::<usize>();
        }
        updated
    }

    /// Strip the resolved URL from every LUT on the timeline, every track and
    /// every clip (graded or not), keeping the asset ids, like
    /// [`Timeline::clear_target_urls`] does for media. Returns how many
    /// entries had one.
    pub fn clear_color_lut_urls(&mut self) -> usize {
        let mut cleared = clear_color_lut_urls(&mut self.tracks.effects);
        for track in self.tracks.children.iter_mut() {
            cleared += track.clear_color_lut_urls();
            cleared += track
                .items
                .iter_mut()
                .filter_map(Item::clip_mut)
                .map(Clip::clear_color_lut_urls)
                .sum::<usize>();
        }
        cleared
    }

    /// Whether the player grades the LUTs of the track `track_id`: it is a
    /// video track. `false` for audio or other tracks and unknown ids.
    pub fn is_track_color_gradable(&self, track_id: &str) -> bool {
        self.tracks
            .get_track_by_id(track_id)
            .is_some_and(|(_, track)| track.is_color_gradable())
    }

    /// The LUTs stored on the track `track_id`, graded or not. `None` when no
    /// track has this id.
    pub fn get_track_color_luts(&self, track_id: &str) -> Option<Vec<ColorLut>> {
        Some(self.tracks.get_track_by_id(track_id)?.1.get_color_luts())
    }

    /// [`Track::set_color_luts`] on the track `track_id`; `false` when it is
    /// not a video track (see [`Timeline::is_track_color_gradable`]).
    pub fn set_track_color_luts(&mut self, track_id: &str, luts: Vec<ColorLut>) -> bool {
        self.graded_track_mut(track_id)
            .is_some_and(|track| track.set_color_luts(luts))
    }

    /// [`Track::push_color_lut`] on the track `track_id`; `false` when it is
    /// not a video track.
    pub fn push_track_color_lut(&mut self, track_id: &str, lut: ColorLut) -> bool {
        self.graded_track_mut(track_id)
            .is_some_and(|track| track.push_color_lut(lut))
    }

    /// [`Track::insert_color_lut_at`] on the track `track_id`; `false` when
    /// it is not a video track.
    pub fn insert_track_color_lut_at(
        &mut self,
        track_id: &str,
        index: usize,
        lut: ColorLut,
    ) -> bool {
        self.graded_track_mut(track_id)
            .is_some_and(|track| track.insert_color_lut_at(index, lut))
    }

    /// [`Track::replace_color_lut_at`] on the track `track_id`; `None` when
    /// it is not a video track.
    pub fn replace_track_color_lut_at(
        &mut self,
        track_id: &str,
        index: usize,
        lut: ColorLut,
    ) -> Option<ColorLut> {
        self.graded_track_mut(track_id)?
            .replace_color_lut_at(index, lut)
    }

    /// [`Track::remove_color_lut_at`] on the track `track_id`, graded or not,
    /// so stale LUTs can always be cleaned up; `None` when no track has this
    /// id.
    pub fn remove_track_color_lut_at(&mut self, track_id: &str, index: usize) -> Option<ColorLut> {
        self.find_track_mut(track_id)?.remove_color_lut_at(index)
    }

    /// [`Track::clear_color_luts`] on the track `track_id`, graded or not;
    /// `false` when no track has this id.
    pub fn clear_track_color_luts(&mut self, track_id: &str) -> bool {
        self.find_track_mut(track_id)
            .is_some_and(|track| track.clear_color_luts())
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

    /// Tracks whose LUTs, and whose clips' LUTs, the player grades: video
    /// tracks.
    fn graded_tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks
            .children
            .iter()
            .filter(|track| track.is_color_gradable())
    }

    fn graded_track_mut(&mut self, track_id: &str) -> Option<&mut Track> {
        self.find_track_mut(track_id)
            .filter(|track| track.is_color_gradable())
    }

    fn find_track_mut(&mut self, track_id: &str) -> Option<&mut Track> {
        let (index, _) = self.tracks.get_track_by_id(track_id)?;
        self.tracks.children.get_mut(index)
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

/// The clips of `track` with video or image media, whose LUTs the player
/// grades when `track` is a video track.
fn graded_clips_of(track: &Track) -> impl Iterator<Item = &Clip> {
    track.items.iter().filter_map(|item| match item {
        Item::Clip(clip) if clip.is_color_gradable() => Some(clip),
        _ => None,
    })
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

/// The usable LUT effects of a stage.
fn usable_lut_effects(effects: &[Effect]) -> Vec<Effect> {
    effects
        .iter()
        .filter(|effect| ColorLut::from_effect(effect).is_some())
        .cloned()
        .collect()
}

/// Store `luts` as the stage's LUT effects, at the position of its first LUT
/// effect (appended when it had none).
fn write_lut_effects(effects: &mut Vec<Effect>, luts: Vec<Effect>) {
    let at = effects
        .iter()
        .take_while(|effect| !is_lut_effect(effect))
        .count();
    effects.retain(|effect| !is_lut_effect(effect));
    effects.splice(at..at, luts);
}

/// The `tellers.ai` object of every LUT effect of the stage, for in-place
/// updates.
fn lut_objects_mut(
    effects: &mut [Effect],
) -> impl Iterator<Item = &mut serde_json::Map<String, serde_json::Value>> {
    effects
        .iter_mut()
        .filter(|effect| is_lut_effect(effect))
        .filter_map(effect_ai_mut)
}

fn effect_ai_mut(effect: &mut Effect) -> Option<&mut serde_json::Map<String, serde_json::Value>> {
    effect.metadata.other.get_mut("tellers.ai")?.as_object_mut()
}
