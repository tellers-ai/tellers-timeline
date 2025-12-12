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
fn default_time_range_schema() -> String {
    "TimeRange.1".to_string()
}
fn default_rational_time_schema() -> String {
    "RationalTime.1".to_string()
}
fn default_effect_schema() -> String {
    "Effect.1".to_string()
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
    pub fn get_effects(&self) -> Vec<Effect> {
        match self {
            Item::Clip(c) => c.effects.clone(),
            Item::Gap(g) => g.effects.clone(),
        }
    }
    pub fn set_effects(&mut self, effects: Vec<Effect>) {
        match self {
            Item::Clip(c) => c.effects = effects,
            Item::Gap(g) => g.effects = effects,
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
    #[serde(default)]
    pub effects: Vec<Effect>,
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
            effects: Vec::new(),
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
            effects: Vec::new(),
        };
        crate::metadata::IdMetadataExt::set_id(&mut c, Some(id.unwrap_or_else(gen_hex_id_12)));
        c
    }

    pub fn get_position(&self) -> MediaReferencePosition {
        let active_media_reference = self.media_references.get(self.active_media_reference_key.as_ref().unwrap()).unwrap();
        let mut x = 0.5;
        let mut y = 0.5;
        let mut rotation = 0.0;
        let mut zoom_x = 1.0;
        let mut zoom_y = 1.0;
        if let MediaReference::GeneratorReference { parameters, .. } = active_media_reference {
            if let Some(resolve_otio_effects) = parameters.resolve_otio.as_ref() {
                for effect in resolve_otio_effects {
                    for parameter in &effect.parameters {
                        match parameter {
                            ResolveOTIOParameter::PointF(param) if param.parameter_id == "position" => {
                                if let Some([x_val, y_val]) = param.parameter_value {
                                    x = x_val;
                                    y = y_val;
                                }
                            }
                            ResolveOTIOParameter::Double(param) if param.parameter_id == "transformationZoomX" => {
                                zoom_x = param.parameter_value;
                            }
                            ResolveOTIOParameter::Double(param) if param.parameter_id == "transformationZoomY" => {
                                zoom_y = param.parameter_value;
                            }
                            ResolveOTIOParameter::Double(param) if param.parameter_id == "transformationRotationAngle" => {
                                rotation = param.parameter_value;
                            }
                            _ => {}
                        }
                    }
                }
            }
        } else {
            for effect in &self.effects {
                if effect.effect_name == "Resolve Effect" {
                    if let Some(resolve_otio_effect) = effect.metadata.resolve_otio.as_ref(){
                        if resolve_otio_effect.effect_name == "Transform" {
                        for parameter in &resolve_otio_effect.parameters {
                        match parameter {
                            ResolveOTIOParameter::Double(param) if param.parameter_id == "transformationPan" => {
                                x = param.parameter_value;
                            }
                            ResolveOTIOParameter::Double(param) if param.parameter_id == "transformationTilt" => {
                                y = param.parameter_value;
                            }
                            ResolveOTIOParameter::Double(param) if param.parameter_id == "transformationZoomX" => {
                                zoom_x = param.parameter_value;
                            }
                            ResolveOTIOParameter::Double(param) if param.parameter_id == "transformationTilt" => {
                                zoom_y = param.parameter_value;
                            }
                            ResolveOTIOParameter::Double(param) if param.parameter_id == "transformationRotationAngle" => {
                                rotation = param.parameter_value;
                            }
                            _ => {}
                        }
                    }
                }
                }
                }
            }
        }
        MediaReferencePosition {
            x,
            y,
            rotation,
            zoom_x,
            zoom_y,
        }
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
    #[serde(default)]
    pub effects: Vec<Effect>,
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
            effects: Vec::new(),
        };
        crate::metadata::IdMetadataExt::set_id(&mut g, Some(id.unwrap_or_else(gen_hex_id_12)));
        g
    }
    pub fn make_gap(duration: Seconds) -> Self {
        Self::new(duration, None)
    }
}

/// Variant type for Resolve_OTIO parameters
/// Variant type for Resolve_OTIO parameters.
/// Uses serde derive for Serialize with custom Deserialize for case-insensitive parsing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum VariantType {
    Int,
    Bool,
    String,
    Double,
    UInt,
    #[serde(rename = "POINTF")]
    PointF,
    Color,
    #[serde(untagged)]
    Unknown(String),
}


#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ResolveOTIOParameterNumber<T: Default> {
    #[serde(rename = "Variant Type")]
    pub variant_type: String,
    #[serde(rename = "Parameter ID", default)]
    pub parameter_id: String,
    #[serde(rename = "Parameter Value", default)]
    pub parameter_value: T,
    #[serde(rename = "Default Parameter Value", default)]
    pub default_parameter_value: Option<T>,
    #[serde(rename = "maxValue", default)]
    pub max_value: Option<T>,
    #[serde(rename = "minValue", default)]
    pub min_value: Option<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ResolveOTIOParameterSimple<T: Default> {
    #[serde(rename = "Variant Type")]
    pub variant_type: String,
    #[serde(rename = "Parameter ID", default)]
    pub parameter_id: String,
    #[serde(rename = "Parameter Value", default)]
    pub parameter_value: T,
    #[serde(rename = "Default Parameter Value", default)]
    pub default_parameter_value: Option<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ResolveOTIOParameterPointF {
    #[serde(rename = "Variant Type")]
    pub variant_type: String,
    #[serde(rename = "Parameter ID", default)]
    pub parameter_id: String,
    #[serde(rename = "Parameter Value", default)]
    pub parameter_value: Option<[f64; 2]>,
    #[serde(rename = "Default Parameter Value", default)]
    pub default_parameter_value: Option<[f64; 2]>,
    #[serde(rename = "Key Frames", default)]
    pub key_frames: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ResolveOTIOParameterUnknown {
    #[serde(rename = "Parameter ID", default)]
    pub parameter_id: String,
    #[serde(rename = "Parameter Value", default)]
    pub parameter_value: Option<serde_json::Value>,
    #[serde(rename = "Default Parameter Value", default)]
    pub default_parameter_value: Option<serde_json::Value>,
    #[serde(rename = "Key Frames", default)]
    pub key_frames: Option<serde_json::Value>,
    #[serde(rename = "Title HTML", default)]
    pub title_html: Option<String>,
}



#[derive(Debug, Clone, PartialEq, JsonSchema)]
pub enum ResolveOTIOParameter {
    Int(ResolveOTIOParameterNumber<i64>),
    UInt(ResolveOTIOParameterNumber<u64>),
    Double(ResolveOTIOParameterNumber<f64>),
    Bool(ResolveOTIOParameterSimple<bool>),
    String(ResolveOTIOParameterSimple<String>),
    PointF(ResolveOTIOParameterPointF),
    Color(ResolveOTIOParameterSimple<String>),
    Unknown(ResolveOTIOParameterUnknown),
}

impl<'de> Deserialize<'de> for ResolveOTIOParameter {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let variant_type_str = value.get("Variant Type").and_then(|v| v.as_str());
        if let Some(_variant_type_str) = variant_type_str {
            #[derive(Deserialize)]
            #[serde(tag = "Variant Type")]
            enum TaggedResolveOTIOParameter {
                #[serde(rename = "Int")]
                Int(ResolveOTIOParameterNumber<i64>),
                #[serde(rename = "UInt")]
                UInt(ResolveOTIOParameterNumber<u64>),
                #[serde(rename = "Double")]
                Double(ResolveOTIOParameterNumber<f64>),
                #[serde(rename = "Bool")]
                Bool(ResolveOTIOParameterSimple<bool>),
                #[serde(rename = "String")]
                String(ResolveOTIOParameterSimple<String>),
                #[serde(rename = "POINTF")]
                PointF(ResolveOTIOParameterPointF),
                #[serde(rename = "Color")]
                Color(ResolveOTIOParameterSimple<String>),
            }

            if let Ok(tagged) = serde_json::from_value::<TaggedResolveOTIOParameter>(value.clone()) {
                return Ok(match tagged {
                    TaggedResolveOTIOParameter::Int(mut v) => {
                        v.variant_type = "Int".to_string();
                        ResolveOTIOParameter::Int(v)
                    }
                    TaggedResolveOTIOParameter::UInt(mut v) => {
                        v.variant_type = "UInt".to_string();
                        ResolveOTIOParameter::UInt(v)
                    }
                    TaggedResolveOTIOParameter::Double(mut v) => {
                        v.variant_type = "Double".to_string();
                        ResolveOTIOParameter::Double(v)
                    }
                    TaggedResolveOTIOParameter::Bool(mut v) => {
                        v.variant_type = "Bool".to_string();
                        ResolveOTIOParameter::Bool(v)
                    }
                    TaggedResolveOTIOParameter::String(mut v) => {
                        v.variant_type = "String".to_string();
                        ResolveOTIOParameter::String(v)
                    }
                    TaggedResolveOTIOParameter::PointF(mut v) => {
                        v.variant_type = "POINTF".to_string();
                        ResolveOTIOParameter::PointF(v)
                    }
                    TaggedResolveOTIOParameter::Color(mut v) => {
                        v.variant_type = "Color".to_string();
                        ResolveOTIOParameter::Color(v)
                    }
                });
            }
        }

        let unknown: ResolveOTIOParameterUnknown = serde_json::from_value(value)
            .map_err(serde::de::Error::custom)?;

        Ok(ResolveOTIOParameter::Unknown(unknown))
    }
}

impl Serialize for ResolveOTIOParameter {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            // For variants with variant_type field, serde will serialize it automatically
            ResolveOTIOParameter::Int(v) => v.serialize(serializer),
            ResolveOTIOParameter::UInt(v) => v.serialize(serializer),
            ResolveOTIOParameter::Double(v) => v.serialize(serializer),
            ResolveOTIOParameter::Bool(v) => v.serialize(serializer),
            ResolveOTIOParameter::String(v) => v.serialize(serializer),
            ResolveOTIOParameter::PointF(v) => v.serialize(serializer),
            ResolveOTIOParameter::Color(v) => v.serialize(serializer),
            // Unknown doesn't have variant_type, serialize directly
            ResolveOTIOParameter::Unknown(v) => v.serialize(serializer),
        }
    }
}


impl ResolveOTIOParameter {
    pub fn parameter_id(&self) -> &String {
        match self {
            ResolveOTIOParameter::Int(v) => &v.parameter_id,
            ResolveOTIOParameter::UInt(v) => &v.parameter_id,
            ResolveOTIOParameter::Double(v) => &v.parameter_id,
            ResolveOTIOParameter::Bool(v) => &v.parameter_id,
            ResolveOTIOParameter::String(v) => &v.parameter_id,
            ResolveOTIOParameter::PointF(v) => &v.parameter_id,
            ResolveOTIOParameter::Color(v) => &v.parameter_id,
            ResolveOTIOParameter::Unknown(v) => &v.parameter_id,
        }
    }
}



/// Resolve_OTIO effect structure (used in media reference parameters array)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ResolveOTIOEffect {
    #[serde(rename = "Effect Name")]
    pub effect_name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "Parameters", default)]
    pub parameters: Vec<ResolveOTIOParameter>,
    #[serde(rename = "Type", default)]
    pub effect_type: u64,
}

/// Generator parameters structure for GeneratorReference
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
pub struct GeneratorParameters {
    #[serde(rename = "Resolve_OTIO", default, skip_serializing_if = "Option::is_none")]
    pub resolve_otio: Option<Vec<ResolveOTIOEffect>>,
    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
}

/// Effect metadata structure - metadata is a map where "Resolve_OTIO" is typed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct EffectMetadata {
    #[serde(rename = "Resolve_OTIO", default, skip_serializing_if = "Option::is_none")]
    pub resolve_otio: Option<ResolveOTIOEffect>,
    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
}

impl Default for EffectMetadata {
    fn default() -> Self {
        EffectMetadata {
            resolve_otio: None,
            other: serde_json::Map::new(),
        }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Effect {
    #[serde(rename = "OTIO_SCHEMA", default = "default_effect_schema")]
    pub otio_schema: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub effect_name: String,
    #[serde(default)]
    pub metadata: EffectMetadata,
}


pub struct MediaReferencePosition {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub zoom_x: f64,
    pub zoom_y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "OTIO_SCHEMA")]
pub enum MediaReference {
    #[serde(rename = "ExternalReference.1")]
    ExternalReference {
        #[serde(rename = "target_url")]
        target_url: String,
        #[serde(default)]
        available_range: Option<TimeRange>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        available_image_bounds: Option<serde_json::Value>,
        #[serde(default, deserialize_with = "deserialize_media_metadata")]
        metadata: serde_json::Value,
    },
    #[serde(rename = "GeneratorReference.1")]
    GeneratorReference {
        #[serde(rename = "generator_kind")]
        generator_kind: String,
        #[serde(default)]
        available_range: Option<TimeRange>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        available_image_bounds: Option<serde_json::Value>,
        #[serde(default, deserialize_with = "deserialize_media_metadata")]
        metadata: serde_json::Value,
        #[serde(default)]
        parameters: GeneratorParameters,
    },
}



impl MediaReference {
    pub fn available_range(&self) -> &Option<TimeRange> {
        match self {
            MediaReference::ExternalReference { available_range, .. } => available_range,
            MediaReference::GeneratorReference { available_range, .. } => available_range,
        }
    }

    pub fn available_range_mut(&mut self) -> &mut Option<TimeRange> {
        match self {
            MediaReference::ExternalReference { available_range, .. } => available_range,
            MediaReference::GeneratorReference { available_range, .. } => available_range,
        }
    }

    pub fn metadata(&self) -> &serde_json::Value {
        match self {
            MediaReference::ExternalReference { metadata, .. } => metadata,
            MediaReference::GeneratorReference { metadata, .. } => metadata,
        }
    }

    pub fn metadata_mut(&mut self) -> &mut serde_json::Value {
        match self {
            MediaReference::ExternalReference { metadata, .. } => metadata,
            MediaReference::GeneratorReference { metadata, .. } => metadata,
        }
    }

    pub fn target_url(&self) -> Option<&String> {
        match self {
            MediaReference::ExternalReference { target_url, .. } => Some(target_url),
            MediaReference::GeneratorReference { .. } => None,
        }
    }

    pub fn generator_kind(&self) -> Option<&String> {
        match self {
            MediaReference::ExternalReference { .. } => None,
            MediaReference::GeneratorReference { generator_kind, .. } => Some(generator_kind),
        }
    }

    pub fn parameters(&self) -> Option<&GeneratorParameters> {
        match self {
            MediaReference::ExternalReference { .. } => None,
            MediaReference::GeneratorReference { parameters, .. } => Some(parameters),
        }
    }

    pub fn media_start(&self) -> Seconds {
        self.available_range()
            .as_ref()
            .map(|tr| tr.start_time.value)
            .unwrap_or(0.0)
    }

    pub fn set_media_start(&mut self, start_seconds: Seconds) {
        let available_range = self.available_range_mut();
        if let Some(tr) = available_range {
            tr.start_time.value = start_seconds;
        } else {
            *available_range = Some(TimeRange {
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
        self.available_range().as_ref().map(|tr| tr.duration.value)
    }

    pub fn set_media_duration(&mut self, duration_seconds: Option<Seconds>) {
        let available_range = self.available_range_mut();
        match duration_seconds {
            Some(v) => {
                if let Some(tr) = available_range {
                    tr.duration.value = v;
                } else {
                    *available_range = Some(TimeRange {
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
                *available_range = None;
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
