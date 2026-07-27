use tellers_timeline_core::{Gap, IdMetadataExt, Item, Stack, Track, TrackKind};

// Track-level public API used by both tellers-app (Flutter/Rust) and tellers-backend
// (Python binding forwards to these): get_track_by_id, get_track_by_name,
// find_tracks_by_name, delete_track.

fn track_with_gap(kind: TrackKind, id: &str, len: f64) -> Track {
    let mut track = Track::new(kind, Some(id.to_string()));
    track.items.push(Item::Gap(Gap::make_gap(len)));
    track
}

fn named_track(kind: TrackKind, id: &str, name: &str) -> Track {
    let mut track = track_with_gap(kind, id, 5.0);
    track.name = Some(name.to_string());
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

#[test]
fn get_track_by_name_finds_a_uniquely_named_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(named_track(TrackKind::Video, "v", "Main"));
    stack
        .children
        .push(named_track(TrackKind::Audio, "a", "music"));

    assert_eq!(stack.get_track_by_name("music").map(|(i, _)| i), Some(1));
    assert_eq!(
        stack
            .get_track_by_name("Main")
            .and_then(|(_, track)| track.get_id()),
        Some("v".to_string())
    );
}

#[test]
fn get_track_by_name_misses_unknown_and_mismatched_case() {
    let mut stack = Stack::default();
    stack
        .children
        .push(named_track(TrackKind::Video, "v", "Main"));

    assert!(stack.get_track_by_name("missing").is_none());
    // Matching is exact: callers own any normalization they want.
    assert!(stack.get_track_by_name("main").is_none());
}

#[test]
fn get_track_by_name_refuses_an_ambiguous_name() {
    let mut stack = Stack::default();
    stack
        .children
        .push(named_track(TrackKind::Video, "v1", "overlay"));
    stack
        .children
        .push(named_track(TrackKind::Video, "v2", "overlay"));

    // Two tracks share the name, so there is no single right answer.
    assert!(stack.get_track_by_name("overlay").is_none());
    // ...but the caller can still see both and decide.
    assert_eq!(
        stack
            .find_tracks_by_name("overlay")
            .iter()
            .map(|(i, _)| *i)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
}

#[test]
fn find_tracks_by_name_skips_unnamed_tracks() {
    let mut stack = Stack::default();
    stack
        .children
        .push(track_with_gap(TrackKind::Video, "v", 5.0));
    stack
        .children
        .push(named_track(TrackKind::Audio, "a", "music"));

    assert!(stack.find_tracks_by_name("").is_empty());
    assert_eq!(stack.find_tracks_by_name("music").len(), 1);
}
