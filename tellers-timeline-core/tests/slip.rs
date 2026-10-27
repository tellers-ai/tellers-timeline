// Slip: a clip picks which part of its media it plays while keeping its place
// and length on the timeline; the source it needs scales with its speed.

mod common;

use common::*;

/// A 4 s clip at timeline 2 s playing 10 s media from source `source_start`,
/// after a 2 s clip; with `synced`, an audio partner of the same media.
fn ranged_stack(source_start: f64, synced: bool) -> Stack {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(2.0, Some("before"))));
    let mut slipped = Item::Clip(clip_with_media_range(4.0, source_start, 0.0, 10.0));
    slipped.set_id(Some("c".to_string()));
    video.items.push(slipped);
    let mut stack = Stack::default();
    if synced {
        let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
        audio.items.push(Item::Gap(Gap::make_gap(2.0)));
        let mut partner = Item::Clip(clip_with_media_range(4.0, source_start, 0.0, 10.0));
        partner.set_id(Some("p".to_string()));
        audio.items.push(partner);
        stack.children.push(audio);
    }
    stack.children.push(video);
    if synced {
        stack
            .sync_item(&["c".to_string(), "p".to_string()])
            .unwrap();
    }
    stack
}

fn source_start(stack: &Stack, id: &str) -> f64 {
    match stack.get_item(id).unwrap().2 {
        Item::Clip(clip) => clip.source_range.start_time.to_seconds(),
        _ => panic!("{id} is not a clip"),
    }
}

fn timeline_range(stack: &Stack, id: &str) -> (f64, f64) {
    let (track_index, item_index, item) = stack.get_item(id).unwrap();
    (
        stack.children[track_index].start_time_of_item(item_index),
        item.duration(),
    )
}

#[test]
fn slip_moves_the_source_window_and_keeps_the_clip_in_place() {
    let mut stack = ranged_stack(1.0, false);
    assert_eq!(stack.item_slip_bounds("c", true), Some((0.0, 6.0)));
    assert_eq!(stack.slip_item("c", 3.5, true), 1);
    assert_eq!(source_start(&stack, "c"), 3.5);
    assert_eq!(timeline_range(&stack, "c"), (2.0, 4.0));
}

#[test]
fn slip_is_clamped_to_the_media() {
    let mut stack = ranged_stack(1.0, false);
    assert_eq!(stack.slip_item("c", 50.0, true), 1);
    assert_eq!(source_start(&stack, "c"), 6.0);
    assert_eq!(stack.slip_item("c", -3.0, true), 1);
    assert_eq!(source_start(&stack, "c"), 0.0);
    // Already there: nothing changes.
    assert_eq!(stack.slip_item("c", 0.0, true), 0);
    assert_eq!(stack.slip_item("missing", 1.0, true), 0);
    assert_eq!(stack.item_slip_bounds("missing", true), None);
}

#[test]
fn slip_bounds_count_the_source_the_speed_plays() {
    // 4 s at 2x plays 8 s of the 10 s media: 2 s of room.
    let mut stack = ranged_stack(0.0, false);
    stack.set_item_speed("c", 2.0, None, true).unwrap();
    assert_eq!(stack.item_slip_bounds("c", true), Some((0.0, 2.0)));
    assert_eq!(stack.slip_item("c", 5.0, true), 1);
    assert_eq!(source_start(&stack, "c"), 2.0);

    // At half speed it plays 2 s: 8 s of room.
    stack.set_item_speed("c", 0.5, None, true).unwrap();
    assert_eq!(stack.item_slip_bounds("c", true), Some((0.0, 8.0)));

    // Backwards plays the same window, so the bounds match the forward speed.
    stack.set_item_speed("c", -2.0, None, true).unwrap();
    stack.slip_item("c", 0.0, true);
    assert_eq!(stack.item_slip_bounds("c", true), Some((0.0, 2.0)));

    // A freeze plays a single frame: it can hold any of them.
    stack.set_item_speed("c", 0.0, None, true).unwrap();
    assert_eq!(stack.item_slip_bounds("c", true), Some((0.0, 10.0)));
    assert_eq!(timeline_range(&stack, "c"), (2.0, 4.0));
}

#[test]
fn slip_moves_the_sync_partners_by_the_same_amount() {
    let mut stack = ranged_stack(1.0, true);
    assert_eq!(stack.slip_item("c", 4.0, true), 2);
    assert_eq!(source_start(&stack, "c"), 4.0);
    assert_eq!(source_start(&stack, "p"), 4.0);
    assert_eq!(timeline_range(&stack, "p"), (2.0, 4.0));

    assert_eq!(stack.slip_item("c", 2.0, false), 1);
    assert_eq!(source_start(&stack, "c"), 2.0);
    assert_eq!(source_start(&stack, "p"), 4.0);
    // Together, the partner (2 s from its media end) limits the slip.
    assert_eq!(stack.item_slip_bounds("c", true), Some((0.0, 4.0)));
}

#[test]
fn a_clip_without_a_known_media_length_slips_freely_forward() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    let mut unranged = clip(4.0, Some("c"));
    for reference in unranged.media_references.values_mut() {
        *reference.available_range_mut() = None;
    }
    video.items.push(Item::Clip(unranged));
    let mut stack = Stack::default();
    stack.children.push(video);
    assert_eq!(
        stack.item_slip_bounds("c", true),
        Some((0.0, f64::INFINITY))
    );
    assert_eq!(stack.slip_item("c", 12.0, true), 1);
    assert_eq!(source_start(&stack, "c"), 12.0);
}
