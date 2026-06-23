use tellers_timeline_core::{InsertPolicy, OverlapPolicy, Timeline};

fn load_space_talking_cat_v2() -> Timeline {
    let otio = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/space_talking_cat_v2.otio"),
    )
    .expect("fixture exists");
    let mut timeline: Timeline = serde_json::from_str(&otio).expect("parse otio");
    timeline.tracks.sanitize();
    timeline
}

fn hh10_audio_track_snapshots(timeline: &Timeline) -> Vec<(String, usize, Vec<f64>)> {
    ["A1", "A2", "A3", "A4", "A5", "A6", "A7"]
        .iter()
        .map(|track_id| {
            let (_, track) = timeline.tracks.get_track_by_id(track_id).unwrap();
            (
                track_id.to_string(),
                track.items.len(),
                track.items.iter().map(|item| item.duration()).collect(),
            )
        })
        .collect()
}

fn assert_hh10_audio_tracks_unchanged(
    timeline: &Timeline,
    snapshots: &[(String, usize, Vec<f64>)],
) {
    for (track_id, items_before, durations_before) in snapshots {
        let (_, track) = timeline.tracks.get_track_by_id(track_id).unwrap();
        assert_eq!(
            track.items.len(),
            *items_before,
            "{track_id} item count changed"
        );
        for (index, duration) in durations_before.iter().enumerate() {
            assert!(
                (track.items[index].duration() - duration).abs() < 1e-6,
                "{track_id}[{index}] duration changed"
            );
        }
    }
}

fn long_clip_start_on_track(timeline: &Timeline, track_id: &str) -> f64 {
    let (_, track) = timeline.tracks.get_track_by_id(track_id).unwrap();
    let mut pos = 0.0;
    for item in &track.items {
        if item.duration() > 60.0 {
            return pos;
        }
        pos += item.duration();
    }
    panic!("no long clip on {track_id}");
}

#[test]
fn space_talking_cat_v2_push_move_paris_to_main_does_not_push_a1_a7() {
    let mut timeline = load_space_talking_cat_v2();
    let snapshots = hh10_audio_track_snapshots(&timeline);
    let hh10_v2_start_before = long_clip_start_on_track(&timeline, "V2");

    assert!(timeline.tracks.move_item_at_time(
        "1acd33f0570a",
        "23d9c4f632ed",
        16.84,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Push,
    ));

    assert_hh10_audio_tracks_unchanged(&timeline, &snapshots);
    assert!(
        (long_clip_start_on_track(&timeline, "V2") - hh10_v2_start_before).abs() < 1e-6,
        "V2 HH10 start must stay put when paris leaves the track"
    );
}

#[test]
fn space_talking_cat_v2_push_backward_move_does_not_push_v2_hh10() {
    let mut timeline = load_space_talking_cat_v2();
    assert!(timeline.tracks.move_item_at_time(
        "1acd33f0570a",
        "23d9c4f632ed",
        16.84,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Push,
    ));
    let snapshots = hh10_audio_track_snapshots(&timeline);
    let hh10_v2_start_before = long_clip_start_on_track(&timeline, "V2");

    assert!(timeline.tracks.move_item_at_time(
        "1acd33f0570a",
        "V2",
        5.41,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Push,
    ));

    assert_hh10_audio_tracks_unchanged(&timeline, &snapshots);
    assert!(
        (long_clip_start_on_track(&timeline, "V2") - hh10_v2_start_before).abs() < 0.01,
        "V2 HH10 must not be pushed on backward move"
    );
}
