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
                duration: RationalTime {
                    otio_schema: "RationalTime.1".to_string(),
                    rate: 1.0,
                    value: 10.0,
                },
                start_time: RationalTime {
                    otio_schema: "RationalTime.1".to_string(),
                    rate: 1.0,
                    value: 0.0,
                },
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
            duration: RationalTime {
                otio_schema: "RationalTime.1".to_string(),
                rate: 1.0,
                value: 10.0,
            },
            start_time: RationalTime {
                otio_schema: "RationalTime.1".to_string(),
                rate: 1.0,
                value: 0.0,
            },
        },
        media_references: refs,
        active_media_reference_key: Some("DEFAULT_MEDIA".to_string()),
        metadata: serde_json::Value::Null,
        effects: Vec::new(),
    }
}

fn crop_param<'a>(resolve_otio: &'a ResolveOTIOEffect, parameter_id: &str) -> Option<f64> {
    resolve_otio.parameters.iter().find_map(|parameter| {
        if let ResolveOTIOParameter::Double(param) = parameter {
            if param.parameter_id == parameter_id {
                return Some(param.parameter_value);
            }
        }
        None
    })
}

fn cropping_effect(clip: &Clip) -> &ResolveOTIOEffect {
    clip.effects
        .iter()
        .find_map(|effect| {
            let resolve_otio = effect.metadata.resolve_otio.as_ref()?;
            if resolve_otio.effect_name == "Cropping" || resolve_otio.name == "Cropping" {
                Some(resolve_otio)
            } else {
                None
            }
        })
        .expect("clip should have a Cropping Resolve effect")
}

#[test]
fn get_crop_defaults_without_cropping_effect() {
    let clip = make_video_clip();
    let crop = clip.get_crop();
    assert_eq!(crop.crop_left, 0.0);
    assert_eq!(crop.crop_right, 0.0);
    assert_eq!(crop.crop_top, 0.0);
    assert_eq!(crop.crop_bottom, 0.0);
}

#[test]
fn get_crop_parses_resolve_cropping_effect() {
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
                        "Effect Name": "Cropping",
                        "Enabled": true,
                        "Name": "Cropping",
                        "Parameters": [
                            {
                                "Variant Type": "Double",
                                "Parameter ID": "cropLeft",
                                "Parameter Value": 0.12,
                                "Default Parameter Value": 0.0,
                                "maxValue": 1.0,
                                "minValue": 0.0
                            },
                            {
                                "Variant Type": "Double",
                                "Parameter ID": "cropRight",
                                "Parameter Value": 0.22,
                                "Default Parameter Value": 0.0,
                                "maxValue": 1.0,
                                "minValue": 0.0
                            },
                            {
                                "Variant Type": "Double",
                                "Parameter ID": "cropTop",
                                "Parameter Value": 0.32,
                                "Default Parameter Value": 0.0,
                                "maxValue": 1.0,
                                "minValue": 0.0
                            },
                            {
                                "Variant Type": "Double",
                                "Parameter ID": "cropBottom",
                                "Parameter Value": 0.42,
                                "Default Parameter Value": 0.0,
                                "maxValue": 1.0,
                                "minValue": 0.0
                            }
                        ],
                        "Type": 3
                    }
                }
            }
        ]
    }
    "#;
    let clip: Clip = serde_json::from_str(json).expect("Failed to parse clip");
    let crop = clip.get_crop();
    assert!((crop.crop_left - 0.12).abs() < 0.001);
    assert!((crop.crop_right - 0.22).abs() < 0.001);
    assert!((crop.crop_top - 0.32).abs() < 0.001);
    assert!((crop.crop_bottom - 0.42).abs() < 0.001);
}

#[test]
fn crop_get_set_round_trip_writes_cropping_effect() {
    let mut clip = make_video_clip();
    clip.set_crop(MediaReferenceCrop {
        crop_left: 0.1,
        crop_right: 0.2,
        crop_top: 0.3,
        crop_bottom: 0.4,
    });

    let crop = clip.get_crop();
    assert!((crop.crop_left - 0.1).abs() < 0.001);
    assert!((crop.crop_right - 0.2).abs() < 0.001);
    assert!((crop.crop_top - 0.3).abs() < 0.001);
    assert!((crop.crop_bottom - 0.4).abs() < 0.001);

    let resolve_otio = cropping_effect(&clip);
    assert_eq!(resolve_otio.effect_type, 3);
    assert!((crop_param(resolve_otio, "cropLeft").unwrap() - 0.1).abs() < 0.001);
    assert!((crop_param(resolve_otio, "cropRight").unwrap() - 0.2).abs() < 0.001);
    assert!((crop_param(resolve_otio, "cropTop").unwrap() - 0.3).abs() < 0.001);
    assert!((crop_param(resolve_otio, "cropBottom").unwrap() - 0.4).abs() < 0.001);
}

#[test]
fn set_crop_clamps_insets() {
    let mut clip = make_video_clip();
    clip.set_crop(MediaReferenceCrop {
        crop_left: -0.5,
        crop_right: 1.5,
        crop_top: f64::INFINITY,
        crop_bottom: f64::NAN,
    });

    let crop = clip.get_crop();
    assert_eq!(crop.crop_left, 0.0);
    assert_eq!(crop.crop_right, 1.0);
    assert_eq!(crop.crop_top, 0.0);
    assert_eq!(crop.crop_bottom, 0.0);
}

#[test]
fn set_crop_replaces_existing_cropping_effect() {
    let mut clip = make_video_clip();
    clip.set_crop(MediaReferenceCrop {
        crop_left: 0.1,
        crop_right: 0.1,
        crop_top: 0.1,
        crop_bottom: 0.1,
    });
    clip.set_crop(MediaReferenceCrop {
        crop_left: 0.5,
        crop_right: 0.6,
        crop_top: 0.7,
        crop_bottom: 0.8,
    });

    let cropping_effects = clip
        .effects
        .iter()
        .filter(|effect| {
            effect
                .metadata
                .resolve_otio
                .as_ref()
                .is_some_and(|resolve_otio| {
                    resolve_otio.effect_name == "Cropping" || resolve_otio.name == "Cropping"
                })
        })
        .count();
    assert_eq!(cropping_effects, 1);

    let crop = clip.get_crop();
    assert!((crop.crop_left - 0.5).abs() < 0.001);
    assert!((crop.crop_bottom - 0.8).abs() < 0.001);
}

#[test]
fn crop_coexists_with_position() {
    let mut clip = make_video_clip();
    clip.set_position(MediaReferencePosition {
        x: 0.4,
        y: 0.6,
        rotation: 90.0,
        zoom_x: 1.3,
        zoom_y: 1.1,
    });
    clip.set_crop(MediaReferenceCrop {
        crop_left: 0.1,
        crop_right: 0.2,
        crop_top: 0.3,
        crop_bottom: 0.4,
    });

    let pos = clip.get_position();
    assert!((pos.x - 0.4).abs() < 0.001);
    assert!((pos.rotation - 90.0).abs() < 0.001);

    let crop = clip.get_crop();
    assert!((crop.crop_left - 0.1).abs() < 0.001);
    assert!((crop.crop_bottom - 0.4).abs() < 0.001);
}

#[test]
fn item_get_crop_on_gap_returns_defaults() {
    let gap = Item::Gap(Gap::new(5.0, None));
    let crop = gap.get_crop();
    assert_eq!(crop.crop_left, 0.0);
    assert_eq!(crop.crop_right, 0.0);
    assert_eq!(crop.crop_top, 0.0);
    assert_eq!(crop.crop_bottom, 0.0);
}

#[test]
fn item_set_crop_on_gap_is_noop() {
    let mut gap = Item::Gap(Gap::new(5.0, None));
    gap.set_crop(MediaReferenceCrop {
        crop_left: 0.5,
        crop_right: 0.5,
        crop_top: 0.5,
        crop_bottom: 0.5,
    });
    let crop = gap.get_crop();
    assert_eq!(crop.crop_left, 0.0);
    assert_eq!(crop.crop_bottom, 0.0);
}

#[test]
fn crop_serialization_roundtrip() {
    let mut clip = make_video_clip();
    clip.set_crop(MediaReferenceCrop {
        crop_left: 0.05,
        crop_right: 0.15,
        crop_top: 0.25,
        crop_bottom: 0.35,
    });

    let json = serde_json::to_string(&clip).expect("serialize clip");
    let clip2: Clip = serde_json::from_str(&json).expect("deserialize clip");
    let crop = clip2.get_crop();
    assert!((crop.crop_left - 0.05).abs() < 0.001);
    assert!((crop.crop_right - 0.15).abs() < 0.001);
    assert!((crop.crop_top - 0.25).abs() < 0.001);
    assert!((crop.crop_bottom - 0.35).abs() < 0.001);
}
