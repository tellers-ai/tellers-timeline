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
fn default_enabled() -> bool {
    true
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
    #[serde(default = "default_enabled")]
    pub enabled: bool,
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
    pub fn bind_default_media_reference_when_needed(&mut self) {
        if let Item::Clip(c) = self {
            c.bind_default_media_reference_when_needed();
        }
    }
    pub fn clamp_to_active_available_range(&mut self) {
        if let Item::Clip(c) = self {
            c.clamp_to_active_available_range();
        }
    }
    pub fn duration(&self) -> Seconds {
        match self {
            Item::Clip(c) => c.source_range.duration.to_seconds(),
            Item::Gap(g) => g.source_range.duration.to_seconds(),
        }
    }
    pub fn set_duration(&mut self, dur: Seconds) {
        match self {
            Item::Clip(c) => c.source_range.duration.set_from_seconds(dur),
            Item::Gap(g) => g.source_range.duration.set_from_seconds(dur),
        }
    }
    pub fn get_enabled(&self) -> bool {
        match self {
            Item::Clip(c) => c.enabled,
            Item::Gap(_g) => true,
        }
    }
    pub fn set_enabled(&mut self, enabled: bool) {
        if let Item::Clip(c) = self {
            c.enabled = enabled;
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
    pub fn get_position(&self) -> MediaReferencePosition {
        match self {
            Item::Clip(c) => c.get_position(),
            Item::Gap(_g) => MediaReferencePosition {
                x: 0.0,
                y: 0.0,
                rotation: 0.0,
                zoom_x: 1.0,
                zoom_y: 1.0,
            },
        }
    }
    pub fn set_position(&mut self, position: MediaReferencePosition) {
        if let Item::Clip(c) = self {
            c.set_position(position);
        }
    }
    pub fn get_volume(&self) -> f64 {
        match self {
            Item::Clip(c) => c.get_volume(),
            Item::Gap(_g) => 1.0,
        }
    }
    pub fn set_volume(&mut self, volume: f64) {
        if let Item::Clip(c) = self {
            c.set_volume(volume);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Clip {
    #[serde(rename = "OTIO_SCHEMA", default = "default_clip_schema")]
    pub otio_schema: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
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
    pub fn bind_default_media_reference_when_needed(&mut self) {
        if self
            .active_media_reference_key
            .as_ref()
            .is_some_and(|key| self.media_references.contains_key(key))
        {
            return;
        }

        self.active_media_reference_key = if self.media_references.contains_key("DEFAULT_MEDIA") {
            Some("DEFAULT_MEDIA".to_string())
        } else {
            self.media_references.keys().next().cloned()
        };
    }

    pub fn clamp_to_active_available_range(&mut self) {
        let Some(active_key) = self
            .active_media_reference_key
            .as_deref()
            .filter(|key| self.media_references.contains_key(*key))
        else {
            self.source_range.duration.value = self.source_range.duration.value.max(0.0);
            return;
        };

        let Some(available_range) = self
            .media_references
            .get(active_key)
            .and_then(|reference| reference.available_range().as_ref())
        else {
            self.source_range.duration.value = self.source_range.duration.value.max(0.0);
            return;
        };

        let media_start = available_range.start_time.to_seconds().max(0.0);
        let media_duration = available_range.duration.to_seconds().max(0.0);
        let media_end = media_start + media_duration;
        let source_start = self.source_range.start_time.to_seconds().max(media_start);
        let requested_end =
            (self.source_range.start_time.to_seconds() + self.source_range.duration.to_seconds())
                .max(source_start);
        let clamped_end = requested_end.min(media_end);

        self.source_range.start_time.set_from_seconds(source_start);
        self.source_range
            .duration
            .set_from_seconds((clamped_end - source_start).max(0.0));
    }

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
            enabled: true,
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
            enabled: true,
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
                    if effect.effect_name == "Rich Text" {
                        for parameter in &effect.parameters {
                            match parameter {
                                ResolveOTIOParameter::PointF(param) if param.parameter_id == "position" => {
                                    if let Some([x_val, y_val]) = param.parameter_value {
                                        // Rich Text uses [0, 1] coordinate system, MediaReferencePosition uses [-0.5, +0.5]
                                        x = x_val - 0.5;
                                        y = y_val - 0.5;
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

    pub fn set_position(&mut self, position: MediaReferencePosition) {
        let active_media_reference = self.media_references.get_mut(self.active_media_reference_key.as_ref().unwrap()).unwrap();

        // Case 1: GeneratorReference - uses "position" (PointF) for x/y coordinates
        if let MediaReference::GeneratorReference { parameters, .. } = active_media_reference {
            let (rich_x, rich_y) = (position.x + 0.5, position.y + 0.5);

            if let Some(resolve_otio_effects) = parameters.resolve_otio.as_mut() {
                let mut found_rich_text_effect = false;
                for effect in resolve_otio_effects.iter_mut() {
                    if effect.effect_name == "Rich Text" && effect.effect_type == 24 {
                        found_rich_text_effect = true;

                        effect.parameters.retain(|param| {
                            match param {
                                ResolveOTIOParameter::PointF(p) if p.parameter_id == "position" => false,
                                ResolveOTIOParameter::Double(p) if p.parameter_id == "transformationZoomX" => false,
                                ResolveOTIOParameter::Double(p) if p.parameter_id == "transformationZoomY" => false,
                                ResolveOTIOParameter::Double(p) if p.parameter_id == "transformationRotationAngle" => false,
                                _ => true,
                            }
                        });

                        effect.parameters.push(ResolveOTIOParameter::PointF(ResolveOTIOParameterPointF {
                            variant_type: "POINTF".to_string(),
                            parameter_id: "position".to_string(),
                            parameter_value: Some([rich_x, rich_y]),
                            default_parameter_value: Some([0.5, 0.5]),
                            key_frames: None,
                        }));
                        effect.parameters.push(ResolveOTIOParameter::Double(ResolveOTIOParameterNumber {
                            variant_type: "Double".to_string(),
                            parameter_id: "transformationZoomX".to_string(),
                            parameter_value: position.zoom_x,
                            default_parameter_value: Some(1.0),
                            max_value: Some(100.0),
                            min_value: Some(0.0),
                        }));
                        effect.parameters.push(ResolveOTIOParameter::Double(ResolveOTIOParameterNumber {
                            variant_type: "Double".to_string(),
                            parameter_id: "transformationZoomY".to_string(),
                            parameter_value: position.zoom_y,
                            default_parameter_value: Some(1.0),
                            max_value: Some(100.0),
                            min_value: Some(0.0),
                        }));
                        effect.parameters.push(ResolveOTIOParameter::Double(ResolveOTIOParameterNumber {
                            variant_type: "Double".to_string(),
                            parameter_id: "transformationRotationAngle".to_string(),
                            parameter_value: position.rotation,
                            default_parameter_value: Some(0.0),
                            max_value: Some(100000.0),
                            min_value: Some(-100000.0),
                        }));
                        break;
                    }
                }
                if !found_rich_text_effect {
                    resolve_otio_effects.push(ResolveOTIOEffect {
                        effect_name: "Rich Text".to_string(),
                        enabled: true,
                        name: "Rich Text".to_string(),
                        parameters: vec![
                            ResolveOTIOParameter::PointF(ResolveOTIOParameterPointF {
                                variant_type: "POINTF".to_string(),
                                parameter_id: "position".to_string(),
                                parameter_value: Some([rich_x, rich_y]),
                                default_parameter_value: Some([0.5, 0.5]),
                                key_frames: None,
                            }),
                            ResolveOTIOParameter::Double(ResolveOTIOParameterNumber {
                                variant_type: "Double".to_string(),
                                parameter_id: "transformationZoomX".to_string(),
                                parameter_value: position.zoom_x,
                                default_parameter_value: Some(1.0),
                                max_value: Some(100.0),
                                min_value: Some(0.0),
                            }),
                            ResolveOTIOParameter::Double(ResolveOTIOParameterNumber {
                                variant_type: "Double".to_string(),
                                parameter_id: "transformationZoomY".to_string(),
                                parameter_value: position.zoom_y,
                                default_parameter_value: Some(1.0),
                                max_value: Some(100.0),
                                min_value: Some(0.0),
                            }),
                            ResolveOTIOParameter::Double(ResolveOTIOParameterNumber {
                                variant_type: "Double".to_string(),
                                parameter_id: "transformationRotationAngle".to_string(),
                                parameter_value: position.rotation,
                                default_parameter_value: Some(0.0),
                                max_value: Some(100000.0),
                                min_value: Some(-100000.0),
                            }),
                        ],
                        effect_type: 24,
                    });
                }

                resolve_otio_effects.retain(|effect| {
                    effect.effect_name != "Transform"
                });
            } else {
                parameters.resolve_otio = Some(vec![ResolveOTIOEffect {
                    effect_name: "Rich Text".to_string(),
                    enabled: true,
                    name: "Rich Text".to_string(),
                    parameters: vec![
                        ResolveOTIOParameter::PointF(ResolveOTIOParameterPointF {
                            variant_type: "POINTF".to_string(),
                            parameter_id: "position".to_string(),
                            parameter_value: Some([rich_x, rich_y]),
                            default_parameter_value: Some([0.5, 0.5]),
                            key_frames: None,
                        }),
                        ResolveOTIOParameter::Double(ResolveOTIOParameterNumber {
                            variant_type: "Double".to_string(),
                            parameter_id: "transformationZoomX".to_string(),
                            parameter_value: position.zoom_x,
                            default_parameter_value: Some(1.0),
                            max_value: Some(100.0),
                            min_value: Some(0.0),
                        }),
                        ResolveOTIOParameter::Double(ResolveOTIOParameterNumber {
                            variant_type: "Double".to_string(),
                            parameter_id: "transformationZoomY".to_string(),
                            parameter_value: position.zoom_y,
                            default_parameter_value: Some(1.0),
                            max_value: Some(100.0),
                            min_value: Some(0.0),
                        }),
                        ResolveOTIOParameter::Double(ResolveOTIOParameterNumber {
                            variant_type: "Double".to_string(),
                            parameter_id: "transformationRotationAngle".to_string(),
                            parameter_value: position.rotation,
                            default_parameter_value: Some(0.0),
                            max_value: Some(100000.0),
                            min_value: Some(-100000.0),
                        }),
                    ],
                    effect_type: 24,
                }]);
            }
        } else {
            // Case 2: ExternalReference - uses "transformationPan" and "transformationTilt" (Double) for x/y coordinates
            self.effects.retain(|effect| {
                if effect.effect_name == "Resolve Effect" {
                    if let Some(resolve_otio_effect) = &effect.metadata.resolve_otio {
                        // Keep Volume effects, remove Transform effects
                        if resolve_otio_effect.effect_name == "Transform" {
                            return false;
                        }
                    }
                }
                true
            });

            let transform_effect = ResolveOTIOEffect {
                effect_name: "Transform".to_string(),
                enabled: true,
                name: "Transform".to_string(),
                parameters: vec![
                    ResolveOTIOParameter::Double(ResolveOTIOParameterNumber {
                        variant_type: "Double".to_string(),
                        parameter_id: "transformationPan".to_string(),
                        parameter_value: position.x,
                        default_parameter_value: Some(0.0),
                        max_value: Some(4.0),
                        min_value: Some(-4.0),
                    }),
                    ResolveOTIOParameter::Double(ResolveOTIOParameterNumber {
                        variant_type: "Double".to_string(),
                        parameter_id: "transformationTilt".to_string(),
                        parameter_value: position.y,
                        default_parameter_value: Some(0.0),
                        max_value: Some(4.0),
                        min_value: Some(-4.0),
                    }),
                    ResolveOTIOParameter::Double(ResolveOTIOParameterNumber {
                        variant_type: "Double".to_string(),
                        parameter_id: "transformationZoomX".to_string(),
                        parameter_value: position.zoom_x,
                        default_parameter_value: Some(1.0),
                        max_value: Some(100.0),
                        min_value: Some(0.0),
                    }),
                    ResolveOTIOParameter::Double(ResolveOTIOParameterNumber {
                        variant_type: "Double".to_string(),
                        parameter_id: "transformationZoomY".to_string(),
                        parameter_value: position.zoom_y,
                        default_parameter_value: Some(1.0),
                        max_value: Some(100.0),
                        min_value: Some(0.0),
                    }),
                    ResolveOTIOParameter::Double(ResolveOTIOParameterNumber {
                        variant_type: "Double".to_string(),
                        parameter_id: "transformationRotationAngle".to_string(),
                        parameter_value: position.rotation,
                        default_parameter_value: Some(0.0),
                        max_value: Some(100000.0),
                        min_value: Some(-100000.0),
                    }),
                ],
                effect_type: 2,
            };

            self.effects.push(Effect {
                otio_schema: default_effect_schema(),
                name: "".to_string(),
                effect_name: "Resolve Effect".to_string(),
                metadata: EffectMetadata {
                    resolve_otio: Some(transform_effect),
                    other: serde_json::Map::new(),
                },
            });
        }
    }

    pub fn get_volume(&self) -> f64 {
        for effect in &self.effects {
            if effect.effect_name == "Resolve Effect" {
                if let Some(resolve_otio_effect) = effect.metadata.resolve_otio.as_ref() {
                    if resolve_otio_effect.name == "Volume" ||
                       resolve_otio_effect.effect_name == "Fairlight Clip Volume and Fades" {
                        for parameter in &resolve_otio_effect.parameters {
                            if let ResolveOTIOParameter::Double(param) = parameter {
                                if param.parameter_id == "volume" {
                                    return param.parameter_value;
                                }
                            }
                        }
                    }
                }
            }
        }
        return 1.0;
    }

    pub fn set_volume(&mut self, volume: f64) {
        self.effects.retain(|effect| {
            if effect.effect_name == "Resolve Effect" {
                if let Some(resolve_otio_effect) = &effect.metadata.resolve_otio {
                    // Keep Transform effects, remove Volume effects
                    if resolve_otio_effect.name == "Volume" || resolve_otio_effect.effect_name == "Fairlight Clip Volume and Fades" {
                        return false;
                    }
                }
            }
            true
        });

        let volume_effect = ResolveOTIOEffect {
            effect_name: "Fairlight Clip Volume and Fades".to_string(),
            enabled: true,
            name: "Volume".to_string(),
            parameters: vec![
                ResolveOTIOParameter::Double(ResolveOTIOParameterNumber {
                    variant_type: "Double".to_string(),
                    parameter_id: "volume".to_string(),
                    parameter_value: volume,
                    default_parameter_value: Some(0.0),
                    max_value: Some(30.0),
                    min_value: Some(-100.0),
                }),
            ],
            effect_type: 62,
        };

        self.effects.push(Effect {
            otio_schema: default_effect_schema(),
            name: "".to_string(),
            effect_name: "Resolve Effect".to_string(),
            metadata: EffectMetadata {
                resolve_otio: Some(volume_effect),
                other: serde_json::Map::new(),
            },
        });
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
        if let Some(variant_type_str) = variant_type_str {
            // Try to deserialize based on variant type
            match variant_type_str {
                "Int" => {
                    if let Ok(mut v) = serde_json::from_value::<ResolveOTIOParameterNumber<i64>>(value.clone()) {
                        v.variant_type = "Int".to_string();
                        return Ok(ResolveOTIOParameter::Int(v));
                    }
                }
                "UInt" => {
                    if let Ok(mut v) = serde_json::from_value::<ResolveOTIOParameterNumber<u64>>(value.clone()) {
                        v.variant_type = "UInt".to_string();
                        return Ok(ResolveOTIOParameter::UInt(v));
                    }
                }
                "Double" => {
                    if let Ok(mut v) = serde_json::from_value::<ResolveOTIOParameterNumber<f64>>(value.clone()) {
                        v.variant_type = "Double".to_string();
                        return Ok(ResolveOTIOParameter::Double(v));
                    }
                }
                "Bool" => {
                    if let Ok(mut v) = serde_json::from_value::<ResolveOTIOParameterSimple<bool>>(value.clone()) {
                        v.variant_type = "Bool".to_string();
                        return Ok(ResolveOTIOParameter::Bool(v));
                    }
                }
                "String" => {
                    if let Ok(mut v) = serde_json::from_value::<ResolveOTIOParameterSimple<String>>(value.clone()) {
                        v.variant_type = "String".to_string();
                        return Ok(ResolveOTIOParameter::String(v));
                    }
                }
                "POINTF" => {
                    if let Ok(mut v) = serde_json::from_value::<ResolveOTIOParameterPointF>(value.clone()) {
                        v.variant_type = "POINTF".to_string();
                        return Ok(ResolveOTIOParameter::PointF(v));
                    }
                }
                "Color" => {
                    if let Ok(mut v) = serde_json::from_value::<ResolveOTIOParameterSimple<String>>(value.clone()) {
                        v.variant_type = "Color".to_string();
                        return Ok(ResolveOTIOParameter::Color(v));
                    }
                }
                _ => {}
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
    #[serde(default, rename = "Enabled")]
    pub enabled: bool,
    #[serde(default, rename = "Name")]
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


#[derive(Clone)]
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
            .map(|tr| tr.start_time.to_seconds())
            .unwrap_or(0.0)
    }

    pub fn set_media_start(&mut self, start_seconds: Seconds) {
        let available_range = self.available_range_mut();
        if let Some(tr) = available_range {
            tr.start_time.set_from_seconds(start_seconds);
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
        self.available_range()
            .as_ref()
            .map(|tr| tr.duration.to_seconds())
    }

    pub fn set_media_duration(&mut self, duration_seconds: Option<Seconds>) {
        let available_range = self.available_range_mut();
        match duration_seconds {
            Some(v) => {
                if let Some(tr) = available_range {
                    tr.duration.set_from_seconds(v);
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

    /// Get the Rich Text Title HTML from a GeneratorReference
    /// Returns None if this is not a GeneratorReference or if the Rich Text effect is not found
    pub fn get_rich_text(&self) -> Option<String> {
        if let MediaReference::GeneratorReference { parameters, .. } = self {
            if let Some(resolve_otio_effects) = &parameters.resolve_otio {
                for effect in resolve_otio_effects {
                    if effect.effect_name == "Rich Text" && effect.effect_type == 24 {
                        for parameter in &effect.parameters {
                            if let ResolveOTIOParameter::Unknown(param) = parameter {
                                if param.parameter_id == "title blob" {
                                    return param.title_html.clone();
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Create a Rich Text GeneratorReference with the given Title HTML and default position
    pub fn create_rich_text_reference(title_html: String) -> MediaReference {
        let mut metadata = serde_json::Map::new();
        let mut resolve_otio = serde_json::Map::new();
        resolve_otio.insert("Generator Type".to_string(), serde_json::Value::String("Rich".to_string()));
        metadata.insert("Resolve_OTIO".to_string(), serde_json::Value::Object(resolve_otio));

        let rich_text_effect = ResolveOTIOEffect {
            effect_name: "Rich Text".to_string(),
            enabled: true,
            name: "Rich Text".to_string(),
            parameters: vec![
                ResolveOTIOParameter::Unknown(ResolveOTIOParameterUnknown {
                    parameter_id: "title blob".to_string(),
                    parameter_value: None,
                    default_parameter_value: None,
                    key_frames: None,
                    title_html: Some(title_html),
                }),
                ResolveOTIOParameter::PointF(ResolveOTIOParameterPointF {
                    variant_type: "POINTF".to_string(),
                    parameter_id: "position".to_string(),
                    parameter_value: Some([0.5, 0.5]),
                    default_parameter_value: Some([0.5, 0.5]),
                    key_frames: None,
                }),
                ResolveOTIOParameter::Double(ResolveOTIOParameterNumber {
                    variant_type: "Double".to_string(),
                    parameter_id: "transformationZoomX".to_string(),
                    parameter_value: 1.0,
                    default_parameter_value: Some(1.0),
                    max_value: Some(4.0),
                    min_value: Some(0.25),
                }),
                ResolveOTIOParameter::Double(ResolveOTIOParameterNumber {
                    variant_type: "Double".to_string(),
                    parameter_id: "transformationZoomY".to_string(),
                    parameter_value: 1.0,
                    default_parameter_value: Some(1.0),
                    max_value: Some(4.0),
                    min_value: Some(0.25),
                }),
                ResolveOTIOParameter::Bool(ResolveOTIOParameterSimple {
                    variant_type: "Bool".to_string(),
                    parameter_id: "transformationZoomLink".to_string(),
                    parameter_value: true,
                    default_parameter_value: Some(true),
                }),
                ResolveOTIOParameter::Double(ResolveOTIOParameterNumber {
                    variant_type: "Double".to_string(),
                    parameter_id: "transformationRotationAngle".to_string(),
                    parameter_value: 0.0,
                    default_parameter_value: Some(0.0),
                    max_value: Some(100000.0),
                    min_value: Some(-100000.0),
                }),
            ],
            effect_type: 24,
        };

        let parameters = GeneratorParameters {
            resolve_otio: Some(vec![rich_text_effect]),
            other: serde_json::Map::new(),
        };

        MediaReference::GeneratorReference {
            generator_kind: "Rich".to_string(),
            available_range: None,
            name: Some("Text".to_string()),
            available_image_bounds: None,
            metadata: serde_json::Value::Object(metadata),
            parameters,
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

impl RationalTime {
    pub fn to_seconds(&self) -> Seconds {
        if self.rate.abs() > f64::EPSILON {
            self.value / self.rate
        } else {
            self.value
        }
    }

    pub fn set_from_seconds(&mut self, seconds: Seconds) {
        self.value = if self.rate.abs() > f64::EPSILON {
            seconds * self.rate
        } else {
            seconds
        };
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
        self.duration.to_seconds()
    }
    pub fn get_start_time(&self) -> Seconds {
        self.start_time.to_seconds()
    }
    pub fn set_duration(&mut self, duration: Seconds) {
        self.duration.set_from_seconds(duration);
    }
    pub fn set_start_time(&mut self, start_time: Seconds) {
        self.start_time.set_from_seconds(start_time);
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
    pub fn add_track_at(&mut self, track: Track, insertion_index: isize) -> bool {
        self.tracks.add_track_at(track, insertion_index)
    }
    pub fn reorder_track(&mut self, id: &str, insertion_index: isize) -> bool {
        self.tracks.reorder_track(id, insertion_index)
    }
    pub fn sync_track_info(&self) -> Vec<crate::SyncTrackInfo> {
        self.tracks.sync_track_info()
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
            enabled: true,
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

    pub fn timeline_ids(&self) -> Vec<String> {
        self.items
            .iter()
            .filter_map(crate::metadata::IdMetadataExt::get_id)
            .collect()
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
