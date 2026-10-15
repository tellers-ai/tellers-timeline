//! Unit tests for `Stack::partner_move_target_allowed`: where the selected
//! member of a sync column may be moved, per track kind, group and policy.

use super::*;
use crate::{Clip, MediaReference, RationalTime, TimeRange};

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

fn clip(duration: f64, id: &str, sync_clips_id: Option<i64>) -> Item {
    let reference = MediaReference::ExternalReference {
        target_url: format!("file:///{id}.mov"),
        available_range: None,
        name: None,
        available_image_bounds: None,
        metadata: serde_json::json!({}),
    };
    let mut clip =
        Clip::new_single_media_reference(range(duration), reference, None, Some(id.to_string()));
    if let Some(sync_clips_id) = sync_clips_id {
        set_resolve_sync_clips_id(&mut clip.metadata, sync_clips_id);
    }
    Item::Clip(clip)
}

fn gap(duration: f64) -> Item {
    Item::Gap(Gap::make_gap(duration))
}

fn track(kind: TrackKind, id: &str, items: Vec<Item>) -> Track {
    let mut track = Track::new(kind, Some(id.to_string()));
    track.items = items;
    track
}

/// Index: 0 a2, 1 a1, 2 v1 (video group: a1, a2), 3 m2, 4 m1 (audio group),
/// 5 vo (free audio track).
///
/// v1: gap[0-5] - B[5-7] (sync 5) - C[7-9] (sync 2)
/// a1: gap[0-7] - C audio 1[7-9] (sync 2)
/// a2: gap[0-5] - B audio[5-7] (sync 5) - C audio 2[7-9] (sync 2)
/// m1: M[0-2] (sync 9); m2: M stem[0-2] (sync 9); vo: empty
fn stack() -> Stack {
    let mut stack = Stack::default();
    stack.children = vec![
        track(
            TrackKind::Audio,
            "a2",
            vec![gap(5.0), clip(2.0, "B audio", Some(5)), clip(2.0, "C audio 2", Some(2))],
        ),
        track(TrackKind::Audio, "a1", vec![gap(7.0), clip(2.0, "C audio 1", Some(2))]),
        track(
            TrackKind::Video,
            "v1",
            vec![gap(5.0), clip(2.0, "B", Some(5)), clip(2.0, "C", Some(2))],
        ),
        track(TrackKind::Audio, "m2", vec![clip(2.0, "M stem", Some(9))]),
        track(TrackKind::Audio, "m1", vec![clip(2.0, "M", Some(9))]),
        track(TrackKind::Audio, "vo", vec![]),
    ];
    stack
}

fn track_index(stack: &Stack, id: &str) -> usize {
    stack.get_track_by_id(id).unwrap().0
}

/// Whether moving `item_id` onto `dest` over `[start, start + 2]` is allowed.
fn allowed(stack: &Stack, item_id: &str, dest: &str, start: f64, policy: OverlapPolicy) -> bool {
    let column = stack.synced_move_items(item_id).expect("a sync column");
    let selected = column.iter().find(|item| item.is_selected).unwrap();
    let column_has_video = column.iter().any(|item| item.track_kind == TrackKind::Video);
    stack.partner_move_target_allowed(
        selected,
        column_has_video,
        track_index(stack, dest),
        start,
        start + 2.0,
        policy,
    )
}

const BOTH: [OverlapPolicy; 2] = [OverlapPolicy::Push, OverlapPolicy::Override];

#[test]
fn the_fixture_has_the_expected_groups() {
    let stack = stack();
    assert_eq!(stack.group_owner_of(track_index(&stack, "a1")), track_index(&stack, "v1"));
    assert_eq!(stack.group_owner_of(track_index(&stack, "a2")), track_index(&stack, "v1"));
    assert_eq!(
        stack.group_owner_of(track_index(&stack, "m2")),
        stack.group_owner_of(track_index(&stack, "m1"))
    );
    assert!(stack.track_in_video_group(track_index(&stack, "a1")));
    assert!(!stack.track_in_video_group(track_index(&stack, "vo")));
}

#[test]
fn a_video_clip_only_moves_onto_video_tracks() {
    let stack = stack();
    for policy in BOTH {
        assert!(allowed(&stack, "B", "v1", 0.0, policy));
        for audio in ["a1", "a2", "m1", "vo"] {
            assert!(!allowed(&stack, "B", audio, 5.0, policy), "B -> {audio} ({policy:?})");
        }
    }
}

#[test]
fn an_audio_partner_never_moves_onto_a_video_track() {
    let stack = stack();
    for policy in BOTH {
        assert!(!allowed(&stack, "B audio", "v1", 5.0, policy));
        assert!(!allowed(&stack, "M", "v1", 5.0, policy));
    }
}

#[test]
fn an_audio_partner_moves_freely_in_time_on_its_own_track() {
    let stack = stack();
    for policy in BOTH {
        for start in [0.0, 6.0, 7.0, 20.0] {
            assert!(allowed(&stack, "B audio", "a2", start, policy), "{start} ({policy:?})");
        }
    }
}

#[test]
fn an_audio_partner_moves_onto_an_empty_spot_of_a_sibling_track() {
    let stack = stack();
    for policy in BOTH {
        assert!(allowed(&stack, "B audio", "a1", 5.0, policy), "{policy:?}");
        assert!(allowed(&stack, "B audio", "a1", 0.0, policy), "{policy:?}");
        assert!(allowed(&stack, "B audio", "a1", 9.0, policy), "{policy:?}");
    }
}

#[test]
fn an_audio_partner_swaps_with_its_own_column_on_a_sibling_track() {
    // C audio 2 onto a1 at 7 lands on C audio 1: the same column, a swap.
    let stack = stack();
    for policy in BOTH {
        assert!(allowed(&stack, "C audio 2", "a1", 7.0, policy), "{policy:?}");
    }
}

#[test]
fn landing_on_another_columns_audio_needs_push() {
    // B audio onto a1 at 7 (or overlapping it from 6) lands on C audio 1:
    // Push moves C's column out of the way, Override would overwrite it.
    let stack = stack();
    for start in [6.0, 7.0, 8.0] {
        assert!(allowed(&stack, "B audio", "a1", start, OverlapPolicy::Push), "{start}");
        assert!(!allowed(&stack, "B audio", "a1", start, OverlapPolicy::Override), "{start}");
    }
}

#[test]
fn an_audio_partner_of_a_video_column_never_leaves_its_group() {
    // vo and m1 belong to other groups: refused even with Push and room.
    let stack = stack();
    for policy in BOTH {
        for dest in ["vo", "m1", "m2"] {
            assert!(!allowed(&stack, "B audio", dest, 20.0, policy), "-> {dest} ({policy:?})");
        }
    }
}

#[test]
fn an_audio_only_column_moves_between_audio_groups_but_not_into_a_video_group() {
    let stack = stack();
    for policy in BOTH {
        assert!(allowed(&stack, "M", "vo", 0.0, policy), "-> vo ({policy:?})");
        assert!(allowed(&stack, "M", "m2", 4.0, policy), "-> m2 ({policy:?})");
        for dest in ["a1", "a2"] {
            assert!(!allowed(&stack, "M", dest, 20.0, policy), "-> {dest} ({policy:?})");
        }
    }
}
