use tellers_timeline_core::{make_gap, Clip, InsertPolicy, Item, MediaSource, OverridePolicy, Track};

fn make_clip(name: &str, start: f64, duration: f64, media_start: f64) -> Item {
    Item::Clip(Clip {
        otio_schema: "Clip.2".to_string(),
        name: Some(name.to_string()),
        start,
        duration,
        source: MediaSource {
            otio_schema: "ExternalReference.1".to_string(),
            url: "media://dummy".to_string(),
            media_start,
            media_duration: None,
            metadata: serde_json::Value::Null,
        },
        metadata: serde_json::Value::Null,
    })
}

#[test]
fn naive_respects_position_inside_gap_without_split() {
    let mut track = Track::default();
    track.append(make_gap(0.0, 10.0));

    let clip = make_clip("ins", 0.0, 2.0, 0.0);
    track.insert_at_time_with(3.0, clip, OverridePolicy::Naive, InsertPolicy::SplitAndInsert);

    // Naive respects effective position (SplitAndInsert -> keep requested), but does not split
    assert_eq!(track.items.len(), 2);
    let (g, c) = match (&track.items[0], &track.items[1]) {
        (Item::Gap(g), Item::Clip(c)) => (g, c),
        _ => panic!("unexpected items order/types: {:#?}", track.items),
    };
    assert!((g.start - 0.0).abs() < 1e-9 && (g.duration - 10.0).abs() < 1e-9);
    assert!((c.start - 3.0).abs() < 1e-9 && (c.duration - 2.0).abs() < 1e-9);
}

#[test]
fn naive_respects_position_inside_clip_without_split() {
    let mut track = Track::default();
    track.append(make_clip("c1", 0.0, 10.0, 0.0));

    let clip = make_clip("ins", 0.0, 2.0, 0.0);
    track.insert_at_time_with(3.0, clip, OverridePolicy::Naive, InsertPolicy::SplitAndInsert);

    // Naive respects effective position but does not split
    assert_eq!(track.items.len(), 2);
    let (c0, ins) = match (&track.items[0], &track.items[1]) {
        (Item::Clip(c0), Item::Clip(ins)) => (c0, ins),
        _ => panic!("unexpected items order/types: {:#?}", track.items),
    };
    assert!((c0.start - 0.0).abs() < 1e-9 && (c0.duration - 10.0).abs() < 1e-9);
    assert!((ins.start - 3.0).abs() < 1e-9 && (ins.duration - 2.0).abs() < 1e-9);
}

#[test]
fn override_split_and_insert_removes_overlaps() {
    let mut track = Track::default();
    track.append(make_clip("c1", 0.0, 10.0, 0.0));
    track.append(make_clip("c2", 10.0, 5.0, 0.0));

    // Insert inside c1 at 3.0 for 4.0 seconds. With Override+Split, we split c1, insert, and remove overlaps in [3,7)
    let ins = make_clip("ins", 0.0, 4.0, 0.0);
    track.insert_at_time_with(3.0, ins, OverridePolicy::Override, InsertPolicy::SplitAndInsert);

    // Expect: c1_left (0..3), ins (3..7), c1_right trimmed to start at 7.0 with remaining (10-7)=3.0, then c2 intact.
    assert_eq!(track.items.len(), 4);
    match (&track.items[0], &track.items[1], &track.items[2], &track.items[3]) {
        (Item::Clip(c0), Item::Clip(ins), Item::Clip(c1r), Item::Clip(c2)) => {
            assert!((c0.start - 0.0).abs() < 1e-9 && (c0.duration - 3.0).abs() < 1e-9);
            assert!((ins.start - 3.0).abs() < 1e-9 && (ins.duration - 4.0).abs() < 1e-9);
            assert!((c1r.start - 7.0).abs() < 1e-9 && (c1r.duration - 3.0).abs() < 1e-9);
            assert!((c2.start - 10.0).abs() < 1e-9 && (c2.duration - 5.0).abs() < 1e-9);
        }
        _ => panic!("unexpected items order/types: {:#?}", track.items),
    }
}

#[test]
fn push_split_and_insert_shifts_after_end() {
    let mut track = Track::default();
    track.append(make_clip("c1", 0.0, 10.0, 0.0));
    track.append(make_clip("c2", 10.0, 5.0, 0.0));

    // Insert inside c1 at 4.0 for 2.0 seconds. With Push+Split, split c1 and insert; items starting at >= end (6.0) shift by 2.0
    let ins = make_clip("ins", 0.0, 2.0, 0.0);
    track.insert_at_time_with(4.0, ins, OverridePolicy::Push, InsertPolicy::SplitAndInsert);

    // Expect: c1_left (0..4), ins (4..6), c1_right (6..10) unchanged timing, c2 shifted to 12..17
    assert_eq!(track.items.len(), 4);
    match (&track.items[0], &track.items[1], &track.items[2], &track.items[3]) {
        (Item::Clip(c0), Item::Clip(ins), Item::Clip(c1r), Item::Clip(c2)) => {
            assert!((c0.start - 0.0).abs() < 1e-9 && (c0.duration - 4.0).abs() < 1e-9);
            assert!((ins.start - 4.0).abs() < 1e-9 && (ins.duration - 2.0).abs() < 1e-9);
            assert!((c1r.start - 6.0).abs() < 1e-9 && (c1r.duration - 4.0).abs() < 1e-9);
            assert!((c2.start - 12.0).abs() < 1e-9 && (c2.duration - 5.0).abs() < 1e-9);
        }
        _ => panic!("unexpected items order/types: {:#?}", track.items),
    }
}

#[test]
fn naive_trailing_gap_when_inserting_after_end() {
    let mut track = Track::default();
    track.append(make_clip("c1", 0.0, 5.0, 0.0));

    let ins = make_clip("ins", 0.0, 1.0, 0.0);
    track.insert_at_time_with(10.0, ins, OverridePolicy::Naive, InsertPolicy::InsertBeforeOrAfter);

    assert_eq!(track.items.len(), 3);
    match (&track.items[0], &track.items[1], &track.items[2]) {
        (Item::Clip(c1), Item::Gap(g), Item::Clip(c2)) => {
            assert!((c1.start - 0.0).abs() < 1e-9 && (c1.duration - 5.0).abs() < 1e-9);
            assert!((g.start - 5.0).abs() < 1e-9 && (g.duration - 5.0).abs() < 1e-9);
            assert!((c2.start - 10.0).abs() < 1e-9 && (c2.duration - 1.0).abs() < 1e-9);
        }
        _ => panic!("unexpected items order/types: {:#?}", track.items),
    }
}

#[test]
fn keep_trailing_gap_when_inserting_after_end() {
    let mut track = Track::default();
    track.append(make_clip("c1", 0.0, 2.0, 0.0));

    let ins = make_clip("ins", 0.0, 1.0, 0.0);
    track.insert_at_time_with(5.0, ins, OverridePolicy::Keep, InsertPolicy::InsertBeforeOrAfter);

    assert_eq!(track.items.len(), 3);
    match (&track.items[0], &track.items[1], &track.items[2]) {
        (Item::Clip(c1), Item::Gap(g), Item::Clip(c2)) => {
            assert!((c1.start - 0.0).abs() < 1e-9 && (c1.duration - 2.0).abs() < 1e-9);
            assert!((g.start - 2.0).abs() < 1e-9 && (g.duration - 3.0).abs() < 1e-9);
            assert!((c2.start - 5.0).abs() < 1e-9 && (c2.duration - 1.0).abs() < 1e-9);
        }
        _ => panic!("unexpected items order/types: {:#?}", track.items),
    }
}

#[test]
fn override_trims_or_places_on_boundary() {
    let mut track = Track::default();
    track.append(make_clip("c1", 0.0, 5.0, 0.0));
    track.append(make_clip("c2", 5.0, 5.0, 0.0));

    let ins = make_clip("ins", 0.0, 4.0, 0.0);
    track.insert_at_time_with(3.0, ins, OverridePolicy::Override, InsertPolicy::InsertBeforeOrAfter);

    assert_eq!(track.items.len(), 3);
    match (&track.items[0], &track.items[1], &track.items[2]) {
        (Item::Clip(c1), Item::Clip(ins), Item::Clip(c2)) => {
            // Insert policy chose the nearer boundary (end of c1 at 5.0)
            assert!((c1.start - 0.0).abs() < 1e-9 && (c1.duration - 5.0).abs() < 1e-9);
            assert!((ins.start - 5.0).abs() < 1e-9 && (ins.duration - 4.0).abs() < 1e-9);
            assert!((c2.start - 9.0).abs() < 1e-9 && (c2.duration - 1.0).abs() < 1e-9);
        }
        _ => panic!("unexpected items order/types: {:#?}", track.items),
    }
}

#[test]
fn push_splits_or_shifts_on_boundary() {
    let mut track = Track::default();
    track.append(make_clip("c1", 0.0, 5.0, 0.0));
    track.append(make_clip("c2", 5.0, 5.0, 0.0));

    let ins = make_clip("ins", 0.0, 2.0, 0.0);
    track.insert_at_time_with(3.0, ins, OverridePolicy::Push, InsertPolicy::InsertBeforeOrAfter);

    assert_eq!(track.items.len(), 3);
    match (&track.items[0], &track.items[1], &track.items[2]) {
        (Item::Clip(c1), Item::Clip(ins), Item::Clip(c2)) => {
            // Insertion point falls on boundary (end of c1 at 5.0), so c1 isn't split
            assert!((c1.start - 0.0).abs() < 1e-9 && (c1.duration - 5.0).abs() < 1e-9);
            assert!((ins.start - 5.0).abs() < 1e-9 && (ins.duration - 2.0).abs() < 1e-9);
            // c2 shifted by +2
            assert!((c2.start - 7.0).abs() < 1e-9 && (c2.duration - 5.0).abs() < 1e-9);
        }
        _ => panic!("unexpected items order/types: {:#?}", track.items),
    }
}


