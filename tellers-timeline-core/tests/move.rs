use tellers_timeline_core::track_methods::track_item_insert::{InsertPolicy, OverlapPolicy};
use tellers_timeline_core::*;
use uuid::Uuid;

fn make_clip_with_id(duration: Seconds, id: Uuid) -> Item {
    let mut it = Item::Clip(Clip {
        otio_schema: "Clip.2".to_string(),
        name: Some("c".to_string()),
        duration,
        source: MediaSource {
            otio_schema: "ExternalReference.1".to_string(),
            url: "mem://".to_string(),
            media_start: 0.0,
            media_duration: None,
            metadata: serde_json::Value::Null,
        },
        metadata: serde_json::Value::Null,
    });
    it.set_id(Some(id));
    it
}

#[test]
fn stack_get_and_remove_item() {
    let mut tl = Timeline::default();
    let mut t1 = Track::default();
    let mut t2 = Track::default();
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();

    t1.items.push(make_clip_with_id(2.0, id1));
    t2.items.push(make_clip_with_id(3.0, id2));

    // assign track ids
    let tid1 = Uuid::new_v4();
    let tid2 = Uuid::new_v4();
    t1.set_id(Some(tid1));
    t2.set_id(Some(tid2));

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
    let tid1 = Uuid::new_v4();
    let tid2 = Uuid::new_v4();
    t1.set_id(Some(tid1));
    t2.set_id(Some(tid2));

    // put one clip in t1 and a gap clip in t2
    let id_move = Uuid::new_v4();
    t1.items.push(make_clip_with_id(2.0, id_move));
    t2.items.push(Item::Gap(Gap::make_gap(1.0)));

    tl.tracks.children.push(t1);
    tl.tracks.children.push(t2);

    // move to track 2 at time 1.0
    let ok = tl.tracks.move_item_at_time(
        id_move,
        tid2,
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
    let found = dest
        .items
        .iter()
        .any(|it| it.get_id().as_ref() == Some(&id_move));
    assert!(found);
}
