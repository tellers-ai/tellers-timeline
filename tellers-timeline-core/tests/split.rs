use tellers_timeline_core::{Clip, Gap, Item, MediaSource, Seconds, Track};

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
fn split_clip_basic() {
    let mut track = Track::default();
    track.append(make_clip(10.0, 0.0));

    track.split_at_time(3.0);

    assert_eq!(track.items.len(), 2);
    match (&track.items[0], &track.items[1]) {
        (Item::Clip(c0), Item::Clip(c1)) => {
            assert_eq!(c0.duration, 3.0);
            assert_eq!(c1.duration, 7.0);
            assert_eq!(c1.source.media_start, 3.0);
        }
        _ => panic!("expected two clips after split"),
    }
}

#[test]
fn split_gap_basic() {
    let mut track = Track::default();
    track.append(Item::Gap(Gap::make_gap(5.0)));

    track.split_at_time(2.0);

    assert_eq!(track.items.len(), 2);
    match (&track.items[0], &track.items[1]) {
        (Item::Gap(g0), Item::Gap(g1)) => {
            assert_eq!(g0.duration, 2.0);
            assert_eq!(g1.duration, 3.0);
        }
        _ => panic!("expected two gaps after split"),
    }
}

#[test]
fn split_at_boundary_noop() {
    let mut track = Track::default();
    track.append(make_clip(5.0, 0.0));

    track.split_at_time(0.0);
    assert_eq!(track.items.len(), 1);

    track.split_at_time(5.0);
    assert_eq!(track.items.len(), 1);
}
