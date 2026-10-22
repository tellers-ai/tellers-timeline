//! Effects naming another track: a `TrackMatte`'s `track`, a Transform's
//! `attach.track` and a filter's or Mask's `detect.track`.
//!
//! The player reads such a reference as the video track whose `name` or
//! `tellers.ai.timeline_id` it is, other than the effect's own track (the
//! track it is on, or its item is on). One that matches nothing does
//! nothing: no matte, no attach, no detection.
//!
//! [`Stack::track_references`] lists them on the top-level tracks and their
//! items (composite clips' own effects included, not their content), and
//! [`crate::validate_timeline`] reports those matching no video track.
//! [`Stack::rename_track`] follows a rename, and [`Stack::delete_track`]
//! unlinks what named the deleted track; see [`Stack::unlink_track_references`].

use serde_json::{Map, Value};

use crate::{Effect, IdMetadataExt, Item, Stack, TellersEffect, TrackKind};

/// The parameter a `TrackMatte` names its track in, and an attach's or a
/// detect's key for it.
const TRACK_KEY: &str = "track";

/// Which parameter of an effect names a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackReferenceKind {
    /// A `TrackMatte`'s `track`: the layer it shows through.
    TrackMatte,
    /// A Transform's `attach.track`; see [`crate::Attach`].
    Attach,
    /// A filter's or Mask's `detect.track`; see [`crate::EffectDetect`].
    Detect,
}

/// An effect naming another track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackReference {
    /// The top-level track the effect is on, or whose item it is on.
    pub track_index: usize,
    /// The item (on that track) the effect is on; `None` for the track's own effects.
    pub item_index: Option<usize>,
    /// The `tellers.ai.timeline_id` of the item, or of the track for its own effects.
    pub owner_id: Option<String>,
    /// The effect's index in its owner's `effects` (all of them).
    pub effect_index: usize,
    /// Bare, e.g. `TrackMatte`.
    pub effect_name: String,
    pub enabled: bool,
    pub kind: TrackReferenceKind,
    /// The track named: a name or a `tellers.ai.timeline_id`.
    pub track: String,
}

/// The parameters object of a tellers effect, if it has one.
fn parameters_mut(effect: &mut Effect) -> Option<&mut Map<String, Value>> {
    effect.metadata.other.get_mut("tellers.ai")?.get_mut("parameters")?.as_object_mut()
}

/// The object holding `kind`'s `track` key in `parameters`.
fn holder_mut(parameters: &mut Map<String, Value>, kind: TrackReferenceKind) -> Option<&mut Map<String, Value>> {
    match kind {
        TrackReferenceKind::TrackMatte => Some(parameters),
        TrackReferenceKind::Attach => parameters.get_mut(crate::ATTACH_PARAMETER)?.as_object_mut(),
        TrackReferenceKind::Detect => parameters.get_mut(crate::DETECT_PARAMETER)?.as_object_mut(),
    }
}

/// The track `effect` names, and in which parameter.
fn effect_track_reference(effect: &TellersEffect) -> Option<(TrackReferenceKind, String)> {
    let (kind, holder) = match effect.effect_name.as_str() {
        "TrackMatte" => (TrackReferenceKind::TrackMatte, &effect.parameters),
        "Transform" => (TrackReferenceKind::Attach, effect.parameters.get(crate::ATTACH_PARAMETER)?.as_object()?),
        _ => (TrackReferenceKind::Detect, effect.parameters.get(crate::DETECT_PARAMETER)?.as_object()?),
    };
    let track = holder.get(TRACK_KEY)?.as_str()?.trim();
    (!track.is_empty()).then(|| (kind, track.to_string()))
}

/// Whether a `detect` effect has been detected (`regions`, or a Mask's `x`).
fn detect_is_resolved(effect: &TellersEffect) -> bool {
    let key = if effect.effect_name == "Mask" { "x" } else { "regions" };
    effect.parameters.contains_key(key)
}

impl Stack {
    /// Every effect naming another track, on the top-level tracks and their
    /// items, in track order.
    pub fn track_references(&self) -> Vec<TrackReference> {
        let mut references = Vec::new();
        for (track_index, track) in self.children.iter().enumerate() {
            let mut push = |item_index: Option<usize>, owner_id: Option<String>, effects: &[Effect]| {
                for (effect_index, effect) in effects.iter().enumerate() {
                    let Some(effect) = TellersEffect::from_effect(effect) else {
                        continue;
                    };
                    if let Some((kind, name)) = effect_track_reference(&effect) {
                        references.push(TrackReference {
                            track_index,
                            item_index,
                            owner_id: owner_id.clone(),
                            effect_index,
                            effect_name: effect.effect_name,
                            enabled: effect.enabled,
                            kind,
                            track: name,
                        });
                    }
                }
            };
            push(None, track.get_id(), &track.effects);
            for (item_index, item) in track.items.iter().enumerate() {
                match item {
                    Item::Clip(clip) => push(Some(item_index), item.get_id(), &clip.effects),
                    Item::Composite(stack) => push(Some(item_index), item.get_id(), &stack.effects),
                    _ => {}
                }
            }
        }
        references
    }

    /// The indices of the video tracks `reference` names, as the player
    /// reads it: by `name` or `tellers.ai.timeline_id`, never the effect's
    /// own track.
    pub fn referenced_tracks(&self, reference: &TrackReference) -> Vec<usize> {
        self.children
            .iter()
            .enumerate()
            .filter(|(index, track)| {
                *index != reference.track_index
                    && track.kind == TrackKind::Video
                    && (track.name.as_deref() == Some(reference.track.as_str())
                        || track.get_id().as_deref() == Some(reference.track.as_str()))
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// The enabled effects naming a track no other video track matches:
    /// they do nothing in the player.
    pub fn dangling_track_references(&self) -> Vec<TrackReference> {
        self.track_references()
            .into_iter()
            .filter(|reference| reference.enabled && self.referenced_tracks(reference).is_empty())
            .collect()
    }

    /// The effects at `track_index` (`item_index`: an item's on it).
    fn effects_at_mut(&mut self, track_index: usize, item_index: Option<usize>) -> Option<&mut Vec<Effect>> {
        let track = self.children.get_mut(track_index)?;
        match item_index {
            None => Some(&mut track.effects),
            Some(index) => match track.items.get_mut(index)? {
                Item::Clip(clip) => Some(&mut clip.effects),
                Item::Composite(stack) => Some(&mut stack.effects),
                _ => None,
            },
        }
    }

    /// Name the track `id` `name` (`None` or blank: no name), and point the
    /// effects that named it by its old name, and only it, at it again: by
    /// `name`, or by its `tellers.ai.timeline_id` when it gets no name or
    /// one another video track already carries. `false` when no track has
    /// this id.
    pub fn rename_track(&mut self, id: &str, name: Option<String>) -> bool {
        let Some((index, track)) = self.get_track_by_id(id) else {
            return false;
        };
        let name = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
        let old = track.name.clone();
        let following: Vec<TrackReference> = match old.as_deref() {
            Some(old) if Some(old) != name.as_deref() => self
                .track_references()
                .into_iter()
                .filter(|r| r.track == old && self.referenced_tracks(r) == [index])
                .collect(),
            _ => Vec::new(),
        };
        let shared = |stack: &Stack, candidate: &str| {
            stack.children.iter().enumerate().any(|(i, t)| {
                i != index
                    && t.kind == TrackKind::Video
                    && (t.name.as_deref() == Some(candidate) || t.get_id().as_deref() == Some(candidate))
            })
        };
        let target = match &name {
            Some(name) if !shared(self, name) => name.clone(),
            _ => id.to_string(),
        };
        self.children[index].name = name;
        for reference in following {
            let Some(effect) = self
                .effects_at_mut(reference.track_index, reference.item_index)
                .and_then(|effects| effects.get_mut(reference.effect_index))
            else {
                continue;
            };
            if let Some(holder) = parameters_mut(effect).and_then(|p| holder_mut(p, reference.kind)) {
                holder.insert(TRACK_KEY.to_string(), Value::String(target.clone()));
            }
        }
        true
    }

    /// Before the track at `index` is removed: unlink the effects that name
    /// it and no other video track, keeping what they produced. A
    /// `TrackMatte` is removed (with no track it would read the track
    /// above); an attach is dropped, its baked `x`/`y` staying; a resolved
    /// `detect` is dropped, its `regions` or shape staying, and an
    /// unresolved one's effect is removed. Returns how many were unlinked.
    pub(crate) fn unlink_track_references(&mut self, index: usize) -> usize {
        let mut unlinked: Vec<TrackReference> = self
            .track_references()
            .into_iter()
            .filter(|r| r.track_index != index && self.referenced_tracks(r) == [index])
            .collect();
        // Later effects first, so removing one keeps the others' indices.
        unlinked.sort_by(|a, b| b.effect_index.cmp(&a.effect_index));
        for reference in &unlinked {
            let Some(effects) = self.effects_at_mut(reference.track_index, reference.item_index) else {
                continue;
            };
            let Some(tellers) = effects.get(reference.effect_index).and_then(TellersEffect::from_effect) else {
                continue;
            };
            let remove_effect = match reference.kind {
                TrackReferenceKind::TrackMatte => true,
                TrackReferenceKind::Attach => false,
                TrackReferenceKind::Detect => !detect_is_resolved(&tellers),
            };
            if remove_effect {
                effects.remove(reference.effect_index);
                continue;
            }
            let key = match reference.kind {
                TrackReferenceKind::Attach => crate::ATTACH_PARAMETER,
                _ => crate::DETECT_PARAMETER,
            };
            if let Some(parameters) = parameters_mut(&mut effects[reference.effect_index]) {
                parameters.remove(key);
            }
        }
        unlinked.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Timeline;
    use serde_json::json;

    fn effect(name: &str, parameters: Value) -> Value {
        json!({ "OTIO_SCHEMA": "Effect.1", "effect_name": format!("tellers.ai.{name}"),
                "metadata": { "tellers.ai": { "parameters": parameters } } })
    }

    fn clip(id: &str, effects: Value) -> Value {
        json!({
            "OTIO_SCHEMA": "Clip.2",
            "metadata": { "tellers.ai": { "timeline_id": id } },
            "source_range": {
                "OTIO_SCHEMA": "TimeRange.1",
                "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 0, "rate": 1 },
                "duration": { "OTIO_SCHEMA": "RationalTime.1", "value": 8, "rate": 1 }
            },
            "effects": effects
        })
    }

    fn track(id: &str, name: &str, kind: &str, children: Value, effects: Value) -> Value {
        json!({ "OTIO_SCHEMA": "Track.1", "kind": kind, "name": name,
                "metadata": { "tellers.ai": { "timeline_id": id } }, "children": children, "effects": effects })
    }

    /// A base track, an overlay naming it in three ways, and a music track.
    fn timeline() -> Timeline {
        serde_json::from_value(json!({
            "OTIO_SCHEMA": "Timeline.1",
            "tracks": { "OTIO_SCHEMA": "Stack.1", "children": [
                track("v1", "Base", "Video", json!([clip("vc", json!([]))]), json!([])),
                track("v2", "Overlay", "Video", json!([clip("label", json!([
                    effect("Transform", json!({ "x": 0.1, "y": 0.2, "attach": { "track": "Base", "target": "people" } })),
                    effect("Mask", json!({ "invert": true, "detect": { "target": "people", "track": "v1" } })),
                    effect("Blur", json!({ "detect": { "target": "faces", "track": "Base" },
                                           "regions": [{ "shape": "ellipse", "x": 0, "y": 0, "width": 0.2, "height": 0.2 }] })),
                ]))]), json!([effect("TrackMatte", json!({ "mode": "alpha", "track": "Base" }))])),
                track("a1", "Music", "Audio", json!([clip("ac", json!([]))]), json!([])),
            ] }
        }))
        .unwrap()
    }

    fn summary(stack: &Stack) -> Vec<(Option<String>, &'static str, String)> {
        stack
            .track_references()
            .into_iter()
            .map(|r| (r.owner_id, match r.kind {
                TrackReferenceKind::TrackMatte => "matte",
                TrackReferenceKind::Attach => "attach",
                TrackReferenceKind::Detect => "detect",
            }, r.track))
            .collect()
    }

    #[test]
    fn lists_the_references_and_what_they_name() {
        let timeline = timeline();
        let stack = &timeline.tracks;
        assert_eq!(summary(stack), vec![
            (Some("v2".into()), "matte", "Base".into()),
            (Some("label".into()), "attach", "Base".into()),
            (Some("label".into()), "detect", "v1".into()),
            (Some("label".into()), "detect", "Base".into()),
        ]);
        assert!(stack.track_references().iter().all(|r| stack.referenced_tracks(r) == [0]));
        assert!(stack.dangling_track_references().is_empty());
        assert!(crate::validate_timeline(&timeline).is_empty());
    }

    #[test]
    fn a_reference_must_name_another_video_track() {
        let mut timeline = timeline();
        let reference = |track: &str| {
            effect("TrackMatte", json!({ "mode": "luma", "track": track }))
        };
        // Its own track, an audio track, nothing, and a disabled effect (ignored).
        let mut disabled: Effect = serde_json::from_value(reference("nowhere")).unwrap();
        disabled.enabled = Some(false);
        let Item::Clip(c) = &mut timeline.tracks.children[0].items[0] else { unreachable!() };
        c.effects = vec![
            serde_json::from_value(reference("Base")).unwrap(),
            serde_json::from_value(reference("Music")).unwrap(),
            serde_json::from_value(reference("nowhere")).unwrap(),
            disabled,
        ];
        let dangling: Vec<_> = timeline.tracks.dangling_track_references().into_iter().map(|r| r.track).collect();
        assert_eq!(dangling, ["Base", "Music", "nowhere"]);
        let errors: Vec<_> = crate::validate_timeline(&timeline).iter().map(|e| e.to_string()).collect();
        assert_eq!(errors.len(), 3);
        assert!(errors[2].contains("TrackMatte") && errors[2].contains("vc") && errors[2].contains("nowhere"), "{}", errors[2]);
    }

    #[test]
    fn renaming_a_track_follows_its_references() {
        let mut timeline = timeline();
        assert!(timeline.rename_track("v1", Some("Interview".into())));
        assert_eq!(timeline.tracks.children[0].name.as_deref(), Some("Interview"));
        let names: Vec<_> = summary(&timeline.tracks).into_iter().map(|(_, _, t)| t).collect();
        assert_eq!(names, ["Interview", "Interview", "v1", "Interview"], "the id reference stays");
        assert!(timeline.tracks.dangling_track_references().is_empty());

        // A name another video track carries would be ambiguous: the timeline id is used.
        assert!(timeline.rename_track("v1", Some("Overlay".into())));
        let names: Vec<_> = summary(&timeline.tracks).into_iter().map(|(_, _, t)| t).collect();
        assert_eq!(names, ["v1", "v1", "v1", "v1"]);
        // Unnamed: references by id keep working.
        assert!(timeline.rename_track("v1", None));
        assert!(timeline.tracks.dangling_track_references().is_empty());
        assert!(!timeline.rename_track("nope", Some("x".into())));
    }

    #[test]
    fn a_shared_name_is_not_rewritten() {
        let mut timeline = timeline();
        timeline.tracks.children[2].kind = TrackKind::Video;
        timeline.tracks.children[2].name = Some("Base".into());
        assert!(timeline.rename_track("v1", Some("Interview".into())));
        let names: Vec<_> = summary(&timeline.tracks).into_iter().map(|(_, _, t)| t).collect();
        assert_eq!(names, ["Base", "Base", "v1", "Base"], "they named two tracks; the other still matches");
    }

    #[test]
    fn deleting_a_track_unlinks_what_named_it() {
        let mut timeline = timeline();
        assert!(timeline.delete_track("v1").is_some());
        assert!(timeline.tracks.track_references().is_empty());
        assert!(crate::validate_timeline(&timeline).is_empty());

        // The matte is removed rather than left to read the track above.
        let overlay = &timeline.tracks.children[0];
        assert!(overlay.effects.is_empty());
        let effects = timeline.get_item_tellers_effects("label").unwrap();
        let names: Vec<_> = effects.iter().map(|e| e.effect_name.as_str()).collect();
        assert_eq!(names, ["Transform", "Blur"], "the unresolved Mask is removed");
        // What was produced stays: the attach's baked x/y, the blur's regions.
        assert_eq!(effects[0].parameters, json!({ "x": 0.1, "y": 0.2 }).as_object().unwrap().clone());
        assert!(effects[1].parameters.get("detect").is_none());
        assert!(effects[1].parameters.get("regions").is_some());
    }

    #[test]
    fn deleting_a_track_keeps_references_another_track_satisfies() {
        let mut timeline = timeline();
        timeline.tracks.children[2].kind = TrackKind::Video;
        timeline.tracks.children[2].name = Some("Base".into());
        assert!(timeline.delete_track("v1").is_some());
        let left: Vec<_> = summary(&timeline.tracks).into_iter().map(|(_, k, t)| (k, t)).collect();
        assert_eq!(left, [("matte", "Base".to_string()), ("attach", "Base".into()), ("detect", "Base".into())]);
    }
}
