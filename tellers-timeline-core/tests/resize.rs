use std::collections::HashMap;
use tellers_timeline_core::{Clip, Item, MediaReference, OverlapPolicy, Seconds, Track, TimeRange, RationalTime};

fn make_clip(duration: Seconds, media_start: Seconds) -> Item {
    let sr = TimeRange {
        otio_schema: "TimeRange.1".to_string(),
        duration: RationalTime { otio_schema: "RationalTime.1".to_string(), rate: 1.0, value: duration },
        start_time: RationalTime { otio_schema: "RationalTime.1".to_string(), rate: 1.0, value: media_start },
    };
    let mut refs: HashMap<String, MediaReference> = HashMap::new();
    refs.insert(
        "DEFAULT_MEDIA".to_string(),
        MediaReference {
            otio_schema: "ExternalReference.1".to_string(),
            target_url: "mem://".to_string(),
            available_range: None,
            name: None,
            available_image_bounds: None,
            metadata: serde_json::Value::Null,
        },
    );
    Item::Clip(Clip {
        otio_schema: "Clip.2".to_string(),
        name: None,
        source_range: sr,
        media_references: refs,
        active_media_reference_key: Some("DEFAULT_MEDIA".to_string()),
        metadata: serde_json::Value::Null,
    })
}

#[test]
fn resize_moves_and_sets_duration_with_override() {
    let mut track = Track::default();
    // Layout: [c0:4][c1:6]
    track.append(make_clip(4.0, 0.0));
    track.append(make_clip(6.0, 0.0));

    // Resize c0 to start at t=3.0 with duration 5.0, overriding overlaps.
    let ok = track.resize_item(0, 3.0, 5.0, OverlapPolicy::Override, false);
    assert!(ok);

    // Expect an item at time 3.0 of duration 5.0
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
fn resize_push_inserts_without_overriding() {
    let mut track = Track::default();
    track.append(make_clip(4.0, 0.0));
    track.append(make_clip(6.0, 0.0));

    // Push policy should not remove overlapped items; it inserts in sequence.
    let ok = track.resize_item(1, 2.0, 2.0, OverlapPolicy::Push, false);
    assert!(ok);

    // After resize, the resized item should be present starting near 2.0
    let idx = track.get_item_at_time(2.0 + 1e-6).unwrap();
    match &track.items[idx] {
        Item::Clip(c) => assert!((c.source_range.duration.value - 2.0).abs() < 1e-9),
        _ => panic!("expected clip after resize with push"),
    }
}
