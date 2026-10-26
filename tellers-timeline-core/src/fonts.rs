//! Custom (remote) fonts declared by a timeline for its rich-text clips.
//!
//! The player (`video-player-js`) renders a Rich Text clip by wrapping its
//! `Title HTML` in an SVG and rasterizing it. A `font-family` used in that HTML
//! only renders if the player can find a matching `@font-face`, so the timeline
//! carries the fonts it needs: family, a fetchable URL, and the optional weight
//! and style of that file. The player matches a declared font to a clip by
//! family name (case-insensitively), downloads it, and embeds it in the SVG.
//!
//! They live in the timeline metadata at
//! `metadata["tellers.ai"]["availableFonts"]`, as an array of objects of one
//! of two distinct shapes (see [`FontSource`]):
//!
//! - a font linked by URL, `{ family, url, weight?, style? }`: the URL is what
//!   the timeline persists and what the player fetches;
//! - a font file uploaded as an asset, `{ family, asset_id, weight?, style? }`:
//!   the asset id is what the timeline persists. Before the timeline goes to
//!   the player (or an export), the host resolves the asset into its `url`
//!   with [`Timeline::set_font_asset_urls`], so the player reads both shapes
//!   the same way and needs no knowledge of assets; the resolved `url` is
//!   stripped with [`Timeline::clear_target_urls`] (or
//!   [`Timeline::clear_font_asset_urls`]) so ephemeral (e.g. presigned) URLs
//!   are not persisted. A linked font's `url` is never touched.
//!
//! Reads drop entries the player itself would drop (missing family, or a URL
//! font whose URL is not `http:`, `https:` or `data:`), and writes store the
//! normalized form. An asset font is kept before its `url` resolves: the
//! player skips it until then.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::Timeline;

/// Metadata key, under the `tellers.ai` namespace, holding the font array.
pub const AVAILABLE_FONTS_KEY: &str = "availableFonts";

/// Key, in an asset font's entry, naming the uploaded font file asset.
pub const FONT_ASSET_ID_KEY: &str = "asset_id";
/// Key of a font entry's URL: a linked font's own, or an uploaded font's,
/// resolved from its asset id.
pub const FONT_URL_KEY: &str = "url";

/// Where a declared font's file comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontSource {
    /// A font file the user linked by URL. Must be `https:`, `http:` or
    /// `data:`, and must allow CORS when remote.
    Url(String),
    /// A font file the user uploaded, stored as the asset `asset_id`.
    /// `url` is where the player fetches it from: resolved from the asset id
    /// before playback or export, and never persisted.
    Asset {
        asset_id: String,
        url: Option<String>,
    },
}

/// A font declared by a timeline for use by its rich-text clips.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "RawFontFace", try_from = "RawFontFace")]
pub struct FontFace {
    /// CSS family name, as referenced by `font-family` in a clip's Title HTML.
    pub family: String,
    /// Where the font file comes from: a URL, or an uploaded asset.
    pub source: FontSource,
    /// Optional CSS `font-weight` of this file: `normal`, `bold`, `400`, or a
    /// variable-font range such as `400 700`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<String>,
    /// Optional CSS `font-style` of this file: `normal`, `italic` or `oblique`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
}

/// The stored JSON shape of a [`FontFace`]: `url` for a URL font,
/// `asset_id` (and, once resolved, `url`) for an asset font.
#[derive(Serialize, Deserialize)]
struct RawFontFace {
    family: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    asset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    weight: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    style: Option<String>,
}

impl From<FontFace> for RawFontFace {
    fn from(font: FontFace) -> Self {
        let (url, asset_id) = match font.source {
            FontSource::Url(url) => (Some(url), None),
            FontSource::Asset { asset_id, url } => (url, Some(asset_id)),
        };
        RawFontFace {
            family: font.family,
            url,
            asset_id,
            weight: font.weight,
            style: font.style,
        }
    }
}

impl TryFrom<RawFontFace> for FontFace {
    type Error = String;

    fn try_from(raw: RawFontFace) -> Result<Self, Self::Error> {
        let source = match (raw.asset_id, raw.url) {
            (Some(asset_id), url) => FontSource::Asset { asset_id, url },
            (None, Some(url)) => FontSource::Url(url),
            (None, None) => return Err("a font needs a url or an asset_id".to_string()),
        };
        Ok(FontFace {
            family: raw.family,
            source,
            weight: raw.weight,
            style: raw.style,
        })
    }
}

impl FontFace {
    /// A font linked by URL, with no explicit weight or style.
    pub fn new(family: impl Into<String>, url: impl Into<String>) -> Self {
        FontFace {
            family: family.into(),
            source: FontSource::Url(url.into()),
            weight: None,
            style: None,
        }
    }

    /// A font uploaded as the asset `asset_id`, with no explicit weight or
    /// style. Its URL is resolved later (see [`Timeline::set_font_asset_urls`]).
    pub fn from_asset(family: impl Into<String>, asset_id: impl Into<String>) -> Self {
        FontFace {
            family: family.into(),
            source: FontSource::Asset {
                asset_id: asset_id.into(),
                url: None,
            },
            weight: None,
            style: None,
        }
    }

    /// The URL the player fetches this font from: a URL font's URL, or an
    /// asset font's resolved URL (`None` until it resolves).
    pub fn url(&self) -> Option<&str> {
        match &self.source {
            FontSource::Url(url) => Some(url),
            FontSource::Asset { url, .. } => url.as_deref(),
        }
    }

    /// The uploaded asset this font is stored as; `None` for a URL font.
    pub fn asset_id(&self) -> Option<&str> {
        match &self.source {
            FontSource::Url(_) => None,
            FontSource::Asset { asset_id, .. } => Some(asset_id),
        }
    }

    /// Set the CSS `font-weight` of this face (`normal`, `bold`, `400`, `400 700`).
    pub fn with_weight(mut self, weight: impl Into<String>) -> Self {
        self.weight = Some(weight.into());
        self
    }

    /// Set the CSS `font-style` of this face (`normal`, `italic`, `oblique`).
    pub fn with_style(mut self, style: impl Into<String>) -> Self {
        self.style = Some(style.into());
        self
    }

    /// This face with its family, URL or asset id, weight and style trimmed,
    /// and an unusable weight, style or resolved asset URL dropped. `None`
    /// when the font can never be used: an empty family, a URL font whose URL
    /// is not `http:`, `https:` or `data:`, or an asset font with no asset id.
    pub fn normalized(&self) -> Option<FontFace> {
        let family = self.family.trim();
        if family.is_empty() {
            return None;
        }
        let source = normalize_font_source(&self.source)?;
        Some(FontFace {
            family: family.to_string(),
            source,
            weight: self
                .weight
                .as_deref()
                .and_then(|w| normalize_font_weight(&serde_json::Value::String(w.to_string()))),
            style: self.style.as_deref().and_then(normalize_font_style),
        })
    }

    /// Whether this font would be kept by [`FontFace::normalized`].
    pub fn is_valid(&self) -> bool {
        self.normalized().is_some()
    }
}

/// The fonts declared in `metadata["tellers.ai"]["availableFonts"]`, normalized
/// and with unusable entries skipped.
pub fn resolve_available_fonts(metadata: &serde_json::Value) -> Vec<FontFace> {
    raw_available_fonts(metadata)
        .map(|entries| entries.iter().filter_map(font_from_json).collect())
        .unwrap_or_default()
}

/// Add `font` to `metadata["tellers.ai"]["availableFonts"]`, creating the
/// `tellers.ai` object and the font array as needed.
///
/// An existing entry with the same family (case-insensitive), weight and style
/// is replaced rather than duplicated, so re-adding a font updates its URL or
/// asset. An asset font is stored without its resolved `url`. Returns `false` —
/// leaving the metadata untouched — when the font is one the player could not
/// use (empty family, an unsupported URL scheme, or no asset id).
pub fn add_available_font(metadata: &mut serde_json::Value, font: FontFace) -> bool {
    let Some(mut font) = font.normalized() else {
        return false;
    };
    // An asset font's URL is resolved for playback, never declared.
    if let FontSource::Asset { url, .. } = &mut font.source {
        *url = None;
    }
    let entry = serde_json::to_value(&font).expect("FontFace serializes to JSON");

    let fonts = available_fonts_array_mut(metadata);
    let existing = fonts
        .iter()
        .position(|raw| font_from_json(raw).is_some_and(|other| same_font_face(&other, &font)));
    match existing {
        Some(index) => fonts[index] = entry,
        None => fonts.push(entry),
    }
    true
}

/// Every distinct asset id of the uploaded fonts declared in `metadata`, in
/// declaration order: the ids to resolve for [`set_font_asset_urls`].
pub fn font_asset_ids(metadata: &serde_json::Value) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for font in resolve_available_fonts(metadata) {
        if let Some(id) = font.asset_id() {
            if !ids.iter().any(|known| known == id) {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

/// Set the `url` of every uploaded font in `metadata` whose asset id is in
/// `urls` (asset id → URL), the field the player loads every font from.
/// Empty or unusable URLs and unknown ids are ignored. Returns how many fonts
/// were updated.
pub fn set_font_asset_urls(
    metadata: &mut serde_json::Value,
    urls: &HashMap<String, String>,
) -> usize {
    let Some(fonts) = raw_available_fonts_mut(metadata) else {
        return 0;
    };
    let mut updated = 0;
    for entry in fonts.iter_mut() {
        let Some(url) = raw_font_asset_id(entry)
            .and_then(|id| urls.get(id))
            .map(|url| url.trim().to_string())
            .filter(|url| is_supported_font_url(url))
        else {
            continue;
        };
        if let Some(entry) = entry.as_object_mut() {
            entry.insert(FONT_URL_KEY.to_string(), serde_json::Value::String(url));
            updated += 1;
        }
    }
    updated
}

/// Remove the resolved `url` of every uploaded font in `metadata`, keeping
/// the asset ids. A linked font (no asset id) keeps its `url`. Returns how
/// many had one.
pub fn clear_font_asset_urls(metadata: &mut serde_json::Value) -> usize {
    let Some(fonts) = raw_available_fonts_mut(metadata) else {
        return 0;
    };
    fonts
        .iter_mut()
        .filter(|entry| raw_font_asset_id(entry).is_some())
        .filter_map(|entry| entry.as_object_mut())
        .filter_map(|entry| entry.remove(FONT_URL_KEY))
        .count()
}

/// Remove every font whose family matches `family` (case-insensitively) from
/// `metadata["tellers.ai"]["availableFonts"]`, returning whether one was
/// present. The array itself is dropped once it is empty.
pub fn remove_available_font(metadata: &mut serde_json::Value, family: &str) -> bool {
    let Some(ai) = metadata
        .get_mut("tellers.ai")
        .and_then(|value| value.as_object_mut())
    else {
        return false;
    };
    let Some(fonts) = ai
        .get_mut(AVAILABLE_FONTS_KEY)
        .and_then(|v| v.as_array_mut())
    else {
        return false;
    };

    let wanted = family.trim().to_lowercase();
    let before = fonts.len();
    fonts.retain(|raw| {
        raw.get("family")
            .and_then(|value| value.as_str())
            .map(|value| value.trim().to_lowercase() != wanted)
            .unwrap_or(true)
    });
    let removed = fonts.len() != before;
    if fonts.is_empty() {
        ai.remove(AVAILABLE_FONTS_KEY);
    }
    removed
}

impl Timeline {
    /// The custom fonts this timeline declares for its rich-text clips, in
    /// declaration order, URL fonts and uploaded (asset) fonts alike. Fonts
    /// the player would ignore are skipped; an asset font is listed before its
    /// `url` resolves.
    pub fn get_fonts(&self) -> Vec<FontFace> {
        resolve_available_fonts(&self.metadata)
    }

    /// Declare `font` on this timeline, replacing an existing entry with the
    /// same family (case-insensitive), weight and style — so uploading a file
    /// for a family linked by URL (or the reverse) switches its source.
    /// Returns `false`, and changes nothing, when the font has no family, a URL
    /// font has an unsupported URL scheme (only `https:`, `http:` and `data:`
    /// are usable by the player), or an asset font has no asset id.
    pub fn add_font(&mut self, font: FontFace) -> bool {
        add_available_font(&mut self.metadata, font)
    }

    /// Every distinct asset id of the uploaded fonts this timeline declares,
    /// in declaration order: the ids to resolve for
    /// [`Timeline::set_font_asset_urls`].
    pub fn font_asset_ids(&self) -> Vec<String> {
        font_asset_ids(&self.metadata)
    }

    /// Set the `url` of every uploaded font whose asset id is in `urls`
    /// (asset id → URL), for the player or an export. Returns how many were
    /// updated.
    pub fn set_font_asset_urls(&mut self, urls: &HashMap<String, String>) -> usize {
        set_font_asset_urls(&mut self.metadata, urls)
    }

    /// [`Timeline::set_font_asset_urls`] for the single asset `asset_id`.
    pub fn set_font_asset_url(&mut self, asset_id: &str, url: &str) -> usize {
        let urls = HashMap::from([(asset_id.trim().to_string(), url.to_string())]);
        self.set_font_asset_urls(&urls)
    }

    /// Remove the resolved `url` of every uploaded font, keeping the asset
    /// ids; linked fonts keep theirs. Returns how many had one.
    /// [`Timeline::clear_target_urls`] does this along with the media
    /// references and effects.
    pub fn clear_font_asset_urls(&mut self) -> usize {
        clear_font_asset_urls(&mut self.metadata)
    }

    /// Remove every declared font with this family (case-insensitive),
    /// returning whether one was present.
    pub fn remove_font(&mut self, family: &str) -> bool {
        remove_available_font(&mut self.metadata, family)
    }
}

/// Two faces of the same family that describe the same file slot: adding one
/// over the other updates the URL instead of declaring a duplicate.
fn same_font_face(left: &FontFace, right: &FontFace) -> bool {
    left.family.eq_ignore_ascii_case(&right.family)
        && left.weight == right.weight
        && left.style == right.style
}

fn raw_available_fonts(metadata: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    metadata
        .get("tellers.ai")?
        .get(AVAILABLE_FONTS_KEY)?
        .as_array()
}

fn raw_available_fonts_mut(
    metadata: &mut serde_json::Value,
) -> Option<&mut Vec<serde_json::Value>> {
    metadata
        .get_mut("tellers.ai")?
        .get_mut(AVAILABLE_FONTS_KEY)?
        .as_array_mut()
}

/// The trimmed, non-empty asset id of a raw font entry.
fn raw_font_asset_id(raw: &serde_json::Value) -> Option<&str> {
    raw.get(FONT_ASSET_ID_KEY)?
        .as_str()
        .map(str::trim)
        .filter(|id| !id.is_empty())
}

/// The font array at `metadata["tellers.ai"]["availableFonts"]`, creating the
/// `tellers.ai` object and the array when they are missing or the wrong type.
fn available_fonts_array_mut(metadata: &mut serde_json::Value) -> &mut Vec<serde_json::Value> {
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
    let ai_map = ai.as_object_mut().unwrap();
    let fonts = ai_map
        .entry(AVAILABLE_FONTS_KEY.to_string())
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    if fonts.as_array().is_none() {
        *fonts = serde_json::Value::Array(Vec::new());
    }
    fonts.as_array_mut().unwrap()
}

/// Read one raw metadata entry, mirroring the player's own normalization: a
/// string family is required, with an asset id (an uploaded font, whose `url`
/// is the resolved one) or a usable url, and a weight, style or resolved url
/// it cannot use is dropped rather than failing the whole entry.
fn font_from_json(raw: &serde_json::Value) -> Option<FontFace> {
    let family = raw.get("family")?.as_str()?.trim();
    if family.is_empty() {
        return None;
    }
    let source = match raw_font_asset_id(raw) {
        Some(asset_id) => FontSource::Asset {
            asset_id: asset_id.to_string(),
            url: raw
                .get(FONT_URL_KEY)
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|url| is_supported_font_url(url))
                .map(str::to_string),
        },
        None => {
            let url = raw.get(FONT_URL_KEY)?.as_str()?.trim();
            if !is_supported_font_url(url) {
                return None;
            }
            FontSource::Url(url.to_string())
        }
    };
    Some(FontFace {
        family: family.to_string(),
        source,
        weight: raw.get("weight").and_then(normalize_font_weight),
        style: raw
            .get("style")
            .and_then(|value| value.as_str())
            .and_then(normalize_font_style),
    })
}

fn normalize_font_source(source: &FontSource) -> Option<FontSource> {
    match source {
        FontSource::Url(url) => {
            let url = url.trim();
            is_supported_font_url(url).then(|| FontSource::Url(url.to_string()))
        }
        FontSource::Asset { asset_id, url } => {
            let asset_id = asset_id.trim();
            if asset_id.is_empty() {
                return None;
            }
            Some(FontSource::Asset {
                asset_id: asset_id.to_string(),
                url: url
                    .as_deref()
                    .map(str::trim)
                    .filter(|url| is_supported_font_url(url))
                    .map(str::to_string),
            })
        }
    }
}

/// The player fetches the font itself, so only schemes it can fetch are usable.
fn is_supported_font_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("data:")
}

/// A CSS `font-weight` the player accepts: `normal`, `bold`, a `[1-9]00` step,
/// or two of those as a variable-font range (`400 700`). Numbers read back as
/// their string form.
fn normalize_font_weight(value: &serde_json::Value) -> Option<String> {
    let raw = match value {
        serde_json::Value::String(text) => text.trim().to_string(),
        serde_json::Value::Number(number) => {
            let as_f64 = number.as_f64()?;
            if as_f64.fract() != 0.0 {
                return None;
            }
            (as_f64 as i64).to_string()
        }
        _ => return None,
    };
    if raw == "normal" || raw == "bold" {
        return Some(raw);
    }
    let steps: Vec<&str> = raw.split_whitespace().collect();
    if steps.is_empty() || steps.len() > 2 || !steps.iter().all(|step| is_weight_step(step)) {
        return None;
    }
    Some(steps.join(" "))
}

fn is_weight_step(step: &str) -> bool {
    let bytes = step.as_bytes();
    bytes.len() == 3 && bytes[0].is_ascii_digit() && bytes[0] != b'0' && &bytes[1..] == b"00"
}

fn normalize_font_style(style: &str) -> Option<String> {
    match style.trim() {
        style @ ("normal" | "italic" | "oblique") => Some(style.to_string()),
        _ => None,
    }
}
