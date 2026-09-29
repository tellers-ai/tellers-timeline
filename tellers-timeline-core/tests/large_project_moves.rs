//! Regression coverage for a realistic long-form edit: ten tracks, nine hours,
//! and 3,600 clips. Time range alone is cheap; item count is the relevant cost.

use std::collections::HashSet;

use tellers_timeline_core::{
    Clip, IdMetadataExt, InsertPolicy, Item, MediaReference, OverlapPolicy, RationalTime, Stack,
    TimeRange, Track, TrackKind,
};

const CLIP_DURATION: f64 = 90.0;
const CLIPS_PER_TRACK: usize = 360; // 9 hours at 90 seconds per clip.

fn range(duration: f64) -> TimeRange {
    TimeRange {
        otio_schema: "TimeRange.1".into(),
        start_time: RationalTime {
            otio_schema: "RationalTime.1".into(),
            rate: 1.0,
            value: 0.0,
        },
        duration: RationalTime {
            otio_schema: "RationalTime.1".into(),
            rate: 1.0,
            value: duration,
        },
    }
}

fn clip(id: &str) -> Item {
    Item::Clip(Clip::new_single_media_reference(
        range(CLIP_DURATION),
        MediaReference::ExternalReference {
            target_url: "file:///long-form-placeholder.mov".into(),
            available_range: Some(range(9.0 * 60.0 * 60.0)),
            name: None,
            available_image_bounds: None,
            metadata: serde_json::json!({}),
        },
        None,
        Some(id.into()),
    ))
}

fn long_form_stack() -> Stack {
    let mut stack = Stack::default();
    for track_number in 0..10 {
        let kind = if track_number % 2 == 0 {
            TrackKind::Video
        } else {
            TrackKind::Audio
        };
        let mut track = Track::new(kind, Some(format!("track-{track_number}")));
        for clip_number in 0..CLIPS_PER_TRACK {
            track.items.push(clip(&format!("{track_number}-{clip_number}")));
        }
        stack.children.push(track);
    }
    stack
}

fn start_of(stack: &Stack, id: &str) -> f64 {
    let (track, item, _) = stack.get_item(id).unwrap();
    stack.children[track].start_time_of_item(item)
}

fn track_of(stack: &Stack, id: &str) -> String {
    let (track, _, _) = stack.get_item(id).unwrap();
    stack.children[track].get_id().unwrap()
}

fn assert_valid_ids(stack: &Stack) {
    let mut ids = HashSet::new();
    for track in &stack.children {
        for item in &track.items {
            if let Some(id) = item.get_id() {
                assert!(ids.insert(id), "duplicate timeline id");
            }
        }
    }
}

#[test]
fn long_form_unsynced_move_keeps_ten_tracks_and_ids() {
    let mut stack = long_form_stack();
    let end = CLIP_DURATION * CLIPS_PER_TRACK as f64;
    assert!(stack.move_item_at_time(
        "2-100",
        "track-2",
        end,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));
    assert_eq!(stack.children.len(), 10);
    assert_eq!(track_of(&stack, "2-100"), "track-2");
    assert_eq!(start_of(&stack, "2-100"), end);
    assert_eq!(stack.get_item("2-100").unwrap().2.duration(), CLIP_DURATION);
    assert_valid_ids(&stack);
}

#[test]
fn long_form_synced_move_preserves_audio_video_channels() {
    let mut stack = long_form_stack();
    stack.sync_item(&["0-100".into(), "1-100".into()]).unwrap();
    let end = CLIP_DURATION * CLIPS_PER_TRACK as f64;
    for time in [end, end + CLIP_DURATION, end + CLIP_DURATION * 2.0] {
        assert!(stack.move_item_at_time(
            "0-100",
            "track-0",
            time,
            true,
            InsertPolicy::SplitAndInsert,
            OverlapPolicy::Override,
        ));
        assert_eq!(track_of(&stack, "0-100"), "track-0");
        assert_eq!(track_of(&stack, "1-100"), "track-1");
        assert_eq!(start_of(&stack, "0-100"), time);
        assert_eq!(start_of(&stack, "1-100"), time);
    }
    assert_eq!(stack.children.len(), 10);
    assert_valid_ids(&stack);
}

#[test]
fn long_form_grouped_moves_preserve_offsets_for_time_and_index_drags() {
    let mut stack = long_form_stack();
    stack.sync_item(&["0-100".into(), "1-100".into()]).unwrap();
    stack.sync_item(&["2-120".into(), "3-120".into()]).unwrap();
    stack.group_item(&["0-100".into(), "2-120".into()]).unwrap();

    let first_start = start_of(&stack, "0-100");
    let second_start = start_of(&stack, "2-120");
    let offset = second_start - first_start;
    let end = CLIP_DURATION * CLIPS_PER_TRACK as f64;
    for leave_gap in [true, false] {
        let mut moved = stack.clone();
        assert!(moved.move_item_at_time(
            "0-100",
            "track-0",
            end,
            leave_gap,
            InsertPolicy::SplitAndInsert,
            OverlapPolicy::Override,
        ));
        for (id, track) in [("0-100", "track-0"), ("1-100", "track-1"),
                            ("2-120", "track-2"), ("3-120", "track-3")] {
            assert_eq!(track_of(&moved, id), track);
            assert_eq!(moved.get_item(id).unwrap().2.duration(), CLIP_DURATION);
        }
        assert_eq!(start_of(&moved, "0-100"), end);
        assert_eq!(start_of(&moved, "1-100"), end);
        assert_eq!(start_of(&moved, "2-120"), end + offset);
        assert_eq!(start_of(&moved, "3-120"), end + offset);
        assert_eq!(moved.children.len(), 10);
        assert_valid_ids(&moved);
    }

    let mut indexed = stack.clone();
    assert!(indexed.move_item_at_index("0-100", "track-0", CLIPS_PER_TRACK, true, OverlapPolicy::Override));
    assert_eq!(track_of(&indexed, "0-100"), "track-0");
    assert_eq!(track_of(&indexed, "2-120"), "track-2");
    assert_valid_ids(&indexed);
}
