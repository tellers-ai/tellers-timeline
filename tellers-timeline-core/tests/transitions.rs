// OTIO `Transition.1` items: parse, preserve, round-trip, and edit semantics.
//
// A transition sits between two track items and takes no track time; its
// `in_offset` / `out_offset` say how far it overlaps each neighbour. Before
// this support, a Resolve export containing one failed to parse entirely
// (it was routed to `Clip::deserialize`, which requires `source_range`).

use tellers_timeline_core::{
    Clip, IdMetadataExt, Item, MediaReference, Stack, TimeRange, Timeline, Track, TrackKind,
    Transition,
};

const RESOLVE_EXPORT: &str = include_str!("fixtures/resolve_transitions.otio");

const TRANSITION_JSON: &str = r#"{
    "OTIO_SCHEMA": "Transition.1",
    "metadata": {
        "Resolve_OTIO": { "Transition Type": "Fusion Transition" },
        "tellers.ai": { "timeline_id": "t-1" }
    },
    "name": "Slide Up",
    "in_offset": { "OTIO_SCHEMA": "RationalTime.1", "rate": 30.0, "value": 0.0 },
    "out_offset": { "OTIO_SCHEMA": "RationalTime.1", "rate": 30.0, "value": 30.0 },
    "transition_type": "Custom_Transition"
}"#;

fn clip(duration: f64, id: &str) -> Item {
    Item::Clip(Clip::new_single_media_reference(
        TimeRange::new(duration, 0.0),
        MediaReference::ExternalReference {
            target_url: "file:///media.mov".to_string(),
            available_range: None,
            name: None,
            available_image_bounds: None,
            metadata: serde_json::json!({}),
        },
        None,
        Some(id.to_string()),
    ))
}

fn transition(id: &str) -> Item {
    Item::Transition(Transition::new(0.5, 0.5, None, Some(id.to_string())))
}

fn track_with(items: Vec<Item>) -> Stack {
    let mut track = Track::new(TrackKind::Video, Some("v1".to_string()));
    track.items = items;
    Stack {
        children: vec![track],
        ..Stack::default()
    }
}

fn ids(stack: &Stack) -> Vec<String> {
    stack.children[0]
        .items
        .iter()
        .map(|it| it.get_id().unwrap_or_default())
        .collect()
}

#[test]
fn transition_schema_deserializes_as_transition() {
    let item: Item = serde_json::from_str(TRANSITION_JSON).expect("transition parses");
    let Item::Transition(t) = &item else {
        panic!("Transition.1 must parse as Item::Transition");
    };
    assert_eq!(t.name.as_deref(), Some("Slide Up"));
    assert_eq!(t.transition_type, "Custom_Transition");
    assert_eq!(t.in_offset_seconds(), 0.0);
    assert_eq!(t.out_offset_seconds(), 1.0);
    assert_eq!(t.overlap_seconds(), 1.0);
    assert_eq!(item.get_id().as_deref(), Some("t-1"));
    // Zero track time, like every OTIO transition.
    assert_eq!(item.duration(), 0.0);
    assert!(item.is_transition());
}

#[test]
fn transition_round_trips_and_keeps_vendor_metadata() {
    let item: Item = serde_json::from_str(TRANSITION_JSON).unwrap();
    let json = serde_json::to_value(&item).unwrap();
    assert_eq!(json["OTIO_SCHEMA"], "Transition.1");
    assert_eq!(json["in_offset"]["rate"], 30.0);
    assert_eq!(json["out_offset"]["value"], 30.0);
    assert_eq!(
        json["metadata"]["Resolve_OTIO"]["Transition Type"],
        "Fusion Transition"
    );
    let reparsed: Item = serde_json::from_value(json).unwrap();
    assert_eq!(item, reparsed);
}

#[test]
fn transition_without_id_gets_one_on_sanitize() {
    let json = r#"{"OTIO_SCHEMA": "Transition.1", "transition_type": "SMPTE_Dissolve",
        "in_offset": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24, "value": 12},
        "out_offset": {"OTIO_SCHEMA": "RationalTime.1", "rate": 24, "value": 12}}"#;
    let item: Item = serde_json::from_str(json).unwrap();
    assert_eq!(item.duration(), 0.0);
    let mut stack = track_with(vec![clip(2.0, "a"), item, clip(2.0, "b")]);
    stack.sanitize();
    let ids = ids(&stack);
    assert_eq!(ids.len(), 3);
    assert!(!ids[1].is_empty());
}

#[test]
fn resolve_export_with_transitions_parses() {
    let timeline: Timeline = serde_json::from_str(RESOLVE_EXPORT).expect("Resolve export parses");
    let video2 = &timeline.tracks.children[1];
    assert_eq!(video2.items.len(), 3);
    assert!(matches!(video2.items[0], Item::Clip(_)));
    assert!(matches!(video2.items[1], Item::Transition(_)));
    assert!(matches!(video2.items[2], Item::Clip(_)));

    // The transition takes no track time: 51 + 75 frames at 30 fps.
    let expected = (51.0 + 75.0) / 30.0;
    assert!((video2.total_duration() - expected).abs() < 1e-9);
    assert!((video2.start_time_of_item(2) - 51.0 / 30.0).abs() < 1e-9);
    // The transition "starts" where the incoming clip starts.
    assert!((video2.start_time_of_item(1) - 51.0 / 30.0).abs() < 1e-9);

    let audio2 = &timeline.tracks.children[3];
    let Item::Transition(fade) = &audio2.items[1] else {
        panic!("audio cross fade must parse as a transition");
    };
    assert_eq!(fade.transition_type, "SMPTE_Dissolve");
    assert_eq!(fade.name.as_deref(), Some("Cross Fade 0 dB"));
}

#[test]
fn resolve_export_survives_sanitize_and_round_trip() {
    let mut timeline: Timeline = serde_json::from_str(RESOLVE_EXPORT).unwrap();
    timeline.sanitize();
    assert!(matches!(
        timeline.tracks.children[1].items[1],
        Item::Transition(_)
    ));
    let out = serde_json::to_string(&timeline).unwrap();
    let reparsed: Timeline = serde_json::from_str(&out).unwrap();
    assert_eq!(timeline, reparsed);
    assert!(matches!(
        reparsed.tracks.children[3].items[1],
        Item::Transition(_)
    ));
}

#[test]
fn sanitize_keeps_transitions_but_drops_adjacent_duplicates() {
    let mut stack = track_with(vec![
        clip(2.0, "a"),
        transition("t1"),
        transition("t2"),
        clip(2.0, "b"),
    ]);
    stack.sanitize();
    assert_eq!(ids(&stack), vec!["a", "t1", "b"]);
}

#[test]
fn sanitize_keeps_fade_at_track_edges() {
    // A transition at the head or tail of a track is a fade from/to black.
    let mut stack = track_with(vec![transition("in"), clip(2.0, "a"), transition("out")]);
    stack.sanitize();
    assert_eq!(ids(&stack), vec!["in", "a", "out"]);
}

#[test]
fn sanitize_drops_a_lone_transition() {
    let mut stack = track_with(vec![transition("t")]);
    stack.sanitize();
    assert!(stack.children[0].items.is_empty());
}

#[test]
fn deleting_a_clip_removes_its_transitions() {
    let mut stack = track_with(vec![
        clip(2.0, "a"),
        transition("t1"),
        clip(2.0, "b"),
        transition("t2"),
        clip(2.0, "c"),
    ]);
    let removed = stack.delete_item("b", false);
    assert_eq!(removed.len(), 1);
    assert_eq!(ids(&stack), vec!["a", "c"]);
    assert!((stack.children[0].total_duration() - 4.0).abs() < 1e-9);
}

#[test]
fn deleting_a_clip_with_gap_replacement_removes_its_transitions() {
    let mut stack = track_with(vec![
        clip(2.0, "a"),
        transition("t1"),
        clip(2.0, "b"),
        transition("t2"),
        clip(2.0, "c"),
    ]);
    stack.delete_item("b", true);
    let items = &stack.children[0].items;
    assert_eq!(items.len(), 3);
    assert!(matches!(items[0], Item::Clip(_)));
    assert!(matches!(items[1], Item::Gap(_)));
    assert!(matches!(items[2], Item::Clip(_)));
    assert!((stack.children[0].total_duration() - 6.0).abs() < 1e-9);
}

#[test]
fn a_transition_can_be_deleted_by_id() {
    let mut stack = track_with(vec![clip(2.0, "a"), transition("t1"), clip(2.0, "b")]);
    let removed = stack.delete_item("t1", false);
    assert_eq!(removed.len(), 1);
    assert!(matches!(removed[0].1, Item::Transition(_)));
    assert_eq!(ids(&stack), vec!["a", "b"]);
    assert!((stack.children[0].total_duration() - 4.0).abs() < 1e-9);
}

#[test]
fn splitting_a_neighbour_keeps_the_transition_at_the_cut() {
    let mut stack = track_with(vec![clip(2.0, "a"), transition("t1"), clip(2.0, "b")]);
    assert!(stack.split_item_at_time("b", 3.0));
    let items = &stack.children[0].items;
    assert_eq!(items.len(), 4);
    assert_eq!(items[0].get_id().as_deref(), Some("a"));
    assert!(items[1].is_transition());
    assert_eq!(items[2].get_id().as_deref(), Some("b"));
    assert!(matches!(items[3], Item::Clip(_)));
    assert!((stack.children[0].total_duration() - 4.0).abs() < 1e-9);
}

#[test]
fn a_transition_cannot_be_resized_or_split() {
    let mut stack = track_with(vec![clip(2.0, "a"), transition("t1"), clip(2.0, "b")]);
    assert!(!stack.split_item_at_time("t1", 2.0));
    let before = stack.clone();
    stack.resize_item("t1", 2.0, 1.0, tellers_timeline_core::OverlapPolicy::Push, false);
    assert_eq!(stack, before);
}

#[test]
fn transition_ids_are_made_unique_with_other_items() {
    let mut stack = track_with(vec![clip(2.0, "dup"), transition("dup"), clip(2.0, "b")]);
    stack.sanitize();
    let ids = ids(&stack);
    assert_eq!(ids[0], "dup");
    assert_ne!(ids[1], "dup");
    assert!(!ids[1].is_empty());
}
