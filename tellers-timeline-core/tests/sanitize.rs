use std::collections::HashSet;
use tellers_timeline_core::{
    Clip, Gap, IdMetadataExt, Item, MediaReference, Stack, Timeline, Track, TrackKind,
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
