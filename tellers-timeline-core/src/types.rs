use schemars::JsonSchema;
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
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

pub(crate) fn gen_hex_id_12() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 6];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn ensure_tellers_ai_with_id(mut meta: serde_json::Value) -> serde_json::Value {
    // Ensure we have an object at the root
    if meta.as_object().is_none() {
        meta = serde_json::Value::Object(serde_json::Map::new());
    }

    // Work with a mutable map and migrate legacy key if present
    let map = meta.as_object_mut().unwrap();

    // Read legacy `tellers_id` from metadata root if present.
    // TODO: remove legacy `tellers_id` once frontend is updated
    let legacy_id_opt = map
        .get("tellers_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Ensure we have an object at metadata["tellers.ai"]
    let ai_entry = map
        .entry("tellers.ai".to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if ai_entry.as_object().is_none() {
        *ai_entry = serde_json::Value::Object(serde_json::Map::new());
    }
    let ai_map = ai_entry.as_object_mut().unwrap();

    // If timeline_id is missing, prefer the legacy id; otherwise generate a new one
    let has_timeline_id = ai_map.get("timeline_id").and_then(|v| v.as_str()).is_some();
    if !has_timeline_id {
        let final_id = legacy_id_opt.unwrap_or_else(gen_hex_id_12);
        ai_map.insert(
            "timeline_id".to_string(),
            serde_json::Value::String(final_id),
        );
    }

    meta
}

fn deserialize_media_metadata<'de, D>(deserializer: D) -> Result<serde_json::Value, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Option::<serde_json::Value>::deserialize(deserializer)?;
    let mut meta = v.unwrap_or(serde_json::Value::Null);

    // Ensure we have an object at the root
    if meta.as_object().is_none() {
        meta = serde_json::Value::Object(serde_json::Map::new());
    }
    let map = meta.as_object_mut().unwrap();

    // Pre-read possible legacy fields before mutably borrowing the tellers.ai map
    let root_media_id = map.get("media_id").cloned();
    let root_score = map.get("score").cloned();
    let root_keyframe_id = map.get("keyframe_id").cloned();

    // Ensure we have an object at metadata["tellers.ai"]
    let ai_entry = map
        .entry("tellers.ai".to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if ai_entry.as_object().is_none() {
        *ai_entry = serde_json::Value::Object(serde_json::Map::new());
    }
    let ai_map = ai_entry.as_object_mut().unwrap();

    // Duplicate legacy media fields under tellers.ai if missing there
    // TODO: remove legacy media fields (media_id, score, keyframe_id) from root once frontend is updated
    if !ai_map.contains_key("media_id") {
        if let Some(v) = root_media_id {
            ai_map.insert("media_id".to_string(), v);
        }
    }
    if !ai_map.contains_key("score") {
        if let Some(v) = root_score {
            ai_map.insert("score".to_string(), v);
        }
    }
    if !ai_map.contains_key("keyframe_id") {
        if let Some(v) = root_keyframe_id {
            ai_map.insert("keyframe_id".to_string(), v);
        }
    }

    Ok(meta)
}

fn deserialize_metadata_with_id<'de, D>(deserializer: D) -> Result<serde_json::Value, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Option::<serde_json::Value>::deserialize(deserializer)?;
    let meta = v.unwrap_or(serde_json::Value::Null);
    Ok(ensure_tellers_ai_with_id(meta))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Timeline {
    #[serde(rename = "OTIO_SCHEMA", default = "default_timeline_schema")]
    pub otio_schema: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub tracks: Stack,
    #[serde(default, deserialize_with = "deserialize_metadata_with_id")]
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
    #[serde(default, deserialize_with = "deserialize_metadata_with_id")]
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
    #[serde(default, deserialize_with = "deserialize_metadata_with_id")]
    pub metadata: serde_json::Value,
}

impl Default for Stack {
    fn default() -> Self {
        Self {
            otio_schema: default_stack_schema(),
            name: None,
            children: vec![],
            metadata: serde_json::Value::Object(serde_json::Map::new()),
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(untagged)]
pub enum Item {
    Clip(Clip),
    Gap(Gap),
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
    pub fn get_source_range(&self) -> TimeRange {
        match self {
            Item::Clip(c) => c.source_range.clone(),
            Item::Gap(g) => g.source_range.clone(),
        }
    }
    pub fn set_source_range(&mut self, source_range: TimeRange) {
        match self {
            Item::Clip(c) => c.source_range = source_range,
            Item::Gap(g) => g.source_range = source_range,
        }
    }
    pub fn get_active_media_reference_key(&self) -> Option<String> {
        match self {
            Item::Clip(c) => c.active_media_reference_key.clone(),
            Item::Gap(_g) => None,
        }
    }
    pub fn set_active_media_reference_key(&mut self, key: Option<String>) {
        if let Item::Clip(c) = self {
            c.active_media_reference_key = key;
        }
    }
    pub fn get_media_references(&self) -> HashMap<String, MediaReference> {
        match self {
            Item::Clip(c) => c.media_references.clone(),
            Item::Gap(_g) => HashMap::new(),
        }
    }
    pub fn set_media_references(&mut self, references: HashMap<String, MediaReference>) {
        if let Item::Clip(c) = self {
            c.media_references = references;
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
    #[serde(default, deserialize_with = "deserialize_metadata_with_id")]
    pub metadata: serde_json::Value,
}

impl Clip {
    pub fn new_single_media_reference(
        source_range: TimeRange,
        reference: MediaReference,
        name: Option<String>,
        id: Option<String>,
    ) -> Self {
        let mut refs = HashMap::new();
        refs.insert("DEFAULT_MEDIA".to_string(), reference);
        let mut c = Clip {
            otio_schema: default_clip_schema(),
            name,
            source_range,
            media_references: refs,
            active_media_reference_key: Some("DEFAULT_MEDIA".to_string()),
            metadata: serde_json::Value::Object(serde_json::Map::new()),
        };
        crate::metadata::IdMetadataExt::set_id(&mut c, Some(id.unwrap_or_else(gen_hex_id_12)));
        c
    }

    pub fn new(
        source_range: TimeRange,
        media_references: HashMap<String, MediaReference>,
        active_media_reference_key: Option<String>,
        name: Option<String>,
        id: Option<String>,
    ) -> Self {
        let mut c = Clip {
            otio_schema: default_clip_schema(),
            name,
            source_range,
            media_references,
            active_media_reference_key,
            metadata: serde_json::Value::Object(serde_json::Map::new()),
        };
        crate::metadata::IdMetadataExt::set_id(&mut c, Some(id.unwrap_or_else(gen_hex_id_12)));
        c
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Gap {
    #[serde(rename = "OTIO_SCHEMA", default = "default_gap_schema")]
    pub otio_schema: String,
    #[serde(default)]
    pub name: Option<String>,
    pub source_range: TimeRange,
    #[serde(default, deserialize_with = "deserialize_metadata_with_id")]
    pub metadata: serde_json::Value,
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
    #[serde(default, deserialize_with = "deserialize_media_metadata")]
    pub metadata: serde_json::Value,
}

impl MediaReference {
    pub fn media_start(&self) -> Seconds {
        self.available_range
            .as_ref()
            .map(|tr| tr.start_time.value)
            .unwrap_or(0.0)
    }

    pub fn set_media_start(&mut self, start_seconds: Seconds) {
        if let Some(tr) = &mut self.available_range {
            tr.start_time.value = start_seconds;
        } else {
            self.available_range = Some(TimeRange {
                otio_schema: default_time_range_schema(),
                duration: RationalTime {
                    otio_schema: default_rational_time_schema(),
                    rate: 1.0,
                    value: 0.0,
                },
                start_time: RationalTime {
                    otio_schema: default_rational_time_schema(),
                    rate: 1.0,
                    value: start_seconds,
                },
            });
        }
    }

    pub fn media_duration(&self) -> Option<Seconds> {
        self.available_range.as_ref().map(|tr| tr.duration.value)
    }

    pub fn set_media_duration(&mut self, duration_seconds: Option<Seconds>) {
        match duration_seconds {
            Some(v) => {
                if let Some(tr) = &mut self.available_range {
                    tr.duration.value = v;
                } else {
                    self.available_range = Some(TimeRange {
                        otio_schema: default_time_range_schema(),
                        duration: RationalTime {
                            otio_schema: default_rational_time_schema(),
                            rate: 1.0,
                            value: v,
                        },
                        start_time: RationalTime {
                            otio_schema: default_rational_time_schema(),
                            rate: 1.0,
                            value: 0.0,
                        },
                    });
                }
            }
            None => {
                self.available_range = None;
            }
        }
    }
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

impl TimeRange {
    pub fn new(duration: Seconds, start_time: Seconds) -> Self {
        Self {
            otio_schema: default_time_range_schema(),
            duration: RationalTime {
                otio_schema: default_rational_time_schema(),
                rate: 1.0,
                value: duration,
            },
            start_time: RationalTime {
                otio_schema: default_rational_time_schema(),
                rate: 1.0,
                value: start_time,
            },
        }
    }
    pub fn get_duration(&self) -> Seconds {
        self.duration.value
    }
    pub fn get_start_time(&self) -> Seconds {
        self.start_time.value
    }
    pub fn set_duration(&mut self, duration: Seconds) {
        self.duration.value = duration;
    }
    pub fn set_start_time(&mut self, start_time: Seconds) {
        self.start_time.value = start_time;
    }
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            otio_schema: default_timeline_schema(),
            name: None,
            tracks: Stack::default(),
            metadata: serde_json::Value::Object(serde_json::Map::new()),
        }
    }
}

impl Timeline {
    pub fn to_json(&self) -> serde_json::Result<String> {
        crate::to_json_with_precision(self, None, true)
    }
    pub fn to_json_with_options(
        &self,
        precision: Option<usize>,
        pretty: bool,
    ) -> serde_json::Result<String> {
        crate::to_json_with_precision(self, precision, pretty)
    }
    pub fn add_track(&mut self, track: Track) {
        self.tracks.add_track(track);
    }
    pub fn add_track_at(&mut self, track: Track, insertion_index: isize) {
        self.tracks.add_track_at(track, insertion_index);
    }
    pub fn delete_track(&mut self, id: &str) -> Option<Track> {
        self.tracks.delete_track(id)
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
            metadata: serde_json::Value::Object(serde_json::Map::new()),
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
