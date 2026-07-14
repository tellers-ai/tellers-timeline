// `Item` dispatches on `OTIO_SCHEMA` when deserializing: a `Gap.*` object must
// land on `Item::Gap`, not be swallowed by the (untagged, listed-first) `Clip`
// variant. Without the custom deserializer every gap parses as an empty clip.

use tellers_timeline_core::{Item, Stack, Timeline};

const GAP_JSON: &str = r#"{
    "OTIO_SCHEMA": "Gap.1",
    "metadata": { "tellers.ai": { "timeline_id": "gap-1" } },
    "source_range": {
        "OTIO_SCHEMA": "TimeRange.1",
        "start_time": { "OTIO_SCHEMA": "RationalTime.1", "rate": 1, "value": 0 },
        "duration": { "OTIO_SCHEMA": "RationalTime.1", "rate": 1, "value": 3 }
    }
}"#;

const CLIP_JSON: &str = r#"{
    "OTIO_SCHEMA": "Clip.2",
    "metadata": { "tellers.ai": { "timeline_id": "clip-1" } },
    "source_range": {
        "OTIO_SCHEMA": "TimeRange.1",
        "start_time": { "OTIO_SCHEMA": "RationalTime.1", "rate": 1, "value": 0 },
        "duration": { "OTIO_SCHEMA": "RationalTime.1", "rate": 1, "value": 5 }
    }
}"#;

#[test]
fn gap_schema_deserializes_as_gap() {
    let item: Item = serde_json::from_str(GAP_JSON).expect("gap parses");
    assert!(matches!(item, Item::Gap(_)), "Gap.1 must parse as Item::Gap");
    assert_eq!(item.duration(), 3.0);
}

#[test]
fn clip_schema_still_deserializes_as_clip() {
    let item: Item = serde_json::from_str(CLIP_JSON).expect("clip parses");
    assert!(matches!(item, Item::Clip(_)));
}

#[test]
fn gap_round_trips_through_serialize() {
    let item: Item = serde_json::from_str(GAP_JSON).unwrap();
    let json = serde_json::to_string(&item).unwrap();
    let reparsed: Item = serde_json::from_str(&json).unwrap();
    assert!(matches!(reparsed, Item::Gap(_)));
    assert_eq!(item, reparsed);
}

#[test]
fn gaps_survive_a_timeline_round_trip() {
    // A gap embedded in a track keeps its Gap identity through parse + reserialize.
    let timeline_json = format!(
        r#"{{
            "OTIO_SCHEMA": "Timeline.1",
            "tracks": {{
                "OTIO_SCHEMA": "Stack.1",
                "children": [{{
                    "OTIO_SCHEMA": "Track.1",
                    "kind": "Video",
                    "children": [{CLIP_JSON}, {GAP_JSON}]
                }}]
            }}
        }}"#
    );
    let timeline: Timeline = serde_json::from_str(&timeline_json).expect("timeline parses");
    let track = &timeline.tracks.children[0];
    assert!(matches!(track.items[0], Item::Clip(_)));
    assert!(matches!(track.items[1], Item::Gap(_)));

    let out = serde_json::to_string(&timeline).unwrap();
    let reparsed: Timeline = serde_json::from_str(&out).unwrap();
    assert!(matches!(reparsed.tracks.children[0].items[1], Item::Gap(_)));

    // And a bare Stack round-trips its gaps too (the editor seeds from Stack JSON).
    let stack_json = serde_json::to_string(&timeline.tracks).unwrap();
    let stack: Stack = serde_json::from_str(&stack_json).unwrap();
    assert!(matches!(stack.children[0].items[1], Item::Gap(_)));
}
