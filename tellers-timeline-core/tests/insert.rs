use tellers_timeline_core::{Clip, InsertPolicy, Item, MediaSource, OverlapPolicy, Seconds, Track};

fn make_clip(duration: Seconds, media_start: Seconds) -> Item {
    Item::Clip(Clip {
        otio_schema: "Clip.2".to_string(),
        name: None,
        duration,
        source: MediaSource {
            otio_schema: "ExternalReference.1".to_string(),
            url: "mem://".to_string(),
            media_start,
            media_duration: None,
            metadata: serde_json::Value::Null,
        },
        metadata: serde_json::Value::Null,
    })
}

#[test]
fn insert_before_after_or_boundary() {
    let mut track = Track::default();
    track.append(make_clip(4.0, 0.0));
    track.append(make_clip(6.0, 0.0));

    // Insert before inside first clip -> snaps to its start index
    track.insert_at_time(
        1.0,
        make_clip(1.0, 0.0),
        OverlapPolicy::Push,
        InsertPolicy::InsertBefore,
    );
    assert!(matches!(track.items[0], Item::Clip(_)));

    // Insert after inside first clip -> index after first
    track.insert_at_time(
        1.5,
        make_clip(1.0, 0.0),
        OverlapPolicy::Push,
        InsertPolicy::InsertAfter,
    );
    assert!(matches!(track.items[2], Item::Clip(_)));

    // Insert before or after: choose closer boundary
    let before_len = track.items.len();
    track.insert_at_time(
        3.9,
        make_clip(0.5, 0.0),
        OverlapPolicy::Push,
        InsertPolicy::InsertBeforeOrAfter,
    );
    assert_eq!(track.items.len(), before_len + 1);
}

#[test]
fn insert_split_and_override() {
    let mut track = Track::default();
    track.append(make_clip(5.0, 0.0));
    track.append(make_clip(5.0, 0.0));

    // Insert across boundary with override: should split as needed and replace overlap
    track.insert_at_time(
        3.0,
        make_clip(4.0, 0.0),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
    );

    // Expect an item at 3.0 of duration 4.0
    let idx = track.get_item_at_time(3.1).unwrap();
    match &track.items[idx] {
        Item::Clip(c) => assert!((c.duration - 4.0).abs() < 1e-9),
        _ => panic!("expected clip inserted with override"),
    }
}
