// What video-player-js cannot draw as written: speed anywhere but a media
// clip (`clip-speed.ts` reads `clip.effects` only), speed it clamps or
// ignores, effects it reads once repeated, and composite clip content
// `convertOtioCompositeStack` / `compositeSegments` skip. Edits refuse them
// where that is cheap; `validate_timeline` reports the rest.

mod common;

use common::*;
use serde_json::json;
use tellers_timeline_core::*;

fn effect(value: serde_json::Value) -> Effect {
    serde_json::from_value(value).unwrap()
}

fn time_warp(speed: serde_json::Value) -> TellersEffect {
    TellersEffect::new("TimeWarp").with_parameter("speed", speed)
}

fn ramp(values: &[f64]) -> serde_json::Value {
    let keyframes: Vec<_> = values.iter().enumerate().map(|(i, v)| json!({ "t": i, "v": v })).collect();
    json!({ "keyframes": keyframes })
}

fn text_clip(id: &str) -> Clip {
    Clip::new_text(1.0, &TextParameters::with_text("Title"), None, Some(id.to_string())).unwrap()
}

fn rich_text_clip(id: &str) -> Clip {
    serde_json::from_value(json!({
        "OTIO_SCHEMA": "Clip.2",
        "metadata": { "tellers.ai": { "timeline_id": id } },
        "source_range": {
            "OTIO_SCHEMA": "TimeRange.1",
            "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 0, "rate": 1 },
            "duration": { "OTIO_SCHEMA": "RationalTime.1", "value": 1, "rate": 1 }
        },
        "media_references": { "DEFAULT_MEDIA": {
            "OTIO_SCHEMA": "GeneratorReference.1", "generator_kind": "Rich", "parameters": {}
        } },
        "active_media_reference_key": "DEFAULT_MEDIA"
    }))
    .unwrap()
}

fn track(kind: TrackKind, id: &str, items: Vec<Item>) -> Track {
    let mut track = Track { kind, items, ..Track::default() };
    track.set_id(Some(id.to_string()));
    track
}

fn timeline(tracks: Vec<Track>) -> Timeline {
    let mut timeline = Timeline::default();
    timeline.tracks.children = tracks;
    timeline
}

fn errors(timeline: &Timeline) -> Vec<String> {
    validate_timeline(timeline).iter().map(ToString::to_string).collect()
}

#[test]
fn unplayable_speed_mirrors_the_players_clamp_and_zero_crossing() {
    let check = |e: TellersEffect| unplayable_speed(&e.to_effect());
    assert_eq!(check(time_warp(json!(2.0))), None);
    assert_eq!(check(time_warp(json!(-0.5))), None);
    assert_eq!(check(time_warp(json!(0.0))), None, "a constant 0 is a freeze");
    assert_eq!(check(time_warp(ramp(&[1.0, 4.0, 1.0]))), None);
    assert_eq!(check(time_warp(ramp(&[-1.0, -4.0]))), None, "backwards throughout");
    assert!(check(time_warp(json!(8.0))).unwrap().contains("outside"));
    assert!(check(time_warp(json!(0.1))).unwrap().contains("outside"));
    assert!(check(time_warp(ramp(&[1.0, 6.0]))).unwrap().contains("keyframe 6"));
    assert!(check(time_warp(ramp(&[1.0, 0.0]))).unwrap().contains("keyframe 0"));
    assert!(check(time_warp(ramp(&[1.0, -1.0]))).unwrap().contains("crosses zero"));
    let linear = effect(json!({ "OTIO_SCHEMA": "LinearTimeWarp.1", "time_scalar": 10.0 }));
    assert!(unplayable_speed(&linear).is_some());
    assert_eq!(unplayable_speed(&TellersEffect::new("Blur").to_effect()), None);
}

#[test]
fn speed_goes_on_media_clips_only() {
    let inner = track(TrackKind::Video, "inner", vec![Item::Clip(clip(2.0, Some("nested")))]);
    let composite = CompositeStack::new(vec![inner], None, Some("comp".to_string()));
    let mut timeline = timeline(vec![
        track(
            TrackKind::Video,
            "v",
            vec![
                Item::Clip(clip(2.0, Some("vc"))),
                Item::Clip(text_clip("tc")),
                Item::Clip(rich_text_clip("rc")),
                Item::Composite(composite),
            ],
        ),
        track(TrackKind::Audio, "a", vec![Item::Clip(clip(2.0, Some("ac")))]),
    ]);

    // Not on a track, the timeline or a composite clip.
    assert!(!timeline.push_track_tellers_effect("v", time_warp(json!(2.0))));
    assert!(!timeline.push_track_tellers_effect("a", time_warp(json!(2.0))));
    assert!(!timeline.push_tellers_effect(time_warp(json!(2.0))));
    assert!(!timeline.push_item_tellers_effect("comp", time_warp(json!(2.0))));
    // Not on generator clips.
    assert!(!timeline.push_item_tellers_effect("tc", time_warp(json!(2.0))));
    assert!(!timeline.push_item_tellers_effect("rc", time_warp(json!(2.0))));
    // Not inside a composite.
    assert!(!timeline.push_item_tellers_effect("nested", time_warp(json!(2.0))));
    // Only speeds the player plays as written.
    assert!(!timeline.push_item_tellers_effect("vc", time_warp(ramp(&[1.0, -1.0]))));
    assert!(!timeline.push_item_tellers_effect("vc", time_warp(ramp(&[1.0, 8.0]))));
    assert!(!timeline.set_item_tellers_effects("vc", vec![time_warp(json!(5.0))]));
    assert!(timeline.push_item_tellers_effect("vc", time_warp(ramp(&[1.0, 4.0, 1.0]))));
    assert!(timeline.push_item_tellers_effect("ac", time_warp(json!(0.0))));
    assert!(errors(&timeline).is_empty(), "{:?}", errors(&timeline));

    // Clip::set_speed refuses generator clips, normal speed aside.
    let mut text = text_clip("t");
    assert_eq!(text.set_speed(2.0, None), Err(SpeedError::NotRetimable));
    assert_eq!(text.set_speed(1.0, None), Ok(()));
    assert!(timeline.tracks.set_item_speed("tc", 2.0, None, false).is_err());
    assert_eq!(timeline.tracks.set_item_speed("vc", 2.0, None, false), Ok(1));
}

#[test]
fn colorlut_is_refused_inside_a_composite() {
    let inner = track(TrackKind::Video, "inner", vec![Item::Clip(clip(2.0, Some("nested")))]);
    let mut timeline = timeline(vec![track(
        TrackKind::Video,
        "v",
        vec![Item::Composite(CompositeStack::new(vec![inner], None, Some("comp".to_string())))],
    )]);
    let lut = TellersEffect::new("ColorLut").with_parameter("target_url", "https://cdn/a.cube");
    assert!(!timeline.push_item_tellers_effect("nested", lut.clone()));
    assert!(timeline.push_item_tellers_effect("nested", TellersEffect::new("Blur")));
    // The composite clip itself is graded as one picture.
    assert!(timeline.push_item_tellers_effect("comp", lut));
}

#[test]
fn validation_reports_speed_the_player_ignores() {
    let mut text = text_clip("tc");
    text.effects.push(time_warp(json!(2.0)).to_effect());
    let mut ramped = clip(2.0, Some("vc"));
    ramped.effects.push(time_warp(ramp(&[1.0, -2.0])).to_effect());
    let mut fast = clip(2.0, Some("fast"));
    fast.effects.push(effect(json!({ "OTIO_SCHEMA": "LinearTimeWarp.1", "time_scalar": 10.0 })));
    let mut disabled = clip(2.0, Some("off"));
    disabled.effects.push(time_warp(json!(10.0)).with_enabled(false).to_effect());
    let mut video = track(
        TrackKind::Video,
        "v",
        vec![Item::Clip(text), Item::Clip(ramped), Item::Clip(fast), Item::Clip(disabled)],
    );
    video.effects.push(time_warp(json!(2.0)).to_effect());
    let mut timeline = timeline(vec![video]);
    timeline.tracks.effects.push(time_warp(json!(2.0)).to_effect());

    let errors = validate_timeline(&timeline);
    let speed_not_on_clip: Vec<_> = errors
        .iter()
        .filter_map(|e| match e {
            ValidationError::SpeedNotOnClip { owner } => Some(owner.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(speed_not_on_clip, ["the timeline", "v"]);
    assert!(errors.iter().any(|e| matches!(e, ValidationError::SpeedOnGenerator { owner } if owner == "tc")));
    let unplayable: Vec<_> = errors
        .iter()
        .filter_map(|e| match e {
            ValidationError::UnplayableSpeed { owner, reason } => Some((owner.as_str(), reason.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(unplayable.len(), 2, "{unplayable:?}");
    assert!(unplayable[0].0 == "vc" && unplayable[0].1.contains("crosses zero"));
    assert!(unplayable[1].0 == "fast" && unplayable[1].1.contains("outside"));
    assert_eq!(errors.len(), 5, "{errors:?}");
}

#[test]
fn validation_reports_effects_the_player_reads_once() {
    let blend = |mode: &str| TellersEffect::new("Blend").with_parameter("mode", mode).to_effect();
    let mut c = clip(2.0, Some("vc"));
    c.effects = vec![
        blend("screen"),
        blend("add"),
        TellersEffect::new("Blend").with_enabled(false).to_effect(),
        TellersEffect::new("Blur").to_effect(),
        TellersEffect::new("Blur").to_effect(),
        effect(json!({ "OTIO_SCHEMA": "LinearTimeWarp.1", "time_scalar": 2.0 })),
        time_warp(json!(3.0)).to_effect(),
    ];
    let mut audio = track(TrackKind::Audio, "a", vec![]);
    audio.effects = vec![
        TellersEffect::new("Volume").to_effect(),
        TellersEffect::new("Volume").to_effect(),
        TellersEffect::new("Pitch").to_effect(),
        TellersEffect::new("Pitch").to_effect(),
    ];
    let timeline = timeline(vec![track(TrackKind::Video, "v", vec![Item::Clip(c)]), audio]);

    assert_eq!(
        errors(&timeline),
        [
            "vc has 2 enabled speed effects; only the first applies",
            "vc has 2 enabled Blend effects; only the first applies",
            "a has 2 enabled Volume effects; only the first applies",
            "a has 2 enabled Pitch effects; only the first applies",
        ]
    );
}

#[test]
fn validation_reports_composite_content_the_player_skips() {
    let mut sped = clip(1.0, Some("sped"));
    sped.effects.push(time_warp(json!(2.0)).to_effect());
    let mut graded = clip(1.0, Some("graded"));
    graded.effects.push(TellersEffect::new("ColorLut").with_parameter("target_url", "https://cdn/a.cube").to_effect());
    let nested = CompositeStack::new(vec![track(TrackKind::Video, "deep", vec![])], None, Some("nested".to_string()));
    let content = vec![
        track(
            TrackKind::Video,
            "t0",
            vec![
                Item::Clip(sped),
                Item::Clip(graded),
                Item::Clip(rich_text_clip("rich")),
                Item::Clip(text_clip("title")),
                Item::Composite(nested),
            ],
        ),
        track(TrackKind::Audio, "t1", vec![Item::Clip(clip(1.0, Some("sound")))]),
    ];
    let timeline = timeline(vec![track(
        TrackKind::Video,
        "v",
        vec![Item::Composite(CompositeStack::new(content, None, Some("comp".to_string())))],
    )]);

    let skipped: Vec<_> = validate_timeline(&timeline)
        .into_iter()
        .map(|e| match e {
            ValidationError::CompositeContentSkipped { owner, content } => {
                assert_eq!(owner, "comp");
                content
            }
            other => panic!("unexpected {other}"),
        })
        .collect();
    assert_eq!(
        skipped,
        ["the speed of sped", "the ColorLut of graded", "generator clip rich", "nested composite nested", "Audio track 1"]
    );
}

#[test]
fn validation_reports_overlapping_media_in_a_composite() {
    let with_gap = |gap: f64, id: &str, length: f64| {
        vec![Item::Gap(Gap::new(gap, None)), Item::Clip(clip(length, Some(id)))]
    };
    let composite = |tracks: Vec<Track>, trim: Option<(f64, f64)>| {
        let mut stack = CompositeStack::new(tracks, None, Some("comp".to_string()));
        if let Some((start, duration)) = trim {
            let mut r = range(duration);
            r.start_time.value = start;
            stack.source_range = Some(r);
        }
        timeline(vec![track(TrackKind::Video, "v", vec![Item::Composite(stack)])])
    };
    let overlapping = |timeline: &Timeline| {
        validate_timeline(timeline).into_iter().find_map(|e| match e {
            ValidationError::CompositeOverlappingMedia { count, .. } => Some(count),
            _ => None,
        })
    };

    // A picture over a video, as the lower-third demo: the second is dropped.
    let both = vec![
        track(TrackKind::Video, "t0", vec![Item::Clip(clip(4.0, Some("bg")))]),
        track(TrackKind::Video, "t1", with_gap(1.0, "pip", 2.0)),
    ];
    assert_eq!(overlapping(&composite(both.clone(), None)), Some(1));
    // Back to back is fine, and text over media too.
    let sequential = vec![
        track(TrackKind::Video, "t0", vec![Item::Clip(clip(2.0, Some("a")))]),
        track(TrackKind::Video, "t1", with_gap(2.0, "b", 2.0)),
        track(TrackKind::Video, "t2", vec![Item::Clip(text_clip("title"))]),
    ];
    assert_eq!(overlapping(&composite(sequential, None)), None);
    // Trimmed to after the first clip ends, nothing overlaps on screen.
    assert_eq!(overlapping(&composite(both, Some((4.0, 1.0)))), None);
}
