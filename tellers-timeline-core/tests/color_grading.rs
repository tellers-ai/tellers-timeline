// Tests for the `.cube` LUT list accessors on the timeline, its tracks and
// its clips. A LUT is an OTIO `ColorLut` effect whose
// `metadata["tellers.ai"]` holds `{ "asset_id", "cube_url"? }`, where `cube_url` is
// the URL the player (video-player-js) reads, resolved from the asset id; the
// effect's `name` is the display name. The stored JSON is part of the
// contract with the player. Timelines written before LUTs were effects hold
// `metadata["tellers.ai"]["color_grading"]` instead, which is still read.

use std::collections::HashMap;

use serde_json::json;
use tellers_timeline_core::{
    clear_color_lut_urls, clear_color_luts, insert_color_lut_at, push_color_lut,
    remove_color_lut_at, replace_color_lut_at, resolve_color_luts, set_color_lut_urls,
    set_color_luts, Clip, ColorLut, Effect, Gap, IdMetadataExt, Item, MediaReference, TimeRange,
    Timeline, Track, TrackKind,
};

fn clip(id: &str) -> Clip {
    let mut refs = HashMap::new();
    refs.insert(
        "DEFAULT_MEDIA".to_string(),
        MediaReference::ExternalReference {
            target_url: "a.mp4".to_string(),
            available_range: None,
            name: None,
            available_image_bounds: None,
            metadata: json!({}),
        },
    );
    Clip::new(
        TimeRange::new(2.0, 0.0),
        refs,
        Some("DEFAULT_MEDIA".to_string()),
        None,
        Some(id.to_string()),
    )
}

fn timeline_with_clip(id: &str) -> Timeline {
    let mut track = Track::default();
    track.kind = TrackKind::Video;
    track.items.push(Item::Clip(clip(id)));
    track
        .items
        .push(Item::Gap(Gap::new(1.0, Some("gap".to_string()))));
    let mut tl = Timeline::default();
    tl.tracks.children.push(track);
    tl
}

fn lut(asset_id: &str) -> ColorLut {
    ColorLut::new(asset_id)
}

fn ids(luts: &[ColorLut]) -> Vec<&str> {
    luts.iter().map(|lut| lut.asset_id.as_str()).collect()
}

fn effects_json(effects: &[Effect]) -> serde_json::Value {
    serde_json::to_value(effects).unwrap()
}

fn effects_from(value: serde_json::Value) -> Vec<Effect> {
    serde_json::from_value(value).unwrap()
}

#[test]
fn push_writes_color_lut_effects() {
    let mut effects = Vec::new();
    let mut metadata = json!({"tellers.ai": {"timeline_id": "x"}, "other": 1});
    assert!(push_color_lut(
        &mut effects,
        &mut metadata,
        lut("slog3").with_name("slog3_to_709.cube")
    ));
    assert!(push_color_lut(
        &mut effects,
        &mut metadata,
        lut("warm").with_url("https://cdn/warm.cube")
    ));
    assert_eq!(
        effects_json(&effects),
        json!([
            {
                "OTIO_SCHEMA": "Effect.1",
                "name": "slog3_to_709.cube",
                "effect_name": "ColorLut",
                "metadata": {"tellers.ai": {"asset_id": "slog3"}}
            },
            {
                "OTIO_SCHEMA": "Effect.1",
                "name": "",
                "effect_name": "ColorLut",
                "metadata": {"tellers.ai": {"asset_id": "warm", "cube_url": "https://cdn/warm.cube"}}
            }
        ])
    );
    assert_eq!(
        metadata,
        json!({"tellers.ai": {"timeline_id": "x"}, "other": 1})
    );
}

#[test]
fn reads_trim_and_skip_effects_without_an_asset_id() {
    let effects = effects_from(json!([
        {"effect_name": "ColorLut", "name": "  ", "metadata": {"tellers.ai": {"asset_id": " a ", "cube_url": " /luts/a.cube "}}},
        {"effect_name": "ColorLut", "metadata": {"tellers.ai": {"asset_id": "   "}}},
        {"effect_name": "ColorLut", "metadata": {"tellers.ai": {"asset_id": 3}}},
        {"effect_name": "ColorLut", "metadata": {}},
        {"effect_name": "Blur", "metadata": {"tellers.ai": {"asset_id": "blur"}}},
        {"effect_name": "ColorLut", "name": "b.cube", "metadata": {"tellers.ai": {"asset_id": "b", "cube_url": ""}}}
    ]));
    assert_eq!(
        resolve_color_luts(&effects, &json!({})),
        vec![
            lut("a").with_url("/luts/a.cube"),
            lut("b").with_name("b.cube")
        ]
    );
    assert!(resolve_color_luts(&[], &json!(null)).is_empty());
}

#[test]
fn legacy_metadata_list_is_read_when_there_is_no_lut_effect() {
    let metadata = json!({"tellers.ai": {"color_grading": [
        {"asset_id": " a ", "name": "  ", "cube": " /luts/a.cube "},
        {"asset_id": "   "},
        {"asset_id": 3},
        {"cube": "https://cdn/no-id.cube"},
        "b",
        {"asset_id": "b", "name": "b.cube", "cube": ""}
    ]}});
    assert_eq!(
        resolve_color_luts(&[], &metadata),
        vec![
            lut("a").with_url("/luts/a.cube"),
            lut("b").with_name("b.cube")
        ]
    );
    assert!(resolve_color_luts(
        &[],
        &json!({"tellers.ai": {"color_grading": {"asset_id": "a"}}})
    )
    .is_empty());

    // Any LUT effect, even an unusable one, shadows the legacy list.
    let effects = effects_from(json!([{"effect_name": "ColorLut", "metadata": {}}]));
    assert!(resolve_color_luts(&effects, &metadata).is_empty());
}

#[test]
fn writes_move_the_legacy_list_into_effects() {
    let mut effects = Vec::new();
    let mut metadata = json!({"tellers.ai": {"timeline_id": "x", "color_grading": [
        {"cube": "orphan.cube"},
        {"asset_id": "a", "name": "a.cube", "cube": "https://cdn/a.cube", "intensity": 0.5},
        {"asset_id": "b"}
    ]}});
    assert!(push_color_lut(&mut effects, &mut metadata, lut("c")));
    assert_eq!(metadata, json!({"tellers.ai": {"timeline_id": "x"}}));
    assert_eq!(
        effects_json(&effects),
        json!([
            {
                "OTIO_SCHEMA": "Effect.1",
                "name": "a.cube",
                "effect_name": "ColorLut",
                "metadata": {"tellers.ai": {"asset_id": "a", "cube_url": "https://cdn/a.cube", "intensity": 0.5}}
            },
            {
                "OTIO_SCHEMA": "Effect.1",
                "name": "",
                "effect_name": "ColorLut",
                "metadata": {"tellers.ai": {"asset_id": "b"}}
            },
            {
                "OTIO_SCHEMA": "Effect.1",
                "name": "",
                "effect_name": "ColorLut",
                "metadata": {"tellers.ai": {"asset_id": "c"}}
            }
        ])
    );
}

#[test]
fn writes_keep_other_effects_in_place() {
    let blur =
        json!({"OTIO_SCHEMA": "Effect.1", "name": "", "effect_name": "Blur", "metadata": {}});
    let sharpen =
        json!({"OTIO_SCHEMA": "Effect.1", "name": "", "effect_name": "Sharpen", "metadata": {}});
    let mut effects = effects_from(json!([
        blur.clone(),
        {"effect_name": "ColorLut", "metadata": {"tellers.ai": {"asset_id": "a"}}},
        sharpen.clone(),
        {"effect_name": "ColorLut", "metadata": {"tellers.ai": {"asset_id": "b"}}}
    ]));
    let mut metadata = json!({});
    assert!(insert_color_lut_at(
        &mut effects,
        &mut metadata,
        1,
        lut("mid")
    ));
    let names: Vec<&str> = effects
        .iter()
        .map(|effect| effect.effect_name.as_str())
        .collect();
    assert_eq!(
        names,
        ["Blur", "ColorLut", "ColorLut", "ColorLut", "Sharpen"]
    );
    assert_eq!(effects_json(&effects)[0], blur);
    assert_eq!(effects_json(&effects)[4], sharpen);
    assert_eq!(
        ids(&resolve_color_luts(&effects, &metadata)),
        ["a", "mid", "b"]
    );

    assert!(clear_color_luts(&mut effects, &mut metadata));
    assert_eq!(effects_json(&effects), json!([blur, sharpen]));
}

#[test]
fn insert_replace_remove_by_index() {
    let (mut e, mut m) = (Vec::new(), json!({}));
    assert!(push_color_lut(&mut e, &mut m, lut("b")));
    assert!(insert_color_lut_at(&mut e, &mut m, 0, lut("a")));
    assert!(insert_color_lut_at(&mut e, &mut m, 2, lut("c")));
    assert!(!insert_color_lut_at(&mut e, &mut m, 4, lut("x")));
    assert_eq!(ids(&resolve_color_luts(&e, &m)), ["a", "b", "c"]);

    assert_eq!(
        replace_color_lut_at(&mut e, &mut m, 1, lut("B")),
        Some(lut("b"))
    );
    assert_eq!(replace_color_lut_at(&mut e, &mut m, 3, lut("x")), None);

    assert_eq!(remove_color_lut_at(&mut e, &mut m, 0), Some(lut("a")));
    assert_eq!(remove_color_lut_at(&mut e, &mut m, 5), None);
    assert_eq!(ids(&resolve_color_luts(&e, &m)), ["B", "c"]);
}

#[test]
fn indices_skip_unusable_effects_and_writes_drop_them() {
    let mut effects = effects_from(json!([
        {"effect_name": "ColorLut", "metadata": {"tellers.ai": {"cube_url": "orphan.cube"}}},
        {"effect_name": "ColorLut", "metadata": {"tellers.ai": {"asset_id": "a", "intensity": 0.5}}},
        {"effect_name": "ColorLut", "metadata": {"tellers.ai": {"asset_id": "b"}}}
    ]));
    assert_eq!(
        remove_color_lut_at(&mut effects, &mut json!({}), 1),
        Some(lut("b"))
    );
    // The unusable effect is gone; untouched effects keep their other keys.
    assert_eq!(
        effects_json(&effects),
        json!([{
            "OTIO_SCHEMA": "Effect.1",
            "name": "",
            "effect_name": "ColorLut",
            "metadata": {"tellers.ai": {"asset_id": "a", "intensity": 0.5}}
        }])
    );
}

#[test]
fn empty_asset_ids_are_rejected_without_changes() {
    let mut effects = vec![lut("a").to_effect()];
    let mut metadata = json!({"tellers.ai": {"color_grading": [{"asset_id": "legacy"}]}});
    let before = (effects.clone(), metadata.clone());
    assert!(!push_color_lut(&mut effects, &mut metadata, lut("  ")));
    assert!(!insert_color_lut_at(
        &mut effects,
        &mut metadata,
        0,
        lut("")
    ));
    assert_eq!(
        replace_color_lut_at(&mut effects, &mut metadata, 0, lut(" ")),
        None
    );
    assert!(!set_color_luts(
        &mut effects,
        &mut metadata,
        vec![lut("ok"), lut("")]
    ));
    assert_eq!((effects, metadata), before);
}

#[test]
fn removing_the_last_lut_leaves_no_effect() {
    let (mut e, mut m) = (Vec::new(), json!({"tellers.ai": {"timeline_id": "x"}}));
    assert!(!clear_color_luts(&mut e, &mut m));
    push_color_lut(&mut e, &mut m, lut("a"));
    assert_eq!(remove_color_lut_at(&mut e, &mut m, 0), Some(lut("a")));
    assert!(e.is_empty());

    push_color_lut(&mut e, &mut m, lut("a"));
    assert!(set_color_luts(&mut e, &mut m, vec![]));
    assert!(e.is_empty());
    assert_eq!(m, json!({"tellers.ai": {"timeline_id": "x"}}));
}

#[test]
fn set_and_clear_urls_by_asset_id() {
    let mut effects = effects_from(json!([
        {"effect_name": "ColorLut", "metadata": {"tellers.ai": {"asset_id": "a", "intensity": 0.5}}},
        {"effect_name": "ColorLut", "metadata": {"tellers.ai": {"asset_id": "b"}}},
        {"effect_name": "ColorLut", "metadata": {"tellers.ai": {"asset_id": "a"}}}
    ]));
    let mut metadata = json!({});
    let urls = HashMap::from([
        ("a".to_string(), " https://cdn/a.cube ".to_string()),
        ("b".to_string(), "  ".to_string()),
        ("zzz".to_string(), "https://cdn/zzz.cube".to_string()),
    ]);
    // Both "a" effects; the empty URL for "b" is ignored.
    assert_eq!(set_color_lut_urls(&mut effects, &mut metadata, &urls), 2);
    let ai = |effects: &[Effect]| -> Vec<serde_json::Value> {
        effects
            .iter()
            .map(|effect| effect.metadata.other["tellers.ai"].clone())
            .collect()
    };
    assert_eq!(
        ai(&effects),
        [
            json!({"asset_id": "a", "intensity": 0.5, "cube_url": "https://cdn/a.cube"}),
            json!({"asset_id": "b"}),
            json!({"asset_id": "a", "cube_url": "https://cdn/a.cube"})
        ]
    );

    assert_eq!(clear_color_lut_urls(&mut effects, &mut metadata), 2);
    assert_eq!(
        ai(&effects),
        [
            json!({"asset_id": "a", "intensity": 0.5}),
            json!({"asset_id": "b"}),
            json!({"asset_id": "a"})
        ]
    );
    assert_eq!(clear_color_lut_urls(&mut effects, &mut metadata), 0);
}

#[test]
fn urls_resolve_on_a_legacy_list() {
    let mut metadata = json!({"tellers.ai": {"color_grading": [{"asset_id": "a"}]}});
    let urls = HashMap::from([("a".to_string(), "https://cdn/a.cube".to_string())]);
    assert_eq!(set_color_lut_urls(&mut [], &mut metadata, &urls), 1);
    assert_eq!(
        resolve_color_luts(&[], &metadata),
        vec![lut("a").with_url("https://cdn/a.cube")]
    );
    assert_eq!(clear_color_lut_urls(&mut [], &mut metadata), 1);
    assert_eq!(
        metadata,
        json!({"tellers.ai": {"color_grading": [{"asset_id": "a"}]}})
    );
}

#[test]
fn timeline_resolves_urls_on_every_stage() {
    let mut tl = timeline_with_clip("c1");
    tl.tracks.children[0].items.push(Item::Clip(clip("c2")));
    tl.push_color_lut(lut("look"));
    tl.set_item_color_luts("c1", vec![lut("log"), lut("shot")]);
    tl.push_item_color_lut("c2", lut("log"));

    assert_eq!(tl.color_lut_asset_ids(), ["look", "log", "shot"]);

    assert_eq!(tl.set_color_lut_url("log", "https://cdn/log.cube"), 2);
    assert_eq!(tl.set_color_lut_url("log", ""), 0);
    let urls = HashMap::from([
        ("look".to_string(), "https://cdn/look.cube".to_string()),
        ("shot".to_string(), "https://cdn/shot.cube".to_string()),
    ]);
    assert_eq!(tl.set_color_lut_urls(&urls), 2);

    assert_eq!(
        tl.get_color_luts(),
        vec![lut("look").with_url("https://cdn/look.cube")]
    );
    assert_eq!(
        tl.get_item_color_luts("c1"),
        Some(vec![
            lut("log").with_url("https://cdn/log.cube"),
            lut("shot").with_url("https://cdn/shot.cube")
        ])
    );
    assert_eq!(
        tl.get_item_color_luts("c2"),
        Some(vec![lut("log").with_url("https://cdn/log.cube")])
    );

    assert_eq!(tl.clear_color_lut_urls(), 4);
    assert_eq!(
        tl.get_item_color_luts("c1"),
        Some(vec![lut("log"), lut("shot")])
    );
    assert_eq!(tl.color_lut_asset_ids(), ["look", "log", "shot"]);
}

#[test]
fn timeline_stage_round_trips_through_json() {
    let mut tl = Timeline::default();
    assert!(tl.get_color_luts().is_empty());
    let luts = vec![
        lut("look")
            .with_name("look.cube")
            .with_url("https://cdn/look.cube"),
        lut("grain"),
    ];
    assert!(tl.set_color_luts(luts.clone()));

    // OTIO timelines have no effects; the root stack carries them.
    assert_eq!(tl.tracks.effects.len(), 2);
    let reparsed: Timeline =
        serde_json::from_str(&tl.to_json_with_options(None, false).unwrap()).unwrap();
    assert_eq!(reparsed.get_color_luts(), luts);

    assert!(tl.clear_color_luts());
    assert!(tl.get_color_luts().is_empty());
}

#[test]
fn timeline_edits_clips_by_id() {
    let mut tl = timeline_with_clip("c1");
    assert!(tl.push_item_color_lut("c1", lut("warm")));
    assert!(tl.insert_item_color_lut_at("c1", 0, lut("log")));
    assert_eq!(
        tl.get_item_color_luts("c1"),
        Some(vec![lut("log"), lut("warm")])
    );
    assert_eq!(
        tl.replace_item_color_lut_at("c1", 1, lut("cool")),
        Some(lut("warm"))
    );
    assert_eq!(tl.remove_item_color_lut_at("c1", 0), Some(lut("log")));

    assert!(!tl.push_item_color_lut("missing", lut("x")));
    assert!(!tl.push_item_color_lut("gap", lut("x")));
    assert_eq!(tl.get_item_color_luts("gap"), None);
    assert_eq!(tl.get_item_color_luts("missing"), None);

    assert!(tl.set_item_color_luts("c1", vec![lut("a"), lut("b")]));
    assert!(tl.clear_item_color_luts("c1"));
    assert_eq!(tl.get_item_color_luts("c1"), Some(vec![]));
}

#[test]
fn track_stage_is_stored_on_the_track_effects() {
    let mut track = Track::default();
    assert!(track.is_color_gradable());
    assert!(track.push_color_lut(lut("track").with_name("track.cube")));
    assert_eq!(
        track.effects,
        vec![lut("track").with_name("track.cube").to_effect()]
    );
    assert_eq!(
        resolve_color_luts(&track.effects, &track.metadata),
        track.get_color_luts()
    );

    let reparsed: Track = serde_json::from_str(&serde_json::to_string(&track).unwrap()).unwrap();
    assert_eq!(
        reparsed.get_color_luts(),
        vec![lut("track").with_name("track.cube")]
    );

    track.kind = TrackKind::Audio;
    assert!(!track.is_color_gradable());
}

#[test]
fn timeline_edits_tracks_by_id() {
    let mut tl = timeline_with_clip("c1");
    let track_id = tl.tracks.children[0].get_id().unwrap();
    assert!(tl.is_track_color_gradable(&track_id));
    assert!(!tl.is_track_color_gradable("missing"));

    assert!(tl.push_track_color_lut(&track_id, lut("warm")));
    assert!(tl.insert_track_color_lut_at(&track_id, 0, lut("film")));
    assert_eq!(
        tl.get_track_color_luts(&track_id),
        Some(vec![lut("film"), lut("warm")])
    );
    assert_eq!(
        tl.replace_track_color_lut_at(&track_id, 1, lut("cool")),
        Some(lut("warm"))
    );
    assert_eq!(
        tl.remove_track_color_lut_at(&track_id, 0),
        Some(lut("film"))
    );
    assert!(tl.set_track_color_luts(&track_id, vec![lut("a"), lut("b")]));
    assert!(tl.clear_track_color_luts(&track_id));
    assert_eq!(tl.get_track_color_luts(&track_id), Some(vec![]));

    // A track list does not touch the clips or the timeline.
    assert!(tl.push_track_color_lut(&track_id, lut("t")));
    assert_eq!(tl.get_item_color_luts("c1"), Some(vec![]));
    assert!(tl.get_color_luts().is_empty());

    assert!(!tl.push_track_color_lut("missing", lut("x")));
    assert_eq!(tl.get_track_color_luts("missing"), None);
    assert_eq!(tl.remove_track_color_lut_at("missing", 0), None);
    assert!(!tl.clear_track_color_luts("missing"));
}

#[test]
fn timeline_writers_refuse_non_video_tracks() {
    let mut tl = mixed_timeline();
    let audio_id = tl.tracks.children[1].get_id().unwrap();
    assert!(!tl.is_track_color_gradable(&audio_id));
    tl.tracks.children[1].push_color_lut(lut("stale"));

    assert!(!tl.push_track_color_lut(&audio_id, lut("x")));
    assert!(!tl.insert_track_color_lut_at(&audio_id, 0, lut("x")));
    assert_eq!(tl.replace_track_color_lut_at(&audio_id, 0, lut("x")), None);
    assert!(!tl.set_track_color_luts(&audio_id, vec![lut("x")]));
    // Still readable and removable, so stale LUTs can be cleaned up.
    assert_eq!(tl.get_track_color_luts(&audio_id), Some(vec![lut("stale")]));
    assert_eq!(
        tl.remove_track_color_lut_at(&audio_id, 0),
        Some(lut("stale"))
    );
    tl.tracks.children[1].push_color_lut(lut("stale"));
    assert!(tl.clear_track_color_luts(&audio_id));
}

#[test]
fn url_resolution_covers_video_tracks_only() {
    let mut tl = mixed_timeline();
    tl.push_color_lut(lut("look"));
    tl.tracks.children[0].push_color_lut(lut("video-track"));
    tl.tracks.children[0].push_color_lut(lut("video-lut"));
    tl.tracks.children[1].push_color_lut(lut("audio-track"));

    // Timeline, then per video track: its graded clips, then the track.
    assert_eq!(
        tl.color_lut_asset_ids(),
        ["look", "video-lut", "video-track"]
    );

    let urls = HashMap::from([
        ("video-track".to_string(), "https://cdn/vt.cube".to_string()),
        ("video-lut".to_string(), "https://cdn/v.cube".to_string()),
        ("audio-track".to_string(), "https://cdn/at.cube".to_string()),
    ]);
    // The video clip's entry and the video track's two entries.
    assert_eq!(tl.set_color_lut_urls(&urls), 3);
    let video_id = tl.tracks.children[0].get_id().unwrap();
    let audio_id = tl.tracks.children[1].get_id().unwrap();
    assert_eq!(
        tl.get_track_color_luts(&video_id),
        Some(vec![
            lut("video-track").with_url("https://cdn/vt.cube"),
            lut("video-lut").with_url("https://cdn/v.cube"),
        ])
    );
    assert_eq!(
        tl.get_track_color_luts(&audio_id),
        Some(vec![lut("audio-track")])
    );

    // Clearing strips URLs on every track, graded or not.
    tl.tracks.children[1].set_color_lut_urls(&urls);
    assert_eq!(tl.clear_color_lut_urls(), 4);
    assert_eq!(
        tl.get_track_color_luts(&video_id),
        Some(vec![lut("video-track"), lut("video-lut")])
    );
}

#[test]
fn item_delegates_to_its_clip_and_ignores_gaps() {
    let mut item = Item::Clip(clip("c1"));
    assert!(item.push_color_lut(lut("clip")));
    assert_eq!(item.get_color_luts(), vec![lut("clip")]);
    assert_eq!(item.remove_color_lut_at(0), Some(lut("clip")));

    let mut gap = Item::Gap(Gap::make_gap(1.0));
    assert!(!gap.push_color_lut(lut("clip")));
    assert!(gap.get_color_luts().is_empty());
    assert_eq!(gap.remove_color_lut_at(0), None);
}

#[test]
fn unknown_asset_ids_are_ignored() {
    let mut tl = Timeline::default();
    assert_eq!(tl.set_color_lut_url("missing", "https://cdn/a.cube"), 0);
    assert_eq!(tl.set_color_lut_urls(&HashMap::new()), 0);

    tl.push_color_lut(lut("a1"));
    assert_eq!(tl.set_color_lut_url("b2", "https://cdn/b.cube"), 0);
    assert_eq!(tl.get_color_luts(), vec![lut("a1")]);
}

/// A timeline with a graded clip `video` on a video track, a text clip `text`
/// on the same track, and a media clip `audio` on an audio track. Each clip
/// already carries a LUT `<id>-lut`, stored directly on the clip.
fn mixed_timeline() -> Timeline {
    fn with_lut(mut c: Clip, id: &str) -> Item {
        c.push_color_lut(lut(&format!("{id}-lut")));
        Item::Clip(c)
    }
    let mut text = clip("text");
    text.media_references.insert(
        "DEFAULT_MEDIA".to_string(),
        MediaReference::create_rich_text_reference("<div>Hi</div>".to_string()),
    );
    let mut video_track = Track::default();
    video_track.kind = TrackKind::Video;
    video_track.items.push(with_lut(clip("video"), "video"));
    video_track.items.push(with_lut(text, "text"));
    let mut audio_track = Track::default();
    audio_track.kind = TrackKind::Audio;
    audio_track.items.push(with_lut(clip("audio"), "audio"));
    let mut tl = Timeline::default();
    tl.tracks.children.push(video_track);
    tl.tracks.children.push(audio_track);
    tl
}

#[test]
fn only_media_clips_on_video_tracks_are_gradable() {
    let tl = mixed_timeline();
    assert!(tl.is_item_color_gradable("video"));
    assert!(!tl.is_item_color_gradable("text"));
    assert!(!tl.is_item_color_gradable("audio"));
    assert!(!tl.is_item_color_gradable("missing"));

    let mut other = timeline_with_clip("c1");
    other.tracks.children[0].kind = TrackKind::Other;
    assert!(!other.is_item_color_gradable("c1"));
    assert!(!timeline_with_clip("c1").is_item_color_gradable("gap"));
}

#[test]
fn timeline_writers_refuse_ungraded_clips() {
    let mut tl = mixed_timeline();
    for id in ["text", "audio"] {
        assert!(!tl.push_item_color_lut(id, lut("x")));
        assert!(!tl.insert_item_color_lut_at(id, 0, lut("x")));
        assert_eq!(tl.replace_item_color_lut_at(id, 0, lut("x")), None);
        assert!(!tl.set_item_color_luts(id, vec![lut("x")]));
        // Still readable and removable, so stale LUTs can be cleaned up.
        assert_eq!(
            tl.get_item_color_luts(id),
            Some(vec![lut(&format!("{id}-lut"))])
        );
    }
    assert_eq!(
        tl.remove_item_color_lut_at("text", 0),
        Some(lut("text-lut"))
    );
    assert!(tl.clear_item_color_luts("audio"));
    assert!(tl.push_item_color_lut("video", lut("x")));
}

#[test]
fn url_resolution_skips_ungraded_clips() {
    let mut tl = mixed_timeline();
    tl.push_color_lut(lut("look"));
    assert_eq!(tl.color_lut_asset_ids(), ["look", "video-lut"]);

    let urls = HashMap::from([
        ("video-lut".to_string(), "https://cdn/v.cube".to_string()),
        ("text-lut".to_string(), "https://cdn/t.cube".to_string()),
        ("audio-lut".to_string(), "https://cdn/a.cube".to_string()),
    ]);
    assert_eq!(tl.set_color_lut_urls(&urls), 1);
    assert_eq!(tl.set_color_lut_url("audio-lut", "https://cdn/a.cube"), 0);
    assert_eq!(
        tl.get_item_color_luts("video"),
        Some(vec![lut("video-lut").with_url("https://cdn/v.cube")])
    );
    assert_eq!(tl.get_item_color_luts("text"), Some(vec![lut("text-lut")]));

    // Clearing strips URLs everywhere, including clips that are not graded.
    let audio = tl.tracks.children[1].items[0].clone();
    if let Item::Clip(mut audio) = audio {
        audio.set_color_lut_urls(&urls);
        tl.tracks.children[1].items[0] = Item::Clip(audio);
    }
    assert_eq!(tl.clear_color_lut_urls(), 2);
}

#[test]
fn clip_gradability_follows_the_active_media_reference() {
    let mut c = clip("c1");
    assert!(c.is_color_gradable());
    c.media_references.insert(
        "TITLE".to_string(),
        MediaReference::create_rich_text_reference("<div>Hi</div>".to_string()),
    );
    c.active_media_reference_key = Some("TITLE".to_string());
    assert!(!c.is_color_gradable());
    c.active_media_reference_key = Some("MISSING".to_string());
    assert!(!c.is_color_gradable());
    c.active_media_reference_key = None;
    assert!(c.is_color_gradable());
}

#[test]
fn every_stage_reads_its_legacy_list() {
    let legacy = json!([{"asset_id": "old", "name": "old.cube"}]);
    let mut tl = timeline_with_clip("c1");
    tl.metadata["tellers.ai"]["color_grading"] = legacy.clone();
    tl.tracks.children[0].metadata["tellers.ai"]["color_grading"] = legacy.clone();
    if let Item::Clip(c) = &mut tl.tracks.children[0].items[0] {
        c.metadata["tellers.ai"]["color_grading"] = legacy.clone();
    }
    let track_id = tl.tracks.children[0].get_id().unwrap();
    let old = vec![lut("old").with_name("old.cube")];
    assert_eq!(tl.get_color_luts(), old);
    assert_eq!(tl.get_track_color_luts(&track_id), Some(old.clone()));
    assert_eq!(tl.get_item_color_luts("c1"), Some(old.clone()));
    assert_eq!(tl.color_lut_asset_ids(), ["old"]);
    assert_eq!(tl.set_color_lut_url("old", "https://cdn/old.cube"), 3);

    assert!(tl.push_item_color_lut("c1", lut("new")));
    assert_eq!(
        tl.get_item_color_luts("c1"),
        Some(vec![
            lut("old")
                .with_name("old.cube")
                .with_url("https://cdn/old.cube"),
            lut("new")
        ])
    );
    let Item::Clip(c) = &tl.tracks.children[0].items[0] else {
        unreachable!()
    };
    assert_eq!(c.effects.len(), 2);
    assert!(c.metadata["tellers.ai"].get("color_grading").is_none());
}

#[test]
fn stacks_and_tracks_without_effects_serialize_as_before() {
    let tl = timeline_with_clip("c1");
    let json: serde_json::Value =
        serde_json::from_str(&tl.to_json_with_options(None, false).unwrap()).unwrap();
    assert!(json["tracks"].get("effects").is_none());
    assert!(json["tracks"]["children"][0].get("effects").is_none());
}

#[test]
fn a_legacy_list_beside_lut_effects_is_ignored() {
    let mut effects = vec![lut("new").to_effect()];
    let mut metadata = json!({"tellers.ai": {"color_grading": [
        {"asset_id": "old", "cube": "https://cdn/stale.cube"}
    ]}});
    let legacy = metadata.clone();
    assert_eq!(resolve_color_luts(&effects, &metadata), vec![lut("new")]);

    let urls = HashMap::from([
        ("new".to_string(), "https://cdn/new.cube".to_string()),
        ("old".to_string(), "https://cdn/old.cube".to_string()),
    ]);
    assert_eq!(set_color_lut_urls(&mut effects, &mut metadata, &urls), 1);
    assert_eq!(clear_color_lut_urls(&mut effects, &mut metadata), 1);
    assert_eq!(metadata, legacy);

    let mut tl = timeline_with_clip("c1");
    tl.tracks.effects = vec![lut("look").to_effect()];
    tl.metadata["tellers.ai"]["color_grading"] = json!([{"asset_id": "old"}]);
    assert_eq!(tl.color_lut_asset_ids(), ["look"]);
}
