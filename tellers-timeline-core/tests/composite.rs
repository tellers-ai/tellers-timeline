// A `Stack.*` inside a track is a composite clip (the player's
// `CompositeStack`): it parses as `Item::Composite`, keeps every key through a
// round trip, and lasts `source_range.duration` or, without a source range, as
// long as its longest child track.

use tellers_timeline_core::track_methods::track_item_insert::{InsertPolicy, OverlapPolicy};
use tellers_timeline_core::*;

const FIXTURE: &str = include_str!("fixtures/composite_clip.json");

fn timeline() -> Timeline {
    serde_json::from_str(FIXTURE).expect("timeline with composite clips parses")
}

fn composite<'a>(timeline: &'a Timeline, id: &str) -> &'a CompositeStack {
    match timeline.tracks.get_item(id) {
        Some((_, _, Item::Composite(stack))) => stack,
        other => panic!("{id} is not a composite: {other:?}"),
    }
}

/// Every id in the timeline, nested composite content included.
fn all_ids(timeline: &Timeline) -> Vec<String> {
    fn walk(tracks: &[Track], out: &mut Vec<String>) {
        for track in tracks {
            out.extend(track.get_id());
            for item in &track.items {
                out.extend(item.get_id());
                if let Item::Composite(stack) = item {
                    walk(&stack.children, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&timeline.tracks.children, &mut out);
    out
}

fn assert_unique(ids: &[String]) {
    let mut sorted = ids.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "duplicate ids in {ids:?}");
}

#[test]
fn nested_stack_parses_as_composite() {
    let timeline = timeline();
    let items = &timeline.tracks.children[0].items;
    assert!(matches!(items[0], Item::Clip(_)));
    assert!(matches!(items[1], Item::Composite(_)));
    assert!(matches!(items[2], Item::Gap(_)));
    assert!(matches!(items[3], Item::Composite(_)));

    let lower = composite(&timeline, "lower-third");
    assert_eq!(lower.name.as_deref(), Some("Lower third"));
    assert_eq!(lower.children.len(), 2);
    assert_eq!(lower.effects.len(), 1);

    // A composite nested in a composite's track is a composite too.
    let montage = composite(&timeline, "montage");
    assert!(matches!(montage.children[1].items[1], Item::Composite(_)));
    assert!(!items[3].get_enabled());
    assert!(items[1].get_enabled());
}

#[test]
fn composite_round_trips_every_key() {
    let timeline = timeline();
    let expected: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
    let written: serde_json::Value = serde_json::from_str(&timeline.to_json().unwrap()).unwrap();
    assert_eq!(written, expected);

    let reparsed: Timeline = serde_json::from_str(&timeline.to_json().unwrap()).unwrap();
    assert_eq!(reparsed, timeline);
}

#[test]
fn composite_duration_is_source_range_else_longest_track() {
    let timeline = timeline();
    let track = &timeline.tracks.children[0];
    // Source range set: 5s, whatever the tracks hold.
    assert_eq!(track.items[1].duration(), 5.0);
    // No source range: the longest child track (3s clip + 1s gap = 4s; the
    // nested track is 1.5s gap + 2s inner composite).
    assert_eq!(track.items[3].duration(), 4.0);
    assert_eq!(composite(&timeline, "montage").content_duration(), 4.0);
    // The inner composite has no source range either: its one 2s track.
    let Item::Composite(inner) = &composite(&timeline, "montage").children[1].items[1] else {
        panic!("inner composite");
    };
    assert_eq!(inner.duration(), 2.0);

    assert_eq!(track.total_duration(), 2.0 + 5.0 + 1.0 + 4.0);
    assert_eq!(track.start_time_of_item(3), 8.0);

    let implicit = track.items[3].get_source_range();
    assert_eq!(implicit.get_start_time(), 0.0);
    assert_eq!(implicit.get_duration(), 4.0);
}

#[test]
fn null_source_range_means_whole_content() {
    let json = r#"{
        "OTIO_SCHEMA": "Stack.1",
        "source_range": null,
        "children": [{
            "OTIO_SCHEMA": "Track.1",
            "kind": "Video",
            "children": [{
                "OTIO_SCHEMA": "Gap.1",
                "source_range": {
                    "OTIO_SCHEMA": "TimeRange.1",
                    "start_time": { "OTIO_SCHEMA": "RationalTime.1", "rate": 24, "value": 0 },
                    "duration": { "OTIO_SCHEMA": "RationalTime.1", "rate": 24, "value": 72 }
                }
            }]
        }]
    }"#;
    let item: Item = serde_json::from_str(json).expect("composite with null source_range");
    assert!(matches!(item, Item::Composite(_)));
    assert_eq!(item.duration(), 3.0);
}

#[test]
fn set_duration_adds_source_range_from_content_start() {
    let mut timeline = timeline();
    let item = &mut timeline.tracks.children[0].items[3];
    item.set_duration(2.5);
    let Item::Composite(stack) = item else {
        unreachable!()
    };
    let range = stack.source_range.as_ref().expect("source range added");
    assert_eq!(range.get_start_time(), 0.0);
    assert_eq!(range.get_duration(), 2.5);
    assert_eq!(item.duration(), 2.5);
}

#[test]
fn split_composite_keeps_content_and_gives_fresh_ids() {
    let mut timeline = timeline();
    // Lower third spans 2..7 on V1; split at 4.
    assert!(timeline.tracks.split_item_at_time("lower-third", 4.0));

    let items = &timeline.tracks.children[0].items;
    let (Item::Composite(left), Item::Composite(right)) = (&items[1], &items[2]) else {
        panic!("split halves are composites: {items:?}");
    };
    assert_eq!(left.get_id().as_deref(), Some("lower-third"));
    assert_ne!(right.get_id().as_deref(), Some("lower-third"));
    assert_eq!(left.source_start(), 0.0);
    assert_eq!(left.duration(), 2.0);
    assert_eq!(right.source_start(), 2.0);
    assert_eq!(right.duration(), 3.0);
    assert_eq!(right.children.len(), 2);
    assert_eq!(timeline.tracks.children[0].total_duration(), 12.0);
    assert_unique(&all_ids(&timeline));
}

#[test]
fn split_composite_without_source_range() {
    let mut timeline = timeline();
    // Montage spans 8..12; split at 9.
    assert!(timeline.tracks.split_item_at_time("montage", 9.0));
    let items = &timeline.tracks.children[0].items;
    assert_eq!(items[3].duration(), 1.0);
    assert_eq!(items[4].duration(), 3.0);
    let Item::Composite(right) = &items[4] else {
        panic!("right half")
    };
    assert_eq!(right.source_start(), 1.0);
    assert!(!items[4].get_enabled(), "split keeps enabled = false");
    assert_unique(&all_ids(&timeline));
}

#[test]
fn move_resize_and_delete_composite() {
    let mut timeline = timeline();
    let v1 = timeline.tracks.children[0].get_id().unwrap();

    // Move the lower third onto the gap after the montage's end.
    assert!(timeline.tracks.move_item_at_time(
        "lower-third",
        &v1,
        20.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));
    let (_, index, item) = timeline.tracks.get_item("lower-third").unwrap();
    assert!(matches!(item, Item::Composite(_)));
    assert_eq!(timeline.tracks.children[0].start_time_of_item(index), 20.0);
    assert_eq!(item.duration(), 5.0);

    // Trim it from the start: the source start advances like a clip's.
    assert!(timeline.tracks.resize_item_with_source_start(
        "lower-third",
        21.0,
        1.0,
        4.0,
        OverlapPolicy::Override,
        false,
    ));
    let (_, index, _) = timeline.tracks.get_item("lower-third").unwrap();
    assert_eq!(timeline.tracks.children[0].start_time_of_item(index), 21.0);
    let lower = composite(&timeline, "lower-third");
    assert_eq!(lower.source_start(), 1.0);
    assert_eq!(lower.duration(), 4.0);

    // Ripple delete pulls the rest of the track in.
    let removed = timeline.tracks.delete_item("montage", false, true);
    assert_eq!(removed.len(), 1);
    assert!(matches!(removed[0].1, Item::Composite(_)));
    assert!(timeline.tracks.get_item("montage").is_none());
    let (_, index, _) = timeline.tracks.get_item("lower-third").unwrap();
    assert_eq!(timeline.tracks.children[0].start_time_of_item(index), 17.0);
}

#[test]
fn insert_composite_built_in_code() {
    let mut timeline = timeline();
    let mut track = Track::new(TrackKind::Video, Some("inner-track".into()));
    track
        .items
        .push(Item::Gap(Gap::new(3.0, Some("inner-gap".into()))));
    let item = Item::Composite(CompositeStack::new(
        vec![track],
        Some("New".into()),
        Some("new".into()),
    ));
    assert_eq!(item.duration(), 3.0);

    let v1 = timeline.tracks.children[0].get_id().unwrap();
    assert!(timeline
        .tracks
        .insert_item_at_time_by_id(
            &v1,
            12.0,
            item,
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            None,
            None,
        )
        .is_some());
    assert_eq!(timeline.tracks.children[0].total_duration(), 15.0);
    assert!(matches!(
        timeline.tracks.get_item("new"),
        Some((_, _, Item::Composite(_)))
    ));
}

#[test]
fn sanitize_keeps_trailing_gaps_inside_composites() {
    let mut timeline = timeline();
    timeline.sanitize();
    // The montage's 1s trailing gap is part of its 4s length.
    assert_eq!(timeline.tracks.children[0].items[3].duration(), 4.0);
    assert_eq!(
        composite(&timeline, "montage").children[0].items.len(),
        2,
        "nested trailing gap kept"
    );
}

#[test]
fn sanitize_makes_nested_ids_unique() {
    let mut timeline = timeline();
    let copy = timeline.tracks.children[0].items[1].clone();
    // Same ids inside, new outer id: sanitize renames the nested duplicates.
    let mut copy = copy;
    copy.set_id(Some("copy".into()));
    timeline.tracks.children[0].items.push(copy);
    timeline.sanitize();
    assert_unique(&all_ids(&timeline));
    assert!(timeline.tracks.get_item("lower-third").is_some());
}

#[test]
fn clear_target_urls_reaches_nested_clips() {
    let mut timeline = timeline();
    timeline.clear_target_urls();
    let montage = composite(&timeline, "montage");
    let Item::Clip(clip) = &montage.children[0].items[0] else {
        panic!("nested clip")
    };
    assert!(clip.media_references["DEFAULT_MEDIA"]
        .target_url()
        .is_none_or(|url| url.is_empty()));
}

#[test]
fn markers_on_a_composite() {
    let mut timeline = timeline();
    let marker = Marker::new(3.0, 0.0, "beat", "", None, None, Some("m1".into()));
    // Timeline 3.0 is 1s into the lower third (which starts at 2.0).
    assert_eq!(
        timeline.add_marker(Some("lower-third"), marker).as_deref(),
        Some("m1")
    );
    let info = timeline.get_marker("m1").unwrap();
    assert_eq!(info.owner_kind, MarkerOwnerKind::Composite);
    assert_eq!(info.timeline_start, 3.0);
    assert_eq!(composite(&timeline, "lower-third").markers[0].start(), 1.0);
}
