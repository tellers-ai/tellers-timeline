//! Effects on detected subjects: a filter or a `Mask` declared with a
//! `detect` parameter.
//!
//! Any filter (`Blur`, `Pixelate`, `Adjustment`, ... `ChromaKey`) and a
//! `Mask` can take `detect`, what to detect in the clip, instead of drawn
//! shapes:
//!
//! ```json
//! { "effect_name": "tellers.ai.Blur", "metadata": { "tellers.ai": { "parameters": {
//!     "strength": 40, "detect": { "target": "faces" } } } } }
//! { "effect_name": "tellers.ai.Mask", "metadata": { "tellers.ai": { "parameters": {
//!     "feather": 4,
//!     "detect": { "target": "people", "pick": { "t": 1, "x": 0.31, "y": -0.16 }, "shape": "outline" }
//! } } } }
//! ```
//!
//! `pick` is one point or a list of them (video-player-js #689). On load
//! the player detects `target` over the clip's source range and resolves
//! the effect: a filter gets `regions` (every subject, or the ones at the
//! picks: points finding nobody are skipped, and two on one subject give it
//! one region), a Mask the keyframed shape of one subject (the first one a
//! pick finds, else the one seen longest) as its `shape`, `x`, `y`, `width`, `height`,
//! `points` and `opacity`. It reports that through `onDetectResolved`
//! (`{ clipId, effectIndex, effectName, parameters }`) for the host to save
//! with [`Timeline::apply_detect_resolution`]. `detect` stays in the saved
//! parameters; dropping `regions` (or the Mask's `x`) detects again.
//!
//! With `track` (a video track's name, id or `tellers.ai.timeline_id`),
//! the subject is detected in the clip of that track playing longest
//! alongside this one, and its regions are moved into this clip's time and
//! box (video-player-js #690): an overlay's Mask with `invert: true` then
//! cuts out a person crossing it. `pick` is then in this clip's seconds and
//! frame coordinates.
//!
//! How well subjects are found (sample rate, models, thresholds) is the
//! player's `detection` option, not the timeline's; `detect` keeps what to
//! detect and how the regions look.
//!
//! The player resolves only clips' effects (not tracks' or composite
//! clips'). Mirrors `EffectDetect` of `schema/timeline.schema.json` in
//! `video-player-js` (#687, #690).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::follow::{is_follow_target, FollowDetect, FollowPick};
use crate::metadata::IdMetadataExt;
use crate::{Item, TellersEffect, Timeline, Track};

/// The parameter of a filter or `Mask` declaring what to detect.
pub const DETECT_PARAMETER: &str = "detect";
const MASK: &str = "Mask";
const REGIONS: &str = "regions";
/// The `Mask` parameters a detection resolves; the Mask's others
/// (`feather`, `corner_radius`, `invert`, `mode`, ...) are kept.
pub const DETECT_MASK_PARAMETERS: [&str; 7] =
    ["shape", "x", "y", "width", "height", "opacity", "points"];

/// What a detected subject's region is: its box (`rectangle` or `ellipse`;
/// faces default to ellipses, others to rounded rectangles) or `outline`,
/// its exact shape, segmented every sample and keyframed as a `path`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DetectShape {
    Rectangle,
    Ellipse,
    Outline,
}

/// The `detect` parameter of a filter or a `Mask`: the subject to detect,
/// where, and how its regions look (left out: the player's defaults).
#[derive(Debug, Clone, PartialEq)]
pub struct EffectDetect {
    /// Faces, a group (people, animals, vehicles) or one COCO category;
    /// see [`crate::is_follow_target`].
    pub target: String,
    /// Detect in the clip of this video track (its name, id or
    /// `tellers.ai.timeline_id`) playing longest alongside this clip,
    /// instead of in this clip's media.
    pub track: Option<String>,
    /// Only the subjects at these points (a Mask: the first one found);
    /// written as one object when there is one point. Empty: a filter
    /// covers every subject and a Mask the one seen longest.
    pub pick: Vec<FollowPick>,
    pub shape: Option<DetectShape>,
    /// `padX`, `padY`, beside `target` in the JSON.
    pub options: FollowDetect,
}

impl EffectDetect {
    /// Detect every `target` subject with the player's defaults.
    pub fn new(target: impl Into<String>) -> Self {
        EffectDetect {
            target: target.into(),
            track: None,
            pick: Vec::new(),
            shape: None,
            options: FollowDetect::default(),
        }
    }

    /// Detect in the clip of the video track `track` (name, id or
    /// `tellers.ai.timeline_id`) instead of in this clip's media.
    pub fn on_track(mut self, track: impl Into<String>) -> Self {
        self.track = Some(track.into());
        self
    }

    /// Add a point to `pick`.
    pub fn with_pick(mut self, pick: FollowPick) -> Self {
        self.pick.push(pick);
        self
    }

    pub fn with_shape(mut self, shape: DetectShape) -> Self {
        self.shape = Some(shape);
        self
    }

    /// The `detect` parameter value, if `value` is a valid one: a known
    /// `target`, and no key the player rejects.
    pub fn from_value(value: &Value) -> Option<EffectDetect> {
        let mut object = value.as_object()?.clone();
        let target = object.remove("target")?.as_str()?.to_string();
        if !is_follow_target(&target) {
            return None;
        }
        let track = match object.remove("track") {
            Some(Value::String(track)) => Some(track),
            Some(_) => return None,
            None => None,
        };
        let pick = match object.remove("pick") {
            Some(Value::Array(picks)) if !picks.is_empty() => picks
                .into_iter()
                .map(|pick| serde_json::from_value(pick).ok())
                .collect::<Option<Vec<FollowPick>>>()?,
            Some(pick @ Value::Object(_)) => vec![serde_json::from_value(pick).ok()?],
            Some(_) => return None,
            None => Vec::new(),
        };
        let shape = match object.remove("shape") {
            Some(shape) => Some(serde_json::from_value(shape).ok()?),
            None => None,
        };
        let options = serde_json::from_value(Value::Object(object)).ok()?;
        Some(EffectDetect { target, track, pick, shape, options })
    }

    pub fn to_value(&self) -> Value {
        let mut object = match serde_json::to_value(self.options) {
            Ok(Value::Object(object)) => object,
            _ => Map::new(),
        };
        object.insert("target".to_string(), self.target.clone().into());
        if let Some(track) = &self.track {
            object.insert("track".to_string(), track.clone().into());
        }
        let pick = match self.pick.as_slice() {
            [] => None,
            [pick] => Some(serde_json::to_value(pick)),
            picks => Some(serde_json::to_value(picks)),
        };
        if let Some(pick) = pick {
            object.insert("pick".to_string(), pick.expect("a pick serializes"));
        }
        if let Some(shape) = self.shape {
            object.insert("shape".to_string(), serde_json::to_value(shape).expect("a shape serializes"));
        }
        Value::Object(object)
    }
}

/// Whether `value` is a valid `detect` parameter.
pub(crate) fn detect_value_is_valid(value: &Value) -> bool {
    EffectDetect::from_value(value).is_some()
}

/// Whether `effect`, declared with `detect`, still has to be resolved: a
/// filter without `regions`, a Mask without `x`.
fn needs_detection(effect: &TellersEffect) -> bool {
    if effect.effect_name == MASK {
        !effect.parameters.contains_key("x")
    } else {
        !effect.parameters.contains_key(REGIONS)
    }
}

/// A `detect` on one of a clip's effects.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemDetect {
    pub item_id: String,
    /// The effect's index in the clip's `effects` (all of them, as the
    /// player's `effectIndex`).
    pub effect_index: usize,
    /// Bare, e.g. `Blur` or `Mask`.
    pub effect_name: String,
    pub detect: EffectDetect,
    /// Whether it has been detected (`regions`, or a Mask's `x`, present).
    pub resolved: bool,
}

/// The ids of every clip of `tracks`, at any depth (inside composite clips too).
fn collect_clip_ids(tracks: &[Track], ids: &mut Vec<String>) {
    for item in tracks.iter().flat_map(|track| &track.items) {
        match item {
            Item::Clip(_) => ids.extend(item.get_id()),
            Item::Composite(stack) => collect_clip_ids(&stack.children, ids),
            _ => {}
        }
    }
}

impl Timeline {
    /// The `detect`s of the clip `item_id`'s enabled filters and Masks, in
    /// effect order; empty when it has none or no clip has this id.
    pub fn get_item_detects(&self, item_id: &str) -> Vec<ItemDetect> {
        let Some(Item::Clip(clip)) = self.tracks.get_nested_item(item_id) else {
            return Vec::new();
        };
        clip.effects
            .iter()
            .enumerate()
            .filter_map(|(effect_index, effect)| {
                let effect = TellersEffect::from_effect(effect).filter(|e| e.enabled)?;
                let spec = effect.spec()?;
                spec.parameter(DETECT_PARAMETER)?;
                let detect = EffectDetect::from_value(effect.parameters.get(DETECT_PARAMETER)?)?;
                Some(ItemDetect {
                    item_id: item_id.to_string(),
                    effect_index,
                    resolved: !needs_detection(&effect),
                    effect_name: effect.effect_name,
                    detect,
                })
            })
            .collect()
    }

    /// Every clip effect with a `detect`, at any depth, in track order.
    pub fn item_detects(&self) -> Vec<ItemDetect> {
        let mut ids = Vec::new();
        collect_clip_ids(&self.tracks.children, &mut ids);
        ids.iter().flat_map(|id| self.get_item_detects(id)).collect()
    }

    /// The `detect`s the player still has to resolve on load: a filter
    /// without `regions`, a Mask without `x`.
    pub fn unresolved_detects(&self) -> Vec<ItemDetect> {
        self.item_detects().into_iter().filter(|d| !d.resolved).collect()
    }

    /// Save what the player detected for the clip `item_id`'s effect at
    /// `effect_index` (its `onDetectResolved` `effectIndex` and
    /// `parameters`): a filter's `regions`, or a Mask's
    /// [`DETECT_MASK_PARAMETERS`]. Other keys are the player's view of the
    /// effect and are ignored, so the effect's own values stay.
    ///
    /// Detection takes a while, so the effect may have changed since the
    /// player read it: the resolution is saved only while that effect is
    /// still an enabled one named `effect_name` with the same `detect` as
    /// `parameters`.
    ///
    /// `false`, changing nothing, when no clip has this id, the effect
    /// changed, `parameters` has none of the detected keys, or the result
    /// is not a valid effect.
    pub fn apply_detect_resolution(
        &mut self,
        item_id: &str,
        effect_index: usize,
        effect_name: &str,
        parameters: &Map<String, Value>,
    ) -> bool {
        let Some(Item::Clip(clip)) = self.tracks.get_nested_item_mut(item_id) else {
            return false;
        };
        let Some(raw) = clip.effects.get_mut(effect_index) else {
            return false;
        };
        let Some(mut effect) = TellersEffect::from_effect(raw).filter(|e| e.enabled) else {
            return false;
        };
        let current = effect.parameters.get(DETECT_PARAMETER).and_then(EffectDetect::from_value);
        let resolved = parameters.get(DETECT_PARAMETER).and_then(EffectDetect::from_value);
        if effect.effect_name != effect_name || current.is_none() || current != resolved {
            return false;
        }
        let keys: &[&str] = if effect.effect_name == MASK { &DETECT_MASK_PARAMETERS } else { &[REGIONS] };
        let mut saved = false;
        for key in keys {
            if let Some(value) = parameters.get(*key) {
                effect.parameters.insert(key.to_string(), value.clone());
                saved = true;
            }
        }
        if !saved || effect.validate().is_err() {
            return false;
        }
        effect.write_into(raw);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn timeline(effects: Value) -> Timeline {
        serde_json::from_value(json!({
            "OTIO_SCHEMA": "Timeline.1",
            "tracks": {
                "OTIO_SCHEMA": "Stack.1",
                "children": [
                    { "OTIO_SCHEMA": "Track.1", "kind": "Video", "children": [{
                        "OTIO_SCHEMA": "Clip.2",
                        "metadata": { "tellers.ai": { "timeline_id": "vc" } },
                        "source_range": {
                            "OTIO_SCHEMA": "TimeRange.1",
                            "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 0, "rate": 1 },
                            "duration": { "OTIO_SCHEMA": "RationalTime.1", "value": 8, "rate": 1 }
                        },
                        "effects": effects
                    }] }
                ]
            }
        }))
        .unwrap()
    }

    fn tellers(name: &str, parameters: Value) -> Value {
        json!({ "OTIO_SCHEMA": "Effect.1", "effect_name": format!("tellers.ai.{name}"),
                "metadata": { "tellers.ai": { "parameters": parameters } } })
    }

    #[test]
    fn detect_uses_the_player_names_and_rejects_other_keys() {
        let value = json!({ "target": "people", "pick": { "t": 1, "x": 0.31, "y": -0.16 },
                            "shape": "outline", "padX": 1.2 });
        let detect = EffectDetect::from_value(&value).unwrap();
        assert_eq!(detect.shape, Some(DetectShape::Outline));
        assert_eq!(detect.pick, vec![FollowPick { t: 1.0, x: 0.31, y: -0.16 }]);
        assert_eq!(detect.options.pad_x, Some(1.2));
        assert_eq!(EffectDetect::from_value(&detect.to_value()), Some(detect));
        assert_eq!(EffectDetect::new("faces").to_value(), json!({ "target": "faces" }));

        assert!(EffectDetect::from_value(&json!({ "target": "unicorn" })).is_none());
        assert!(EffectDetect::from_value(&json!({ "pick": { "t": 0, "x": 0, "y": 0 } })).is_none());
        assert!(EffectDetect::from_value(&json!({ "target": "faces", "shape": "path" })).is_none());
        assert!(EffectDetect::from_value(&json!({ "target": "faces", "modelUrl": "x" })).is_none());
        // How well subjects are found is the player's setting now (video-player-js #690).
        for key in crate::MOVED_DETECTION_SETTINGS {
            assert!(EffectDetect::from_value(&json!({ "target": "faces", key: 1 })).is_none(), "{key}");
        }

        // `track`: detect in another track's clip (video-player-js #690).
        let crossing = json!({ "target": "people", "track": "Base", "pick": { "t": 1.0, "x": 0.0, "y": 0.0 } });
        let detect = EffectDetect::from_value(&crossing).unwrap();
        assert_eq!(detect.track.as_deref(), Some("Base"));
        assert_eq!(detect.to_value(), crossing);
        assert_eq!(EffectDetect::new("people").on_track("v1").to_value(), json!({ "target": "people", "track": "v1" }));
        assert!(EffectDetect::from_value(&json!({ "target": "people", "track": 2 })).is_none());

        // `pick` can be a list of points (video-player-js #689), not an empty one.
        let faces = json!({ "target": "faces",
                            "pick": [{ "t": 1.0, "x": -0.2, "y": -0.1 }, { "t": 3.2, "x": 0.25, "y": -0.15 }] });
        let detect = EffectDetect::from_value(&faces).unwrap();
        assert_eq!(detect.pick.len(), 2);
        assert_eq!(detect.to_value()["pick"], faces["pick"]);
        let one = EffectDetect::new("faces").with_pick(FollowPick { t: 0.0, x: 0.0, y: 0.0 });
        assert!(one.to_value()["pick"].is_object());
        assert!(EffectDetect::from_value(&json!({ "target": "faces", "pick": [] })).is_none());
        assert!(EffectDetect::from_value(&json!({ "target": "faces", "pick": [{ "t": 0 }] })).is_none());
        assert!(EffectDetect::from_value(&json!({ "target": "faces", "pick": 1 })).is_none());
    }

    #[test]
    fn filters_and_masks_take_detect_but_not_their_regions() {
        let with = |name: &str, detect: Value| {
            TellersEffect::new(name).with_parameter(DETECT_PARAMETER, detect).validate()
        };
        for name in ["Blur", "Pixelate", "Adjustment", "ChromaKey", "Mask"] {
            assert!(with(name, json!({ "target": "faces" })).is_ok(), "{name}");
            assert!(with(name, json!({ "target": "nobody" })).is_err(), "{name}");
        }
        // Every filter (each takes `regions`) and the Mask, nothing else.
        let takes_detect: Vec<_> = crate::EFFECT_SPECS
            .iter()
            .filter(|spec| spec.parameter(DETECT_PARAMETER).is_some())
            .map(|spec| spec.name)
            .collect();
        let filters: Vec<_> = crate::EFFECT_SPECS
            .iter()
            .filter(|spec| spec.parameter(REGIONS).is_some() || spec.name == MASK)
            .map(|spec| spec.name)
            .collect();
        assert_eq!(takes_detect, filters);
        assert_eq!(takes_detect.len(), 11);
        for name in ["Transform", "Border", "Stabilize", "ColorLut"] {
            assert!(with(name, json!({ "target": "faces" })).is_err(), "{name}");
        }
        let region_with_detect = TellersEffect::new("Blur")
            .with_parameter(REGIONS, json!([{ "shape": "ellipse", "detect": { "target": "faces" } }]));
        assert!(region_with_detect.validate().is_err());
    }

    #[test]
    fn lists_what_the_player_resolves_and_saves_its_result() {
        let mut timeline = timeline(json!([
            tellers("Blur", json!({ "strength": 40, "detect": { "target": "faces" } })),
            tellers("Transform", json!({ "x": 0.1 })),
            tellers("Mask", json!({ "feather": 4, "invert": true,
                                    "detect": { "target": "people", "shape": "outline" } })),
            tellers("Pixelate", json!({ "regions": [], "detect": { "target": "car" } })),
        ]));
        let unresolved = timeline.unresolved_detects();
        assert_eq!(
            unresolved.iter().map(|d| (d.effect_index, d.effect_name.as_str())).collect::<Vec<_>>(),
            vec![(0, "Blur"), (2, "Mask")]
        );
        assert_eq!(timeline.item_detects().len(), 3);

        // A filter keeps its own parameters and gets the regions.
        let blur = json!({ "strength": 99, "detect": { "target": "faces" },
                           "regions": [{ "shape": "ellipse", "x": 0.1, "y": 0, "width": 0.2, "height": 0.3 }] });
        assert!(timeline.apply_detect_resolution("vc", 0, "Blur", blur.as_object().unwrap()));
        // A Mask gets its shape and keeps feather and invert.
        let mask = json!({ "detect": { "target": "people", "shape": "outline" }, "feather": 0,
                           "shape": "path", "x": { "keyframes": [{ "t": 0, "v": 0 }, { "t": 8, "v": 0.2 }] },
                           "y": 0, "opacity": 1, "points": [[0, -0.2], [0.1, 0.2], [-0.1, 0.2]] });
        assert!(timeline.apply_detect_resolution("vc", 2, "Mask", mask.as_object().unwrap()));
        assert!(timeline.unresolved_detects().is_empty());

        let effects = timeline.get_item_tellers_effects("vc").unwrap();
        assert_eq!(effects[0].parameters["strength"], json!(40));
        assert_eq!(effects[0].parameters[REGIONS], blur[REGIONS]);
        assert_eq!(effects[2].parameters["shape"], json!("path"));
        assert_eq!(effects[2].parameters["feather"], json!(4));
        assert_eq!(effects[2].parameters["invert"], json!(true));
    }

    #[test]
    fn a_resolution_for_a_changed_effect_is_dropped() {
        let mut timeline = timeline(json!([
            tellers("Blur", json!({ "detect": { "target": "faces" } })),
        ]));
        let resolved = |target: &str| {
            json!({ "detect": { "target": target }, "regions": [] }).as_object().unwrap().clone()
        };
        assert!(!timeline.apply_detect_resolution("vc", 0, "Blur", &resolved("people")));
        assert!(!timeline.apply_detect_resolution("vc", 0, "Pixelate", &resolved("faces")));
        assert!(!timeline.apply_detect_resolution("vc", 1, "Blur", &resolved("faces")));
        assert!(!timeline.apply_detect_resolution("nope", 0, "Blur", &resolved("faces")));
        let no_regions = json!({ "detect": { "target": "faces" } }).as_object().unwrap().clone();
        assert!(!timeline.apply_detect_resolution("vc", 0, "Blur", &no_regions));
        assert_eq!(timeline.unresolved_detects().len(), 1);
        assert!(timeline.apply_detect_resolution("vc", 0, "Blur", &resolved("faces")));
    }
}
