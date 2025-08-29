use std::collections::HashMap;
use tellers_timeline_core::{
    Clip, IdMetadataExt, Item, MediaReference, RationalTime, TimeRange, Track,
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
        MediaReference {
            otio_schema: "ExternalReference.1".to_string(),
            target_url: "media://dummy".to_string(),
            available_range: None,
            name: None,
            available_image_bounds: None,
            metadata: serde_json::Value::Null,
        },
    );
    Item::Clip(Clip {
        otio_schema: "Clip.2".to_string(),
        name: Some(name.to_string()),
        source_range: sr,
        media_references: refs,
        active_media_reference_key: Some("DEFAULT_MEDIA".to_string()),
        metadata: serde_json::Value::Null,
    })
}

#[test]
fn delete_clip_by_index_no_gap() {
    let mut track = Track::default();
    let c1 = match make_clip("c1", 5.0, 0.0) {
        Item::Clip(c) => c,
        _ => unreachable!(),
    };
    let c2 = make_clip("c2", 3.0, 0.0);
    track.append(Item::Clip(c1.clone()));
    track.append(c2.clone());

    let deleted = track.delete_clip(0, false);
    assert!(deleted);
    assert_eq!(track.items.len(), 1);
    match &track.items[0] {
        Item::Clip(c) => assert!((c.source_range.duration.value - 3.0).abs() < 1e-9),
        _ => panic!("expected clip"),
    }
}

#[test]
fn delete_clip_by_index_with_gap_and_merge() {
    let mut track = Track::default();
    track.append(make_clip("c1", 5.0, 0.0));
    track.append(Item::Gap(tellers_timeline_core::Gap::make_gap(2.0)));
    track.append(make_clip("c2", 3.0, 0.0));

    let deleted = track.delete_clip(0, true);
    assert!(deleted);
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
    track.append(Item::Clip(c1));
    track.append(make_clip("c2", 6.0, 0.0));

    let (idx, _it) = track.get_item_by_id(&id).expect("id should exist");
    let deleted = track.delete_clip(idx, true);
    assert!(deleted);
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
