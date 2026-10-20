// Resizing a clip with a speed: an edge drag moves the clip's edge on the
// timeline, and the source start moves by the media those timeline seconds
// play (video-player-js plays the integral of the speed from
// `source_range.start_time` over `source_range.duration`).

mod common;

use common::*;
use tellers_timeline_core::*;

const EPS: f64 = 1e-6;

/// 1x to 3x over the first 2 s, then 3x.
fn ramp_speed() -> Animatable {
    Animatable::from(vec![Keyframe::new(0.0, 1.0), Keyframe::new(2.0, 3.0)])
}

fn ramp() -> Effect {
    TellersEffect::new("TimeWarp")
        .with_parameter("speed", ramp_speed().to_value())
        .to_effect()
}

/// A track with a 2 s gap then `clip` (id "c") from 2 s.
fn stack_with(mut clip: Clip) -> Stack {
    clip.set_id(Some("c".to_string()));
    let mut track = Track::new(TrackKind::Video, Some("v".to_string()));
    track.items.push(Item::Gap(Gap::make_gap(2.0)));
    track.items.push(Item::Clip(clip));
    let mut stack = Stack::default();
    stack.children.push(track);
    stack
}

fn clip_at_speed(duration: f64, source_start: f64, speed: f64) -> Clip {
    let mut c = clip(duration, None);
    c.source_range.start_time.value = source_start;
    c.set_speed(speed, None).unwrap();
    c
}

fn clip_of(stack: &Stack, id: &str) -> Clip {
    match stack.get_item(id).unwrap().2 {
        Item::Clip(clip) => clip.clone(),
        other => panic!("expected a clip, got {other:?}"),
    }
}

fn start_of(stack: &Stack, id: &str) -> f64 {
    let (t, i, _) = stack.get_item(id).unwrap();
    stack.children[t].start_time_of_item(i)
}

fn assert_close(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < EPS,
        "{what}: {actual} != {expected}"
    );
}

/// Trim (positive) or extend (negative) the left edge of "c" by `delta`
/// timeline seconds, as the app does: source start and duration move 1:1.
fn drag_left_edge(stack: &mut Stack, delta: f64, clamp_to_media: bool) -> bool {
    let clip = clip_of(stack, "c");
    let source_start = clip.source_range.start_time.to_seconds();
    let duration = clip.source_range.duration.to_seconds();
    stack.modify_item(
        "c",
        source_start + delta,
        duration - delta,
        clamp_to_media,
        true,
        false,
    )
}

fn drag_right_edge(stack: &mut Stack, duration: f64, clamp_to_media: bool) -> bool {
    let source_start = clip_of(stack, "c").source_range.start_time.to_seconds();
    stack.modify_item("c", source_start, duration, clamp_to_media, false, false)
}

#[test]
fn left_trim_moves_the_source_by_what_the_trimmed_seconds_play() {
    for (speed, source_after) in [(2.0, 12.0), (0.5, 10.5), (1.0, 11.0)] {
        let mut stack = stack_with(clip_at_speed(4.0, 10.0, speed));
        assert!(drag_left_edge(&mut stack, 1.0, false));
        let c = clip_of(&stack, "c");
        assert_close(start_of(&stack, "c"), 3.0, "start");
        assert_close(c.source_range.duration.to_seconds(), 3.0, "duration");
        assert_close(
            c.source_range.start_time.to_seconds(),
            source_after,
            "source start",
        );
    }
}

#[test]
fn left_extension_moves_the_source_back_by_what_the_new_seconds_play() {
    for (speed, source_after) in [(2.0, 8.0), (0.5, 9.5)] {
        let mut stack = stack_with(clip_at_speed(4.0, 10.0, speed));
        assert!(drag_left_edge(&mut stack, -1.0, false));
        let c = clip_of(&stack, "c");
        assert_close(start_of(&stack, "c"), 1.0, "start");
        assert_close(c.source_range.duration.to_seconds(), 5.0, "duration");
        assert_close(
            c.source_range.start_time.to_seconds(),
            source_after,
            "source start",
        );
    }
}

#[test]
fn right_trim_and_extension_keep_the_source_start() {
    let mut stack = stack_with(clip_at_speed(4.0, 10.0, 2.0));
    assert!(drag_right_edge(&mut stack, 3.0, false));
    let c = clip_of(&stack, "c");
    assert_close(start_of(&stack, "c"), 2.0, "start");
    assert_close(c.source_range.start_time.to_seconds(), 10.0, "source start");
    assert_close(c.source_range.duration.to_seconds(), 3.0, "duration");
    assert_close(c.source_duration(), 6.0, "source played");

    let mut stack = stack_with(clip_at_speed(4.0, 10.0, 0.5));
    assert!(drag_right_edge(&mut stack, 6.0, false));
    let c = clip_of(&stack, "c");
    assert_close(c.source_range.start_time.to_seconds(), 10.0, "source start");
    assert_close(c.source_range.duration.to_seconds(), 6.0, "duration");
    assert_close(c.source_duration(), 3.0, "source played");
}

#[test]
fn a_freeze_keeps_its_frame_whichever_edge_moves() {
    let mut stack = stack_with(clip_at_speed(4.0, 3.0, 0.0));
    assert!(drag_left_edge(&mut stack, 1.0, false));
    assert_close(
        clip_of(&stack, "c").source_range.start_time.to_seconds(),
        3.0,
        "trimmed",
    );
    assert!(drag_left_edge(&mut stack, -2.0, false));
    let c = clip_of(&stack, "c");
    assert_close(start_of(&stack, "c"), 1.0, "start");
    assert_close(c.source_range.start_time.to_seconds(), 3.0, "extended");
    assert_close(c.source_range.duration.to_seconds(), 5.0, "duration");

    // A freeze plays no media, so the media start does not stop it.
    let mut frozen = clip_with_media_range(4.0, 0.5, 0.0, 10.0);
    frozen.set_speed(0.0, None).unwrap();
    let mut stack = stack_with(frozen);
    assert!(drag_left_edge(&mut stack, -2.0, true));
    let c = clip_of(&stack, "c");
    assert_close(start_of(&stack, "c"), 0.0, "start");
    assert_close(c.source_range.start_time.to_seconds(), 0.5, "source start");
    assert_close(c.source_range.duration.to_seconds(), 6.0, "duration");
}

#[test]
fn a_ramp_trims_by_its_integral_and_its_keyframes_follow_the_edge() {
    let mut c = clip(4.0, None);
    c.source_range.start_time.value = 10.0;
    c.effects = vec![ramp()];
    // The ramp plays 4 s of source in its first 2 s, then 3 s per second.
    assert_close(c.source_duration(), 10.0, "source played before");
    let mut stack = stack_with(c);

    assert!(drag_left_edge(&mut stack, 2.0, false));
    let c = clip_of(&stack, "c");
    assert_close(start_of(&stack, "c"), 4.0, "start");
    assert_close(c.source_range.start_time.to_seconds(), 14.0, "source start");
    // What is left plays as before: 3x from the new start.
    assert_close(c.source_duration(), 6.0, "source played after");
    let speed = c.get_speed().unwrap().speed;
    assert_eq!(
        speed,
        Animatable::from(vec![Keyframe::new(-2.0, 1.0), Keyframe::new(0.0, 3.0)])
    );

    // Extending back over the trimmed part replays the ramp: the clip is
    // back where it was.
    assert!(drag_left_edge(&mut stack, -2.0, false));
    let c = clip_of(&stack, "c");
    assert_close(start_of(&stack, "c"), 2.0, "start");
    assert_close(c.source_range.start_time.to_seconds(), 10.0, "source start");
    assert_close(c.source_duration(), 10.0, "source played");
    assert_eq!(c.get_speed().unwrap().speed, ramp_speed());
}

#[test]
fn a_ramp_trimmed_mid_segment_starts_from_the_source_shown_there() {
    let mut c = clip(4.0, None);
    c.source_range.start_time.value = 5.0;
    c.effects = vec![ramp()];
    let mut stack = stack_with(c.clone());
    assert!(drag_left_edge(&mut stack, 1.0, false));
    // ∫₀¹ (1 + t) dt = 1.5
    let trimmed = clip_of(&stack, "c");
    assert_close(
        trimmed.source_range.start_time.to_seconds(),
        6.5,
        "source start",
    );
    assert_close(trimmed.source_duration(), 10.0 - 1.5, "source played");

    // In front of its first keyframe a ramp holds its first speed (1x).
    let mut stack = stack_with(c);
    assert!(drag_left_edge(&mut stack, -1.0, false));
    let extended = clip_of(&stack, "c");
    assert_close(
        extended.source_range.start_time.to_seconds(),
        4.0,
        "source start",
    );
    assert_close(extended.source_duration(), 11.0, "source played");
    assert_eq!(
        extended.get_speed().unwrap().speed,
        Animatable::from(vec![Keyframe::new(1.0, 1.0), Keyframe::new(3.0, 3.0)])
    );
}

#[test]
fn left_extension_stops_at_the_start_of_the_media_it_plays() {
    // 2x from source 2: the edge can move back 1 s before reaching source 0.
    let mut stack = stack_with(clip_at_speed(4.0, 2.0, 2.0));
    assert!(drag_left_edge(&mut stack, -3.0, false));
    let c = clip_of(&stack, "c");
    assert_close(start_of(&stack, "c"), 1.0, "start");
    assert_close(c.source_range.start_time.to_seconds(), 0.0, "source start");
    assert_close(c.source_range.duration.to_seconds(), 5.0, "duration");

    // At half speed from source 1, 2 s back plays only 1 s of source: it fits.
    let mut stack = stack_with(clip_at_speed(4.0, 1.0, 0.5));
    assert!(drag_left_edge(&mut stack, -2.0, false));
    let c = clip_of(&stack, "c");
    assert_close(start_of(&stack, "c"), 0.0, "start");
    assert_close(c.source_range.start_time.to_seconds(), 0.0, "source start");
    assert_close(c.source_range.duration.to_seconds(), 6.0, "duration");

    // Clamping to the media stops at its available start.
    let mut fast = clip_with_media_range(4.0, 2.0, 1.0, 20.0);
    fast.set_speed(2.0, None).unwrap();
    let mut stack = stack_with(fast);
    assert!(drag_left_edge(&mut stack, -2.0, true));
    let c = clip_of(&stack, "c");
    assert_close(start_of(&stack, "c"), 1.5, "start");
    assert_close(c.source_range.start_time.to_seconds(), 1.0, "source start");
    assert_close(c.source_range.duration.to_seconds(), 4.5, "duration");
}

#[test]
fn right_extension_stops_at_the_end_of_the_media_it_plays() {
    // 12 s of media; at 2x from source 2 there is room for 5 s on the timeline.
    let mut fast = clip_with_media_range(4.0, 2.0, 0.0, 12.0);
    fast.set_speed(2.0, None).unwrap();
    let mut stack = stack_with(fast);
    assert!(drag_right_edge(&mut stack, 8.0, true));
    let c = clip_of(&stack, "c");
    assert_close(c.source_range.start_time.to_seconds(), 2.0, "source start");
    assert_close(c.source_range.duration.to_seconds(), 5.0, "duration");

    // After a 2x left trim the media left past the end counts at 2x too.
    let mut fast = clip_with_media_range(4.0, 2.0, 0.0, 12.0);
    fast.set_speed(2.0, None).unwrap();
    let mut stack = stack_with(fast);
    assert!(drag_left_edge(&mut stack, 1.0, true));
    assert!(drag_right_edge(&mut stack, 8.0, true));
    let c = clip_of(&stack, "c");
    assert_close(c.source_range.start_time.to_seconds(), 4.0, "source start");
    assert_close(c.source_range.duration.to_seconds(), 4.0, "duration");
}

#[test]
fn sync_partners_move_by_what_their_own_speed_plays() {
    // Both partners at 2x.
    let mut stack = two_synced_clips();
    assert_eq!(stack.set_item_speed("vA", 2.0, None, true), Ok(2));
    assert!(stack.modify_item("vA", 1.0, 3.0, false, true, false));
    for id in ["vA", "aA"] {
        let c = clip_of(&stack, id);
        assert_close(start_of(&stack, id), 1.0, id);
        assert_close(c.source_range.start_time.to_seconds(), 2.0, id);
        assert_close(c.source_range.duration.to_seconds(), 3.0, id);
    }

    // Only the video is sped up: the audio keeps its 1:1 trim.
    let mut stack = two_synced_clips();
    assert_eq!(stack.set_item_speed("vA", 2.0, None, false), Ok(1));
    assert!(stack.modify_item("vA", 1.0, 3.0, false, true, false));
    assert_close(
        clip_of(&stack, "vA").source_range.start_time.to_seconds(),
        2.0,
        "vA",
    );
    assert_close(
        clip_of(&stack, "aA").source_range.start_time.to_seconds(),
        1.0,
        "aA",
    );
    assert_close(start_of(&stack, "aA"), 1.0, "aA start");

    // Extending a synced pair to the left, with push (resize_item_with_source_start).
    let mut stack = two_synced_clips();
    assert!(stack.modify_item("vB", 2.0, 2.0, false, true, false));
    assert_eq!(stack.set_item_speed("vB", 0.5, None, true), Ok(2));
    assert!(stack.modify_item("vB", 1.0, 3.0, false, true, true));
    for id in ["vB", "aB"] {
        let c = clip_of(&stack, id);
        assert_close(c.source_range.start_time.to_seconds(), 1.5, id);
        assert_close(c.source_range.duration.to_seconds(), 3.0, id);
    }
    assert_close(
        start_of(&stack, "vB"),
        start_of(&stack, "aB"),
        "partners aligned",
    );
}
