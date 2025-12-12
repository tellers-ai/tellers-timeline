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
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
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

impl VariantType {
    /// Parse from string (case-insensitive)
    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "int" => VariantType::Int,
            "bool" | "boolean" => VariantType::Bool,
            "string" => VariantType::String,
            "double" => VariantType::Double,
            "uint" => VariantType::UInt,
            "pointf" => VariantType::PointF,
            "color" => VariantType::Color,
            _ => VariantType::Unknown(s.to_string()),
        }
    }
}

impl<'de> Deserialize<'de> for VariantType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(VariantType::from_str(&s))
    }
}

impl schemars::JsonSchema for VariantType {
    fn schema_name() -> String {
        "VariantType".to_string()
    }

    fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::String.into()),
            enum_values: Some(vec![
                serde_json::Value::String("Int".to_string()),
                serde_json::Value::String("Float".to_string()),
                serde_json::Value::String("Bool".to_string()),
                serde_json::Value::String("String".to_string()),
                serde_json::Value::String("Double".to_string()),
                serde_json::Value::String("Long".to_string()),
                serde_json::Value::String("UInt".to_string()),
                serde_json::Value::String("POINTF".to_string()),
                serde_json::Value::String("Color".to_string()),
            ]),
            ..Default::default()
        }
        .into()
    }
}


#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ResolveOTIOParameterNumber<T> {
    pub parameter_id: Option<String>,
    pub parameter_value: Option<T>,
    pub default_parameter_value: Option<T>,
    pub max_value: Option<T>,
    pub min_value: Option<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ResolveOTIOParameterSimple<T> {
    pub parameter_id: Option<String>,
    pub parameter_value: Option<T>,
    pub default_parameter_value: Option<T>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ResolveOTIOParameterUnknown {
    pub parameter_id: Option<String>,
    pub parameter_value: Option<serde_json::Value>,
    pub default_parameter_value: Option<serde_json::Value>,
    pub key_frames: Option<serde_json::Value>,
    pub title_html: Option<String>,
}



/// Resolve_OTIO parameter as a discriminated union based on "Variant Type"
#[derive(Debug, Clone, PartialEq)]
pub enum ResolveOTIOParameter {
    Int(ResolveOTIOParameterNumber<i64>),
    UInt (ResolveOTIOParameterNumber<u64>),
    Double (ResolveOTIOParameterNumber<f64>),
    Bool (ResolveOTIOParameterSimple<bool>),
    String (ResolveOTIOParameterSimple<String>),
    PointF (ResolveOTIOParameterNumber<[f64; 2]>),
    Color (ResolveOTIOParameterSimple<String>),
    Unknown (ResolveOTIOParameterUnknown),
}

impl<'de> Deserialize<'de> for ResolveOTIOParameter {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // First deserialize into a temporary struct to get variant_type
        #[derive(Deserialize)]
        struct TempParameter {
            #[serde(rename = "Parameter ID", default)]
            parameter_id: Option<String>,
            #[serde(rename = "Parameter Value", default)]
            parameter_value: serde_json::Value,
            #[serde(rename = "Default Parameter Value", default)]
            default_parameter_value: Option<serde_json::Value>,
            #[serde(rename = "Title HTML", default)]
            title_html: Option<String>,
            #[serde(rename = "Key Frames", default)]
            key_frames: Option<serde_json::Value>,
            #[serde(rename = "Variant Type", default)]
            variant_type: Option<String>,
            #[serde(default)]
            max_value: Option<f64>,
            #[serde(default)]
            min_value: Option<f64>,
        }

        let temp: TempParameter = TempParameter::deserialize(deserializer)?;

        let variant_type = temp.variant_type.as_deref().map(VariantType::from_str);

        // Check if we need to handle special cases based on Parameter ID when Variant Type is missing
        let use_param_id_fallback = variant_type.is_none();
        let param_id_for_fallback = temp.parameter_id.as_deref();

        match variant_type {
            Some(VariantType::Int) => {
                // For Int variant, max_value and min_value are i64 (same type as parameter_value)
                Ok(ResolveOTIOParameter::Int {
                    parameter_id: temp.parameter_id.clone(),
                    parameter_value: temp.parameter_value.as_i64(),
                    default_parameter_value: temp.default_parameter_value.and_then(|v| v.as_i64()),
                    max_value: temp.max_value.map(|v| v as i64),
                    min_value: temp.min_value.map(|v| v as i64),
                })
            }
            Some(VariantType::UInt) => {
                // For UInt variant, max_value and min_value are u64 (same type as parameter_value)
                // Convert from i64 or f64 to u64, ensuring non-negative values
                let parameter_value = temp.parameter_value.as_i64()
                    .and_then(|v| if v >= 0 { Some(v as u64) } else { None })
                    .or_else(|| temp.parameter_value.as_f64().and_then(|v| if v >= 0.0 && v <= u64::MAX as f64 && v.fract() == 0.0 { Some(v as u64) } else { None }));

                let default_parameter_value = temp.default_parameter_value.and_then(|v| {
                    v.as_i64()
                        .and_then(|x| if x >= 0 { Some(x as u64) } else { None })
                        .or_else(|| v.as_f64().and_then(|x| if x >= 0.0 && x <= u64::MAX as f64 && x.fract() == 0.0 { Some(x as u64) } else { None }))
                });

                Ok(ResolveOTIOParameter::UInt {
                    parameter_id: temp.parameter_id.clone(),
                    parameter_value,
                    default_parameter_value,
                    max_value: temp.max_value.and_then(|v| if v >= 0.0 && v <= u64::MAX as f64 { Some(v as u64) } else { None }),
                    min_value: temp.min_value.and_then(|v| if v >= 0.0 && v <= u64::MAX as f64 { Some(v as u64) } else { None }),
                })
            }
            Some(VariantType::Double) => {
                Ok(ResolveOTIOParameter::Double {
                    parameter_id: temp.parameter_id.clone(),
                    parameter_value: temp.parameter_value.as_f64(),
                    default_parameter_value: temp.default_parameter_value.and_then(|v| v.as_f64()),
                    max_value: temp.max_value,
                    min_value: temp.min_value,
                })
            }
            Some(VariantType::Bool) => {
                Ok(ResolveOTIOParameter::Bool {
                    parameter_id: temp.parameter_id.clone(),
                    parameter_value: temp.parameter_value.as_bool(),
                    default_parameter_value: temp.default_parameter_value.and_then(|v| v.as_bool()),
                })
            }
            Some(VariantType::String) => {
                Ok(ResolveOTIOParameter::String {
                    parameter_id: temp.parameter_id.clone(),
                    parameter_value: temp.parameter_value.as_str().map(|s| s.to_string()),
                    default_parameter_value: temp.default_parameter_value.and_then(|v| v.as_str().map(|s| s.to_string())),
                })
            }
            Some(VariantType::PointF) => {
                // PointF is an array of two floats [x, y]
                let parameter_value = if let Some(arr) = temp.parameter_value.as_array() {
                    if arr.len() >= 2 {
                        if let (Some(x), Some(y)) = (arr[0].as_f64(), arr[1].as_f64()) {
                            Some([x, y])
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let default_parameter_value = temp.default_parameter_value.and_then(|v| {
                    if let Some(arr) = v.as_array() {
                        if arr.len() >= 2 {
                            if let (Some(x), Some(y)) = (arr[0].as_f64(), arr[1].as_f64()) {
                                Some([x, y])
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                });

                Ok(ResolveOTIOParameter::PointF {
                    parameter_id: temp.parameter_id.clone(),
                    parameter_value,
                    default_parameter_value,
                    key_frames: temp.key_frames.clone(),
                })
            }
            Some(VariantType::Color) => {
                // Color is a string (e.g., "#000000")
                Ok(ResolveOTIOParameter::Color {
                    parameter_id: temp.parameter_id.clone(),
                    parameter_value: temp.parameter_value.as_str().map(|s| s.to_string()),
                    default_parameter_value: temp.default_parameter_value.and_then(|v| v.as_str().map(|s| s.to_string())),
                })
            }
            _ => {
                // If no Variant Type, try to infer from Parameter ID
                if use_param_id_fallback {
                    if let Some(param_id) = param_id_for_fallback {
                        match param_id {
                            "title blob" => {
                                // Special case: title blob has Title HTML but no Variant Type
                                // Store Title HTML in parameter_value as a string
                                Ok(ResolveOTIOParameter::String {
                                    parameter_id: temp.parameter_id.clone(),
                                    parameter_value: temp.title_html.clone(),
                                    default_parameter_value: None,
                                })
                            }
                            _ => {
                                // For other parameters without Variant Type, use Unknown
                                Ok(ResolveOTIOParameter::Unknown {
                                    parameter_id: temp.parameter_id.clone(),
                                    parameter_value: Some(temp.parameter_value),
                                    default_parameter_value: temp.default_parameter_value,
                                    key_frames: temp.key_frames.clone(),
                                    title_html: temp.title_html.clone(),
                                })
                            }
                        }
                    } else {
                        // No Parameter ID, use Unknown
                        Ok(ResolveOTIOParameter::Unknown {
                            parameter_id: temp.parameter_id.clone(),
                            parameter_value: Some(temp.parameter_value),
                            default_parameter_value: temp.default_parameter_value,
                            key_frames: temp.key_frames.clone(),
                            title_html: temp.title_html.clone(),
                        })
                    }
                } else {
                    // Has Variant Type but it's unknown, use Unknown
                    Ok(ResolveOTIOParameter::Unknown {
                        parameter_id: temp.parameter_id.clone(),
                        parameter_value: Some(temp.parameter_value),
                        default_parameter_value: temp.default_parameter_value,
                        key_frames: temp.key_frames.clone(),
                        title_html: temp.title_html.clone(),
                    })
                }
            }
        }
    }
}

impl Serialize for ResolveOTIOParameter {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("ResolveOTIOParameter", 8)?;

        match self {
            ResolveOTIOParameter::Int { parameter_id, parameter_value, default_parameter_value, max_value, min_value } => {
                state.serialize_field("Variant Type", "Int")?;
                state.serialize_field("Parameter ID", parameter_id)?;
                state.serialize_field("Parameter Value", parameter_value)?;
                state.serialize_field("Default Parameter Value", default_parameter_value)?;
                state.serialize_field("Title HTML", &None::<String>)?;
                state.serialize_field("Key Frames", &None::<serde_json::Value>)?;
                state.serialize_field("maxValue", &max_value.map(|v| v as f64))?;
                state.serialize_field("minValue", &min_value.map(|v| v as f64))?;
            }
            ResolveOTIOParameter::UInt { parameter_id, parameter_value, default_parameter_value, max_value, min_value } => {
                state.serialize_field("Variant Type", "UInt")?;
                state.serialize_field("Parameter ID", parameter_id)?;
                state.serialize_field("Parameter Value", parameter_value)?;
                state.serialize_field("Default Parameter Value", default_parameter_value)?;
                state.serialize_field("Title HTML", &None::<String>)?;
                state.serialize_field("Key Frames", &None::<serde_json::Value>)?;
                state.serialize_field("maxValue", &max_value.map(|v| v as f64))?;
                state.serialize_field("minValue", &min_value.map(|v| v as f64))?;
            }
            ResolveOTIOParameter::Double { parameter_id, parameter_value, default_parameter_value, max_value, min_value } => {
                state.serialize_field("Variant Type", "Double")?;
                state.serialize_field("Parameter ID", parameter_id)?;
                state.serialize_field("Parameter Value", parameter_value)?;
                state.serialize_field("Default Parameter Value", default_parameter_value)?;
                state.serialize_field("Title HTML", &None::<String>)?;
                state.serialize_field("Key Frames", &None::<serde_json::Value>)?;
                state.serialize_field("maxValue", max_value)?;
                state.serialize_field("minValue", min_value)?;
            }
            ResolveOTIOParameter::Bool { parameter_id, parameter_value, default_parameter_value } => {
                state.serialize_field("Variant Type", "Bool")?;
                state.serialize_field("Parameter ID", parameter_id)?;
                state.serialize_field("Parameter Value", parameter_value)?;
                state.serialize_field("Default Parameter Value", default_parameter_value)?;
                state.serialize_field("Title HTML", &None::<String>)?;
                state.serialize_field("Key Frames", &None::<serde_json::Value>)?;
            }
            ResolveOTIOParameter::String { parameter_id, parameter_value, default_parameter_value } => {
                state.serialize_field("Variant Type", "String")?;
                state.serialize_field("Parameter ID", parameter_id)?;
                state.serialize_field("Parameter Value", parameter_value)?;
                state.serialize_field("Default Parameter Value", default_parameter_value)?;
                state.serialize_field("Title HTML", &None::<String>)?;
                state.serialize_field("Key Frames", &None::<serde_json::Value>)?;
            }
            ResolveOTIOParameter::PointF { parameter_id, parameter_value, default_parameter_value, key_frames } => {
                state.serialize_field("Variant Type", "POINTF")?;
                state.serialize_field("Parameter ID", parameter_id)?;
                state.serialize_field("Parameter Value", parameter_value)?;
                state.serialize_field("Default Parameter Value", default_parameter_value)?;
                state.serialize_field("Title HTML", &None::<String>)?;
                state.serialize_field("Key Frames", key_frames)?;
            }
            ResolveOTIOParameter::Color { parameter_id, parameter_value, default_parameter_value } => {
                state.serialize_field("Variant Type", "Color")?;
                state.serialize_field("Parameter ID", parameter_id)?;
                state.serialize_field("Parameter Value", parameter_value)?;
                state.serialize_field("Default Parameter Value", default_parameter_value)?;
                state.serialize_field("Title HTML", &None::<String>)?;
                state.serialize_field("Key Frames", &None::<serde_json::Value>)?;
            }
            ResolveOTIOParameter::Unknown { parameter_id, parameter_value, default_parameter_value, key_frames, title_html } => {
                // For unknown, we try to preserve the original variant type if it exists
                // But since we don't store it, we'll just serialize as-is
                // Note: max_value and min_value are not included for Unknown variant
                state.serialize_field("Parameter ID", parameter_id)?;
                state.serialize_field("Parameter Value", parameter_value)?;
                state.serialize_field("Default Parameter Value", default_parameter_value)?;
                state.serialize_field("Title HTML", title_html)?;
                state.serialize_field("Key Frames", key_frames)?;
            }
        }

        state.end()
    }
}

impl schemars::JsonSchema for ResolveOTIOParameter {
    fn schema_name() -> String {
        "ResolveOTIOParameter".to_string()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::*;
        SchemaObject {
            metadata: Some(Box::new(Metadata {
                description: Some("Resolve_OTIO parameter as discriminated union".to_string()),
                ..Default::default()
            })),
            instance_type: Some(InstanceType::Object.into()),
            object: Some(Box::new(ObjectValidation {
                properties: {
                    let mut props = schemars::Map::new();
                    props.insert("Parameter ID".to_string(), gen.subschema_for::<Option<String>>());
                    props.insert("Parameter Value".to_string(), gen.subschema_for::<serde_json::Value>());
                    props.insert("Default Parameter Value".to_string(), gen.subschema_for::<Option<serde_json::Value>>());
                    props.insert("Title HTML".to_string(), gen.subschema_for::<Option<String>>());
                    props.insert("Key Frames".to_string(), gen.subschema_for::<Option<serde_json::Value>>());
                    props.insert("Variant Type".to_string(), gen.subschema_for::<String>());
                    props.insert("maxValue".to_string(), gen.subschema_for::<Option<f64>>());
                    props.insert("minValue".to_string(), gen.subschema_for::<Option<f64>>());
                    props
                },
                required: std::collections::BTreeSet::new(),
                ..Default::default()
            })),
            ..Default::default()
        }
        .into()
    }
}

impl ResolveOTIOParameter {
    /// Get parameter value as f64


    /// Get the parameter ID
    pub fn parameter_id(&self) -> Option<&String> {
        match self {
            ResolveOTIOParameter::Int { parameter_id, .. }
            | ResolveOTIOParameter::UInt { parameter_id, .. }
            | ResolveOTIOParameter::Double { parameter_id, .. }
            | ResolveOTIOParameter::Bool { parameter_id, .. }
            | ResolveOTIOParameter::String { parameter_id, .. }
            | ResolveOTIOParameter::PointF { parameter_id, .. }
            | ResolveOTIOParameter::Color { parameter_id, .. }
            | ResolveOTIOParameter::Unknown { parameter_id, .. } => parameter_id.as_ref(),
        }
    }

    /// Get the title HTML
    pub fn title_html(&self) -> Option<&String> {
        match self {
            ResolveOTIOParameter::Unknown { title_html, .. } => title_html.as_ref(),
            _ => None,
        }
    }
}

/// Resolve_OTIO effect data structure (used in Effect.metadata)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ResolveOTIOData {
    #[serde(rename = "Effect Name", default)]
    pub effect_name: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "Parameters", default)]
    pub parameters: Option<Vec<ResolveOTIOParameter>>,
    #[serde(rename = "Type", default)]
    pub effect_type: Option<u64>,
}

/// Resolve_OTIO effect structure (used in media reference parameters array)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ResolveOTIOEffect {
    #[serde(rename = "Effect Name")]
    pub effect_name: String,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "Parameters", default)]
    pub parameters: Option<Vec<ResolveOTIOParameter>>,
    #[serde(rename = "Type", default)]
    pub effect_type: Option<u64>,
}

/// Generator parameters structure for GeneratorReference
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
pub struct GeneratorParameters {
    #[serde(rename = "Resolve_OTIO", default)]
    pub resolve_otio: Option<Vec<ResolveOTIOEffect>>,
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
    pub metadata: serde_json::Value,
}

/// Video transformation output coordinates
#[derive(Debug, Clone, PartialEq)]
pub struct VideoEffectOutput {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Audio effect output (gain/volume)
#[derive(Debug, Clone, PartialEq)]
pub struct AudioEffectOutput {
    pub gain: Option<f64>,
}

/// Text effect parameters
#[derive(Debug, Clone, PartialEq)]
pub struct TextEffectParams {
    pub position: [f64; 2],
    pub zoom_x: f64,
    pub zoom_y: f64,
    pub rotation: f64,
}

impl Effect {
    /// Parse Resolve_OTIO video transformation effects and convert to output coordinates.
    /// Returns None if the effect doesn't contain valid video transformation parameters.
    /// This method is defensive and won't panic on unexpected data structures.
    pub fn parse_video_effect(&self) -> Option<VideoEffectOutput> {
        // Deserialize Resolve_OTIO metadata
        let resolve_data: ResolveOTIOData = serde_json::from_value(
            self.metadata.get("Resolve_OTIO")?.clone()
        ).ok()?;

        let parameters = resolve_data.parameters?;

        // Initialize with default values
        let mut pan = 0.0;      // OTIO: -0.5 to 0.5, where 0 is center
        let mut tilt = 0.0;     // OTIO: -0.5 to 0.5, where 0 is center
        let mut zoom_x = 1.0;    // OTIO: normalized 0-1
        let mut zoom_y = 1.0;    // OTIO: normalized 0-1
        let mut _flip_y = false;

        // Collect all parameters
        for param in parameters {
            match param.parameter_id().and_then(|id| Some(id.as_str())) {
                Some("transformationPan") => {
                    if let ResolveOTIOParameter::Double { parameter_value, .. } = &param {
                        if let Some(num) = parameter_value {
                            pan = *num;
                        }
                    }
                }
                Some("transformationTilt") => {
                    if let ResolveOTIOParameter::Double { parameter_value, .. } = &param {
                        if let Some(num) = parameter_value {
                            tilt = *num;
                        }
                    }
                }
                Some("transformationZoomX") => {
                    if let ResolveOTIOParameter::Double { parameter_value, .. } = &param {
                        if let Some(num) = parameter_value {
                            zoom_x = *num;
                        }
                    }
                }
                Some("transformationZoomY") => {
                    if let ResolveOTIOParameter::Double { parameter_value, .. } = &param {
                        if let Some(num) = parameter_value {
                            zoom_y = *num;
                        }
                    }
                }
                Some("transformationFlipY") => {
                    if let ResolveOTIOParameter::Bool { parameter_value, .. } = &param {
                        if let Some(b) = parameter_value {
                            _flip_y = *b;
                        }
                    }
                }
                _ => {
                    // Ignore unknown parameters
                }
            }
        }

        // Convert OTIO coordinates to our coordinate system
        // OTIO: origin at center, X: -0.5 (left) to 0.5 (right), Y: -0.5 (bottom) to 0.5 (top)
        // Our system: origin at top-left, X: 0 (left) to 1 (right), Y: 0 (top) to 1 (bottom)
        // Calculate the position taking zoom into account:
        // 1. The zoom affects how much space is available for movement
        // 2. We need to center the zoomed content and then apply the pan/tilt
        Some(VideoEffectOutput {
            x: pan - zoom_x / 2.0 + 0.5,
            y: tilt - zoom_y / 2.0 + 0.5,
            width: zoom_x,
            height: zoom_y,
        })
    }

    /// Parse Resolve_OTIO audio effects (volume/gain).
    /// Returns None if the effect doesn't contain valid audio parameters.
    /// This method is defensive and won't panic on unexpected data structures.
    pub fn parse_audio_effect(&self) -> Option<AudioEffectOutput> {
        // Deserialize Resolve_OTIO metadata
        let resolve_data: ResolveOTIOData = serde_json::from_value(
            self.metadata.get("Resolve_OTIO")?.clone()
        ).ok()?;

        let parameters = resolve_data.parameters?;

        // Look for volume or gain parameters
        for param in parameters {
            if let Some(param_id) = param.parameter_id() {
                if param_id == "volume" {
                    let gain_value = match &param {
                        ResolveOTIOParameter::Double { parameter_value, .. } => *parameter_value,
                        _ => None,
                    };
                    if let Some(gain) = gain_value {
                        return Some(AudioEffectOutput {
                            gain: Some(gain),
                        });
                    }
                }
            }
        }

        None
    }

    /// Parse Resolve_OTIO text transformation parameters.
    /// Returns default values if parameters are not found.
    /// This method is defensive and won't panic on unexpected data structures.
    pub fn parse_text_effect(&self) -> TextEffectParams {
        let mut result = TextEffectParams {
            position: [0.5, 0.5],
            zoom_x: 1.0,
            zoom_y: 1.0,
            rotation: 0.0,
        };

        // Deserialize Resolve_OTIO metadata
        let resolve_otio_value = match self.metadata.get("Resolve_OTIO") {
            Some(v) => v,
            None => return result,
        };

        let resolve_data: ResolveOTIOData = match serde_json::from_value(
            resolve_otio_value.clone()
        ) {
            Ok(d) => d,
            Err(_) => return result,
        };

        let parameters = match resolve_data.parameters {
            Some(p) => p,
            None => return result,
        };

        // Parse parameters
        for param in parameters {
            match param.parameter_id().and_then(|id| Some(id.as_str())) {
                Some("position") => {
                    match &param {
                        ResolveOTIOParameter::PointF { parameter_value, .. } => {
                            if let Some(arr) = parameter_value {
                                result.position = *arr;
                            }
                        }
                        ResolveOTIOParameter::Unknown { parameter_value, .. } => {
                            if let Some(val) = parameter_value {
                                if let Some(arr) = val.as_array() {
                                    if arr.len() >= 2 {
                                        if let (Some(x), Some(y)) = (arr[0].as_f64(), arr[1].as_f64()) {
                                            result.position = [x, y];
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Some("transformationZoomX") => {
                    if let ResolveOTIOParameter::Double { parameter_value, .. } = &param {
                        if let Some(num) = parameter_value {
                            result.zoom_x = *num;
                        }
                    }
                }
                Some("transformationZoomY") => {
                    if let ResolveOTIOParameter::Double { parameter_value, .. } = &param {
                        if let Some(num) = parameter_value {
                            result.zoom_y = *num;
                        }
                    }
                }
                Some("transformationRotationAngle") => {
                    if let ResolveOTIOParameter::Double { parameter_value, .. } = &param {
                        if let Some(num) = parameter_value {
                            result.rotation = *num;
                        }
                    }
                }
                _ => {
                    // Ignore unknown parameters
                }
            }
        }

        result
    }
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
