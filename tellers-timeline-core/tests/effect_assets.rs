// Tests for resolving the `target_url` of effects from their `asset_id`
// parameter: `Timeline::effect_asset_ids`, `set_effect_target_urls` and
// `clear_effect_target_urls` (also run by `clear_target_urls`). They cover
// every effect on every stage, whatever its `effect_name`.

use std::collections::HashMap;

use serde_json::json;
use tellers_timeline_core::{
    clear_effect_target_urls, effect_asset_id, set_effect_target_urls, Effect, Item, Timeline,
};

fn effect(name: &str, parameters: serde_json::Value) -> serde_json::Value {
    json!({
        "OTIO_SCHEMA": "Effect.1",
        "effect_name": name,
        "metadata": { "tellers.ai": { "parameters": parameters } }
    })
}

fn range() -> serde_json::Value {
    json!({
        "OTIO_SCHEMA": "TimeRange.1",
        "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 0, "rate": 1 },
        "duration": { "OTIO_SCHEMA": "RationalTime.1", "value": 2, "rate": 1 }
    })
}

/// A LUT on every stage: the root stack, a video and an audio track, a clip,
/// a gap and a clip nested inside a composite clip; `b` is used twice.
fn timeline() -> Timeline {
    serde_json::from_value(json!({
        "OTIO_SCHEMA": "Timeline.1",
        "tracks": {
            "OTIO_SCHEMA": "Stack.1",
            "effects": [effect("tellers.ai.ColorLut", json!({ "asset_id": "root" }))],
            "children": [
                {
                    "OTIO_SCHEMA": "Track.1",
                    "kind": "Video",
                    "effects": [effect("tellers.ai.ColorLut", json!({ "asset_id": "track" }))],
                    "children": [
                        {
                            "OTIO_SCHEMA": "Clip.2",
                            "source_range": range(),
                            "effects": [
                                effect("tellers.ai.Blur", json!({ "strength": 2 })),
                                effect("tellers.ai.ColorLut", json!({ "asset_id": " b " }))
                            ]
                        },
                        {
                            "OTIO_SCHEMA": "Gap.1",
                            "source_range": range(),
                            "effects": [effect("tellers.ai.ColorLut", json!({ "asset_id": "gap" }))]
                        },
                        {
                            "OTIO_SCHEMA": "Stack.1",
                            "effects": [effect("tellers.ai.ColorLut", json!({ "asset_id": "composite" }))],
                            "children": [{
                                "OTIO_SCHEMA": "Track.1",
                                "kind": "Video",
                                "children": [{
                                    "OTIO_SCHEMA": "Clip.2",
                                    "source_range": range(),
                                    "effects": [effect("tellers.ai.ColorLut", json!({ "asset_id": "b" }))]
                                }]
                            }]
                        }
                    ]
                },
                {
                    "OTIO_SCHEMA": "Track.1",
                    "kind": "Audio",
                    "children": [{
                        "OTIO_SCHEMA": "Clip.2",
                        "source_range": range(),
                        "effects": [effect("tellers.ai.SomeFutureEffect", json!({ "asset_id": "audio" }))]
                    }]
                }
            ]
        }
    }))
    .unwrap()
}

/// Every effect's `target_url`, in timeline order.
fn target_urls(timeline: &Timeline) -> Vec<String> {
    let json = serde_json::to_value(timeline).unwrap();
    let mut urls = Vec::new();
    fn walk(value: &serde_json::Value, urls: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(map) => {
                if let Some(url) = map
                    .get("parameters")
                    .and_then(|p| p.get("target_url"))
                    .and_then(|u| u.as_str())
                {
                    urls.push(url.to_string());
                }
                map.values().for_each(|v| walk(v, urls));
            }
            serde_json::Value::Array(items) => items.iter().for_each(|v| walk(v, urls)),
            _ => {}
        }
    }
    walk(&json, &mut urls);
    urls
}

fn urls(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

#[test]
fn lists_every_asset_id_once_in_timeline_order() {
    assert_eq!(
        timeline().effect_asset_ids(),
        ["root", "track", "b", "gap", "composite", "audio"]
    );
}

#[test]
fn sets_target_urls_on_every_stage() {
    let mut tl = timeline();
    let updated = tl.set_effect_target_urls(&urls(&[
        ("root", "https://cdn/root.cube"),
        ("track", "https://cdn/track.cube"),
        ("b", "https://cdn/b.cube"),
        ("gap", "https://cdn/gap.cube"),
        ("composite", "https://cdn/composite.cube"),
        ("audio", "https://cdn/audio.bin"),
        ("unknown", "https://cdn/unknown"),
    ]));
    assert_eq!(updated, 7);
    assert_eq!(target_urls(&tl).len(), 7);

    // Other parameters are kept; the id is kept as written.
    let Item::Clip(clip) = &tl.tracks.children[0].items[0] else {
        panic!("expected a clip");
    };
    assert_eq!(
        clip.effects[1].metadata.other["tellers.ai"]["parameters"],
        json!({ "asset_id": " b ", "target_url": "https://cdn/b.cube" })
    );
    assert_eq!(clip.effects[0].metadata.other["tellers.ai"]["parameters"], json!({ "strength": 2 }));

    assert_eq!(tl.set_effect_target_url("track", "https://cdn/track2.cube"), 1);
    assert!(target_urls(&tl).contains(&"https://cdn/track2.cube".to_string()));
}

#[test]
fn empty_urls_are_ignored() {
    let mut tl = timeline();
    assert_eq!(tl.set_effect_target_urls(&urls(&[("root", "  ")])), 0);
    assert!(target_urls(&tl).is_empty());
}

#[test]
fn clear_target_urls_strips_effect_urls_and_keeps_asset_ids() {
    let mut tl = timeline();
    tl.set_effect_target_urls(&urls(&[("b", "https://cdn/b.cube"), ("gap", "https://cdn/gap.cube")]));
    assert_eq!(target_urls(&tl).len(), 3);

    let mut cleared = tl.clone();
    assert_eq!(cleared.clear_effect_target_urls(), 3);
    assert!(target_urls(&cleared).is_empty());
    assert_eq!(cleared.effect_asset_ids(), timeline().effect_asset_ids());
    assert_eq!(cleared.clear_effect_target_urls(), 0);

    tl.clear_target_urls();
    assert_eq!(tl, cleared);
}

#[test]
fn item_clear_target_urls_reaches_composite_children() {
    let mut tl = timeline();
    tl.set_effect_target_urls(&urls(&[("composite", "https://cdn/c.cube"), ("b", "https://cdn/b.cube")]));
    let composite = &mut tl.tracks.children[0].items[2];
    assert_eq!(composite.clear_effect_target_urls(), 2);
    // The video clip's `b` stays.
    assert_eq!(target_urls(&tl), ["https://cdn/b.cube"]);
}

#[test]
fn effect_list_functions() {
    let mut effects: Vec<Effect> = serde_json::from_value(json!([
        effect("tellers.ai.ColorLut", json!({ "asset_id": "a" })),
        effect("tellers.ai.ColorLut", json!({ "asset_id": "" })),
        { "OTIO_SCHEMA": "Effect.1", "effect_name": "tellers.ai.ColorLut", "metadata": {} }
    ]))
    .unwrap();
    assert_eq!(effect_asset_id(&effects[0]), Some("a"));
    assert_eq!(effect_asset_id(&effects[1]), None);
    assert_eq!(effect_asset_id(&effects[2]), None);

    let map = urls(&[("a", "https://cdn/a.cube"), ("", "https://cdn/empty")]);
    assert_eq!(set_effect_target_urls(&mut effects, &map), 1);
    assert_eq!(clear_effect_target_urls(&mut effects), 1);
    assert_eq!(
        effects[0].metadata.other["tellers.ai"]["parameters"],
        json!({ "asset_id": "a" })
    );
}
