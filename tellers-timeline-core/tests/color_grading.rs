// Tests for the `.cube` LUT list accessors on the timeline and its clips.
// They read and write `metadata["tellers.ai"]["color_grading"]` as an ordered
// list of `{ "name"?, "cube": <url> }`, the shape video-player-js reads, so
// the stored JSON is part of the contract with the player.

use serde_json::json;
use tellers_timeline_core::{
    clear_color_luts, insert_color_lut_at, push_color_lut, remove_color_lut_at,
    replace_color_lut_at, resolve_color_luts, set_color_luts, Clip, ColorLut, Gap, Item,
    MediaReference, TimeRange, Timeline, Track, TrackKind,
};

fn clip(id: &str) -> Clip {
    let mut refs = std::collections::HashMap::new();
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

fn lut(url: &str) -> ColorLut {
    ColorLut::new(url)
}

fn urls(luts: &[ColorLut]) -> Vec<&str> {
    luts.iter().map(|lut| lut.url.as_str()).collect()
}

#[test]
fn push_writes_the_player_list_shape() {
    let mut metadata = json!({"tellers.ai": {"timeline_id": "x"}, "other": 1});
    assert!(push_color_lut(
        &mut metadata,
        lut("https://cdn/slog3_to_709.cube").with_name("slog3_to_709.cube")
    ));
    assert!(push_color_lut(&mut metadata, lut("https://cdn/warm.cube")));
    assert_eq!(
        metadata,
        json!({
            "tellers.ai": {
                "timeline_id": "x",
                "color_grading": [
                    {"cube": "https://cdn/slog3_to_709.cube", "name": "slog3_to_709.cube"},
                    {"cube": "https://cdn/warm.cube"}
                ]
            },
            "other": 1
        })
    );
}

#[test]
fn reads_trim_and_skip_what_the_player_ignores() {
    let metadata = json!({"tellers.ai": {"color_grading": [
        {"cube": " /luts/warm.cube ", "name": "  "},
        {"cube": "   "},
        {"cube": 3},
        {"name": "x.cube"},
        "https://cdn/x.cube",
        {"cube": "b.cube", "name": "b"}
    ]}});
    assert_eq!(
        resolve_color_luts(&metadata),
        vec![lut("/luts/warm.cube"), lut("b.cube").with_name("b")]
    );
    assert!(resolve_color_luts(&json!(null)).is_empty());
}

#[test]
fn reads_a_legacy_single_object_as_one_entry() {
    let mut metadata =
        json!({"tellers.ai": {"color_grading": {"cube": "old.cube", "name": "old"}}});
    assert_eq!(
        resolve_color_luts(&metadata),
        vec![lut("old.cube").with_name("old")]
    );
    // The next write upgrades it to the list form.
    assert!(push_color_lut(&mut metadata, lut("new.cube")));
    assert_eq!(
        metadata["tellers.ai"]["color_grading"],
        json!([{"cube": "old.cube", "name": "old"}, {"cube": "new.cube"}])
    );
}

#[test]
fn insert_replace_remove_by_index() {
    let mut metadata = json!({});
    assert!(push_color_lut(&mut metadata, lut("b.cube")));
    assert!(insert_color_lut_at(&mut metadata, 0, lut("a.cube")));
    assert!(insert_color_lut_at(&mut metadata, 2, lut("c.cube")));
    assert!(!insert_color_lut_at(&mut metadata, 4, lut("x.cube")));
    assert_eq!(
        urls(&resolve_color_luts(&metadata)),
        ["a.cube", "b.cube", "c.cube"]
    );

    assert_eq!(
        replace_color_lut_at(&mut metadata, 1, lut("B.cube")),
        Some(lut("b.cube"))
    );
    assert_eq!(replace_color_lut_at(&mut metadata, 3, lut("x.cube")), None);

    assert_eq!(remove_color_lut_at(&mut metadata, 0), Some(lut("a.cube")));
    assert_eq!(remove_color_lut_at(&mut metadata, 5), None);
    assert_eq!(urls(&resolve_color_luts(&metadata)), ["B.cube", "c.cube"]);
}

#[test]
fn indices_skip_unusable_entries_and_writes_drop_them() {
    let mut metadata = json!({"tellers.ai": {"color_grading": [
        {"cube": ""}, {"cube": "a.cube", "intensity": 0.5}, {"cube": "b.cube"}
    ]}});
    assert_eq!(remove_color_lut_at(&mut metadata, 1), Some(lut("b.cube")));
    // The unusable entry is gone; untouched entries keep their other keys.
    assert_eq!(
        metadata["tellers.ai"]["color_grading"],
        json!([{"cube": "a.cube", "intensity": 0.5}])
    );
}

#[test]
fn empty_urls_are_rejected_without_changes() {
    let mut metadata = json!({"tellers.ai": {"color_grading": [{"cube": "a.cube"}]}});
    let before = metadata.clone();
    assert!(!push_color_lut(&mut metadata, lut("  ")));
    assert!(!insert_color_lut_at(&mut metadata, 0, lut("")));
    assert_eq!(replace_color_lut_at(&mut metadata, 0, lut(" ")), None);
    assert!(!set_color_luts(
        &mut metadata,
        vec![lut("ok.cube"), lut("")]
    ));
    assert_eq!(metadata, before);
}

#[test]
fn removing_the_last_lut_drops_the_key() {
    let mut metadata = json!({"tellers.ai": {"timeline_id": "x"}});
    assert!(!clear_color_luts(&mut metadata));
    push_color_lut(&mut metadata, lut("a.cube"));
    assert_eq!(remove_color_lut_at(&mut metadata, 0), Some(lut("a.cube")));
    assert_eq!(metadata, json!({"tellers.ai": {"timeline_id": "x"}}));

    push_color_lut(&mut metadata, lut("a.cube"));
    assert!(set_color_luts(&mut metadata, vec![]));
    assert_eq!(metadata, json!({"tellers.ai": {"timeline_id": "x"}}));
}

#[test]
fn timeline_stage_round_trips_through_json() {
    let mut tl = Timeline::default();
    assert!(tl.get_color_luts().is_empty());
    assert!(tl.set_color_luts(vec![lut("look.cube").with_name("look"), lut("grain.cube")]));

    let reparsed: Timeline =
        serde_json::from_str(&tl.to_json_with_options(None, false).unwrap()).unwrap();
    assert_eq!(
        reparsed.get_color_luts(),
        vec![lut("look.cube").with_name("look"), lut("grain.cube")]
    );

    assert!(tl.clear_color_luts());
    assert!(tl.get_color_luts().is_empty());
}

#[test]
fn timeline_edits_clips_by_id() {
    let mut tl = timeline_with_clip("c1");
    assert!(tl.push_item_color_lut("c1", lut("warm.cube")));
    assert!(tl.insert_item_color_lut_at("c1", 0, lut("log.cube")));
    assert_eq!(
        tl.get_item_color_luts("c1"),
        Some(vec![lut("log.cube"), lut("warm.cube")])
    );
    assert_eq!(
        tl.replace_item_color_lut_at("c1", 1, lut("cool.cube")),
        Some(lut("warm.cube"))
    );
    assert_eq!(tl.remove_item_color_lut_at("c1", 0), Some(lut("log.cube")));

    assert!(!tl.push_item_color_lut("missing", lut("x.cube")));
    assert!(!tl.push_item_color_lut("gap", lut("x.cube")));
    assert_eq!(tl.get_item_color_luts("gap"), None);
    assert_eq!(tl.get_item_color_luts("missing"), None);

    assert!(tl.set_item_color_luts("c1", vec![lut("a.cube"), lut("b.cube")]));
    assert!(tl.clear_item_color_luts("c1"));
    assert_eq!(tl.get_item_color_luts("c1"), Some(vec![]));
}

#[test]
fn color_lut_urls_lists_every_url_once() {
    let mut tl = timeline_with_clip("c1");
    tl.tracks.children[0].items.push(Item::Clip(clip("c2")));
    tl.push_color_lut(lut("look.cube"));
    tl.set_item_color_luts("c1", vec![lut("log.cube"), lut("shot.cube")]);
    tl.push_item_color_lut("c2", lut("log.cube"));

    assert_eq!(tl.color_lut_urls(), ["look.cube", "log.cube", "shot.cube"]);
}

#[test]
fn tracks_are_not_a_grading_stage() {
    let mut tl = timeline_with_clip("c1");
    push_color_lut(&mut tl.tracks.children[0].metadata, lut("track.cube"));
    assert!(tl.color_lut_urls().is_empty());
}

#[test]
fn item_delegates_to_its_clip_and_ignores_gaps() {
    let mut item = Item::Clip(clip("c1"));
    assert!(item.push_color_lut(lut("clip.cube")));
    assert_eq!(item.get_color_luts(), vec![lut("clip.cube")]);
    assert_eq!(item.remove_color_lut_at(0), Some(lut("clip.cube")));

    let mut gap = Item::Gap(Gap::make_gap(1.0));
    assert!(!gap.push_color_lut(lut("clip.cube")));
    assert!(gap.get_color_luts().is_empty());
    assert_eq!(gap.remove_color_lut_at(0), None);
}
