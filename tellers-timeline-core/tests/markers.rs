mod common;

use common::*;
use tellers_timeline_core::{Marker, MarkerOwnerKind, MarkerUpdate};

/// One video track `t1`: clip `a` (0..10, source 0..10), clip `b` (10..20,
/// source 5..15).
fn timeline() -> Timeline {
    let mut tl = Timeline::default();
    let mut track = Track::new(TrackKind::Video, Some("t1".to_string()));
    track.items.push(Item::Clip(clip(10.0, Some("a"))));
    let mut b = clip(10.0, Some("b"));
    b.source_range.start_time.set_from_seconds(5.0);
    track.items.push(Item::Clip(b));
    tl.add_track(track);
    tl
}

fn marker(start: f64, id: &str) -> Marker {
    Marker::new(
        start,
        0.0,
        "note",
        "a comment",
        Some("BLUE".to_string()),
        Some("user-1".to_string()),
        Some(id.to_string()),
    )
}

#[test]
fn add_markers_on_stack_track_and_clip() {
    let mut tl = timeline();
    assert_eq!(tl.add_marker(None, marker(1.0, "m-stack")).as_deref(), Some("m-stack"));
    assert_eq!(tl.add_marker(Some("t1"), marker(2.0, "m-track")).as_deref(), Some("m-track"));
    assert_eq!(tl.add_marker(Some("b"), marker(12.0, "m-clip")).as_deref(), Some("m-clip"));
    assert_eq!(tl.add_marker(Some("missing"), marker(0.0, "m-x")), None);

    // Clip markers are stored in the clip's source time: 12 - 10 + 5 = 7.
    let Item::Clip(b) = &tl.tracks.children[0].items[1] else { panic!() };
    assert_eq!(b.markers.len(), 1);
    assert!((b.markers[0].start() - 7.0).abs() < 1e-9);

    let infos = tl.get_markers();
    assert_eq!(infos.len(), 3);
    let clip_info = tl.get_marker("m-clip").unwrap();
    assert_eq!(clip_info.owner_kind, MarkerOwnerKind::Clip);
    assert_eq!(clip_info.owner_id.as_deref(), Some("b"));
    assert_eq!(clip_info.track_id.as_deref(), Some("t1"));
    assert!((clip_info.timeline_start - 12.0).abs() < 1e-9);
    assert!(clip_info.visible);
    assert_eq!(clip_info.marker.get_created_by().as_deref(), Some("user-1"));
    assert_eq!(clip_info.marker.color, "BLUE");

    let track_info = tl.get_marker("m-track").unwrap();
    assert_eq!(track_info.owner_kind, MarkerOwnerKind::Track);
    assert_eq!(track_info.track_id.as_deref(), Some("t1"));
}

#[test]
fn duplicate_marker_id_is_refused() {
    let mut tl = timeline();
    assert!(tl.add_marker(None, marker(1.0, "m")).is_some());
    assert!(tl.add_marker(Some("a"), marker(1.0, "m")).is_none());
}

#[test]
fn clip_marker_follows_its_clip() {
    let mut tl = timeline();
    tl.add_marker(Some("b"), marker(12.0, "m")).unwrap();
    // Removing `a` (ripple) moves `b` to 0..10: the marker moves with it.
    assert!(!tl.tracks.delete_item("a", false).is_empty());
    let info = tl.get_marker("m").unwrap();
    assert!((info.timeline_start - 2.0).abs() < 1e-9, "{}", info.timeline_start);
}

#[test]
fn update_and_delete_marker() {
    let mut tl = timeline();
    tl.add_marker(Some("b"), marker(12.0, "m")).unwrap();
    assert!(tl.update_marker(
        "m",
        MarkerUpdate {
            name: Some("renamed".to_string()),
            comment: None,
            color: Some("GREEN".to_string()),
            timeline_start: Some(15.0),
            duration: Some(2.0),
        }
    ));
    let info = tl.get_marker("m").unwrap();
    assert_eq!(info.marker.name, "renamed");
    assert_eq!(info.marker.comment, "a comment");
    assert_eq!(info.marker.color, "GREEN");
    assert!((info.timeline_start - 15.0).abs() < 1e-9);
    assert!((info.duration - 2.0).abs() < 1e-9);

    assert!(!tl.update_marker("nope", MarkerUpdate::default()));
    assert!(tl.delete_marker("m").is_some());
    assert!(tl.get_marker("m").is_none());
    assert!(tl.delete_marker("m").is_none());
}

#[test]
fn trimmed_clip_hides_marker() {
    let mut tl = timeline();
    tl.add_marker(Some("a"), marker(8.0, "m")).unwrap();
    let Item::Clip(a) = &mut tl.tracks.children[0].items[0] else { panic!() };
    a.source_range.duration.set_from_seconds(5.0);
    assert!(!tl.get_marker("m").unwrap().visible);
}

#[test]
fn split_keeps_each_marker_on_its_half() {
    let mut tl = timeline();
    tl.add_marker(Some("b"), marker(11.0, "left")).unwrap();
    tl.add_marker(Some("b"), marker(17.0, "right")).unwrap();
    assert!(tl.tracks.split_item_at_time("b", 14.0));

    let left = tl.get_marker("left").unwrap();
    let right = tl.get_marker("right").unwrap();
    assert_eq!(left.owner_id.as_deref(), Some("b"));
    assert_ne!(right.owner_id.as_deref(), Some("b"));
    assert!((left.timeline_start - 11.0).abs() < 1e-9);
    assert!((right.timeline_start - 17.0).abs() < 1e-9);
    assert_eq!(tl.get_markers().len(), 2);
}

#[test]
fn markers_round_trip_through_otio_json() {
    let mut tl = timeline();
    tl.add_marker(None, marker(1.0, "m-stack")).unwrap();
    tl.add_marker(Some("t1"), marker(2.0, "m-track")).unwrap();
    tl.add_marker(Some("a"), marker(3.0, "m-clip")).unwrap();

    let json = tl.to_json().unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let stack_marker = &value["tracks"]["markers"][0];
    assert_eq!(stack_marker["OTIO_SCHEMA"], "Marker.2");
    assert_eq!(stack_marker["comment"], "a comment");
    assert_eq!(stack_marker["metadata"]["tellers.ai"]["created_by"], "user-1");

    let back: Timeline = serde_json::from_str(&json).unwrap();
    assert_eq!(back.get_markers(), tl.get_markers());
}

#[test]
fn timeline_without_markers_does_not_serialize_them() {
    let json = timeline().to_json().unwrap();
    assert!(!json.contains("\"markers\""));
}

#[test]
fn plain_otio_marker_gets_an_id_and_defaults() {
    let json = r#"{
        "OTIO_SCHEMA": "Timeline.1",
        "tracks": {
            "OTIO_SCHEMA": "Stack.1",
            "children": [],
            "markers": [{
                "OTIO_SCHEMA": "Marker.2",
                "name": "from resolve",
                "marked_range": {
                    "OTIO_SCHEMA": "TimeRange.1",
                    "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 25.0, "value": 50.0},
                    "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 25.0, "value": 0.0}
                }
            }]
        }
    }"#;
    let tl: Timeline = serde_json::from_str(json).unwrap();
    let infos = tl.get_markers();
    assert_eq!(infos.len(), 1);
    assert!(infos[0].marker.get_id().is_some());
    assert_eq!(infos[0].marker.color, "RED");
    assert!((infos[0].timeline_start - 2.0).abs() < 1e-9);
}
