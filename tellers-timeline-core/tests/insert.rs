use std::collections::HashMap;
use tellers_timeline_core::{
    Clip, IdMetadataExt, InsertItemAtTimeResult, InsertPolicy, Item, MediaReference, OverlapPolicy,
    RationalTime, Seconds, Stack, TimeRange, Track, TrackKind,
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
        active_media_reference_key: None,
        metadata: serde_json::Value::Null,
        effects: Vec::new(),
    })
}

fn make_clip_with_default_available_range(duration: Seconds, media_duration: Seconds) -> Item {
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
    let available_range = TimeRange {
        otio_schema: "TimeRange.1".to_string(),
        duration: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: 1.0,
            value: media_duration,
        },
        start_time: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: 1.0,
            value: 0.0,
        },
    };
    let mut refs: HashMap<String, MediaReference> = HashMap::new();
    refs.insert(
        "ALT".to_string(),
        MediaReference::ExternalReference {
            target_url: "mem://alt".to_string(),
            available_range: Some(available_range.clone()),
            name: None,
            available_image_bounds: None,
            metadata: serde_json::Value::Null,
        },
    );
    refs.insert(
        "DEFAULT_MEDIA".to_string(),
        MediaReference::ExternalReference {
            target_url: "mem://default".to_string(),
            available_range: Some(available_range),
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
        active_media_reference_key: None,
        metadata: serde_json::Value::Null,
        effects: Vec::new(),
    })
}

fn make_clip_with_mixed_rates(
    source_start: Seconds,
    source_duration: Seconds,
    source_rate: f64,
    media_start: Seconds,
    media_duration: Seconds,
    media_rate: f64,
) -> Item {
    let sr = TimeRange {
        otio_schema: "TimeRange.1".to_string(),
        duration: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: source_rate,
            value: source_duration,
        },
        start_time: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: source_rate,
            value: source_start,
        },
    };
    let available_range = TimeRange {
        otio_schema: "TimeRange.1".to_string(),
        duration: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: media_rate,
            value: media_duration,
        },
        start_time: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: media_rate,
            value: media_start,
        },
    };
    let mut refs: HashMap<String, MediaReference> = HashMap::new();
    refs.insert(
        "DEFAULT_MEDIA".to_string(),
        MediaReference::ExternalReference {
            target_url: "mem://default".to_string(),
            available_range: Some(available_range),
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

fn stack_with_items(items: Vec<Item>) -> Stack {
    let mut track = Track::default();
    track.items = items;
    Stack {
        children: vec![track],
        ..Stack::default()
    }
}

fn identified_clip(id: &str, duration: Seconds) -> Item {
    let mut item = make_clip(duration, 0.0);
    item.set_id(Some(id.to_string()));
    item
}

fn stack_with_lower_audio_and_video_above() -> Stack {
    let audio = Track::new(TrackKind::Audio, Some("audio-below".to_string()));
    let mut video = Track::new(TrackKind::Video, Some("video-above".to_string()));
    video.items = vec![
        identified_clip("video-1", 4.0),
        identified_clip("video-2", 4.0),
    ];
    Stack {
        children: vec![audio, video],
        ..Stack::default()
    }
}

fn assert_track_ids(track: &Track, expected_ids: &[&str]) {
    let ids = track
        .items
        .iter()
        .map(|item| item.get_id().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(ids, expected_ids);
}

fn assert_track_durations(track: &Track, expected_durations: &[Seconds]) {
    let durations = track
        .items
        .iter()
        .map(|item| item.duration())
        .collect::<Vec<_>>();
    assert_eq!(durations, expected_durations);
}

#[test]
fn insert_before_after_or_boundary() {
    let mut stack = stack_with_items(vec![make_clip(4.0, 0.0), make_clip(6.0, 0.0)]);

    // Insert before inside first clip -> snaps to its start index
    stack.insert_item_at_time(
        0,
        1.0,
        make_clip(1.0, 0.0),
        OverlapPolicy::Push,
        InsertPolicy::InsertBefore,
        None,
    );
    let track = &stack.children[0];
    assert!(matches!(track.items[0], Item::Clip(_)));

    // Insert after inside first clip -> index after first
    stack.insert_item_at_time(
        0,
        1.5,
        make_clip(1.0, 0.0),
        OverlapPolicy::Push,
        InsertPolicy::InsertAfter,
        None,
    );
    let track = &stack.children[0];
    assert!(matches!(track.items[2], Item::Clip(_)));

    // Insert before or after: choose closer boundary
    let before_len = track.items.len();
    stack.insert_item_at_time(
        0,
        3.9,
        make_clip(0.5, 0.0),
        OverlapPolicy::Push,
        InsertPolicy::InsertBeforeOrAfter,
        None,
    );
    let track = &stack.children[0];
    assert_eq!(track.items.len(), before_len + 1);
}

#[test]
fn insert_plain_item_without_id_returns_assigned_id() {
    let mut stack = stack_with_items(vec![]);

    let result = stack.insert_item_at_time(
        0,
        0.0,
        make_clip(1.0, 0.0),
        OverlapPolicy::Push,
        InsertPolicy::InsertBefore,
        None,
    );

    let Some(InsertItemAtTimeResult::ItemId(inserted_id)) = result else {
        panic!("expected inserted id");
    };
    assert_eq!(
        stack.children[0].items[0].get_id().as_deref(),
        Some(inserted_id.as_str())
    );
}

#[test]
fn insert_plain_item_at_index_without_id_returns_assigned_id() {
    let mut stack = stack_with_items(vec![make_clip(1.0, 0.0)]);
    let track_id = stack.children[0].get_id().unwrap();

    let result =
        stack.insert_item_at_index(&track_id, 1, make_clip(1.0, 0.0), OverlapPolicy::Push, None);

    let Some(InsertItemAtTimeResult::ItemId(inserted_id)) = result else {
        panic!("expected inserted id");
    };
    assert_eq!(
        stack.children[0].items[1].get_id().as_deref(),
        Some(inserted_id.as_str())
    );
}

#[test]
fn insert_missing_active_key_does_not_clamp_to_default_media() {
    let mut stack = stack_with_items(vec![]);

    stack.insert_item_at_time(
        0,
        0.0,
        make_clip_with_default_available_range(5.0, 3.0),
        OverlapPolicy::Push,
        InsertPolicy::InsertBefore,
        None,
    );

    let track = &stack.children[0];
    match &track.items[0] {
        Item::Clip(clip) => {
            assert_eq!(clip.active_media_reference_key.as_deref(), None);
            assert_eq!(clip.source_range.duration.value, 5.0);
        }
        _ => panic!("expected inserted clip"),
    }
}

#[test]
fn insert_clamp_converts_rational_time_rates() {
    let mut stack = stack_with_items(vec![]);

    stack.insert_item_at_time(
        0,
        0.0,
        make_clip_with_mixed_rates(0.0, 120.0, 24.0, 1.0, 3.0, 1.0),
        OverlapPolicy::Push,
        InsertPolicy::InsertBefore,
        None,
    );

    let track = &stack.children[0];
    match &track.items[0] {
        Item::Clip(clip) => {
            assert_eq!(clip.source_range.start_time.value, 24.0);
            assert_eq!(clip.source_range.duration.value, 72.0);
            assert_eq!(clip.source_range.start_time.rate, 24.0);
            assert_eq!(clip.source_range.duration.rate, 24.0);
        }
        _ => panic!("expected inserted clip"),
    }
}

#[test]
fn insert_split_and_override() {
    let mut stack = stack_with_items(vec![make_clip(5.0, 0.0), make_clip(5.0, 0.0)]);

    // Insert across boundary with override: should split as needed and replace overlap
    stack.insert_item_at_time(
        0,
        3.0,
        make_clip(4.0, 0.0),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        None,
    );

    // Expect an item at 3.0 of duration 4.0
    let track = &stack.children[0];
    let idx = track.get_item_at_time(3.1).unwrap();
    match &track.items[idx] {
        Item::Clip(c) => assert!((c.source_range.duration.value - 4.0).abs() < 1e-9),
        _ => panic!("expected clip inserted with override"),
    }
}

#[test]
fn insert_split_and_override_at_zero() {
    let mut stack = stack_with_items(vec![make_clip(5.0, 0.0)]);

    // Insert across boundary with override: should split as needed and replace overlap
    stack.insert_item_at_time(
        0,
        0.0,
        make_clip(4.0, 0.0),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        None,
    );
    let track = &stack.children[0];
    assert_eq!(track.items.len(), 2);
    assert_eq!(track.items[0].duration(), 4.0);
    assert_eq!(track.items[1].duration(), 1.0);
    match (&track.items[0], &track.items[1]) {
        (Item::Clip(c0), Item::Clip(c1)) => {
            assert_eq!(c0.source_range.duration.value, 4.0);
            assert_eq!(c1.source_range.duration.value, 1.0);
        }
        _ => panic!("expected two clips after insert"),
    }
}

#[test]
fn insert_plain_item_override_only_edits_destination_track() {
    let mut stack = stack_with_lower_audio_and_video_above();

    stack.insert_item_at_time(
        0,
        0.0,
        identified_clip("music", 8.0),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        None,
    );

    assert_track_ids(&stack.children[1], &["video-1", "video-2"]);
    assert_track_durations(&stack.children[1], &[4.0, 4.0]);
    assert_track_ids(&stack.children[0], &["music"]);
}

#[test]
fn insert_plain_item_push_only_edits_destination_track() {
    let mut stack = stack_with_lower_audio_and_video_above();

    stack.insert_item_at_time(
        0,
        0.0,
        identified_clip("music", 8.0),
        OverlapPolicy::Push,
        InsertPolicy::SplitAndInsert,
        None,
    );

    assert_track_ids(&stack.children[1], &["video-1", "video-2"]);
    assert_track_durations(&stack.children[1], &[4.0, 4.0]);
    assert_track_ids(&stack.children[0], &["music"]);
}
