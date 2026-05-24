use std::collections::HashMap;
use tellers_timeline_core::{
    Clip, Gap, IdMetadataExt, Item, MediaReference, OverlapPolicy, RationalTime, Seconds, Stack,
    TimeRange, Track, TrackKind,
};

fn make_clip(duration: Seconds, media_start: Seconds) -> Item {
    let sr = TimeRange {
        otio_schema: "TimeRange.1".to_string(),
        duration: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: 1.0,
            value: duration,
        },
        start_time: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: 1.0,
            value: media_start,
        },
    };
    let mut refs: HashMap<String, MediaReference> = HashMap::new();
    refs.insert(
        "DEFAULT_MEDIA".to_string(),
        MediaReference::ExternalReference {
            target_url: "mem://".to_string(),
            available_range: None,
            name: None,
            available_image_bounds: None,
            metadata: serde_json::Value::Null,
        },
    );
    Item::Clip(Clip {
        otio_schema: "Clip.2".to_string(),
        enabled: true,
        name: None,
        source_range: sr,
        media_references: refs,
        active_media_reference_key: Some("DEFAULT_MEDIA".to_string()),
        metadata: serde_json::Value::Null,
        effects: Vec::new(),
    })
}

fn media_ref(url: &str) -> MediaReference {
    MediaReference::ExternalReference {
        target_url: url.to_string(),
        available_range: None,
        name: None,
        available_image_bounds: None,
        metadata: serde_json::Value::Null,
    }
}

fn make_clip_with_references(duration: Seconds, active_key: Option<&str>) -> Item {
    let sr = TimeRange {
        otio_schema: "TimeRange.1".to_string(),
        duration: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: 1.0,
            value: duration,
        },
        start_time: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: 1.0,
            value: 0.0,
        },
    };
    let mut refs: HashMap<String, MediaReference> = HashMap::new();
    refs.insert("ALT".to_string(), media_ref("mem://alt"));
    refs.insert("DEFAULT_MEDIA".to_string(), media_ref("mem://default"));
    Item::Clip(Clip {
        otio_schema: "Clip.2".to_string(),
        enabled: true,
        name: None,
        source_range: sr,
        media_references: refs,
        active_media_reference_key: active_key.map(str::to_string),
        metadata: serde_json::Value::Null,
        effects: Vec::new(),
    })
}

#[test]
fn resize_moves_and_sets_duration_with_override() {
    let mut track = Track::default();
    // Layout: [c0:4][c1:6]
    let mut c0 = make_clip(4.0, 0.0);
    c0.set_id(Some("c0".to_string()));
    track.items.push(c0);
    track.items.push(make_clip(6.0, 0.0));
    let mut stack = Stack {
        children: vec![track],
        ..Stack::default()
    };

    // Resize c0 to start at t=3.0 with duration 5.0, overriding overlaps.
    let ok = stack.resize_item("c0", 3.0, 5.0, OverlapPolicy::Override, false);
    assert!(ok);

    // Expect an item at time 3.0 of duration 5.0
    let track = &stack.children[0];
    let idx = track.get_item_at_time(3.0 + 1e-6).unwrap();
    match &track.items[idx] {
        Item::Clip(c) => assert!((c.source_range.duration.value - 5.0).abs() < 1e-9),
        _ => panic!("expected clip after resize"),
    }

    // Ensure sanitize kept a valid sequence
    let total: Seconds = track.items.iter().map(|i| i.duration().max(0.0)).sum();
    assert!(total >= 5.0);
}

#[test]
fn resize_gap_duration_preserves_following_clip() {
    let mut track = Track::default();
    track
        .items
        .push(Item::Gap(Gap::new(2.0, Some("gap".to_string()))));
    let mut clip = make_clip(3.0, 0.0);
    clip.set_id(Some("clip".to_string()));
    track.items.push(clip);
    let mut stack = Stack {
        children: vec![track],
        ..Stack::default()
    };

    assert!(stack.resize_item("gap", 0.0, 1.0, OverlapPolicy::Override, false));

    let track = &stack.children[0];
    assert_eq!(track.items.len(), 2);
    assert!(matches!(track.items[0], Item::Gap(_)));
    assert_eq!(track.items[0].duration(), 1.0);
    let (clip_track_index, clip_item_index, clip_item) = stack.get_item("clip").unwrap();
    assert_eq!(clip_track_index, 0);
    assert_eq!(track.start_time_of_item(clip_item_index), 1.0);
    assert_eq!(clip_item.duration(), 3.0);
}

#[test]
fn resize_audio_clip_duration_preserves_following_clip() {
    let mut track = Track::new(TrackKind::Audio, Some("a".to_string()));
    let mut first = make_clip(2.0, 0.0);
    first.set_id(Some("first".to_string()));
    let mut second = make_clip(3.0, 0.0);
    second.set_id(Some("second".to_string()));
    track.items.push(first);
    track.items.push(second);
    let mut stack = Stack {
        children: vec![track],
        ..Stack::default()
    };

    assert!(stack.resize_item("first", 0.0, 4.0, OverlapPolicy::Override, false));

    let (first_track_index, first_item_index, first_item) = stack.get_item("first").unwrap();
    let (second_track_index, second_item_index, second_item) = stack.get_item("second").unwrap();
    assert_eq!(first_track_index, 0);
    assert_eq!(second_track_index, 0);
    assert_eq!(first_item_index, 0);
    assert_eq!(second_item_index, 1);
    assert_eq!(first_item.duration(), 4.0);
    assert_eq!(second_item.duration(), 3.0);
    assert_eq!(
        stack.children[second_track_index].start_time_of_item(second_item_index),
        4.0
    );
}

#[test]
fn resize_missing_active_reference_does_not_bind_default_media() {
    let mut track = Track::default();
    let mut clip = make_clip_with_references(4.0, None);
    clip.set_id(Some("clip".to_string()));
    track.items.push(clip);
    let mut stack = Stack {
        children: vec![track],
        ..Stack::default()
    };

    assert!(stack.resize_item("clip", 0.0, 3.0, OverlapPolicy::Override, false));

    let track = &stack.children[0];
    match &track.items[0] {
        Item::Clip(clip) => assert_eq!(clip.active_media_reference_key.as_deref(), None),
        _ => panic!("expected resized clip"),
    }
}

#[test]
fn resize_preserves_valid_non_default_active_reference() {
    let mut track = Track::default();
    let mut clip = make_clip_with_references(4.0, Some("ALT"));
    clip.set_id(Some("clip".to_string()));
    track.items.push(clip);
    let mut stack = Stack {
        children: vec![track],
        ..Stack::default()
    };

    assert!(stack.resize_item("clip", 0.0, 3.0, OverlapPolicy::Override, false));

    let track = &stack.children[0];
    match &track.items[0] {
        Item::Clip(clip) => assert_eq!(clip.active_media_reference_key.as_deref(), Some("ALT")),
        _ => panic!("expected resized clip"),
    }
}

#[test]
fn resize_push_inserts_without_overriding() {
    let mut track = Track::default();
    track.items.push(make_clip(4.0, 0.0));
    let mut c1 = make_clip(6.0, 0.0);
    c1.set_id(Some("c1".to_string()));
    track.items.push(c1);
    let mut stack = Stack {
        children: vec![track],
        ..Stack::default()
    };

    // Push policy should not remove overlapped items; it inserts in sequence.
    let ok = stack.resize_item("c1", 2.0, 2.0, OverlapPolicy::Push, false);
    assert!(ok);

    // After resize, the resized item should be present starting near 2.0
    let track = &stack.children[0];
    let idx = track.get_item_at_time(2.0 + 1e-6).unwrap();
    match &track.items[idx] {
        Item::Clip(c) => assert!((c.source_range.duration.value - 2.0).abs() < 1e-9),
        _ => panic!("expected clip after resize with push"),
    }
}
