//! Auto-tracking zoom: a `Transform` that follows a detected subject.
//!
//! The player's `VideoPlayer.followTransform(region, options)` plans a
//! camera path over a clip and returns `Transform` keyframes (`x`, `y`,
//! `scale`). The recipe they came from, the detected region and the
//! options, is kept beside them as the Transform's `follow` parameter:
//!
//! ```json
//! { "effect_name": "tellers.ai.Transform", "name": "Follow",
//!   "metadata": { "tellers.ai": { "parameters": {
//!     "x": { "keyframes": [ ... ] }, "y": 0.15, "scale": 4.1,
//!     "follow": { "target": "people", "fill": 0.6, "aspect": 3.16,
//!                 "region": { "shape": "rectangle", "x": { "keyframes": [ ... ] }, ... } }
//!   } } } }
//! ```
//!
//! Playback reads only the baked `x`, `y` and `scale`; `follow` is what
//! recomputes them (call `followTransform` again with its `region` and
//! settings, then [`Timeline::set_item_follow`] with the new keyframes).
//! Mirrors the `follow` of `schema/timeline.schema.json` in
//! `video-player-js`.
//!
//! A follow can also be declared before anything is detected: a `target`
//! and a `pick` (where the subject is, at one time), with no `region` and
//! no keyframes yet. On load the player then detects `target` in the clip,
//! takes the subject at `pick` (`VideoPlayer.trackAt`), computes the
//! `region` and keyframes and reports them (`onFollowResolved`) for the
//! host to save with [`Timeline::set_item_follow`]. A follow needs a
//! `region`, or both `target` and `pick`.
//!
//! An item's follow is the `follow` of its first enabled `Transform`, the
//! one the player applies.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::effects::mask_shape_is_valid;
use crate::metadata::IdMetadataExt;
use crate::{Item, TellersEffect, Timeline, Track};

const TRANSFORM: &str = "Transform";
/// The `Transform` parameter holding the recipe.
pub const FOLLOW_PARAMETER: &str = "follow";
/// The `Transform` parameters a follow computes.
pub const FOLLOW_TRANSFORM_PARAMETERS: [&str; 3] = ["x", "y", "scale"];
/// `detectRegions` targets a followed region can come from: faces or a
/// group of COCO categories.
pub const FOLLOW_GROUP_TARGETS: [&str; 4] = ["faces", "people", "animals", "vehicles"];
/// The 80 COCO categories, each also a `detectRegions` target on its own.
pub const COCO_CATEGORIES: [&str; 80] = [
    "person", "bicycle", "car", "motorcycle", "airplane", "bus", "train", "truck", "boat",
    "traffic light", "fire hydrant", "stop sign", "parking meter", "bench", "bird", "cat",
    "dog", "horse", "sheep", "cow", "elephant", "bear", "zebra", "giraffe", "backpack",
    "umbrella", "handbag", "tie", "suitcase", "frisbee", "skis", "snowboard", "sports ball",
    "kite", "baseball bat", "baseball glove", "skateboard", "surfboard", "tennis racket",
    "bottle", "wine glass", "cup", "fork", "knife", "spoon", "bowl", "banana", "apple",
    "sandwich", "orange", "broccoli", "carrot", "hot dog", "pizza", "donut", "cake", "chair",
    "couch", "potted plant", "bed", "dining table", "toilet", "tv", "laptop", "mouse", "remote",
    "keyboard", "cell phone", "microwave", "oven", "toaster", "sink", "refrigerator", "book",
    "clock", "vase", "scissors", "teddy bear", "hair drier", "toothbrush",
];

/// Whether `target` is a `detectRegions` target: one of
/// [`FOLLOW_GROUP_TARGETS`] or [`COCO_CATEGORIES`].
pub fn is_follow_target(target: &str) -> bool {
    FOLLOW_GROUP_TARGETS.contains(&target) || COCO_CATEGORIES.contains(&target)
}

/// Where a followed subject was chosen: at `t` clip seconds, at (`x`, `y`)
/// in clip space before crop ((0, 0) the clip's center, ±0.5 its edges,
/// like regions).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FollowPick {
    pub t: f64,
    pub x: f64,
    pub y: f64,
}

/// The recipe of an auto-tracking zoom: the region followed (or what to
/// detect and where) and the `followTransform` options (left out: the
/// player's defaults, in brackets).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Follow {
    /// The detected region followed, a `Mask` shape in clip space (a
    /// `detectRegions` region); `None` until the player resolves a follow
    /// declared with `target` and `pick`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<Map<String, Value>>,
    /// Where the subject was chosen; with `target`, enough to find it again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pick: Option<FollowPick>,
    /// The `detectRegions` target the region came from; see [`is_follow_target`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Share of the frame the region fills, its larger side (0.6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<f64>,
    /// The clip's aspect ratio over the frame's (1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect: Option<f64>,
    /// Zoom limits, from the scale at which the clip just covers the frame (1, 2.5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_zoom: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_zoom: Option<f64>,
    /// The camera holds until the subject's center leaves this distance, frame fractions (0.06).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dead_zone: Option<f64>,
    /// Zoom changes below this ratio are ignored (0.15).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom_dead_zone: Option<f64>,
    /// Smoothing in seconds (0.6 pan, 0.8 zoom).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pan_smoothing: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom_smoothing: Option<f64>,
    /// Where the subject sits, as an offset from the frame's center (0, 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_y: Option<f64>,
    /// Misses up to this long keep the framing, seconds (0.5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold: Option<f64>,
    /// Keyframes whose removal moves the camera less than this are dropped (0.002).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<f64>,
}

impl Follow {
    /// A follow of `region` with the player's default settings.
    pub fn new(region: Map<String, Value>) -> Self {
        Follow { region: Some(region), ..Self::empty() }
    }

    /// A follow of the `target` subject at `pick`, for the player to detect
    /// and resolve; see the [module docs](self).
    pub fn declared(target: impl Into<String>, pick: FollowPick) -> Self {
        Follow { target: Some(target.into()), pick: Some(pick), ..Self::empty() }
    }

    fn empty() -> Self {
        Follow {
            region: None,
            pick: None,
            target: None,
            fill: None,
            aspect: None,
            min_zoom: None,
            max_zoom: None,
            dead_zone: None,
            zoom_dead_zone: None,
            pan_smoothing: None,
            zoom_smoothing: None,
            anchor_x: None,
            anchor_y: None,
            hold: None,
            tolerance: None,
        }
    }

    /// The `follow` parameter value, if `value` is a valid one.
    pub fn from_value(value: &Value) -> Option<Follow> {
        let follow: Follow = serde_json::from_value(value.clone()).ok()?;
        follow.is_valid().then_some(follow)
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("a follow serializes")
    }

    /// Whether the region followed is known; a follow declared with only
    /// `target` and `pick` is not, until the player resolves it.
    pub fn is_resolved(&self) -> bool {
        self.region.is_some()
    }

    /// Whether every key `other` sets has the same value here: this follow
    /// is `other` with keys added (a resolved region, filled-in defaults).
    pub fn extends(&self, other: &Follow) -> bool {
        let (Value::Object(this), Value::Object(other)) = (self.to_value(), other.to_value()) else {
            return false;
        };
        other.iter().all(|(key, value)| this.get(key) == Some(value))
    }

    /// A `region` (`Mask`-shaped), or both `target` and `pick`; a known
    /// target.
    fn is_valid(&self) -> bool {
        let region_ok = self
            .region
            .as_ref()
            .is_none_or(|region| mask_shape_is_valid(&Value::Object(region.clone())));
        region_ok
            && (self.region.is_some() || (self.target.is_some() && self.pick.is_some()))
            && self.target.as_deref().is_none_or(is_follow_target)
    }
}

/// Whether `value` is a valid `follow` parameter.
pub(crate) fn follow_value_is_valid(value: &Value) -> bool {
    Follow::from_value(value).is_some()
}

/// The ids of every item of `tracks`, at any depth (inside composite clips too).
fn collect_item_ids(tracks: &[Track], ids: &mut Vec<String>) {
    for item in tracks.iter().flat_map(|track| &track.items) {
        ids.extend(item.get_id());
        if let Item::Composite(stack) = item {
            collect_item_ids(&stack.children, ids);
        }
    }
}

/// The index (among tellers effects) of the first enabled Transform.
fn first_transform(effects: &[TellersEffect]) -> Option<usize> {
    effects
        .iter()
        .position(|effect| effect.enabled && effect.effect_name == TRANSFORM)
}

impl Timeline {
    /// The follow of the clip or composite clip `item_id`: the `follow` of
    /// its first enabled Transform. `None` when it has none (or an invalid
    /// one), or no item has this id.
    pub fn get_item_follow(&self, item_id: &str) -> Option<Follow> {
        let effects = self.get_item_tellers_effects(item_id)?;
        let transform = &effects[first_transform(&effects)?];
        Follow::from_value(transform.parameters.get(FOLLOW_PARAMETER)?)
    }

    /// Make the clip or composite clip `item_id` follow `follow.region`:
    /// write `follow` and the keyframes `followTransform` computed from it
    /// (`transform`: any of `x`, `y`, `scale`; none for a follow declared
    /// with `target` and `pick`, which the player resolves) on its first enabled
    /// Transform, keeping that Transform's other parameters, or on a new
    /// Transform named "Follow" when it has none. `false`, changing
    /// nothing, when `follow` or a `transform` value is invalid, `transform`
    /// has another key, the item is not on a video (or other) track, or no
    /// item has this id.
    pub fn set_item_follow(
        &mut self,
        item_id: &str,
        follow: &Follow,
        transform: Map<String, Value>,
    ) -> bool {
        if !follow.is_valid()
            || transform
                .keys()
                .any(|key| !FOLLOW_TRANSFORM_PARAMETERS.contains(&key.as_str()))
        {
            return false;
        }
        let Some(effects) = self.get_item_tellers_effects(item_id) else {
            return false;
        };
        let index = first_transform(&effects);
        let mut effect = match index {
            Some(i) => effects[i].clone(),
            None => TellersEffect::new(TRANSFORM).with_name("Follow"),
        };
        effect.parameters.extend(transform);
        effect
            .parameters
            .insert(FOLLOW_PARAMETER.to_string(), follow.to_value());
        if effect.validate().is_err() {
            return false;
        }
        match index {
            Some(i) => self.replace_item_tellers_effect_at(item_id, i, effect).is_some(),
            None => self.push_item_tellers_effect(item_id, effect),
        }
    }

    /// Every clip or composite clip with a follow, at any depth, in track
    /// order, with its follow.
    pub fn item_follows(&self) -> Vec<(String, Follow)> {
        let mut ids = Vec::new();
        collect_item_ids(&self.tracks.children, &mut ids);
        ids.into_iter()
            .filter_map(|id| Some((id.clone(), self.get_item_follow(&id)?)))
            .collect()
    }

    /// The follows the player still has to resolve, as it decides on load:
    /// on a clip (the player resolves no composite clip's), with no
    /// `region` yet (declared with `target` and `pick`) or with a `region`
    /// but not all of `x`, `y` and `scale` baked beside it.
    pub fn unresolved_follows(&self) -> Vec<(String, Follow)> {
        self.item_follows()
            .into_iter()
            .filter(|(id, follow)| {
                matches!(self.tracks.get_nested_item(id), Some(Item::Clip(_)))
                    && (!follow.is_resolved() || !self.item_follow_is_baked(id))
            })
            .collect()
    }

    /// Whether the follow Transform of `item_id` has all of `x`, `y` and
    /// `scale`.
    fn item_follow_is_baked(&self, item_id: &str) -> bool {
        let Some(effects) = self.get_item_tellers_effects(item_id) else {
            return false;
        };
        first_transform(&effects).is_some_and(|i| {
            FOLLOW_TRANSFORM_PARAMETERS
                .iter()
                .all(|key| effects[i].parameters.contains_key(*key))
        })
    }

    /// Save what the player resolved for `item_id` (its `onFollowResolved`
    /// `parameters`: the Transform's `x`, `y`, `scale` and `follow`, now
    /// with its `region`): [`Self::set_item_follow`] with them. Other keys
    /// are the player's view of the Transform and are ignored, so the
    /// item's own values stay.
    ///
    /// Detection takes a while, so the item's follow may have changed since
    /// the player read it: the resolution is saved only while every key the
    /// item's current follow sets (`target`, `pick`, a `region`, any
    /// setting) still has the same value in the resolved one. The player
    /// fills in defaults and the region, which the current follow may
    /// leave out.
    ///
    /// `false`, changing nothing, when `follow` is missing, invalid or
    /// still unresolved, the item has no follow or a different one now, or
    /// `set_item_follow` refuses.
    pub fn apply_follow_resolution(&mut self, item_id: &str, parameters: &Map<String, Value>) -> bool {
        let Some(follow) = parameters.get(FOLLOW_PARAMETER).and_then(Follow::from_value) else {
            return false;
        };
        let Some(current) = self.get_item_follow(item_id) else {
            return false;
        };
        if !follow.is_resolved() || !follow.extends(&current) {
            return false;
        }
        let transform = FOLLOW_TRANSFORM_PARAMETERS
            .iter()
            .filter_map(|key| Some((key.to_string(), parameters.get(*key)?.clone())))
            .collect();
        self.set_item_follow(item_id, &follow, transform)
    }

    /// Drop the follow of the clip or composite clip `item_id`; its baked
    /// keyframes stay, as a plain Transform. `true` when it had one.
    pub fn clear_item_follow(&mut self, item_id: &str) -> bool {
        let Some(effects) = self.get_item_tellers_effects(item_id) else {
            return false;
        };
        let Some(index) = first_transform(&effects) else {
            return false;
        };
        effects[index].parameters.contains_key(FOLLOW_PARAMETER)
            && self.set_item_tellers_effect_parameter(item_id, index, FOLLOW_PARAMETER, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn timeline() -> Timeline {
        let clip = |id: &str| json!({
            "OTIO_SCHEMA": "Clip.2",
            "metadata": { "tellers.ai": { "timeline_id": id } },
            "source_range": {
                "OTIO_SCHEMA": "TimeRange.1",
                "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 0, "rate": 1 },
                "duration": { "OTIO_SCHEMA": "RationalTime.1", "value": 8, "rate": 1 }
            }
        });
        serde_json::from_value(json!({
            "OTIO_SCHEMA": "Timeline.1",
            "tracks": {
                "OTIO_SCHEMA": "Stack.1",
                "children": [
                    { "OTIO_SCHEMA": "Track.1", "kind": "Video",
                      "metadata": { "tellers.ai": { "timeline_id": "v" } }, "children": [clip("vc")] },
                    { "OTIO_SCHEMA": "Track.1", "kind": "Audio",
                      "metadata": { "tellers.ai": { "timeline_id": "a" } }, "children": [clip("ac")] }
                ]
            }
        }))
        .unwrap()
    }

    fn walker() -> Follow {
        let region = json!({
            "shape": "rectangle", "width": 0.1, "height": 0.4,
            "x": { "keyframes": [ { "t": 0, "v": 0.4 }, { "t": 7, "v": -0.4 } ] }, "y": 0.1
        });
        let mut follow = Follow::new(region.as_object().unwrap().clone());
        follow.target = Some("people".into());
        follow.aspect = Some((16.0 / 9.0) / (9.0 / 16.0));
        follow.pan_smoothing = Some(0.8);
        follow
    }

    fn baked() -> Map<String, Value> {
        json!({ "x": { "keyframes": [ { "t": 0, "v": -1.5 }, { "t": 7, "v": 1.5 } ] }, "y": 0.15, "scale": 4.1 })
            .as_object()
            .unwrap()
            .clone()
    }

    #[test]
    fn follow_uses_the_player_names() {
        let value = walker().to_value();
        assert_eq!(value["target"], "people");
        assert_eq!(value["panSmoothing"], 0.8);
        assert!(value.get("fill").is_none());
        assert_eq!(Follow::from_value(&value), Some(walker()));
        assert!(Follow::from_value(&json!({ "fill": 0.6 })).is_none(), "region is required");
        assert!(Follow::from_value(&json!({ "region": {}, "speed": 2 })).is_none());
        assert!(Follow::from_value(&json!({ "region": {}, "target": "cars" })).is_none());
        for target in ["faces", "vehicles", "dog", "sports ball", "toothbrush"] {
            assert!(Follow::from_value(&json!({ "region": {}, "target": target })).is_some(), "{target}");
        }
        assert!(Follow::from_value(&json!({ "region": { "shape": "star" } })).is_none());
        assert!(Follow::from_value(&json!({ "region": {}, "fill": { "keyframes": [] } })).is_none());
        assert!(TellersEffect::new("Transform")
            .with_parameter("follow", value)
            .validate()
            .is_ok());
        assert!(TellersEffect::new("Transform")
            .with_parameter("follow", json!({ "fill": 0.6 }))
            .validate()
            .is_err());
    }

    #[test]
    fn set_get_and_clear_an_item_follow() {
        let mut timeline = timeline();
        assert_eq!(timeline.get_item_follow("vc"), None);
        assert!(timeline.set_item_follow("vc", &walker(), baked()));
        assert_eq!(timeline.get_item_follow("vc"), Some(walker()));
        let transform = &timeline.get_item_tellers_effects("vc").unwrap()[0];
        assert_eq!(transform.name.as_deref(), Some("Follow"));
        assert_eq!(transform.parameters["scale"], 4.1);

        // Re-tuning updates the same Transform and keeps its other parameters.
        assert!(timeline.set_item_tellers_effect_parameter("vc", 0, "opacity", Some(json!(0.5))));
        let mut tuned = walker();
        tuned.fill = Some(0.4);
        assert!(timeline.set_item_follow("vc", &tuned, json!({ "scale": 3.0 }).as_object().unwrap().clone()));
        let effects = timeline.get_item_tellers_effects("vc").unwrap();
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].parameters["opacity"], 0.5);
        assert_eq!(effects[0].parameters["scale"], 3.0);
        assert_eq!(timeline.get_item_follow("vc").unwrap().fill, Some(0.4));

        assert!(timeline.clear_item_follow("vc"));
        assert!(!timeline.clear_item_follow("vc"));
        assert_eq!(timeline.get_item_follow("vc"), None);
        assert_eq!(timeline.get_item_tellers_effects("vc").unwrap()[0].parameters["scale"], 3.0);
    }

    #[test]
    fn set_item_follow_refuses_what_the_player_would_not_take() {
        let mut timeline = timeline();
        assert!(!timeline.set_item_follow("ac", &walker(), baked()), "no Transform on audio");
        assert!(!timeline.set_item_follow("nope", &walker(), baked()));
        let rotation = json!({ "rotation": 4 }).as_object().unwrap().clone();
        assert!(!timeline.set_item_follow("vc", &walker(), rotation));
        let mut bad = walker();
        bad.target = Some("cars".into());
        assert!(!timeline.set_item_follow("vc", &bad, baked()));
        assert!(timeline.get_item_tellers_effects("vc").unwrap().is_empty());
    }

    #[test]
    fn a_follow_needs_a_region_or_a_target_and_pick() {
        let pick = FollowPick { t: 2.4, x: 0.18, y: -0.05 };
        let declared = Follow::declared("people", pick);
        assert!(!declared.is_resolved());
        let value = declared.to_value();
        assert_eq!(value, json!({ "target": "people", "pick": { "t": 2.4, "x": 0.18, "y": -0.05 } }));
        assert_eq!(Follow::from_value(&value), Some(declared));
        assert!(Follow::from_value(&json!({ "target": "people" })).is_none());
        assert!(Follow::from_value(&json!({ "pick": { "t": 0, "x": 0, "y": 0 } })).is_none());
        assert!(Follow::from_value(&json!({ "target": "people", "pick": { "t": 0, "x": 0 } })).is_none());
        assert!(Follow::from_value(&json!({ "target": "people", "pick": { "t": 0, "x": 0, "y": 0, "z": 1 } })).is_none());
        assert!(Follow::from_value(&json!({ "target": "people", "pick": { "t": "now", "x": 0, "y": 0 } })).is_none());
        let mut both = walker();
        both.pick = Some(pick);
        assert!(both.is_resolved());
        assert_eq!(Follow::from_value(&both.to_value()), Some(both));
    }

    #[test]
    fn declare_list_and_save_a_resolved_follow() {
        let mut timeline = timeline();
        let pick = FollowPick { t: 2.4, x: 0.18, y: -0.05 };
        // Declared in the OTIO: no region, no keyframes.
        assert!(timeline.set_item_follow("vc", &Follow::declared("people", pick), Map::new()));
        let transform = &timeline.get_item_tellers_effects("vc").unwrap()[0];
        assert!(transform.parameters.get("x").is_none());
        assert_eq!(timeline.unresolved_follows().len(), 1);
        assert_eq!(timeline.item_follows()[0].0, "vc");

        // What onFollowResolved hands back, with the player's view of other parameters.
        let mut resolved = walker();
        resolved.pick = Some(pick);
        let mut parameters = baked();
        parameters.insert("opacity".into(), json!(0.2));
        parameters.insert("follow".into(), resolved.to_value());
        assert!(timeline.set_item_tellers_effect_parameter("vc", 0, "opacity", Some(json!(0.9))));
        assert!(timeline.apply_follow_resolution("vc", &parameters));
        assert!(timeline.unresolved_follows().is_empty());
        assert_eq!(timeline.get_item_follow("vc"), Some(resolved));
        let params = &timeline.get_item_tellers_effects("vc").unwrap()[0].parameters;
        assert_eq!(params["scale"], 4.1);
        assert_eq!(params["opacity"], 0.9, "the item's own opacity stays");

        // A follow already applied: the same resolution again is fine, a stale one is not.
        assert!(timeline.apply_follow_resolution("vc", &parameters));
        // An unresolved or missing follow is not a resolution.
        let mut unresolved = baked();
        unresolved.insert("follow".into(), Follow::declared("people", pick).to_value());
        assert!(!timeline.apply_follow_resolution("vc", &unresolved));
        assert!(!timeline.apply_follow_resolution("vc", &baked()));
    }

    #[test]
    fn unresolved_matches_what_the_player_resolves() {
        let mut timeline = timeline();
        // A region without baked keyframes is still to resolve.
        assert!(timeline.set_item_follow("vc", &walker(), Map::new()));
        assert_eq!(timeline.unresolved_follows().len(), 1);
        let partial = json!({ "x": 0.1, "scale": 2.0 }).as_object().unwrap().clone();
        assert!(timeline.set_item_follow("vc", &walker(), partial));
        assert_eq!(timeline.unresolved_follows().len(), 1, "y is missing");
        assert!(timeline.set_item_follow("vc", &walker(), baked()));
        assert!(timeline.unresolved_follows().is_empty());
        assert_eq!(timeline.item_follows().len(), 1);
    }

    #[test]
    fn a_stale_resolution_is_not_saved() {
        let mut timeline = timeline();
        let pick = FollowPick { t: 2.4, x: 0.18, y: -0.05 };
        let declared = Follow::declared("people", pick);
        assert!(timeline.set_item_follow("vc", &declared, Map::new()));

        // The player resolves it, filling in defaults and the region.
        let mut resolved = walker();
        resolved.pick = Some(pick);
        resolved.fill = Some(0.6);
        let mut parameters = baked();
        parameters.insert("follow".into(), resolved.to_value());

        // Meanwhile the follow was re-picked: the old result is dropped.
        let moved = Follow::declared("people", FollowPick { t: 2.4, x: -0.3, y: 0.0 });
        assert!(timeline.set_item_follow("vc", &moved, Map::new()));
        assert!(!timeline.apply_follow_resolution("vc", &parameters));
        assert_eq!(timeline.get_item_follow("vc"), Some(moved));

        // A different setting set by the host is a change too.
        let mut tuned = declared.clone();
        tuned.fill = Some(0.4);
        assert!(timeline.set_item_follow("vc", &tuned, Map::new()));
        assert!(!timeline.apply_follow_resolution("vc", &parameters));

        // Removed meanwhile: nothing to save onto.
        assert!(timeline.set_item_follow("vc", &declared, Map::new()));
        assert!(timeline.clear_item_follow("vc"));
        assert!(!timeline.apply_follow_resolution("vc", &parameters));

        // Unchanged: saved.
        assert!(timeline.set_item_follow("vc", &declared, Map::new()));
        assert!(timeline.apply_follow_resolution("vc", &parameters));
        assert_eq!(timeline.get_item_follow("vc"), Some(resolved));
    }

    #[test]
    fn follows_inside_composite_clips_are_listed() {
        let mut timeline = timeline();
        let clip = timeline.tracks.children[0].items[0].clone();
        let inner = Track { items: vec![clip], ..Track::default() };
        timeline.tracks.children[0].items[0] =
            Item::Composite(crate::CompositeStack::new(vec![inner], None, Some("comp".into())));
        assert!(timeline.set_item_follow("vc", &walker(), baked()));
        assert_eq!(timeline.item_follows().iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["vc"]);
        // The player resolves clips, not composite clips.
        let declared = Follow::declared("people", FollowPick { t: 0.0, x: 0.0, y: 0.0 });
        assert!(timeline.set_item_follow("comp", &declared, Map::new()));
        assert_eq!(timeline.item_follows().len(), 2);
        assert!(timeline.unresolved_follows().is_empty());
    }

    #[test]
    fn follow_is_read_from_the_first_enabled_transform() {
        let mut timeline = timeline();
        let off = TellersEffect::new("Transform")
            .with_enabled(false)
            .with_parameter("follow", walker().to_value());
        assert!(timeline.push_item_tellers_effect("vc", off));
        assert_eq!(timeline.get_item_follow("vc"), None);
        assert!(timeline.set_item_follow("vc", &walker(), baked()));
        let effects = timeline.get_item_tellers_effects("vc").unwrap();
        assert_eq!(effects.len(), 2);
        assert_eq!(effects[1].name.as_deref(), Some("Follow"));
        assert!(timeline.get_item_follow("vc").is_some());
    }
}
