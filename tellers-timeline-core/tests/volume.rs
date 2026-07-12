use tellers_timeline_core::Track;

fn clip_json() -> &'static str {
    r#"{
        "OTIO_SCHEMA": "Clip.2",
        "source_range": {
            "OTIO_SCHEMA": "TimeRange.1",
            "start_time": { "OTIO_SCHEMA": "RationalTime.1", "rate": 1, "value": 0 },
            "duration": { "OTIO_SCHEMA": "RationalTime.1", "rate": 1, "value": 2 }
        }
    }"#
}

#[test]
fn track_set_volume_applies_to_every_clip() {
    let track_json = format!(
        r#"{{
            "OTIO_SCHEMA": "Track.1",
            "kind": "Audio",
            "children": [{c}, {c}]
        }}"#,
        c = clip_json(),
    );
    let mut track: Track = serde_json::from_str(&track_json).unwrap();

    // Clips start at the default volume.
    for item in &track.items {
        assert_eq!(item.get_volume(), 1.0);
    }

    track.set_volume(0.25);

    // Every item now reflects the track volume.
    for item in &track.items {
        assert_eq!(item.get_volume(), 0.25);
    }
}
