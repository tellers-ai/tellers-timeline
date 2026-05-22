use std::collections::HashSet;
use tellers_timeline_core::{
    Clip, Gap, IdMetadataExt, Item, MediaReference, RationalTime, Stack, TimeRange, Timeline, Track,
    TrackKind,
};

fn clip(duration: f64, id: Option<&str>) -> Clip {
    Clip::new_single_media_reference(
        tellers_timeline_core::TimeRange {
            otio_schema: "TimeRange.1".to_string(),
            start_time: tellers_timeline_core::RationalTime {
                otio_schema: "RationalTime.1".to_string(),
                rate: 1.0,
                value: 0.0,
            },
            duration: tellers_timeline_core::RationalTime {
                otio_schema: "RationalTime.1".to_string(),
                rate: 1.0,
                value: duration,
            },
        },
        MediaReference::ExternalReference {
            target_url: "file:///media.mov".to_string(),
            available_range: None,
            name: None,
            available_image_bounds: Some(serde_json::Value::Null),
            metadata: serde_json::json!({}),
        },
        None,
        id.map(str::to_string),
    )
}

fn clip_with_missing_active_default_reference() -> Clip {
    let mut clip = clip(5.0, Some("clip-1"));
    clip.active_media_reference_key = None;
    clip.source_range.duration.value = 5.0;
    clip.media_references.insert(
        "ALT".to_string(),
        MediaReference::ExternalReference {
            target_url: "file:///alt.mov".to_string(),
            available_range: None,
            name: None,
            available_image_bounds: Some(serde_json::Value::Null),
            metadata: serde_json::json!({}),
        },
    );
    clip.media_references.insert(
        "DEFAULT_MEDIA".to_string(),
        MediaReference::ExternalReference {
            target_url: "file:///media.mov".to_string(),
            available_range: Some(TimeRange {
                otio_schema: "TimeRange.1".to_string(),
                start_time: RationalTime {
                    otio_schema: "RationalTime.1".to_string(),
                    rate: 1.0,
                    value: 0.0,
                },
                duration: RationalTime {
                    otio_schema: "RationalTime.1".to_string(),
                    rate: 1.0,
                    value: 3.0,
                },
            }),
            name: None,
            available_image_bounds: Some(serde_json::Value::Null),
            metadata: serde_json::json!({}),
        },
    );
    clip
}

fn all_stack_ids(stack: &Stack) -> Vec<String> {
    let mut ids = Vec::new();
    for track in &stack.children {
        ids.push(track.get_id().expect("track should have a timeline id"));
        for item in &track.items {
            ids.push(item.get_id().expect("item should have a timeline id"));
        }
    }
    ids
}

#[test]
fn track_sanitize_removes_trailing_gap_after_merging() {
    let mut track = Track::new(TrackKind::Video, Some("video".to_string()));
    track.items.push(Item::Clip(clip(1.0, Some("clip-1"))));
    track.items.push(Item::Gap(Gap::make_gap(1.0)));
    track.items.push(Item::Gap(Gap::make_gap(2.0)));

    track.sanitize();

    assert_eq!(track.items.len(), 1);
    assert_eq!(track.items[0].get_id().as_deref(), Some("clip-1"));
}

#[test]
fn track_sanitize_keeps_interior_gap() {
    let mut track = Track::new(TrackKind::Video, Some("video".to_string()));
    track.items.push(Item::Clip(clip(1.0, Some("clip-1"))));
    track.items.push(Item::Gap(Gap::make_gap(2.0)));
    track.items.push(Item::Clip(clip(1.0, Some("clip-2"))));

    track.sanitize();

    assert_eq!(track.items.len(), 3);
    assert!(matches!(track.items[1], Item::Gap(_)));
    assert_eq!(track.items[1].duration(), 2.0);
}

#[test]
fn track_sanitize_missing_active_key_does_not_clamp_to_default_media() {
    let mut track = Track::new(TrackKind::Video, Some("video".to_string()));
    track.items.push(Item::Clip(clip_with_missing_active_default_reference()));

    track.sanitize();

    match &track.items[0] {
        Item::Clip(clip) => {
            assert_eq!(clip.active_media_reference_key.as_deref(), None);
            assert_eq!(clip.source_range.duration.value, 5.0);
        }
        _ => panic!("expected sanitized clip"),
    }
}

#[test]
fn stack_sanitize_assigns_missing_timeline_ids_and_repairs_duplicates() {
    let mut first = Track::new(TrackKind::Video, Some("same-id".to_string()));
    first.set_id(None);
    first.items.push(Item::Clip(clip(2.0, Some("same-id"))));
    first.items.push(Item::Gap(Gap::make_gap(1.0)));
    first.items[1].set_id(None);

    let mut second = Track::new(TrackKind::Audio, Some("same-id".to_string()));
    second.items.push(Item::Clip(clip(2.0, Some("same-id"))));

    let mut stack = Stack::default();
    stack.children.push(first);
    stack.children.push(second);

    stack.sanitize();

    let ids = all_stack_ids(&stack);
    let unique: HashSet<_> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len());
    assert!(ids.iter().all(|id| !id.is_empty()));
    assert_eq!(stack.children[0].items[0].get_id().as_deref(), Some("same-id"));
}

#[test]
fn timeline_sanitize_uses_stack_timeline_id_repair() {
    let mut timeline = Timeline::default();
    let mut track = Track::new(TrackKind::Video, Some("duplicate".to_string()));
    track.items.push(Item::Clip(clip(1.0, Some("duplicate"))));
    timeline.tracks.children.push(track);

    timeline.sanitize();

    let ids = all_stack_ids(&timeline.tracks);
    let unique: HashSet<_> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len());
}
