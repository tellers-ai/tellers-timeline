use std::collections::HashMap;
use tellers_timeline_core::*;

fn make_video_clip() -> Clip {
    let mut refs: HashMap<String, MediaReference> = HashMap::new();
    refs.insert(
        "DEFAULT_MEDIA".to_string(),
        MediaReference::ExternalReference {
            target_url: "mem://".to_string(),
            available_range: Some(TimeRange {
                otio_schema: "TimeRange.1".to_string(),
                duration: RationalTime { otio_schema: "RationalTime.1".to_string(), rate: 1.0, value: 10.0 },
                start_time: RationalTime { otio_schema: "RationalTime.1".to_string(), rate: 1.0, value: 0.0 },
            }),
            name: None,
            available_image_bounds: None,
            metadata: serde_json::Value::Null,
        },
    );
    Clip {
        otio_schema: "Clip.2".to_string(),
        enabled: true,
        name: Some("c".to_string()),
        source_range: TimeRange {
            otio_schema: "TimeRange.1".to_string(),
            duration: RationalTime { otio_schema: "RationalTime.1".to_string(), rate: 1.0, value: 10.0 },
            start_time: RationalTime { otio_schema: "RationalTime.1".to_string(), rate: 1.0, value: 0.0 },
        },
        media_references: refs,
        active_media_reference_key: Some("DEFAULT_MEDIA".to_string()),
        metadata: serde_json::Value::Null,
        effects: Vec::new(),
    }
}

#[test]
fn get_position_defaults_to_center_without_transform_effect() {
    let clip = make_video_clip();
    let pos = clip.get_position();
    // Resolve pan/tilt space: 0.0 is the screen center.
    assert_eq!(pos.x, 0.0);
    assert_eq!(pos.y, 0.0);
    assert_eq!(pos.rotation, 0.0);
    assert_eq!(pos.zoom_x, 1.0);
    assert_eq!(pos.zoom_y, 1.0);
}

#[test]
fn get_position_defaults_to_center_with_empty_transform_parameters() {
    // DaVinci Resolve exports untouched clips with a Transform effect whose
    // Parameters array is empty; absent parameters mean Resolve defaults
    // (pan = 0, tilt = 0 → centered).
    let json = r#"
    {
        "OTIO_SCHEMA": "Clip.2",
        "name": "Resolve Clip",
        "source_range": {
            "OTIO_SCHEMA": "TimeRange.1",
            "start_time": { "OTIO_SCHEMA": "RationalTime.1", "rate": 30.0, "value": 0.0 },
            "duration": { "OTIO_SCHEMA": "RationalTime.1", "rate": 30.0, "value": 100.0 }
        },
        "media_references": {
            "DEFAULT_MEDIA": {
                "OTIO_SCHEMA": "ExternalReference.1",
                "metadata": {},
                "name": "clip.mp4",
                "available_range": null,
                "available_image_bounds": null,
                "target_url": "mem://clip.mp4"
            }
        },
        "active_media_reference_key": "DEFAULT_MEDIA",
        "metadata": {},
        "effects": [
            {
                "OTIO_SCHEMA": "Effect.1",
                "name": "",
                "effect_name": "Resolve Effect",
                "metadata": {
                    "Resolve_OTIO": {
                        "Effect Name": "Transform",
                        "Enabled": true,
                        "Name": "Transform",
                        "Parameters": [],
                        "Type": 2
                    }
                }
            }
        ]
    }
    "#;
    let clip: Clip = serde_json::from_str(json).expect("Failed to parse clip");
    let pos = clip.get_position();
    assert_eq!(pos.x, 0.0);
    assert_eq!(pos.y, 0.0);
}

#[test]
fn position_get_set_round_trip_keeps_clip_centered() {
    // Regression for tellers-app#2253: saving and restoring the position of a
    // default (centered) clip must not write a pan/tilt offset.
    let mut clip = make_video_clip();
    let saved = clip.get_position();
    clip.set_position(saved);

    let pos = clip.get_position();
    assert_eq!(pos.x, 0.0);
    assert_eq!(pos.y, 0.0);

    // The written Transform effect must store pan/tilt 0 (Resolve center).
    let transform = clip
        .effects
        .iter()
        .find_map(|e| e.metadata.resolve_otio.as_ref())
        .expect("set_position should create a Transform effect");
    for parameter in &transform.parameters {
        if let ResolveOTIOParameter::Double(param) = parameter {
            if param.parameter_id == "transformationPan" || param.parameter_id == "transformationTilt" {
                assert_eq!(param.parameter_value, 0.0, "{} must stay centered", param.parameter_id);
            }
        }
    }
}
