use std::collections::HashMap;
use tellers_timeline_core::{
    Clip, IdMetadataExt, Item, MediaReference, RationalTime, Stack, TimeRange, Track,
};

fn make_clip(name: &str, duration: f64, media_start: f64) -> Item {
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
            target_url: "media://dummy".to_string(),
            available_range: None,
            name: None,
            available_image_bounds: None,
            metadata: serde_json::Value::Null,
        },
    );
    Item::Clip(Clip {
        otio_schema: "Clip.2".to_string(),
        enabled: true,
        name: Some(name.to_string()),
        source_range: sr,
        media_references: refs,
        active_media_reference_key: Some("DEFAULT_MEDIA".to_string()),
        metadata: serde_json::Value::Null,
        effects: Vec::new(),
    })
}

#[test]
fn delete_clip_by_index_no_gap() {
    let mut track = Track::default();
    let mut c1 = match make_clip("c1", 5.0, 0.0) {
        Item::Clip(c) => c,
        _ => unreachable!(),
    };
    c1.set_id(Some("c1".to_string()));
    let c2 = make_clip("c2", 3.0, 0.0);
    track.items.push(Item::Clip(c1.clone()));
    track.items.push(c2.clone());
    let mut stack = Stack {
        children: vec![track],
        ..Stack::default()
    };

    let deleted = stack.delete_item("c1", false);
    assert_eq!(deleted.len(), 1);
    let track = &stack.children[0];
    assert_eq!(track.items.len(), 1);
    match &track.items[0] {
        Item::Clip(c) => assert!((c.source_range.duration.value - 3.0).abs() < 1e-9),
        _ => panic!("expected clip"),
    }
}

#[test]
fn delete_clip_by_index_with_gap_and_merge() {
    let mut track = Track::default();
    let mut c1 = make_clip("c1", 5.0, 0.0);
    c1.set_id(Some("c1".to_string()));
    track.items.push(c1);
    track
        .items
        .push(Item::Gap(tellers_timeline_core::Gap::make_gap(2.0)));
    track.items.push(make_clip("c2", 3.0, 0.0));
    let mut stack = Stack {
        children: vec![track],
        ..Stack::default()
    };

    let deleted = stack.delete_item("c1", true);
    assert_eq!(deleted.len(), 1);
    let track = &stack.children[0];
    // Expect leading gap of 5.0 merged with following 2.0 -> 7.0, then c2
    assert_eq!(track.items.len(), 2);
    match (&track.items[0], &track.items[1]) {
        (Item::Gap(g), Item::Clip(c2)) => {
            assert!((g.source_range.duration.value - 7.0).abs() < 1e-9);
            assert!((c2.source_range.duration.value - 3.0).abs() < 1e-9);
        }
        _ => panic!("unexpected items: {:#?}", track.items),
    }
}

#[test]
fn delete_clip_via_getter_with_gap() {
    let mut track = Track::default();
    // Build two clips and set an id on the first
    let mut c1 = match make_clip("c1", 4.0, 0.0) {
        Item::Clip(c) => c,
        _ => unreachable!(),
    };
    let id = "id-c1".to_string();
    c1.set_id(Some(id.clone()));
    track.items.push(Item::Clip(c1));
    track.items.push(make_clip("c2", 6.0, 0.0));
    let mut stack = Stack {
        children: vec![track],
        ..Stack::default()
    };

    let deleted = stack.delete_item(&id, true);
    assert_eq!(deleted.len(), 1);
    let track = &stack.children[0];
    // Expect gap(4.0) then c2, and no adjacent gaps to merge
    assert_eq!(track.items.len(), 2);
    match (&track.items[0], &track.items[1]) {
        (Item::Gap(g), Item::Clip(c2)) => {
            assert!((g.source_range.duration.value - 4.0).abs() < 1e-9);
            assert!((c2.source_range.duration.value - 6.0).abs() < 1e-9);
        }
        _ => panic!("unexpected items: {:#?}", track.items),
    }
}
