use std::collections::HashMap;
use tellers_timeline_core::{
    Clip, IdMetadataExt, Item, MediaReference, RationalTime, Seconds, Stack, TimeRange, Track,
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

#[test]
fn split_clip_basic() {
    let mut track = Track::default();
    let mut clip = make_clip(10.0, 0.0);
    clip.set_id(Some("clip".to_string()));
    track.items.push(clip);
    let mut stack = Stack {
        children: vec![track],
        ..Stack::default()
    };

    assert!(stack.split_item_at_time("clip", 3.0));

    let track = &stack.children[0];
    assert_eq!(track.items.len(), 2);
    match (&track.items[0], &track.items[1]) {
        (Item::Clip(c0), Item::Clip(c1)) => {
            assert_eq!(c0.source_range.duration.value, 3.0);
            assert_eq!(c1.source_range.duration.value, 7.0);
            assert_eq!(c1.source_range.start_time.value, 3.0);
        }
        _ => panic!("expected two clips after split"),
    }
}

#[test]
fn split_at_boundary_noop() {
    let mut track = Track::default();
    let mut clip = make_clip(5.0, 0.0);
    clip.set_id(Some("clip".to_string()));
    track.items.push(clip);
    let mut stack = Stack {
        children: vec![track],
        ..Stack::default()
    };

    assert!(stack.split_item_at_time("clip", 0.0));
    assert_eq!(stack.children[0].items.len(), 1);

    assert!(stack.split_item_at_time("clip", 5.0));
    assert_eq!(stack.children[0].items.len(), 1);
}
