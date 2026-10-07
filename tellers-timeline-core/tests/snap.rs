// Tests for `Stack::find_snap`: the clip-edge snapping query the editor runs
// while dragging or resizing a clip.

use tellers_timeline_core::{
    Clip, Gap, Item, MediaReference, RationalTime, SnapEdge, SnapTarget, Stack, TimeRange, Track,
    TrackKind,
};

fn range(duration: f64) -> TimeRange {
    TimeRange {
        otio_schema: "TimeRange.1".to_string(),
        start_time: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: 1.0,
            value: 0.0,
        },
        duration: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: 1.0,
            value: duration,
        },
    }
}

fn clip(duration: f64, id: &str) -> Item {
    Item::Clip(Clip::new_single_media_reference(
        range(duration),
        MediaReference::ExternalReference {
            target_url: "file:///video.mov".to_string(),
            available_range: Some(range(100.0)),
            name: None,
            available_image_bounds: Some(serde_json::Value::Null),
            metadata: serde_json::json!({}),
        },
        None,
        Some(id.to_string()),
    ))
}

fn gap(duration: f64) -> Item {
    Item::Gap(Gap::make_gap(duration))
}

fn track(kind: TrackKind, id: &str, items: Vec<Item>) -> Track {
    let mut track = Track::new(kind, Some(id.to_string()));
    track.items = items;
    track
}

/// v1: [a 0-4][gap 4-6][b 6-10]   v2: [gap 0-3][c 3-5]
fn stack() -> Stack {
    Stack {
        children: vec![
            track(
                TrackKind::Video,
                "v1",
                vec![clip(4.0, "a"), gap(2.0), clip(4.0, "b")],
            ),
            track(TrackKind::Video, "v2", vec![gap(3.0), clip(2.0, "c")]),
        ],
        ..Stack::default()
    }
}

fn item_edge(target: &SnapTarget) -> (Option<&str>, SnapEdge) {
    match target {
        SnapTarget::ItemEdge { item_id, edge, .. } => (item_id.as_deref(), *edge),
        SnapTarget::Point { .. } => panic!("expected an item edge, got {target:?}"),
    }
}

#[test]
fn snaps_start_to_nearest_clip_end() {
    let found = stack().find_snap(4.2, 1.0, 0.3, &[], &[]).unwrap();
    assert_eq!(item_edge(&found.target), (Some("a"), SnapEdge::End));
    assert_eq!(found.moving_edge, SnapEdge::Start);
    assert!((found.target_time - 4.0).abs() < 1e-9);
    assert!((found.delta + 0.2).abs() < 1e-9);
}

#[test]
fn snaps_end_to_clip_start_on_another_track() {
    // Range 1.1..2.9: its end is 0.1 from c's start (3), its start is far from 0.
    let found = stack().find_snap(1.1, 1.8, 0.3, &[], &[]).unwrap();
    assert_eq!(item_edge(&found.target), (Some("c"), SnapEdge::Start));
    assert_eq!(found.moving_edge, SnapEdge::End);
    match found.target {
        SnapTarget::ItemEdge {
            track_id,
            track_index,
            ..
        } => {
            assert_eq!(track_id.as_deref(), Some("v2"));
            assert_eq!(track_index, 1);
        }
        _ => unreachable!(),
    }
    assert!((found.delta - 0.1).abs() < 1e-9);
}

#[test]
fn picks_the_closest_edge() {
    // Start 5.85: c's end (5) is 0.85 away, b's start (6) is 0.15 away.
    let found = stack().find_snap(5.85, 0.5, 1.0, &[], &[]).unwrap();
    assert_eq!(item_edge(&found.target), (Some("b"), SnapEdge::Start));
}

#[test]
fn nothing_outside_tolerance() {
    assert!(stack().find_snap(5.5, 0.1, 0.2, &[], &[]).is_none());
}

#[test]
fn disabled_without_positive_tolerance() {
    assert!(stack().find_snap(4.0, 1.0, 0.0, &[], &[]).is_none());
    assert!(stack().find_snap(4.0, 1.0, -1.0, &[], &[]).is_none());
}

#[test]
fn gaps_are_not_targets() {
    // The gap on v2 ends at 3 where c starts: only c is reported.
    let found = stack().find_snap(3.1, 0.5, 0.2, &[], &[]).unwrap();
    assert_eq!(item_edge(&found.target), (Some("c"), SnapEdge::Start));
}

#[test]
fn moving_clip_is_not_a_target_of_itself() {
    // Dragging b from 6 to 6.1: without exclusion it would snap to its own old start.
    let found = stack().find_snap(6.1, 4.0, 0.3, &["b".to_string()], &[]);
    assert!(found.is_none(), "{found:?}");
}

#[test]
fn sync_partners_and_group_members_are_excluded() {
    let mut stack = stack();
    let a_audio = clip(4.0, "a-audio");
    stack
        .children
        .push(track(TrackKind::Audio, "a1", vec![a_audio]));
    stack
        .sync_item(&["a".to_string(), "a-audio".to_string()])
        .unwrap();
    stack
        .group_item(&["b".to_string(), "c".to_string()])
        .unwrap();

    // a's and a-audio's end (4) would be the hit without exclusion.
    assert!(stack
        .find_snap(4.1, 0.5, 0.2, &["a".to_string()], &[])
        .is_none());
    assert!(stack.find_snap(4.1, 0.5, 0.2, &[], &[]).is_some());
    // c (grouped with b) ends at 5: excluded while dragging b.
    assert!(stack
        .find_snap(5.1, 0.5, 0.2, &["b".to_string()], &[])
        .is_none());
    assert!(stack.find_snap(5.1, 0.5, 0.2, &[], &[]).is_some());
}

#[test]
fn single_edge_for_resize() {
    let found = stack().find_snap(9.9, 0.0, 0.2, &[], &[]).unwrap();
    assert_eq!(item_edge(&found.target), (Some("b"), SnapEdge::End));
    assert_eq!(found.moving_edge, SnapEdge::Start);
}

#[test]
fn extra_points_snap_too() {
    // Playhead at 7.5, closer than any clip edge.
    let found = stack().find_snap(7.4, 1.0, 0.3, &[], &[20.0, 7.5]).unwrap();
    assert_eq!(found.target, SnapTarget::Point { index: 1 });
    assert!((found.target_time - 7.5).abs() < 1e-9);
}

#[test]
fn never_snaps_before_zero() {
    // End 0.1 is near a's start (0), but snapping it would start the range at -0.9.
    assert!(stack().find_snap(-0.9, 1.0, 0.2, &[], &[]).is_none());
}
