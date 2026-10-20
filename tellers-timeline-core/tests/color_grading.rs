// Tests for `.cube` LUTs: a LUT is an ordinary `tellers.ai.ColorLut` tellers
// effect whose `metadata["tellers.ai"].parameters` holds `asset_id` and the
// `target_url` the player (video-player-js) reads, resolved from the asset id
// (see `tests/effect_assets.rs`). Legacy forms (`ColorLut`, keys beside
// `parameters`, `cube_url`) are upgraded by `sanitize`.

use serde_json::json;
use tellers_timeline_core::{
    effect_spec, upgrade_legacy_color_lut_effects, Clip, Effect, EffectTarget, IdMetadataExt, Item, TellersEffect,
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

#[test]
fn sanitize_upgrades_legacy_lut_effects_on_every_stage() {
    let legacy = || {
        effects_from(json!([
            {"effect_name": "ColorLut", "name": "look.cube", "metadata": {"tellers.ai": {"asset_id": "a"}}},
            {"effect_name": "Blur", "metadata": {}}
        ]))
    };
    let mut tl = timeline_with_clip("c1");
    tl.tracks.effects = legacy();
    tl.tracks.children[0].effects = legacy();
    if let Item::Clip(clip) = &mut tl.tracks.children[0].items[0] {
        clip.effects = legacy();
    }

    tl.sanitize();

    let renamed = json!([
        {"OTIO_SCHEMA": "Effect.1", "name": "look.cube", "effect_name": "tellers.ai.ColorLut", "metadata": {"tellers.ai": {"parameters": {"asset_id": "a"}}}},
        {"OTIO_SCHEMA": "Effect.1", "name": "", "effect_name": "Blur", "metadata": {}}
    ]);
    assert_eq!(effects_json(&tl.tracks.effects), renamed);
    assert_eq!(effects_json(&tl.tracks.children[0].effects), renamed);
    let Item::Clip(clip) = &tl.tracks.children[0].items[0] else {
        panic!("expected a clip");
    };
    assert_eq!(effects_json(&clip.effects), renamed);
    assert_eq!(asset_ids(&tl.get_tellers_effects()), ["a"]);
    assert_eq!(tl.effect_asset_ids(), ["a"]);
}

#[test]
fn upgrade_moves_legacy_keys_into_parameters() {
    let mut effects = effects_from(json!([
        // Both keys beside `parameters`, under the current name.
        {"effect_name": "tellers.ai.ColorLut", "metadata": {"tellers.ai": {"asset_id": "a", "cube_url": "https://old/a.cube", "note": 1}}},
        // A key already in `parameters` wins over the legacy one.
        {"effect_name": "tellers.ai.ColorLut", "metadata": {"tellers.ai": {"asset_id": "stale", "parameters": {"asset_id": "b"}}}},
        // Already current: untouched.
        {"effect_name": "tellers.ai.ColorLut", "metadata": {"tellers.ai": {"parameters": {"asset_id": "c"}}}},
        // Not a LUT: untouched even with an asset_id.
        {"effect_name": "tellers.ai.Blur", "metadata": {"tellers.ai": {"asset_id": "x"}}}
    ]));
    assert_eq!(upgrade_legacy_color_lut_effects(&mut effects), 2);
    let ai: Vec<serde_json::Value> = effects
        .iter()
        .map(|effect| effect.metadata.other["tellers.ai"].clone())
        .collect();
    assert_eq!(
        ai,
        [
            json!({"note": 1, "parameters": {"asset_id": "a", "target_url": "https://old/a.cube"}}),
            json!({"parameters": {"asset_id": "b"}}),
            json!({"parameters": {"asset_id": "c"}}),
            json!({"asset_id": "x"})
        ]
    );
    assert_eq!(upgrade_legacy_color_lut_effects(&mut effects), 0);
}

#[test]
fn upgrade_renames_a_parameters_cube_url_target_url() {
    let mut effects = effects_from(json!([
        {"effect_name": "tellers.ai.ColorLut", "metadata": {"tellers.ai": {"parameters": {"asset_id": "a", "cube_url": "https://old/a.cube"}}}},
        {"effect_name": "tellers.ai.ColorLut", "metadata": {"tellers.ai": {"parameters": {"asset_id": "b", "cube_url": "https://old/b.cube", "target_url": "https://new/b.cube"}}}}
    ]));

    assert_eq!(upgrade_legacy_color_lut_effects(&mut effects), 2);
    let parameters: Vec<_> = effects
        .iter()
        .map(|e| e.metadata.other["tellers.ai"]["parameters"].clone())
        .collect();
    assert_eq!(
        parameters,
        vec![
            json!({"asset_id": "a", "target_url": "https://old/a.cube"}),
            json!({"asset_id": "b", "target_url": "https://new/b.cube"}),
        ]
    );
    assert_eq!(upgrade_legacy_color_lut_effects(&mut effects), 0);
}
