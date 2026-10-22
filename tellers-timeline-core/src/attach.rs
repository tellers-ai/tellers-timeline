//! A clip kept on a subject of another track: a `Transform`'s `attach`.
//!
//! A label or a sticker can ride on a person seen in the clip of another
//! video track playing alongside (video-player-js #690):
//!
//! ```json
//! { "effect_name": "tellers.ai.Transform", "metadata": { "tellers.ai": { "parameters": {
//!     "attach": { "track": "Base", "target": "people",
//!                 "pick": { "t": 1.2, "x": 0.2, "y": 0 }, "anchorY": -0.7, "hide": true }
//! } } } }
//! ```
//!
//! On load the player detects `target` in the clip of the video track
//! `track` (its name, id or `tellers.ai.timeline_id`) playing longest
//! alongside this one, takes the subject at `pick` (in this clip's seconds
//! and frame coordinates; without it the subject seen longest), and writes
//! `x` and `y` keyframes (and `opacity` with `hide`) on the Transform, and
//! the subject's `region` (and, with `hide`, the clip's own `opacity`) in
//! `attach`. It reports them through `onFollowResolved`, like a follow,
//! for the host to save with [`Timeline::apply_attach_resolution`].
//!
//! Playback reads only `x`, `y` and `opacity`; dropping them beside a
//! `region` recomputes them without detection. An attach needs `track`,
//! and a `region` or a `target`. It is meant for a clip centered in the
//! frame (text, full-frame images): Placement, rotation and groups of
//! either clip are not taken into account.
//!
//! An item's attach is the `attach` of its first enabled `Transform`, the
//! one the player applies. Mirrors the Transform's `attach` of
//! `schema/timeline.schema.json` in `video-player-js`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::effects::mask_shape_is_valid;
use crate::follow::{collect_item_ids, first_transform, is_follow_target, FollowDetect, FollowPick};
use crate::{Animatable, Item, TellersEffect, Timeline};

const TRANSFORM: &str = "Transform";
/// The `Transform` parameter holding the attach.
pub const ATTACH_PARAMETER: &str = "attach";
/// The `Transform` parameters an attach computes (`opacity` only with
/// `hide`).
pub const ATTACH_TRANSFORM_PARAMETERS: [&str; 3] = ["x", "y", "opacity"];

/// A clip kept on a subject detected in the clip of another track (left
/// out: the player's defaults, in brackets).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Attach {
    /// The video track (name, id or `tellers.ai.timeline_id`) whose clip
    /// playing longest alongside this one the subject is detected in.
    pub track: String,
    /// What to detect; see [`is_follow_target`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// The subject at this point, in this clip's seconds and frame
    /// coordinates; without it, the subject seen longest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pick: Option<FollowPick>,
    /// How the subject's region looks; the player's defaults when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detect: Option<FollowDetect>,
    /// Where this clip's center sits on the subject, as an offset from the
    /// subject's center in its widths (0) and heights (0; -0.5 its top).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_y: Option<f64>,
    /// Smoothing of the subject's motion, seconds (0.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smoothing: Option<f64>,
    /// The clip disappears while the subject is unseen, instead of staying
    /// where it was last seen (false).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hide: Option<bool>,
    /// With `hide`: the clip's own opacity, an [`Animatable`] multiplied
    /// into the resolved one (written on resolution).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<Value>,
    /// The subject's region, a `Mask` shape in frame coordinates and this
    /// clip's seconds; `None` until the player resolves the attach.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<Map<String, Value>>,
}

impl Attach {
    /// An attach to the `target` subject of the clip on `track`, for the
    /// player to detect and resolve; see the [module docs](self).
    pub fn declared(track: impl Into<String>, target: impl Into<String>) -> Self {
        Attach {
            track: track.into(),
            target: Some(target.into()),
            pick: None,
            detect: None,
            anchor_x: None,
            anchor_y: None,
            smoothing: None,
            hide: None,
            opacity: None,
            region: None,
        }
    }

    /// The `attach` parameter value, if `value` is a valid one.
    pub fn from_value(value: &Value) -> Option<Attach> {
        let attach: Attach = serde_json::from_value(value.clone()).ok()?;
        attach.is_valid().then_some(attach)
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("an attach serializes")
    }

    /// Whether the subject's region is known.
    pub fn is_resolved(&self) -> bool {
        self.region.is_some()
    }

    /// Whether every key `other` sets has the same value here: this attach
    /// is `other` with keys added (a resolved region, the clip's opacity).
    pub fn extends(&self, other: &Attach) -> bool {
        let (Value::Object(this), Value::Object(other)) = (self.to_value(), other.to_value()) else {
            return false;
        };
        other.iter().all(|(key, value)| this.get(key) == Some(value))
    }

    /// A non-empty `track`; a `region` (`Mask`-shaped) or a known `target`;
    /// an animatable `opacity`.
    fn is_valid(&self) -> bool {
        let region_ok = self
            .region
            .as_ref()
            .is_none_or(|region| mask_shape_is_valid(&Value::Object(region.clone())));
        !self.track.is_empty()
            && region_ok
            && (self.region.is_some() || self.target.is_some())
            && self.target.as_deref().is_none_or(is_follow_target)
            && self.opacity.as_ref().is_none_or(|o| Animatable::from_value(o).is_some())
    }
}

/// Whether `value` is a valid `attach` parameter.
pub(crate) fn attach_value_is_valid(value: &Value) -> bool {
    Attach::from_value(value).is_some()
}

impl Timeline {
    /// The attach of the clip or composite clip `item_id`: the `attach` of
    /// its first enabled Transform. `None` when it has none (or an invalid
    /// one), or no item has this id.
    pub fn get_item_attach(&self, item_id: &str) -> Option<Attach> {
        let effects = self.get_item_tellers_effects(item_id)?;
        let transform = &effects[first_transform(&effects)?];
        Attach::from_value(transform.parameters.get(ATTACH_PARAMETER)?)
    }

    /// Keep the clip or composite clip `item_id` on a subject of another
    /// track: write `attach` and what the player computed from it
    /// (`transform`: any of `x`, `y`, `opacity`; none for an attach the
    /// player is to resolve) on its first enabled Transform, keeping that
    /// Transform's other parameters, or on a new Transform named "Attach"
    /// when it has none. `false`, changing nothing, when `attach` or a
    /// `transform` value is invalid, `transform` has another key, the item
    /// is not on a video (or other) track, or no item has this id.
    pub fn set_item_attach(
        &mut self,
        item_id: &str,
        attach: &Attach,
        transform: Map<String, Value>,
    ) -> bool {
        if !attach.is_valid()
            || transform
                .keys()
                .any(|key| !ATTACH_TRANSFORM_PARAMETERS.contains(&key.as_str()))
        {
            return false;
        }
        let Some(effects) = self.get_item_tellers_effects(item_id) else {
            return false;
        };
        let index = first_transform(&effects);
        let mut effect = match index {
            Some(i) => effects[i].clone(),
            None => TellersEffect::new(TRANSFORM).with_name("Attach"),
        };
        effect.parameters.extend(transform);
        effect
            .parameters
            .insert(ATTACH_PARAMETER.to_string(), attach.to_value());
        if effect.validate().is_err() {
            return false;
        }
        match index {
            Some(i) => self.replace_item_tellers_effect_at(item_id, i, effect).is_some(),
            None => self.push_item_tellers_effect(item_id, effect),
        }
    }

    /// Every clip or composite clip with an attach, at any depth, in track
    /// order, with its attach.
    pub fn item_attaches(&self) -> Vec<(String, Attach)> {
        let mut ids = Vec::new();
        collect_item_ids(&self.tracks.children, &mut ids);
        ids.into_iter()
            .filter_map(|id| Some((id.clone(), self.get_item_attach(&id)?)))
            .collect()
    }

    /// The attaches the player still has to resolve, as it decides on load:
    /// on a clip (not a composite clip), with no `region` yet, or with a
    /// `region` but not both `x` and `y` beside it.
    pub fn unresolved_attaches(&self) -> Vec<(String, Attach)> {
        self.item_attaches()
            .into_iter()
            .filter(|(id, attach)| {
                matches!(self.tracks.get_nested_item(id), Some(Item::Clip(_)))
                    && (!attach.is_resolved() || !self.item_attach_is_baked(id))
            })
            .collect()
    }

    /// Whether the attach Transform of `item_id` has both `x` and `y`.
    fn item_attach_is_baked(&self, item_id: &str) -> bool {
        let Some(effects) = self.get_item_tellers_effects(item_id) else {
            return false;
        };
        first_transform(&effects)
            .is_some_and(|i| ["x", "y"].iter().all(|key| effects[i].parameters.contains_key(*key)))
    }

    /// Save what the player resolved for `item_id` (its `onFollowResolved`
    /// `parameters`: the Transform's `x`, `y`, with `hide` its `opacity`,
    /// and `attach`, now with its `region`): [`Self::set_item_attach`] with
    /// them. Other keys are the player's view of the Transform and are
    /// ignored, so the item's own values stay.
    ///
    /// The item's attach may have changed during detection: the resolution
    /// is saved only while every key the item's current attach sets still
    /// has the same value in the resolved one.
    ///
    /// `false`, changing nothing, when `attach` is missing, invalid or
    /// still unresolved, the item has no attach or a different one now, or
    /// `set_item_attach` refuses.
    pub fn apply_attach_resolution(&mut self, item_id: &str, parameters: &Map<String, Value>) -> bool {
        let Some(attach) = parameters.get(ATTACH_PARAMETER).and_then(Attach::from_value) else {
            return false;
        };
        let Some(current) = self.get_item_attach(item_id) else {
            return false;
        };
        if !attach.is_resolved() || !attach.extends(&current) {
            return false;
        }
        let hide = attach.hide == Some(true);
        let transform = ATTACH_TRANSFORM_PARAMETERS
            .iter()
            .filter(|key| hide || **key != "opacity")
            .filter_map(|key| Some((key.to_string(), parameters.get(*key)?.clone())))
            .collect();
        self.set_item_attach(item_id, &attach, transform)
    }

    /// Drop the attach of the clip or composite clip `item_id`; its baked
    /// keyframes stay, as a plain Transform. `true` when it had one.
    pub fn clear_item_attach(&mut self, item_id: &str) -> bool {
        let Some(effects) = self.get_item_tellers_effects(item_id) else {
            return false;
        };
        let Some(index) = first_transform(&effects) else {
            return false;
        };
        effects[index].parameters.contains_key(ATTACH_PARAMETER)
            && self.set_item_tellers_effect_parameter(item_id, index, ATTACH_PARAMETER, None)
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
                    { "OTIO_SCHEMA": "Track.1", "kind": "Video", "name": "Base",
                      "metadata": { "tellers.ai": { "timeline_id": "v" } }, "children": [clip("vc")] },
                    { "OTIO_SCHEMA": "Track.1", "kind": "Video", "name": "Labels",
                      "metadata": { "tellers.ai": { "timeline_id": "l" } }, "children": [clip("label")] },
                    { "OTIO_SCHEMA": "Track.1", "kind": "Audio",
                      "metadata": { "tellers.ai": { "timeline_id": "a" } }, "children": [clip("ac")] }
                ]
            }
        }))
        .unwrap()
    }

    fn label() -> Attach {
        let mut attach = Attach::declared("Base", "people");
        attach.pick = Some(FollowPick { t: 1.2, x: 0.2, y: 0.0 });
        attach.anchor_y = Some(-0.7);
        attach.hide = Some(true);
        attach
    }

    fn region() -> Map<String, Value> {
        json!({ "shape": "rectangle", "width": 0.1, "height": 0.4,
                "x": { "keyframes": [ { "t": 0, "v": 0.3 }, { "t": 8, "v": -0.3 } ] }, "y": 0.1,
                "opacity": { "keyframes": [ { "t": 0, "v": 1 }, { "t": 6, "v": 0 } ] } })
            .as_object()
            .unwrap()
            .clone()
    }

    /// What `onFollowResolved` hands back for [`label`], with the player's
    /// view of other parameters.
    fn resolution(attach: &Attach) -> Map<String, Value> {
        let mut resolved = attach.clone();
        resolved.region = Some(region());
        resolved.opacity = Some(json!(1));
        json!({ "x": { "keyframes": [ { "t": 0, "v": 0.3 }, { "t": 8, "v": -0.3 } ] }, "y": -0.18,
                "opacity": { "keyframes": [ { "t": 0, "v": 1 }, { "t": 6, "v": 0 } ] },
                "scale": 9, "attach": resolved.to_value() })
            .as_object()
            .unwrap()
            .clone()
    }

    #[test]
    fn attach_uses_the_player_names() {
        let value = label().to_value();
        assert_eq!(value, json!({ "track": "Base", "target": "people",
                                  "pick": { "t": 1.2, "x": 0.2, "y": 0.0 }, "anchorY": -0.7, "hide": true }));
        assert_eq!(Attach::from_value(&value), Some(label()));
        assert!(Attach::from_value(&json!({ "target": "people" })).is_none(), "track is required");
        assert!(Attach::from_value(&json!({ "track": "", "target": "people" })).is_none());
        assert!(Attach::from_value(&json!({ "track": "Base" })).is_none(), "a region or a target");
        assert!(Attach::from_value(&json!({ "track": "Base", "region": {} })).is_some());
        assert!(Attach::from_value(&json!({ "track": "Base", "target": "unicorn" })).is_none());
        assert!(Attach::from_value(&json!({ "track": "Base", "target": "dog", "tolerance": 0.002 })).is_none());
        assert!(Attach::from_value(&json!({ "track": "Base", "target": "dog", "detect": { "fps": 8 } })).is_none());
        assert!(Attach::from_value(&json!({ "track": "Base", "target": "dog", "detect": { "padX": 1.2 } })).is_some());
        assert!(Attach::from_value(&json!({ "track": "Base", "target": "dog", "opacity": "half" })).is_none());
        assert!(Attach::from_value(&json!({ "track": "Base", "target": "dog",
                                           "opacity": { "keyframes": [ { "t": 0, "v": 0.5 } ] } })).is_some());
        assert!(TellersEffect::new("Transform").with_parameter("attach", value).validate().is_ok());
        assert!(TellersEffect::new("Transform")
            .with_parameter("attach", json!({ "track": "Base" }))
            .validate()
            .is_err());
        assert!(TellersEffect::new("Mask")
            .with_parameter("attach", label().to_value())
            .validate()
            .is_err());
    }

    #[test]
    fn declare_list_and_save_a_resolved_attach() {
        let mut timeline = timeline();
        assert!(timeline.set_item_attach("label", &label(), Map::new()));
        let transform = &timeline.get_item_tellers_effects("label").unwrap()[0];
        assert_eq!(transform.name.as_deref(), Some("Attach"));
        assert_eq!(timeline.get_item_attach("label"), Some(label()));
        assert_eq!(timeline.unresolved_attaches().len(), 1);
        assert_eq!(timeline.item_attaches()[0].0, "label");

        assert!(timeline.set_item_tellers_effect_parameter("label", 0, "scale", Some(json!(0.5))));
        let parameters = resolution(&label());
        assert!(timeline.apply_attach_resolution("label", &parameters));
        assert!(timeline.unresolved_attaches().is_empty());
        let params = &timeline.get_item_tellers_effects("label").unwrap()[0].parameters;
        assert_eq!(params["y"], -0.18);
        assert_eq!(params["opacity"], parameters["opacity"], "with hide, the resolved opacity");
        assert_eq!(params["scale"], 0.5, "the item's own scale stays");
        let saved = timeline.get_item_attach("label").unwrap();
        assert_eq!(saved.region, Some(region()));
        assert_eq!(saved.opacity, Some(json!(1)));

        // Dropping the baked keyframes beside the region resolves it again, without detection.
        assert!(timeline.set_item_tellers_effect_parameter("label", 0, "x", None));
        assert_eq!(timeline.unresolved_attaches().len(), 1);

        assert!(timeline.clear_item_attach("label"));
        assert!(!timeline.clear_item_attach("label"));
        assert_eq!(timeline.get_item_attach("label"), None);
        assert_eq!(timeline.get_item_tellers_effects("label").unwrap()[0].parameters["y"], -0.18);
    }

    #[test]
    fn without_hide_the_opacity_is_the_items_own() {
        let mut timeline = timeline();
        let mut attach = label();
        attach.hide = None;
        assert!(timeline.set_item_attach("label", &attach, Map::new()));
        assert!(timeline.set_item_tellers_effect_parameter("label", 0, "opacity", Some(json!(0.8))));
        let mut parameters = resolution(&attach);
        let mut resolved = Attach::from_value(&parameters["attach"]).unwrap();
        resolved.opacity = None;
        parameters.insert("attach".into(), resolved.to_value());
        assert!(timeline.apply_attach_resolution("label", &parameters));
        assert_eq!(timeline.get_item_tellers_effects("label").unwrap()[0].parameters["opacity"], 0.8);
    }

    #[test]
    fn a_stale_or_unresolved_attach_is_not_saved() {
        let mut timeline = timeline();
        assert!(timeline.set_item_attach("label", &label(), Map::new()));
        let parameters = resolution(&label());

        // Re-anchored meanwhile: the old result is dropped.
        let mut moved = label();
        moved.anchor_y = Some(0.6);
        assert!(timeline.set_item_attach("label", &moved, Map::new()));
        assert!(!timeline.apply_attach_resolution("label", &parameters));
        assert_eq!(timeline.get_item_attach("label"), Some(moved));

        // Another track is another subject.
        let mut elsewhere = label();
        elsewhere.track = "Labels".into();
        assert!(timeline.set_item_attach("label", &elsewhere, Map::new()));
        assert!(!timeline.apply_attach_resolution("label", &parameters));

        assert!(timeline.set_item_attach("label", &label(), Map::new()));
        let mut unresolved = parameters.clone();
        unresolved.insert("attach".into(), label().to_value());
        assert!(!timeline.apply_attach_resolution("label", &unresolved));
        assert!(!timeline.apply_attach_resolution("nope", &parameters));
        assert!(timeline.apply_attach_resolution("label", &parameters));
    }

    #[test]
    fn set_item_attach_refuses_what_the_player_would_not_take() {
        let mut timeline = timeline();
        let baked = json!({ "x": 0.1, "y": 0.2 }).as_object().unwrap().clone();
        assert!(!timeline.set_item_attach("ac", &label(), baked.clone()), "no Transform on audio");
        assert!(!timeline.set_item_attach("nope", &label(), baked.clone()));
        let scale = json!({ "scale": 2 }).as_object().unwrap().clone();
        assert!(!timeline.set_item_attach("label", &label(), scale));
        let mut bad = label();
        bad.target = Some("cars".into());
        assert!(!timeline.set_item_attach("label", &bad, baked));
        assert!(timeline.get_item_tellers_effects("label").unwrap().is_empty());
    }

    #[test]
    fn follow_and_attach_share_the_first_transform() {
        let mut timeline = timeline();
        let follow = crate::Follow::declared("people", FollowPick { t: 0.0, x: 0.0, y: 0.0 });
        assert!(timeline.set_item_follow("label", &follow, Map::new()));
        assert!(timeline.set_item_attach("label", &label(), Map::new()));
        let effects = timeline.get_item_tellers_effects("label").unwrap();
        assert_eq!(effects.len(), 1);
        assert_eq!(timeline.get_item_follow("label"), Some(follow));
        assert_eq!(timeline.get_item_attach("label"), Some(label()));
    }
}
