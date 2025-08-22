use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
    pub kind: TrackKind,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Stack {
    #[serde(rename = "OTIO_SCHEMA", default = "default_stack_schema")]
    pub otio_schema: String,
    #[serde(default)]
    pub children: Vec<Track>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl Default for Stack {
    fn default() -> Self {
        Self {
            otio_schema: default_stack_schema(),
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Item {
    Clip(Clip),
    Gap(Gap),
}

impl Item {
    pub fn duration(&self) -> Seconds {
        match self {
            Item::Clip(c) => c.duration,
            Item::Gap(g) => g.duration,
        }
    }
    pub fn set_duration(&mut self, dur: Seconds) {
        match self {
            Item::Clip(c) => c.duration = dur,
            Item::Gap(g) => g.duration = dur,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Clip {
    #[serde(rename = "OTIO_SCHEMA", default = "default_clip_schema")]
    pub otio_schema: String,
    #[serde(default)]
    pub name: Option<String>,
    pub duration: Seconds,
    pub source: MediaSource,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Gap {
    #[serde(rename = "OTIO_SCHEMA", default = "default_gap_schema")]
    pub otio_schema: String,
    pub duration: Seconds,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl Gap {
    pub fn make_gap(duration: Seconds) -> Self {
        Gap {
            otio_schema: default_gap_schema(),
            duration,
            metadata: serde_json::Value::Object(serde_json::Map::new()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct MediaSource {
    #[serde(rename = "OTIO_SCHEMA", default = "default_external_ref_schema")]
    pub otio_schema: String,
    pub url: String,
    /// Offset into the media in seconds
    #[serde(default)]
    pub media_start: Seconds,
    /// Optional media duration if known
    #[serde(default)]
    pub media_duration: Option<Seconds>,
    #[serde(default)]
    pub metadata: serde_json::Value,
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
        Self {
            otio_schema: default_track_schema(),
            kind: TrackKind::Video,
            items: vec![],
            metadata: serde_json::Value::Null,
        }
    }
}

impl Track {
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
    pub fn get_track_by_id(&self, id: uuid::Uuid) -> Option<(usize, &Track)> {
        for (i, tr) in self.children.iter().enumerate() {
            if crate::metadata::IdMetadataExt::get_id(tr).as_ref() == Some(&id) {
                return Some((i, tr));
            }
        }
        None
    }
}
