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

/// A clip as DaVinci Resolve exports it, with the given Composite effect parameters.
fn resolve_clip_json(parameters: &str, enabled: bool) -> String {
    format!(
        r#"
    {{
        "OTIO_SCHEMA": "Clip.2",
        "name": "happy_horse_cheerful_meadow.mp4",
        "source_range": {{
            "OTIO_SCHEMA": "TimeRange.1",
            "start_time": {{ "OTIO_SCHEMA": "RationalTime.1", "rate": 30.0, "value": 0.0 }},
            "duration": {{ "OTIO_SCHEMA": "RationalTime.1", "rate": 30.0, "value": 75.0 }}
        }},
        "media_references": {{
            "DEFAULT_MEDIA": {{
                "OTIO_SCHEMA": "ExternalReference.1",
                "metadata": {{}},
                "name": "happy_horse_cheerful_meadow.mp4",
                "available_range": null,
                "available_image_bounds": null,
                "target_url": "/Users/me/happy_horse_cheerful_meadow.mp4"
            }}
        }},
        "active_media_reference_key": "DEFAULT_MEDIA",
        "metadata": {{ "Resolve_OTIO": {{ "Link Group ID": 1 }} }},
        "effects": [
            {{
                "OTIO_SCHEMA": "Effect.1",
                "metadata": {{
                    "Resolve_OTIO": {{
                        "Display Type": 1,
                        "Effect Name": "Transform",
                        "Enabled": true,
                        "Name": "Transform",
                        "Parameters": [
                            {{
                                "Default Parameter Value": 1.0,
                                "Key Frames": {{}},
                                "Parameter ID": "transformationZoomX",
                                "Parameter Value": 0.5,
                                "Variant Type": "Double",
                                "maxValue": 100.0,
                                "minValue": 0.0
                            }}
                        ],
                        "Type": 2
                    }}
                }},
                "name": "",
                "effect_name": "Resolve Effect"
            }},
            {{
                "OTIO_SCHEMA": "Effect.1",
                "metadata": {{
                    "Resolve_OTIO": {{
                        "Display Type": 1,
                        "Effect Name": "Composite",
                        "Enabled": {enabled},
                        "Name": "Composite",
                        "Parameters": [{parameters}],
                        "Type": 1
                    }}
                }},
                "name": "",
                "effect_name": "Resolve Effect"
            }}
        ]
    }}
    "#
    )
}

const RESOLVE_COLOR_MODE_PARAMETER: &str = r#"
    {
        "Default Parameter Value": 0,
        "Parameter ID": "composite mode",
        "Parameter Value": 16,
        "Variant Type": "UInt"
    }"#;

fn composite_effect(clip: &Clip) -> &ResolveOTIOEffect {
    clip.effects
        .iter()
        .find_map(|effect| {
            let resolve_otio = effect.metadata.resolve_otio.as_ref()?;
            if resolve_otio.effect_name == "Composite" {
                Some(resolve_otio)
            } else {
                None
            }
        })
        .expect("clip should have a Composite Resolve effect")
}

fn composite_mode_param(resolve_otio: &ResolveOTIOEffect) -> u64 {
    resolve_otio
        .parameters
        .iter()
        .find_map(|parameter| match parameter {
            ResolveOTIOParameter::UInt(param) if param.parameter_id == "composite mode" => {
                Some(param.parameter_value)
            }
            _ => None,
        })
        .expect("Composite effect should carry a UInt \"composite mode\" parameter")
}

#[test]
fn codes_follow_the_resolve_enum() {
    assert_eq!(CompositeMode::ALL.len(), 32);
    assert_eq!(CompositeMode::Normal.resolve_code(), 0);
    assert_eq!(CompositeMode::Multiply.resolve_code(), 4);
    assert_eq!(CompositeMode::Color.resolve_code(), 16);
    assert_eq!(CompositeMode::Alpha.resolve_code(), 28);
    assert_eq!(CompositeMode::InvertedLuminosity.resolve_code(), 31);
    for (code, mode) in CompositeMode::ALL.iter().enumerate() {
        assert_eq!(mode.resolve_code(), code as u64);
        assert_eq!(CompositeMode::from_resolve_code(code as u64), Some(*mode));
    }
    assert_eq!(CompositeMode::from_resolve_code(32), None);
    assert_eq!(CompositeMode::from_resolve_code(u64::MAX), None);
}

#[test]
fn names_round_trip_and_parse_loosely() {
    for mode in CompositeMode::ALL {
        assert_eq!(mode.as_str().parse::<CompositeMode>(), Ok(mode));
        assert_eq!(mode.to_string(), mode.as_str());
    }
    assert_eq!("Hard Light".parse::<CompositeMode>(), Ok(CompositeMode::HardLight));
    assert_eq!("hardlight".parse::<CompositeMode>(), Ok(CompositeMode::HardLight));
    assert_eq!("SOFT_LIGHT".parse::<CompositeMode>(), Ok(CompositeMode::SoftLight));
    assert_eq!("Inverted Alpha".parse::<CompositeMode>(), Ok(CompositeMode::InvertedAlpha));
    assert_eq!("Colorize".parse::<CompositeMode>(), Ok(CompositeMode::Color));
    assert_eq!(" luminosity ".parse::<CompositeMode>(), Ok(CompositeMode::Luminosity));
    assert!("not a mode".parse::<CompositeMode>().is_err());
    assert!("".parse::<CompositeMode>().is_err());
}

#[test]
fn only_alpha_and_luminance_modes_are_masks() {
    let masks: Vec<CompositeMode> = CompositeMode::ALL
        .iter()
        .copied()
        .filter(|mode| mode.is_mask())
        .collect();
    assert_eq!(
        masks,
        vec![
            CompositeMode::LumaMask,
            CompositeMode::Alpha,
            CompositeMode::InvertedAlpha,
            CompositeMode::Luminosity,
            CompositeMode::InvertedLuminosity,
        ]
    );
}

#[test]
fn get_composite_mode_defaults_to_normal_without_effect() {
    let clip = make_video_clip();
    assert_eq!(clip.get_composite_mode(), CompositeMode::Normal);
}

#[test]
fn get_composite_mode_parses_resolve_export() {
    let clip: Clip =
        serde_json::from_str(&resolve_clip_json(RESOLVE_COLOR_MODE_PARAMETER, true)).unwrap();
    assert_eq!(clip.get_composite_mode(), CompositeMode::Color);
    // The other Resolve effects are untouched by reading.
    assert_eq!(clip.get_position().zoom_x, 0.5);
}

#[test]
fn get_composite_mode_treats_resolve_default_export_as_normal() {
    // Resolve writes the Composite effect with an empty parameter list for Normal.
    let clip: Clip = serde_json::from_str(&resolve_clip_json("", true)).unwrap();
    assert_eq!(clip.get_composite_mode(), CompositeMode::Normal);
}

#[test]
fn get_composite_mode_ignores_disabled_effect() {
    let clip: Clip =
        serde_json::from_str(&resolve_clip_json(RESOLVE_COLOR_MODE_PARAMETER, false)).unwrap();
    assert_eq!(clip.get_composite_mode(), CompositeMode::Normal);
}

#[test]
fn get_composite_mode_ignores_unknown_code() {
    let parameter = r#"{ "Parameter ID": "composite mode", "Parameter Value": 200, "Variant Type": "UInt" }"#;
    let clip: Clip = serde_json::from_str(&resolve_clip_json(parameter, true)).unwrap();
    assert_eq!(clip.get_composite_mode(), CompositeMode::Normal);
}

#[test]
fn get_composite_mode_accepts_int_parameter() {
    let parameter = r#"{ "Parameter ID": "composite mode", "Parameter Value": 28, "Variant Type": "Int" }"#;
    let clip: Clip = serde_json::from_str(&resolve_clip_json(parameter, true)).unwrap();
    assert_eq!(clip.get_composite_mode(), CompositeMode::Alpha);
}

#[test]
fn set_composite_mode_writes_resolve_effect() {
    let mut clip = make_video_clip();
    clip.set_composite_mode(CompositeMode::Alpha);

    assert_eq!(clip.get_composite_mode(), CompositeMode::Alpha);
    let effect = composite_effect(&clip);
    assert!(effect.enabled);
    assert_eq!(effect.name, "Composite");
    assert_eq!(effect.effect_type, 1);
    assert_eq!(composite_mode_param(effect), 28);
    let composite_effects = clip
        .effects
        .iter()
        .filter(|effect| effect.effect_name == "Resolve Effect")
        .count();
    assert_eq!(composite_effects, 1);
}

#[test]
fn set_composite_mode_replaces_existing_effect() {
    let mut clip: Clip =
        serde_json::from_str(&resolve_clip_json(RESOLVE_COLOR_MODE_PARAMETER, true)).unwrap();
    assert_eq!(clip.effects.len(), 2);

    clip.set_composite_mode(CompositeMode::Screen);

    assert_eq!(clip.effects.len(), 2, "the Transform effect is kept, Composite replaced");
    assert_eq!(clip.get_composite_mode(), CompositeMode::Screen);
    assert_eq!(clip.get_position().zoom_x, 0.5);

    clip.set_composite_mode(CompositeMode::Normal);
    assert_eq!(clip.effects.len(), 2);
    assert_eq!(clip.get_composite_mode(), CompositeMode::Normal);
    assert_eq!(composite_mode_param(composite_effect(&clip)), 0);
}

#[test]
fn set_composite_mode_re_enables_disabled_effect() {
    let mut clip: Clip =
        serde_json::from_str(&resolve_clip_json(RESOLVE_COLOR_MODE_PARAMETER, false)).unwrap();
    clip.set_composite_mode(CompositeMode::Multiply);
    assert_eq!(clip.get_composite_mode(), CompositeMode::Multiply);
    assert!(composite_effect(&clip).enabled);
}

#[test]
fn composite_mode_coexists_with_crop_and_position() {
    let mut clip = make_video_clip();
    clip.set_composite_mode(CompositeMode::Luminosity);
    clip.set_crop(MediaReferenceCrop {
        crop_left: 0.1,
        crop_right: 0.0,
        crop_top: 0.0,
        crop_bottom: 0.0,
    });
    clip.set_position(MediaReferencePosition {
        x: 0.1,
        y: 0.2,
        rotation: 0.0,
        zoom_x: 1.0,
        zoom_y: 1.0,
    });

    assert_eq!(clip.get_composite_mode(), CompositeMode::Luminosity);
    assert_eq!(clip.get_crop().crop_left, 0.1);
    assert_eq!(clip.get_position().x, 0.1);
}

#[test]
fn composite_mode_survives_serialization_round_trip() {
    let mut clip = make_video_clip();
    clip.set_composite_mode(CompositeMode::InvertedLuminosity);

    let json = serde_json::to_string(&clip).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let resolve_otio = &value["effects"][0]["metadata"]["Resolve_OTIO"];
    assert_eq!(resolve_otio["Effect Name"], "Composite");
    assert_eq!(resolve_otio["Type"], 1);
    assert_eq!(resolve_otio["Parameters"][0]["Parameter ID"], "composite mode");
    assert_eq!(resolve_otio["Parameters"][0]["Parameter Value"], 31);
    assert_eq!(resolve_otio["Parameters"][0]["Variant Type"], "UInt");

    let parsed: Clip = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.get_composite_mode(), CompositeMode::InvertedLuminosity);
}

#[test]
fn item_accessors_pass_through_and_ignore_gaps() {
    let mut item = Item::Clip(make_video_clip());
    assert_eq!(item.get_composite_mode(), CompositeMode::Normal);
    item.set_composite_mode(CompositeMode::Overlay);
    assert_eq!(item.get_composite_mode(), CompositeMode::Overlay);

    let mut gap = Item::Gap(Gap::new(2.0, None));
    gap.set_composite_mode(CompositeMode::Overlay);
    assert_eq!(gap.get_composite_mode(), CompositeMode::Normal);
    assert!(gap.get_effects().is_empty());
}
