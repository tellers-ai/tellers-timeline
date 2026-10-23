// Tests for `.cube` LUTs: a LUT is an ordinary `tellers.ai.ColorLut` tellers
// effect whose `metadata["tellers.ai"].parameters` holds `asset_id` and the
// `target_url` the player (video-player-js) reads, resolved from the asset id
// (see `tests/effect_assets.rs`).

use serde_json::json;
use tellers_timeline_core::{
    effect_spec, Clip, Effect, EffectTarget, IdMetadataExt, Item, TellersEffect,
    TimeRange, Timeline, Track, TrackKind,
};

fn clip(id: &str) -> Clip {
    Clip::new(TimeRange::new(2.0, 0.0), Default::default(), None, None, Some(id.to_string()))
}

fn timeline_with_clip(id: &str) -> Timeline {
    let mut track = Track::default();
    track.kind = TrackKind::Video;
    track.items.push(Item::Clip(clip(id)));
    let mut tl = Timeline::default();
    tl.tracks.children.push(track);
    tl
}

fn effects_json(effects: &[Effect]) -> serde_json::Value {
    serde_json::to_value(effects).unwrap()
}

fn effects_from(value: serde_json::Value) -> Vec<Effect> {
    serde_json::from_value(value).unwrap()
}

fn lut(asset_id: &str) -> TellersEffect {
    TellersEffect::new("ColorLut").with_parameter("asset_id", asset_id)
}

fn asset_ids(effects: &[TellersEffect]) -> Vec<&str> {
    effects
        .iter()
        .filter(|e| e.effect_name == "ColorLut")
        .filter_map(|e| e.parameters.get("asset_id")?.as_str())
        .collect()
}

#[test]
fn color_lut_is_a_tellers_video_effect() {
    let spec = effect_spec("tellers.ai.ColorLut").unwrap();
    assert_eq!(spec.target, EffectTarget::Video);
    assert!(lut("a").with_parameter("target_url", "https://cdn/a.cube").validate().is_ok());
    assert!(lut("a").with_parameter("cube_url", "https://cdn/a.cube").validate().is_err());
    assert!(TellersEffect::new("ColorLut").with_parameter("asset_id", 3).validate().is_err());
}

#[test]
fn luts_are_written_and_ordered_like_any_effect() {
    let mut tl = timeline_with_clip("c1");
    assert!(tl.push_item_tellers_effect("c1", lut("log_to_709").with_name("log.cube")));
    assert!(tl.push_item_tellers_effect("c1", TellersEffect::new("Blur").with_parameter("strength", 2.0)));
    assert!(tl.insert_item_tellers_effect_at("c1", 1, lut("look")));

    let effects = tl.get_item_tellers_effects("c1").unwrap();
    let names: Vec<_> = effects.iter().map(|e| e.effect_name.as_str()).collect();
    assert_eq!(names, ["ColorLut", "ColorLut", "Blur"]);
    assert_eq!(asset_ids(&effects), ["log_to_709", "look"]);

    let Item::Clip(clip) = &tl.tracks.children[0].items[0] else {
        panic!("expected a clip");
    };
    assert_eq!(
        effects_json(&clip.effects[..1]),
        json!([{
            "OTIO_SCHEMA": "Effect.1",
            "name": "log.cube",
            "effect_name": "tellers.ai.ColorLut",
            "metadata": { "tellers.ai": { "parameters": { "asset_id": "log_to_709" } } }
        }])
    );

    // Track and timeline stages hold LUTs the same way.
    assert!(tl.push_tellers_effect(lut("grade")));
    let track_id = tl.tracks.children[0].get_id().unwrap();
    assert!(tl.push_track_tellers_effect(&track_id, lut("track_look")));
    assert_eq!(asset_ids(&tl.get_tellers_effects()), ["grade"]);
    assert_eq!(asset_ids(&tl.get_track_tellers_effects(&track_id).unwrap()), ["track_look"]);

    assert_eq!(tl.remove_item_tellers_effect_at("c1", 0).unwrap().parameters["asset_id"], "log_to_709");
}

#[test]
fn a_bare_color_lut_name_is_read() {
    let effects = effects_from(json!([
        {"effect_name": "ColorLut", "metadata": {"tellers.ai": {"parameters": {"asset_id": "old"}}}}
    ]));
    let read = TellersEffect::from_effect(&effects[0]).unwrap();
    assert_eq!(read.effect_name, "ColorLut");
    assert_eq!(read.parameters["asset_id"], "old");
}
