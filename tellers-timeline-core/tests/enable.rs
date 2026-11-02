//! Turning clips on and off, alone or with their sync partners.

mod common;
use common::*;

fn enabled(stack: &Stack, id: &str) -> bool {
    stack.get_item(id).unwrap().2.get_enabled()
}

fn start_of(stack: &Stack, id: &str) -> f64 {
    let (track_index, item_index, _) = stack.get_item(id).unwrap();
    stack.children[track_index].start_time_of_item(item_index)
}

/// A video and two audio channels in one sync column, after a free clip.
fn column() -> Stack {
    let mut stack = Stack::default();
    for (kind, name, items) in [
        (
            TrackKind::Audio,
            "a2",
            vec![
                Item::Gap(Gap::make_gap(2.0)),
                synced_clip_item(3.0, "aud2", 1),
            ],
        ),
        (
            TrackKind::Audio,
            "a1",
            vec![
                Item::Gap(Gap::make_gap(2.0)),
                synced_clip_item(3.0, "aud1", 1),
            ],
        ),
        (
            TrackKind::Video,
            "v",
            vec![
                Item::Clip(clip(2.0, Some("free"))),
                synced_clip_item(3.0, "vid", 1),
            ],
        ),
    ] {
        let mut track = Track::new(kind, Some(name.into()));
        track.items = items;
        stack.children.push(track);
    }
    stack
}

#[test]
fn disabling_with_sync_clips_turns_off_the_whole_column() {
    let mut stack = column();

    assert_eq!(stack.set_item_enabled("vid", false, true), 3);
    assert!(!enabled(&stack, "vid"));
    assert!(!enabled(&stack, "aud1"));
    assert!(!enabled(&stack, "aud2"));
    // Nothing moves: a disabled clip keeps its slot.
    assert_eq!(start_of(&stack, "vid"), 2.0);
    assert_eq!(start_of(&stack, "aud1"), 2.0);

    assert_eq!(stack.set_item_enabled("aud1", true, true), 3);
    assert!(enabled(&stack, "vid") && enabled(&stack, "aud1") && enabled(&stack, "aud2"));
}

#[test]
fn disabling_without_sync_clips_turns_off_that_clip_only() {
    let mut stack = column();

    assert_eq!(stack.set_item_enabled("aud1", false, false), 1);
    assert!(!enabled(&stack, "aud1"));
    assert!(enabled(&stack, "vid"));
    assert!(enabled(&stack, "aud2"));
}

#[test]
fn an_unsynced_clip_is_set_alone_and_gaps_are_refused() {
    let mut stack = column();

    assert_eq!(stack.set_item_enabled("free", false, true), 1);
    assert!(!enabled(&stack, "free"));
    assert!(enabled(&stack, "vid"));

    let gap_id = stack.children[0].items[0].get_id().unwrap();
    assert_eq!(stack.set_item_enabled(&gap_id, false, true), 0);
    assert_eq!(stack.set_item_enabled("missing", false, true), 0);
}

#[test]
fn a_disabled_clip_round_trips_through_otio() {
    let mut stack = column();
    stack.set_item_enabled("vid", false, true);

    let timeline = Timeline {
        tracks: stack,
        ..Timeline::default()
    };
    let json = tellers_timeline_core::to_json_with_precision(&timeline, None, false).unwrap();
    let back: Timeline = serde_json::from_str(&json).unwrap();

    assert!(!enabled(&back.tracks, "vid"));
    assert!(!enabled(&back.tracks, "aud2"));
    assert!(enabled(&back.tracks, "free"));
}
