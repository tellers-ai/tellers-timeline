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
//! An item's follow is the `follow` of its first enabled `Transform`, the
//! one the player applies.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::effects::mask_shape_is_valid;
use crate::{TellersEffect, Timeline};

const TRANSFORM: &str = "Transform";
/// The `Transform` parameter holding the recipe.
pub const FOLLOW_PARAMETER: &str = "follow";
/// The `Transform` parameters a follow computes.
pub const FOLLOW_TRANSFORM_PARAMETERS: [&str; 3] = ["x", "y", "scale"];
/// `detectRegions` targets a followed region can come from.
pub const FOLLOW_TARGETS: [&str; 4] = ["faces", "people", "animals", "vehicles"];

/// The recipe of an auto-tracking zoom: the region followed and the
/// `followTransform` options (left out: the player's defaults, in brackets).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Follow {
    /// The detected region followed, a `Mask` shape in clip space (a
    /// `detectRegions` region).
    pub region: Map<String, Value>,
    /// The `detectRegions` target the region came from; see [`FOLLOW_TARGETS`].
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
        Follow {
            region,
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

    /// A `Mask`-shaped region and a known target.
    fn is_valid(&self) -> bool {
        mask_shape_is_valid(&Value::Object(self.region.clone()))
            && self
                .target
                .as_deref()
                .is_none_or(|target| FOLLOW_TARGETS.contains(&target))
    }
}

/// Whether `value` is a valid `follow` parameter.
pub(crate) fn follow_value_is_valid(value: &Value) -> bool {
    Follow::from_value(value).is_some()
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
    /// (`transform`: any of `x`, `y`, `scale`) on its first enabled
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
