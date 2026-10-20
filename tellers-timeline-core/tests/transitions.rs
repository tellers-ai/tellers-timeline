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
    let removed = stack.delete_item("b", false, true);
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
    stack.delete_item("b", true, true);
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
    let removed = stack.delete_item("t1", false, true);
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

#[test]
fn transitions_and_composite_clips_load_together() {
    // A transition between a clip and a composite clip, and one inside the
    // composite's own track: both item kinds parse, time and round-trip.
    let rt = |v: f64| serde_json::json!({ "OTIO_SCHEMA": "RationalTime.1", "rate": 1.0, "value": v });
    let range = |d: f64| serde_json::json!({ "OTIO_SCHEMA": "TimeRange.1", "start_time": rt(0.0), "duration": rt(d) });
    let media = serde_json::json!({ "DEFAULT_MEDIA": { "OTIO_SCHEMA": "ExternalReference.1", "target_url": "" } });
    let clip = |d: f64| serde_json::json!({ "OTIO_SCHEMA": "Clip.2", "source_range": range(d), "media_references": media });
    let transition = serde_json::json!({
        "OTIO_SCHEMA": "Transition.1", "transition_type": "SMPTE_Dissolve",
        "in_offset": rt(0.5), "out_offset": rt(0.5)
    });
    let timeline = serde_json::json!({
        "OTIO_SCHEMA": "Timeline.1",
        "tracks": { "OTIO_SCHEMA": "Stack.1", "children": [{
            "OTIO_SCHEMA": "Track.1", "kind": "video",
            "children": [
                clip(2.0),
                transition,
                { "OTIO_SCHEMA": "Stack.1", "children": [{
                    "OTIO_SCHEMA": "Track.1", "kind": "video",
                    "children": [clip(1.0), transition, clip(2.0)]
                }] }
            ]
        }] }
    });
    let mut parsed: Timeline = serde_json::from_str(&timeline.to_string()).unwrap();
    let items = &parsed.tracks.children[0].items;
    assert!(matches!(items[0], Item::Clip(_)));
    assert!(items[1].is_transition());
    let Item::Composite(composite) = &items[2] else {
        panic!("expected a composite clip");
    };
    assert!(composite.children[0].items[1].is_transition());
    // The composite lasts as long as its track (1 + 2); transitions add nothing.
    assert!((items[2].duration() - 3.0).abs() < 1e-9);
    assert!((parsed.tracks.children[0].total_duration() - 5.0).abs() < 1e-9);

    // Once written (sanitize gives every item, nested ones included, an id),
    // the timeline round-trips unchanged.
    parsed.sanitize();
    let round_trip = |timeline: &Timeline| -> Timeline {
        serde_json::from_str(&serde_json::to_string(timeline).unwrap()).unwrap()
    };
    let written = round_trip(&parsed);
    let reparsed = round_trip(&written);
    assert_eq!(reparsed, written);
    let Item::Composite(composite) = &reparsed.tracks.children[0].items[2] else {
        panic!("expected a composite clip after the round trip");
    };
    assert!(composite.children[0].items[1].is_transition());
}

#[test]
fn a_transition_holds_no_tellers_effects() {
    let stack = track_with(vec![clip(2.0, "a"), transition("t1"), clip(2.0, "b")]);
    let mut timeline = Timeline::default();
    timeline.tracks = stack;
    assert_eq!(timeline.get_item_tellers_effects("t1"), None);
    assert!(!timeline.push_item_tellers_effect(
        "t1",
        tellers_timeline_core::TellersEffect::new("Blur")
    ));
    assert!(timeline.push_item_tellers_effect(
        "a",
        tellers_timeline_core::TellersEffect::new("Blur")
    ));
}

// Typed kind, direction and colour, read like the player's `otioTransition`.

fn typed(transition_type: &str, parameters: serde_json::Value) -> Transition {
    serde_json::from_value(serde_json::json!({
        "OTIO_SCHEMA": "Transition.1",
        "transition_type": transition_type,
        "in_offset": { "OTIO_SCHEMA": "RationalTime.1", "rate": 30.0, "value": 15.0 },
        "out_offset": { "OTIO_SCHEMA": "RationalTime.1", "rate": 30.0, "value": 15.0 },
        "metadata": { "tellers.ai": { "timeline_id": "t-1", "parameters": parameters } }
    }))
    .unwrap()
}

#[test]
fn kind_reads_transition_type_like_the_player() {
    use tellers_timeline_core::TransitionKind::*;
    for (transition_type, kind) in [
        ("SMPTE_Dissolve", Dissolve),
        ("Custom_Transition", Dissolve),
        ("", Dissolve),
        ("Cross Dissolve", Dissolve),
        ("dip_to_color", DipToColor),
        ("DipToBlack", DipToColor),
        ("Dip To White", DipToColor),
        ("WIPE", Wipe),
        ("Push", Push),
        ("slide", Slide),
        ("Zoom", Zoom),
    ] {
        assert_eq!(typed(transition_type, serde_json::json!({})).kind(), kind, "{transition_type}");
    }
    assert_eq!(Transition::new(0.5, 0.5, None, None).kind(), Dissolve);
}

#[test]
fn set_kind_writes_the_player_name_and_keeps_an_equivalent_type() {
    use tellers_timeline_core::TransitionKind;
    let mut t = typed("Custom_Transition", serde_json::json!({}));
    t.set_kind(TransitionKind::Push);
    assert_eq!(t.transition_type, "Push");
    t.set_kind(TransitionKind::Dissolve);
    assert_eq!(t.transition_type, "SMPTE_Dissolve");

    let mut black = typed("DipToBlack", serde_json::json!({}));
    black.set_kind(TransitionKind::DipToColor);
    assert_eq!(black.transition_type, "DipToBlack");
    assert_eq!(TransitionKind::parse("Bogus"), None);
    assert_eq!(TransitionKind::parse("crossDissolve"), Some(TransitionKind::Dissolve));
}

#[test]
fn direction_defaults_to_left_and_round_trips() {
    use tellers_timeline_core::TransitionDirection;
    assert_eq!(typed("Wipe", serde_json::json!({})).direction(), TransitionDirection::Left);
    // The player only reads exact lowercase names.
    assert_eq!(typed("Wipe", serde_json::json!({ "direction": "Up" })).direction(), TransitionDirection::Left);
    assert_eq!(typed("Wipe", serde_json::json!({ "direction": "up" })).direction(), TransitionDirection::Up);

    let mut t = Transition::new(0.5, 0.5, Some("Slide".into()), Some("t-2".into()));
    t.set_direction(Some(TransitionDirection::Down));
    let json = serde_json::to_value(&t).unwrap();
    assert_eq!(json["metadata"]["tellers.ai"]["parameters"]["direction"], "down");
    assert_eq!(json["metadata"]["tellers.ai"]["timeline_id"], "t-2");
    let back: Transition = serde_json::from_value(json).unwrap();
    assert_eq!(back.direction(), TransitionDirection::Down);

    t.set_direction(None);
    assert!(t.metadata["tellers.ai"]["parameters"].get("direction").is_none());
    // Clearing a parameter that is not there adds nothing.
    let mut bare = Transition::new(0.5, 0.5, None, Some("t-3".into()));
    bare.set_direction(None);
    assert!(bare.metadata["tellers.ai"].get("parameters").is_none());
}

#[test]
fn color_follows_the_dip_name_then_parameters() {
    assert_eq!(typed("DipToColor", serde_json::json!({})).color(), "#000000");
    assert_eq!(typed("DipToColor", serde_json::json!({ "color": "  " })).color(), "#000000");
    assert_eq!(typed("DipToColor", serde_json::json!({ "color": " #ff0000 " })).color(), "#ff0000");
    assert_eq!(typed("DipToWhite", serde_json::json!({ "color": "#ff0000" })).color(), "#ffffff");
    assert_eq!(typed("DipToBlack", serde_json::json!({})).color(), "#000000");
}

#[test]
fn set_color_turns_a_named_dip_into_dip_to_color() {
    let mut t = typed("DipToWhite", serde_json::json!({}));
    t.set_color(Some("#00ff00"));
    assert_eq!(t.transition_type, "DipToColor");
    assert_eq!(t.color(), "#00ff00");

    t.set_color(None);
    assert_eq!(t.color(), "#000000");
    assert!(t.metadata["tellers.ai"]["parameters"].get("color").is_none());

    // A blank colour clears, and leaves a named dip as it is.
    let mut white = typed("DipToWhite", serde_json::json!({ "color": "#123456" }));
    white.set_color(Some(" "));
    assert_eq!(white.transition_type, "DipToWhite");
    assert!(white.metadata["tellers.ai"]["parameters"].get("color").is_none());
}

#[test]
fn typed_setters_repair_non_object_metadata() {
    let mut t = Transition::new(0.5, 0.5, Some("Push".into()), None);
    t.metadata = serde_json::json!({ "tellers.ai": "broken" });
    t.set_direction(Some(tellers_timeline_core::TransitionDirection::Right));
    assert_eq!(t.metadata["tellers.ai"]["parameters"]["direction"], "right");
}

#[test]
fn set_kind_named_writes_the_colour_a_dip_name_implies() {
    let mut t = typed("DipToBlack", serde_json::json!({}));
    assert!(t.set_kind_named("Dip To White"));
    assert_eq!(t.transition_type, "DipToColor");
    assert_eq!(t.color(), "#ffffff");

    let mut push = typed("Push", serde_json::json!({ "direction": "up" }));
    assert!(push.set_kind_named("cross dissolve"));
    assert_eq!(push.transition_type, "SMPTE_Dissolve");
    assert!(!push.set_kind_named("Spin"));
    assert_eq!(push.transition_type, "SMPTE_Dissolve");
}
