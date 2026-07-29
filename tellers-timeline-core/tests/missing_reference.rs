//! OTIO's `MissingReference` is accepted as an external reference with no
//! `target_url`.
//!
//! The OTIO AAF adapter answers every SourceMob that records no locator URL
//! with a `MissingReference`, so an imported Avid sequence used to fail the
//! whole project load with `unknown variant MissingReference.1` — including
//! for clips whose media had been resolved, since the schema tag has nothing
//! to do with whether we found the asset.

use tellers_timeline_core::{Item, MediaReference, Timeline};

/// A one-clip timeline whose only media reference uses `schema`, shaped like
/// what the AAF adapter emits.
fn timeline_json(schema: &str, extra_fields: &str) -> String {
    format!(
        r#"{{
            "OTIO_SCHEMA": "Timeline.1",
            "name": "SEQ",
            "tracks": {{
                "OTIO_SCHEMA": "Stack.1",
                "name": "tracks",
                "children": [{{
                    "OTIO_SCHEMA": "Track.1",
                    "name": "V1",
                    "kind": "Video",
                    "children": [{{
                        "OTIO_SCHEMA": "Clip.2",
                        "name": "A001C001",
                        "active_media_reference_key": "DEFAULT_MEDIA",
                        "media_references": {{
                            "DEFAULT_MEDIA": {{
                                "OTIO_SCHEMA": "{schema}",
                                "name": "A001C001",
                                {extra_fields}
                                "metadata": {{ "media_id": "asset-1" }}
                            }}
                        }},
                        "source_range": {{
                            "OTIO_SCHEMA": "TimeRange.1",
                            "start_time": {{
                                "OTIO_SCHEMA": "RationalTime.1",
                                "rate": 25.0, "value": 0.0
                            }},
                            "duration": {{
                                "OTIO_SCHEMA": "RationalTime.1",
                                "rate": 25.0, "value": 100.0
                            }}
                        }}
                    }}]
                }}]
            }}
        }}"#
    )
}

fn only_reference(timeline: &Timeline) -> &MediaReference {
    let track = &timeline.tracks.children[0];
    let Item::Clip(clip) = &track.items[0] else {
        panic!("expected a clip");
    };
    clip.media_references
        .values()
        .next()
        .expect("clip keeps its media reference")
}

#[test]
fn missing_reference_parses_as_an_empty_external_reference() {
    let json = timeline_json("MissingReference.1", "");

    let timeline: Timeline = serde_json::from_str(&json).expect("MissingReference is accepted");

    match only_reference(&timeline) {
        MediaReference::ExternalReference { target_url, .. } => {
            assert_eq!(target_url, "", "no media to point at yet");
        }
        other => panic!("expected an external reference, got {other:?}"),
    }
}

#[test]
fn missing_reference_keeps_the_metadata_that_links_it_to_an_asset() {
    // The whole point of keeping the clip: the media_id survives, so a
    // resolved clip is still linked even when the AAF recorded no path.
    let json = timeline_json("MissingReference.1", "");

    let timeline: Timeline = serde_json::from_str(&json).unwrap();

    let MediaReference::ExternalReference { metadata, .. } = only_reference(&timeline) else {
        panic!("expected an external reference");
    };
    assert_eq!(metadata["media_id"], "asset-1");
}

#[test]
fn missing_reference_re_serializes_as_an_external_reference() {
    let json = timeline_json("MissingReference.1", "");
    let timeline: Timeline = serde_json::from_str(&json).unwrap();

    let out = serde_json::to_string(&timeline).unwrap();

    assert!(out.contains("ExternalReference.1"));
    assert!(!out.contains("MissingReference.1"));
}

#[test]
fn an_ordinary_external_reference_is_untouched() {
    let json = timeline_json("ExternalReference.1", r#""target_url": "s3://bucket/a.mxf","#);

    let timeline: Timeline = serde_json::from_str(&json).unwrap();

    match only_reference(&timeline) {
        MediaReference::ExternalReference { target_url, .. } => {
            assert_eq!(target_url, "s3://bucket/a.mxf");
        }
        other => panic!("expected an external reference, got {other:?}"),
    }
}

#[test]
fn a_genuinely_unknown_reference_schema_is_still_rejected() {
    let json = timeline_json("SomeFutureReference.1", "");

    let result: Result<Timeline, _> = serde_json::from_str(&json);

    assert!(
        result.is_err(),
        "only MissingReference is aliased; anything else should still fail loudly"
    );
}

#[test]
fn image_sequence_reference_is_still_rejected() {
    // OTIO registers four MediaReference subclasses — ExternalReference,
    // GeneratorReference, MissingReference and ImageSequenceReference — and
    // this enum models two of them. An OTIO file using an image sequence
    // therefore still fails to load, the same way MissingReference did.
    // Recorded so the gap is visible rather than discovered in the app.
    let json = timeline_json("ImageSequenceReference.1", "");

    let result: Result<Timeline, _> = serde_json::from_str(&json);

    assert!(result.is_err());
}
