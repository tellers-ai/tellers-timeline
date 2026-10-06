// Tests for the `.cube` LUT accessors at the timeline, clip and asset stages.
// They read and write `metadata["tellers.ai"]["color_grading"]` as
// `{ "name"?, "cube": <url> }`, the shape video-player-js reads, so the
// stored JSON is part of the contract with the player.

use serde_json::json;
use tellers_timeline_core::{
    remove_color_lut, resolve_color_lut, set_color_lut, Clip, ColorLut, Gap, Item, MediaReference,
    TimeRange, Timeline, Track, TrackKind,
};

fn external(url: &str) -> MediaReference {
    MediaReference::ExternalReference {
        target_url: url.to_string(),
        available_range: None,
        name: None,
        available_image_bounds: None,
        metadata: json!({}),
    }
}

fn clip(id: &str) -> Clip {
    let mut refs = std::collections::HashMap::new();
    refs.insert("DEFAULT_MEDIA".to_string(), external("a.mp4"));
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

fn look() -> ColorLut {
    ColorLut::new("https://cdn.example.test/look.cube").with_name("look.cube")
}

#[test]
fn set_color_lut_writes_the_player_shape() {
    let mut metadata = json!({"tellers.ai": {"timeline_id": "x"}, "other": 1});
    assert!(set_color_lut(&mut metadata, look()));
    assert_eq!(
        metadata,
        json!({
            "tellers.ai": {
                "timeline_id": "x",
                "color_grading": {"cube": "https://cdn.example.test/look.cube", "name": "look.cube"}
            },
            "other": 1
        })
    );
    assert_eq!(resolve_color_lut(&metadata), Some(look()));
}

#[test]
fn reads_trim_and_skip_what_the_player_ignores() {
    let read = |grading: serde_json::Value| {
        resolve_color_lut(&json!({"tellers.ai": {"color_grading": grading}}))
    };
    assert_eq!(
        read(json!({"cube": " /luts/warm.cube ", "name": "  "})),
        Some(ColorLut::new("/luts/warm.cube"))
    );
    assert_eq!(read(json!({"cube": "   "})), None);
    assert_eq!(read(json!({"cube": 3})), None);
    assert_eq!(read(json!({"name": "x.cube"})), None);
    assert_eq!(read(json!("https://cdn/x.cube")), None);
    assert_eq!(resolve_color_lut(&json!(null)), None);
}

#[test]
fn set_rejects_an_empty_url_and_keeps_metadata() {
    let mut metadata = json!({"a": 1});
    assert!(!set_color_lut(&mut metadata, ColorLut::new("  ")));
    assert_eq!(metadata, json!({"a": 1}));
}

#[test]
fn set_replaces_and_preserves_unknown_grading_keys() {
    let mut metadata = json!({"tellers.ai": {"color_grading": {"cube": "old.cube", "name": "old", "intensity": 0.5}}});
    assert!(set_color_lut(&mut metadata, ColorLut::new("new.cube")));
    assert_eq!(
        metadata["tellers.ai"]["color_grading"],
        json!({"cube": "new.cube", "intensity": 0.5})
    );
}

#[test]
fn remove_drops_the_grading_object() {
    let mut metadata = json!({"tellers.ai": {"timeline_id": "x"}});
    assert!(!remove_color_lut(&mut metadata));
    set_color_lut(&mut metadata, look());
    assert!(remove_color_lut(&mut metadata));
    assert_eq!(metadata, json!({"tellers.ai": {"timeline_id": "x"}}));
}

#[test]
fn timeline_stage_round_trips_through_json() {
    let mut tl = Timeline::default();
    assert_eq!(tl.get_color_lut(), None);
    assert!(tl.set_color_lut(look()));

    let reparsed: Timeline =
        serde_json::from_str(&tl.to_json_with_options(None, false).unwrap()).unwrap();
    assert_eq!(reparsed.get_color_lut(), Some(look()));

    assert!(tl.remove_color_lut());
    assert_eq!(tl.get_color_lut(), None);
    assert!(!tl.remove_color_lut());
}

#[test]
fn clip_and_asset_stages_are_separate() {
    let mut c = clip("c1");
    assert!(c.set_color_lut(ColorLut::new("clip.cube")));
    assert!(c.set_asset_color_lut(ColorLut::new("asset.cube")));

    assert_eq!(c.get_color_lut(), Some(ColorLut::new("clip.cube")));
    assert_eq!(c.get_asset_color_lut(), Some(ColorLut::new("asset.cube")));
    assert_eq!(
        c.media_references["DEFAULT_MEDIA"].metadata()["tellers.ai"]["color_grading"]["cube"],
        "asset.cube"
    );
    // Asset first, then clip: the order the player grades in.
    assert_eq!(c.color_lut_urls(), vec!["asset.cube", "clip.cube"]);

    assert!(c.remove_asset_color_lut());
    assert_eq!(c.get_asset_color_lut(), None);
    assert_eq!(c.get_color_lut(), Some(ColorLut::new("clip.cube")));
}

#[test]
fn asset_stage_follows_the_active_media_reference() {
    let mut c = clip("c1");
    c.media_references
        .insert("PROXY".to_string(), external("p.mp4"));
    c.active_media_reference_key = Some("PROXY".to_string());
    assert!(c.set_asset_color_lut(ColorLut::new("proxy.cube")));
    assert_eq!(
        c.media_references["PROXY"].get_color_lut(),
        Some(ColorLut::new("proxy.cube"))
    );
    assert_eq!(c.media_references["DEFAULT_MEDIA"].get_color_lut(), None);

    // Without an active key the player falls back to DEFAULT_MEDIA.
    c.active_media_reference_key = None;
    assert_eq!(c.get_asset_color_lut(), None);

    c.active_media_reference_key = Some("MISSING".to_string());
    assert!(!c.set_asset_color_lut(ColorLut::new("x.cube")));
}

#[test]
fn timeline_writes_items_by_id() {
    let mut tl = timeline_with_clip("c1");
    assert!(tl.set_item_color_lut("c1", ColorLut::new("clip.cube")));
    assert!(tl.set_item_asset_color_lut("c1", ColorLut::new("asset.cube")));
    assert_eq!(
        tl.get_item_color_lut("c1"),
        Some(ColorLut::new("clip.cube"))
    );
    assert_eq!(
        tl.get_item_asset_color_lut("c1"),
        Some(ColorLut::new("asset.cube"))
    );

    assert!(!tl.set_item_color_lut("missing", ColorLut::new("x.cube")));
    assert!(!tl.set_item_color_lut("gap", ColorLut::new("x.cube")));
    assert_eq!(tl.get_item_color_lut("gap"), None);

    assert!(tl.remove_item_color_lut("c1"));
    assert!(tl.remove_item_asset_color_lut("c1"));
    assert_eq!(tl.get_item_color_lut("c1"), None);
    assert_eq!(tl.get_item_asset_color_lut("c1"), None);
}

#[test]
fn color_lut_urls_lists_every_stage_once() {
    let mut tl = timeline_with_clip("c1");
    tl.tracks.children[0].items.push(Item::Clip(clip("c2")));
    tl.set_color_lut(ColorLut::new("look.cube"));
    tl.set_item_asset_color_lut("c1", ColorLut::new("log.cube"));
    tl.set_item_color_lut("c1", ColorLut::new("shot.cube"));
    tl.set_item_asset_color_lut("c2", ColorLut::new("log.cube"));

    assert_eq!(
        tl.color_lut_urls(),
        vec!["look.cube", "log.cube", "shot.cube"]
    );
}

#[test]
fn tracks_are_not_a_grading_stage() {
    let mut tl = timeline_with_clip("c1");
    set_color_lut(
        &mut tl.tracks.children[0].metadata,
        ColorLut::new("track.cube"),
    );
    assert!(tl.color_lut_urls().is_empty());
}

#[test]
fn item_delegates_to_its_clip_and_ignores_gaps() {
    let mut item = Item::Clip(clip("c1"));
    assert!(item.set_color_lut(ColorLut::new("clip.cube")));
    assert!(item.set_asset_color_lut(ColorLut::new("asset.cube")));
    assert_eq!(item.get_color_lut(), Some(ColorLut::new("clip.cube")));
    assert_eq!(
        item.get_asset_color_lut(),
        Some(ColorLut::new("asset.cube"))
    );

    let mut gap = Item::Gap(Gap::make_gap(1.0));
    assert!(!gap.set_color_lut(ColorLut::new("clip.cube")));
    assert_eq!(gap.get_color_lut(), None);
    assert!(!gap.remove_asset_color_lut());
}
