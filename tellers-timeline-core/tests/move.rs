use tellers_timeline_core::track_methods::track_item_insert::{InsertPolicy, OverlapPolicy};
use std::collections::HashMap;
use tellers_timeline_core::*;

fn make_clip_with_id(duration: Seconds, id: &str) -> Item {
    let mut refs: HashMap<String, MediaReference> = HashMap::new();
    refs.insert(
        "DEFAULT_MEDIA".to_string(),
        MediaReference {
            otio_schema: "ExternalReference.1".to_string(),
            target_url: "mem://".to_string(),
            available_range: Some(TimeRange {
                otio_schema: "TimeRange.1".to_string(),
                duration: RationalTime { otio_schema: "RationalTime.1".to_string(), rate: 1.0, value: duration },
                start_time: RationalTime { otio_schema: "RationalTime.1".to_string(), rate: 1.0, value: 0.0 },
            }),
            name: None,
            available_image_bounds: None,
            metadata: serde_json::Value::Null,
        },
    );
    let mut it = Item::Clip(Clip {
        otio_schema: "Clip.2".to_string(),
        name: Some("c".to_string()),
        source_range: TimeRange { otio_schema: "TimeRange.1".to_string(), duration: RationalTime { otio_schema: "RationalTime.1".to_string(), rate: 1.0, value: duration }, start_time: RationalTime { otio_schema: "RationalTime.1".to_string(), rate: 1.0, value: 0.0 } },
        media_references: refs,
        active_media_reference_key: Some("DEFAULT_MEDIA".to_string()),
        metadata: serde_json::Value::Null,
    });
    it.set_id(Some(id.to_string()));
    it
}

#[test]
fn stack_get_and_remove_item() {
    let mut tl = Timeline::default();
    let mut t1 = Track::default();
    let mut t2 = Track::default();
    let id1 = "id-1";
    let id2 = "id-2";

    t1.items.push(make_clip_with_id(2.0, id1));
    t2.items.push(make_clip_with_id(3.0, id2));

    // assign track ids
    let tid1 = "tid-1".to_string();
    let tid2 = "tid-2".to_string();
    t1.set_id(Some(tid1.clone()));
    t2.set_id(Some(tid2.clone()));

    tl.tracks.children.push(t1);
    tl.tracks.children.push(t2);

    // get item by id
    let got = tl.tracks.get_item(id1).expect("should find");
    assert_eq!(got.0, 0);
    assert_eq!(got.1, 0);

    // remove without gap
    let removed = tl.tracks.delete_item(id1, false).expect("removed");
    assert_eq!(removed.0, 0);
    assert!(matches!(removed.1, Item::Clip(_)));
    assert!(tl.tracks.children[0].items.is_empty());
}

#[test]
fn stack_move_item_between_tracks_at_time() {
    let mut tl = Timeline::default();
    let mut t1 = Track::default();
    let mut t2 = Track::default();

    // assign track ids
    let tid1 = "tid-1".to_string();
    let tid2 = "tid-2".to_string();
    t1.set_id(Some(tid1.clone()));
    t2.set_id(Some(tid2.clone()));

    // put one clip in t1 and a gap clip in t2
    let id_move = "id-move";
    t1.items.push(make_clip_with_id(2.0, id_move));
    t2.items.push(Item::Gap(Gap::make_gap(1.0)));

    tl.tracks.children.push(t1);
    tl.tracks.children.push(t2);

    // move to track 2 at time 1.0
    let ok = tl.tracks.move_item_at_time(
        id_move,
        &tid2,
        1.0,
        false,
        InsertPolicy::InsertBeforeOrAfter,
        OverlapPolicy::Override,
    );
    assert!(ok);

    // source track is now empty
    assert!(tl.tracks.children[0].items.is_empty());

    // dest track now contains two items, and the moved clip is present
    let dest = &tl.tracks.children[1];
    assert_eq!(dest.items.len(), 2);
    let found = dest.items.iter().any(|it| it.get_id().as_deref() == Some(id_move));
    assert!(found);
}
