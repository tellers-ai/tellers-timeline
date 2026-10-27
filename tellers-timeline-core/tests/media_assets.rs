// Tests for resolving the `target_url` of media references from their
// `metadata["tellers.ai"]["media_id"]`: `Timeline::media_asset_ids`,
// `set_media_target_urls` and `clear_media_asset_target_urls`. They cover
// every clip, those inside composite clips (nested ones included) too.

use std::collections::HashMap;

use serde_json::json;
use tellers_timeline_core::{MediaReference, Timeline};

fn range() -> serde_json::Value {
    json!({
        "OTIO_SCHEMA": "TimeRange.1",
        "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 0, "rate": 1 },
        "duration": { "OTIO_SCHEMA": "RationalTime.1", "value": 2, "rate": 1 }
    })
}

fn reference(media_id: Option<&str>, url: &str) -> serde_json::Value {
    let metadata = match media_id {
        Some(id) => json!({ "tellers.ai": { "media_id": id } }),
        None => json!({}),
    };
    json!({ "OTIO_SCHEMA": "ExternalReference.1", "target_url": url, "metadata": metadata })
}

fn clip(id: &str, media_id: Option<&str>, url: &str) -> serde_json::Value {
    json!({
        "OTIO_SCHEMA": "Clip.2",
        "metadata": { "tellers.ai": { "timeline_id": id } },
        "source_range": range(),
        "active_media_reference_key": "DEFAULT_MEDIA",
        "media_references": { "DEFAULT_MEDIA": reference(media_id, url) }
    })
}

fn composite(children: Vec<serde_json::Value>) -> serde_json::Value {
    json!({
        "OTIO_SCHEMA": "Stack.1",
        "children": [{ "OTIO_SCHEMA": "Track.1", "kind": "Video", "children": children }]
    })
}

/// `a` on a video track, `b` inside a composite, `c` inside a composite
/// nested in it, `a` again on an audio track, and a clip with a plain URL
/// and no media id.
fn timeline() -> Timeline {
    serde_json::from_value(json!({
        "OTIO_SCHEMA": "Timeline.1",
        "tracks": {
            "OTIO_SCHEMA": "Stack.1",
            "children": [
                {
                    "OTIO_SCHEMA": "Track.1",
                    "kind": "Video",
                    "children": [
                        clip("1", Some("a"), ""),
                        { "OTIO_SCHEMA": "Gap.1", "source_range": range() },
                        composite(vec![
                            clip("2", Some(" b "), ""),
                            composite(vec![clip("3", Some("c"), "")]),
                        ]),
                        clip("4", None, "https://raw.example/plain.mp4"),
                    ]
                },
                {
                    "OTIO_SCHEMA": "Track.1",
                    "kind": "Audio",
                    "children": [clip("5", Some("a"), "")]
                }
            ]
        }
    }))
    .unwrap()
}

fn urls(timeline: &Timeline) -> Vec<String> {
    let mut urls = Vec::new();
    timeline.for_each_clip(&mut |clip| {
        urls.push(
            clip.active_media_reference()
                .and_then(MediaReference::target_url)
                .cloned()
                .unwrap_or_default(),
        )
    });
    urls
}

#[test]
fn media_asset_ids_cover_clips_inside_composites() {
    assert_eq!(timeline().media_asset_ids(), vec!["a", "b", "c"]);
}

#[test]
fn set_media_target_urls_reaches_nested_clips() {
    let mut tl = timeline();
    let updated = tl.set_media_target_urls(&HashMap::from([
        ("a".to_string(), "https://cdn.example/a".to_string()),
        ("b".to_string(), " https://cdn.example/b ".to_string()),
        ("c".to_string(), "https://cdn.example/c".to_string()),
        (
            "unknown".to_string(),
            "https://cdn.example/unknown".to_string(),
        ),
    ]));
    assert_eq!(updated, 4);
    assert_eq!(
        urls(&tl),
        vec![
            "https://cdn.example/a",
            "https://cdn.example/b",
            "https://cdn.example/c",
            "https://raw.example/plain.mp4",
            "https://cdn.example/a",
        ]
    );
}

#[test]
fn set_media_target_url_ignores_an_empty_url() {
    let mut tl = timeline();
    assert_eq!(tl.set_media_target_url("c", "  "), 0);
    assert_eq!(tl.set_media_target_url(" c ", "https://cdn.example/c"), 1);
    assert_eq!(urls(&tl)[2], "https://cdn.example/c");
}

#[test]
fn clear_media_asset_target_urls_keeps_plain_urls() {
    let mut tl = timeline();
    tl.set_media_target_urls(&HashMap::from([
        ("b".to_string(), "https://cdn.example/b".to_string()),
        ("c".to_string(), "https://cdn.example/c".to_string()),
    ]));
    assert_eq!(tl.clear_media_asset_target_urls(), 2);
    assert_eq!(
        urls(&tl),
        vec!["", "", "", "https://raw.example/plain.mp4", ""]
    );
    assert_eq!(tl.media_asset_ids(), vec!["a", "b", "c"]);
}

#[test]
fn a_legacy_root_media_id_is_read() {
    let tl: Timeline = serde_json::from_value(json!({
        "OTIO_SCHEMA": "Timeline.1",
        "tracks": {
            "OTIO_SCHEMA": "Stack.1",
            "children": [{
                "OTIO_SCHEMA": "Track.1",
                "kind": "Video",
                "children": [composite(vec![json!({
                    "OTIO_SCHEMA": "Clip.2",
                    "source_range": range(),
                    "media_references": { "DEFAULT_MEDIA": {
                        "OTIO_SCHEMA": "ExternalReference.1",
                        "target_url": "",
                        "metadata": { "media_id": "legacy" }
                    }}
                })])]
            }]
        }
    }))
    .unwrap();
    assert_eq!(tl.media_asset_ids(), vec!["legacy"]);
}
