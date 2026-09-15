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
//! `metadata["tellers.ai"]["availableFonts"]`, as an array of
//! `{ family, url, weight?, style? }` objects — the exact shape the player
//! takes in its `availableFonts` option, so a caller can hand
//! [`Timeline::get_fonts`] straight to the player.
//!
//! Reads drop entries the player itself would drop (missing family, or a URL
//! that is not `http:`, `https:` or `data:`), and writes store the normalized
//! form, so anything this module returns is a font the player will use.

use serde::{Deserialize, Serialize};

use crate::Timeline;

/// Metadata key, under the `tellers.ai` namespace, holding the font array.
pub const AVAILABLE_FONTS_KEY: &str = "availableFonts";

/// A font declared by a timeline for use by its rich-text clips.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontFace {
    /// CSS family name, as referenced by `font-family` in a clip's Title HTML.
    pub family: String,
    /// Where the font file is fetched from. Must be `https:`, `http:` or
    /// `data:`, and must allow CORS when remote.
    pub url: String,
    /// Optional CSS `font-weight` of this file: `normal`, `bold`, `400`, or a
    /// variable-font range such as `400 700`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<String>,
    /// Optional CSS `font-style` of this file: `normal`, `italic` or `oblique`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
}

impl FontFace {
    /// A font face with no explicit weight or style.
    pub fn new(family: impl Into<String>, url: impl Into<String>) -> Self {
        FontFace {
            family: family.into(),
            url: url.into(),
            weight: None,
            style: None,
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

    /// This face with family/url/weight/style trimmed, and an unusable weight or
    /// style dropped. `None` when the player would ignore the font entirely:
    /// an empty family, or a URL that is not `http:`, `https:` or `data:`.
    pub fn normalized(&self) -> Option<FontFace> {
        let family = self.family.trim();
        let url = self.url.trim();
        if family.is_empty() || !is_supported_font_url(url) {
            return None;
        }
        Some(FontFace {
            family: family.to_string(),
            url: url.to_string(),
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
/// is replaced rather than duplicated, so re-adding a font updates its URL.
/// Returns `false` — leaving the metadata untouched — when the font is one the
/// player could not use (empty family, or an unsupported URL scheme).
pub fn add_available_font(metadata: &mut serde_json::Value, font: FontFace) -> bool {
    let Some(font) = font.normalized() else {
        return false;
    };
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
    /// declaration order. Fonts the player would ignore are skipped; the result
    /// can be handed to the player's `availableFonts` option as-is.
    pub fn get_fonts(&self) -> Vec<FontFace> {
        resolve_available_fonts(&self.metadata)
    }

    /// Declare `font` on this timeline, replacing an existing entry with the
    /// same family (case-insensitive), weight and style. Returns `false`, and
    /// changes nothing, when the font has no family or an unsupported URL
    /// scheme (only `https:`, `http:` and `data:` are usable by the player).
    pub fn add_font(&mut self, font: FontFace) -> bool {
        add_available_font(&mut self.metadata, font)
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
/// string family and url are required, and a weight or style it cannot use is
/// dropped rather than failing the whole entry.
fn font_from_json(raw: &serde_json::Value) -> Option<FontFace> {
    let family = raw.get("family")?.as_str()?.trim();
    let url = raw.get("url")?.as_str()?.trim();
    if family.is_empty() || !is_supported_font_url(url) {
        return None;
    }
    Some(FontFace {
        family: family.to_string(),
        url: url.to_string(),
        weight: raw.get("weight").and_then(normalize_font_weight),
        style: raw
            .get("style")
            .and_then(|value| value.as_str())
            .and_then(normalize_font_style),
    })
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
