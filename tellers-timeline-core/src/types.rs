use schemars::JsonSchema;
use serde::{de::Error as _, Deserialize, Serialize};
use std::collections::HashMap;

pub type Seconds = f64;

fn default_timeline_schema() -> String {
    "Timeline.1".to_string()
}
fn default_track_schema() -> String {
    "Track.1".to_string()
}
fn default_stack_schema() -> String {
    "Stack.1".to_string()
}
fn default_clip_schema() -> String {
    "Clip.2".to_string()
}
fn default_gap_schema() -> String {
    "Gap.1".to_string()
}
fn default_external_ref_schema() -> String {
    "ExternalReference.1".to_string()
}
fn default_time_range_schema() -> String {
    "TimeRange.1".to_string()
}
fn default_rational_time_schema() -> String {
    "RationalTime.1".to_string()
}

fn gen_hex_id_12() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 6];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Timeline {
    #[serde(rename = "OTIO_SCHEMA", default = "default_timeline_schema")]
    pub otio_schema: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub tracks: Stack,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Track {
    #[serde(rename = "OTIO_SCHEMA", default = "default_track_schema")]
    pub otio_schema: String,
    #[serde(deserialize_with = "deserialize_track_kind_case_insensitive")]
    pub kind: TrackKind,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "children", default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Stack {
    #[serde(rename = "OTIO_SCHEMA", default = "default_stack_schema")]
    pub otio_schema: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub children: Vec<Track>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl Default for Stack {
    fn default() -> Self {
        Self {
            otio_schema: default_stack_schema(),
            name: None,
            children: vec![],
            metadata: serde_json::Value::Null,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    Video,
    Audio,
    Other,
}

fn deserialize_track_kind_case_insensitive<'de, D>(deserializer: D) -> Result<TrackKind, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let lower = s.to_ascii_lowercase();
    match lower.as_str() {
        "video" => Ok(TrackKind::Video),
        "audio" => Ok(TrackKind::Audio),
        "other" => Ok(TrackKind::Other),
        _ => Err(D::Error::unknown_variant(&s, &["video", "audio", "other"])),
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq)]
pub enum Item {
    Clip(Clip),
    Gap(Gap),
}

impl<'de> Deserialize<'de> for Item {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut v = serde_json::Value::deserialize(deserializer)?;
        // Handle externally tagged enum shape produced by our Serialize: { "Clip": {..} } or { "Gap": {..} }
        if let Some(map) = v.as_object() {
            if map.len() == 1 {
                if let Some(inner) = map.get("Clip").cloned().or_else(|| map.get("Gap").cloned()) {
                    v = inner;
                }
            }
        }
        if let Some(t) = v.get("type").and_then(|t| t.as_str()) {
            if t == "gap" {
                let mut vv = v.clone();
                if let Some(map) = vv.as_object_mut() {
                    map.remove("type");
                }
                let gap: Gap = serde_json::from_value(vv)
                    .map_err(|e| D::Error::custom(format!("gap: {}", e)))?;
                return Ok(Item::Gap(gap));
            } else if t == "clip" {
                let mut vv = v.clone();
                if let Some(map) = vv.as_object_mut() {
                    map.remove("type");
                }
                let clip: Clip = serde_json::from_value(vv)
                    .map_err(|e| D::Error::custom(format!("clip: {}", e)))?;
                return Ok(Item::Clip(clip));
            }
        }
        // Prefer explicit OTIO shapes: OTIO_SCHEMA hints
        if let Some(schema) = v.get("OTIO_SCHEMA").and_then(|s| s.as_str()) {
            if schema.starts_with("Gap.") {
                if let Ok(gap) = serde_json::from_value::<Gap>(v.clone()) {
                    return Ok(Item::Gap(gap));
                }
            }
            if schema.starts_with("Clip.") {
                if let Ok(clip) = serde_json::from_value::<Clip>(v.clone()) {
                    return Ok(Item::Clip(clip));
                }
            }
        }

        // Prefer explicit OTIO Clip shape
        if v.get("source_range").is_some() && v.get("media_references").is_some() {
            let source_range: TimeRange = serde_json::from_value(
                v.get("source_range")
                    .cloned()
                    .ok_or_else(|| D::Error::custom("missing source_range"))?,
            )
            .map_err(|e| D::Error::custom(format!("source_range: {}", e)))?;
            let media_references: std::collections::HashMap<String, MediaReference> =
                serde_json::from_value(v.get("media_references").cloned().unwrap())
                    .map_err(|e| D::Error::custom(format!("media_references: {}", e)))?;
            let name = v
                .get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.to_string());
            let active_media_reference_key = v
                .get("active_media_reference_key")
                .and_then(|k| k.as_str())
                .map(|s| s.to_string())
                .or_else(|| Some("DEFAULT_MEDIA".to_string()));
            let metadata = v
                .get("metadata")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            return Ok(Item::Clip(Clip {
                otio_schema: default_clip_schema(),
                name,
                source_range,
                media_references,
                active_media_reference_key,
                metadata,
            }));
        }
        if let Ok(clip) = serde_json::from_value::<Clip>(v.clone()) {
            return Ok(Item::Clip(clip));
        }
        if let Ok(gap) = serde_json::from_value::<Gap>(v.clone()) {
            return Ok(Item::Gap(gap));
        }
        eprintln!("Failed to parse Item: {}", v);
        Err(D::Error::custom("Item must be a clip or gap"))
    }
}

// No longer used; OTIO-only shape is parsed directly above

impl Item {
    pub fn duration(&self) -> Seconds {
        match self {
            Item::Clip(c) => c.source_range.duration.value,
            Item::Gap(g) => g.source_range.duration.value,
        }
    }
    pub fn set_duration(&mut self, dur: Seconds) {
        match self {
            Item::Clip(c) => c.source_range.duration.value = dur,
            Item::Gap(g) => g.source_range.duration.value = dur,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Clip {
    #[serde(rename = "OTIO_SCHEMA", default = "default_clip_schema")]
    pub otio_schema: String,
    #[serde(default)]
    pub name: Option<String>,
    pub source_range: TimeRange,
    #[serde(default)]
    pub media_references: HashMap<String, MediaReference>,
    #[serde(default)]
    pub active_media_reference_key: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl Clip {
    pub fn new_single(
        source_range: TimeRange,
        reference_key: String,
        reference: MediaReference,
        name: Option<String>,
        id: Option<String>,
    ) -> Self {
        let mut refs = HashMap::new();
        refs.insert(reference_key.clone(), reference);
        let mut c = Clip {
            otio_schema: default_clip_schema(),
            name,
            source_range,
            media_references: refs,
            active_media_reference_key: Some(reference_key),
            metadata: serde_json::Value::Null,
        };
        crate::metadata::IdMetadataExt::set_id(&mut c, Some(id.unwrap_or_else(gen_hex_id_12)));
        c
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq)]
pub struct Gap {
    #[serde(rename = "OTIO_SCHEMA", default = "default_gap_schema")]
    pub otio_schema: String,
    #[serde(default)]
    pub name: Option<String>,
    pub source_range: TimeRange,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl<'de> Deserialize<'de> for Gap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v = serde_json::Value::deserialize(deserializer)?;

        // Extract fields with fallbacks
        let otio_schema = v
            .get("OTIO_SCHEMA")
            .and_then(|s| s.as_str())
            .unwrap_or(&default_gap_schema())
            .to_string();
        let name = v
            .get("name")
            .and_then(|n| n.as_str())
            .map(|s| s.to_string());
        let metadata = v
            .get("metadata")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        // Prefer explicit source_range
        if let Some(sr_val) = v.get("source_range") {
            let source_range: TimeRange = serde_json::from_value(sr_val.clone())
                .map_err(|e| D::Error::custom(format!("gap.source_range: {}", e)))?;
            return Ok(Gap {
                otio_schema,
                name,
                source_range,
                metadata,
            });
        }

        // Backward-compat: allow `{ duration: number }` form
        if let Some(dur) = v.get("duration").and_then(|d| d.as_f64()) {
            let source_range = TimeRange {
                otio_schema: default_time_range_schema(),
                duration: RationalTime {
                    otio_schema: default_rational_time_schema(),
                    rate: 1.0,
                    value: dur,
                },
                start_time: RationalTime {
                    otio_schema: default_rational_time_schema(),
                    rate: 1.0,
                    value: 0.0,
                },
            };
            return Ok(Gap {
                otio_schema,
                name,
                source_range,
                metadata,
            });
        }

        Err(D::Error::custom("gap: missing source_range or duration"))
    }
}

impl Gap {
    pub fn new(duration: Seconds, id: Option<String>) -> Self {
        let mut g = Gap {
            otio_schema: default_gap_schema(),
            name: None,
            source_range: TimeRange {
                otio_schema: default_time_range_schema(),
                duration: RationalTime {
                    otio_schema: default_rational_time_schema(),
                    rate: 1.0,
                    value: duration,
                },
                start_time: RationalTime {
                    otio_schema: default_rational_time_schema(),
                    rate: 1.0,
                    value: 0.0,
                },
            },
            metadata: serde_json::Value::Object(serde_json::Map::new()),
        };
        crate::metadata::IdMetadataExt::set_id(&mut g, Some(id.unwrap_or_else(gen_hex_id_12)));
        g
    }
    pub fn make_gap(duration: Seconds) -> Self {
        Self::new(duration, None)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct MediaReference {
    #[serde(rename = "OTIO_SCHEMA", default = "default_external_ref_schema")]
    pub otio_schema: String,
    #[serde(rename = "target_url")]
    pub target_url: String,
    #[serde(default)]
    pub available_range: Option<TimeRange>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub available_image_bounds: Option<serde_json::Value>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct RationalTime {
    #[serde(rename = "OTIO_SCHEMA", default = "default_rational_time_schema")]
    pub otio_schema: String,
    pub rate: f64,
    pub value: Seconds,
}

impl Default for RationalTime {
    fn default() -> Self {
        Self {
            otio_schema: default_rational_time_schema(),
            rate: 1.0,
            value: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct TimeRange {
    #[serde(rename = "OTIO_SCHEMA", default = "default_time_range_schema")]
    pub otio_schema: String,
    pub duration: RationalTime,
    pub start_time: RationalTime,
}

impl Default for TimeRange {
    fn default() -> Self {
        Self {
            otio_schema: default_time_range_schema(),
            duration: RationalTime::default(),
            start_time: RationalTime::default(),
        }
    }
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            otio_schema: default_timeline_schema(),
            name: None,
            tracks: Stack::default(),
            metadata: serde_json::Value::Null,
        }
    }
}

impl Default for Track {
    fn default() -> Self {
        Self::new(TrackKind::Video, None)
    }
}

impl Track {
    pub fn new(kind: TrackKind, id: Option<String>) -> Self {
        let mut t = Track {
            otio_schema: default_track_schema(),
            kind,
            name: None,
            items: vec![],
            metadata: serde_json::Value::Null,
        };
        crate::metadata::IdMetadataExt::set_id(&mut t, Some(id.unwrap_or_else(gen_hex_id_12)));
        t
    }
    pub fn start_time_of_item(&self, index: usize) -> Seconds {
        let mut acc: Seconds = 0.0;
        for (i, it) in self.items.iter().enumerate() {
            if i >= index {
                break;
            }
            acc += it.duration().max(0.0);
        }
        acc
    }

    pub fn total_duration(&self) -> Seconds {
        self.items.iter().map(|it| it.duration().max(0.0)).sum()
    }
}

impl Stack {
    pub fn get_track_by_id(&self, id: &str) -> Option<(usize, &Track)> {
        for (i, tr) in self.children.iter().enumerate() {
            if crate::metadata::IdMetadataExt::get_id(tr).as_deref() == Some(id) {
                return Some((i, tr));
            }
        }
        None
    }
}
