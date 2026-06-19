use tellers_timeline_core::{Gap, IdMetadataExt, Item, Stack, Track, TrackKind};

// Track-level public API used by both tellers-app (Flutter/Rust) and tellers-backend
// (Python binding forwards to these): get_track_by_id, delete_track.

fn track_with_gap(kind: TrackKind, id: &str, len: f64) -> Track {
    let mut track = Track::new(kind, Some(id.to_string()));
    track.items.push(Item::Gap(Gap::make_gap(len)));
    track
}

#[test]
fn get_track_by_id_finds_existing_and_misses_unknown() {
    let mut stack = Stack::default();
    stack.children.push(track_with_gap(TrackKind::Video, "v", 5.0));
    stack.children.push(track_with_gap(TrackKind::Audio, "a", 5.0));

    assert_eq!(stack.get_track_by_id("v").map(|(index, _)| index), Some(0));
    assert_eq!(stack.get_track_by_id("a").map(|(index, _)| index), Some(1));
    assert!(stack.get_track_by_id("missing").is_none());
}

#[test]
fn delete_track_removes_only_the_named_track() {
    let mut stack = Stack::default();
    stack.children.push(track_with_gap(TrackKind::Video, "v", 5.0));
    stack.children.push(track_with_gap(TrackKind::Audio, "a", 5.0));

    let removed = stack.delete_track("v").expect("track removed");
    assert_eq!(removed.get_id().as_deref(), Some("v"));
    assert_eq!(stack.children.len(), 1);
    assert_eq!(stack.children[0].get_id().as_deref(), Some("a"));

    // Deleting an unknown track is a no-op.
    assert!(stack.delete_track("missing").is_none());
    assert_eq!(stack.children.len(), 1);
}
