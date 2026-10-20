// Clip speed as video-player-js reads it (`clip-speed.ts`): OTIO's
// `LinearTimeWarp.1` and `FreezeFrame.1`, and a tellers `TimeWarp` effect,
// the first enabled one applying.

mod common;

use common::*;
use serde_json::json;
use tellers_timeline_core::*;

fn effect(value: serde_json::Value) -> Effect {
    serde_json::from_value(value).unwrap()
}

fn clip_with_effects(duration: f64, effects: Vec<Effect>) -> Clip {
    let mut clip = clip(duration, Some("c"));
    clip.effects = effects;
    clip
}

#[test]
fn reads_linear_time_warp_freeze_frame_and_time_warp_like_the_player() {
    let warp = clip_with_effects(
        4.0,
        vec![effect(json!({"OTIO_SCHEMA": "LinearTimeWarp.1", "time_scalar": 2.0}))],
    );
    let speed = warp.get_speed().unwrap();
    assert_eq!(speed.speed, Animatable::Value(2.0));
    assert!(speed.preserve_pitch);
    assert_eq!(warp.source_duration(), 8.0);

    let freeze = clip_with_effects(
        4.0,
        vec![effect(json!({"OTIO_SCHEMA": "FreezeFrame.1", "time_scalar": 0.0}))],
    );
    assert!(freeze.get_speed().unwrap().is_freeze());
    assert_eq!(freeze.source_duration(), 0.0);
    assert_eq!(freeze.timeline_duration_for_source(1.0), None);

    // A ramp from 1x to 3x over 2 s plays 4 s of source, then 3 s per second.
    let ramp = clip_with_effects(
        4.0,
        vec![TellersEffect::new("TimeWarp")
            .with_parameter(
                "speed",
                Animatable::from(vec![Keyframe::new(0.0, 1.0), Keyframe::new(2.0, 3.0)]).to_value(),
            )
            .with_parameter("preserve_pitch", false)
            .to_effect()],
    );
    let speed = ramp.get_speed().unwrap();
    assert!(!speed.preserve_pitch);
    assert!((ramp.source_offset(2.0) - 4.0).abs() < 1e-9);
    assert!((ramp.source_duration() - 10.0).abs() < 1e-9);
    assert!((ramp.timeline_duration_for_source(4.0).unwrap() - 2.0).abs() < 1e-6);

    assert_eq!(clip(4.0, None).get_speed(), None);
    assert_eq!(clip(4.0, None).source_duration(), 4.0);
}

#[test]
fn first_enabled_time_effect_wins_and_speed_is_clamped() {
    let clip = clip_with_effects(
        2.0,
        vec![
            effect(json!({"OTIO_SCHEMA": "FreezeFrame.1", "enabled": false})),
            effect(json!({"OTIO_SCHEMA": "LinearTimeWarp.1", "time_scalar": -1.0})),
            effect(json!({
                "OTIO_SCHEMA": "LinearTimeWarp.1",
                "time_scalar": 10.0,
                "metadata": {"tellers.ai": {"preserve_pitch": false}}
            })),
            effect(json!({"OTIO_SCHEMA": "LinearTimeWarp.1", "time_scalar": 0.5})),
        ],
    );
    let speed = clip.get_speed().unwrap();
    // Reverse is skipped; preserve_pitch beside `parameters` is read too.
    assert_eq!(speed.speed, Animatable::Value(10.0));
    assert!(!speed.preserve_pitch);
    assert_eq!(clip.source_duration(), 2.0 * MAX_CLIP_SPEED);
}

#[test]
fn set_speed_writes_otio_time_effects_and_replaces_the_others() {
    let mut clip = clip_with_effects(
        4.0,
        vec![
            TellersEffect::new("Blur").to_effect(),
            TellersEffect::new("TimeWarp").with_parameter("speed", 3.0).to_effect(),
            effect(json!({"OTIO_SCHEMA": "FreezeFrame.1"})),
        ],
    );
    clip.set_speed(2.0, Some(false)).unwrap();
    let written = serde_json::to_value(&clip.effects).unwrap();
    assert_eq!(written.as_array().unwrap().len(), 2);
    assert_eq!(written[0]["effect_name"], json!("tellers.ai.Blur"));
    assert_eq!(written[1]["OTIO_SCHEMA"], json!("LinearTimeWarp.1"));
    assert_eq!(written[1]["effect_name"], json!("LinearTimeWarp"));
    assert_eq!(written[1]["time_scalar"], json!(2.0));
    assert_eq!(written[1]["metadata"]["tellers.ai"]["parameters"]["preserve_pitch"], json!(false));

    // `None` keeps the pitch setting.
    clip.set_speed(0.0, None).unwrap();
    let speed = clip.get_speed().unwrap();
    assert!(speed.is_freeze() && !speed.preserve_pitch);
    assert_eq!(clip.effects[1].otio_schema, "FreezeFrame.1");

    clip.set_speed(1.0, None).unwrap();
    assert_eq!(clip.get_speed(), None);
    assert_eq!(clip.effects.len(), 1);

    assert_eq!(clip.set_speed(5.0, None), Err(SpeedError::OutOfRange(5.0)));
    assert_eq!(clip.set_speed(0.1, None), Err(SpeedError::OutOfRange(0.1)));
    clip.set_speed(0.5, None).unwrap();
    assert!(clip.clear_speed());
    assert!(!clip.clear_speed());
}

#[test]
fn split_advances_the_right_half_by_the_source_the_left_half_plays() {
    let mut fast = clip(4.0, Some("fast"));
    fast.source_range.start_time.value = 10.0;
    fast.set_speed(2.0, None).unwrap();
    let mut frozen = clip(4.0, Some("frozen"));
    frozen.source_range.start_time.value = 3.0;
    frozen.set_speed(0.0, None).unwrap();
    let mut track = Track::new(TrackKind::Video, Some("v".to_string()));
    track.items.push(Item::Clip(fast));
    track.items.push(Item::Clip(frozen));
    let mut stack = Stack::default();
    stack.children.push(track);

    assert!(stack.split_item_at_time("fast", 1.0));
    assert!(stack.split_item_at_time("frozen", 5.0));
    let items = &stack.children[0].items;
    assert_eq!(items.len(), 4);
    assert_eq!(source_start(&items[1]), 12.0);
    assert_eq!(items[1].duration(), 3.0);
    // A freeze holds the same frame on both halves.
    assert_eq!(source_start(&items[3]), 3.0);
}

#[test]
fn clamping_to_the_media_counts_the_source_a_fast_clip_plays() {
    // 10 s of media from 0; at 2x, 4 s on the timeline from 4 s plays 8 s.
    let mut fast = clip_with_media_range(4.0, 4.0, 0.0, 10.0);
    fast.set_speed(2.0, None).unwrap();
    fast.clamp_to_active_available_range();
    assert_eq!(fast.source_range.duration.to_seconds(), 3.0);

    // At half speed the same clip fits.
    let mut slow = clip_with_media_range(4.0, 4.0, 0.0, 10.0);
    slow.set_speed(0.5, None).unwrap();
    slow.clamp_to_active_available_range();
    assert_eq!(slow.source_range.duration.to_seconds(), 4.0);

    // A freeze is never shortened.
    let mut frozen = clip_with_media_range(20.0, 9.0, 0.0, 10.0);
    frozen.set_speed(0.0, None).unwrap();
    frozen.clamp_to_active_available_range();
    assert_eq!(frozen.source_range.duration.to_seconds(), 20.0);
}

#[test]
fn set_item_speed_reaches_the_sync_partners() {
    let mut stack = two_synced_clips();
    assert_eq!(stack.set_item_speed("vA", 2.0, None, true), Ok(2));
    let speed_of = |stack: &Stack, id: &str| match stack.get_item(id).unwrap().2 {
        Item::Clip(clip) => clip.get_speed(),
        _ => None,
    };
    assert_eq!(speed_of(&stack, "aA").unwrap().speed, Animatable::Value(2.0));
    assert_eq!(speed_of(&stack, "vB"), None);

    assert_eq!(stack.set_item_speed("vB", 0.5, None, false), Ok(1));
    assert_eq!(speed_of(&stack, "aB"), None);
    assert_eq!(stack.set_item_speed("missing", 2.0, None, true), Ok(0));
    assert!(stack.set_item_speed("vA", 8.0, None, true).is_err());
    assert_eq!(speed_of(&stack, "aA").unwrap().speed, Animatable::Value(2.0));
}
