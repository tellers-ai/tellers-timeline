use tellers_timeline_core::{Clip, Track};

const RICH_TEXT_REF: &str = r##"{
    "OTIO_SCHEMA": "GeneratorReference.1",
    "generator_kind": "Rich",
    "parameters": {
        "Resolve_OTIO": [
            {
                "Effect Name": "Rich Text",
                "Enabled": true,
                "Name": "Rich Text",
                "Parameters": [
                    { "Parameter ID": "title blob", "Title HTML": "<p>Hi</p>" }
                ],
                "Type": 24
            }
        ]
    }
}"##;

fn source_range() -> &'static str {
    r#"{ "OTIO_SCHEMA": "TimeRange.1",
         "start_time": { "OTIO_SCHEMA": "RationalTime.1", "rate": 1, "value": 0 },
         "duration": { "OTIO_SCHEMA": "RationalTime.1", "rate": 1, "value": 2 } }"#
}

#[test]
fn clip_get_rich_text_reads_active_reference_title() {
    let clip_json = format!(
        r#"{{
            "OTIO_SCHEMA": "Clip.2",
            "active_media_reference_key": "DEFAULT_MEDIA",
            "media_references": {{ "DEFAULT_MEDIA": {rich} }},
            "source_range": {sr}
        }}"#,
        rich = RICH_TEXT_REF,
        sr = source_range(),
    );
    let clip: Clip = serde_json::from_str(&clip_json).unwrap();
    assert_eq!(clip.get_rich_text(), Some("<p>Hi</p>".to_string()));
}

#[test]
fn track_average_volume_means_clip_volumes_ignoring_gaps() {
    let track_json = format!(
        r#"{{
            "OTIO_SCHEMA": "Track.1",
            "kind": "Audio",
            "children": [
                {{ "OTIO_SCHEMA": "Clip.2", "source_range": {sr} }},
                {{ "OTIO_SCHEMA": "Clip.2", "source_range": {sr} }},
                {{ "OTIO_SCHEMA": "Gap.1", "source_range": {sr} }}
            ]
        }}"#,
        sr = source_range(),
    );
    let mut track: Track = serde_json::from_str(&track_json).unwrap();

    // Two clips at the default volume (1.0); the gap is ignored.
    assert_eq!(track.average_volume(), 1.0);

    track.set_volume(0.5);
    assert_eq!(track.average_volume(), 0.5);
}
