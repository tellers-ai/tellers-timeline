use tellers_timeline_core::types::Clip;
use serde_json;

#[test]
fn test_parse_clip_with_effects() {
    let json_with_effects = r#"
    {
        "OTIO_SCHEMA": "Clip.1",
        "name": "Test Clip",
        "source_range": {
            "OTIO_SCHEMA": "TimeRange.1",
            "start_time": {
                "OTIO_SCHEMA": "RationalTime.1",
                "rate": 24.0,
                "value": 0.0
            },
            "duration": {
                "OTIO_SCHEMA": "RationalTime.1",
                "rate": 24.0,
                "value": 100.0
            }
        },
        "media_references": {},
        "active_media_reference_key": null,
        "metadata": {},
        "effects": [
            {
                "OTIO_SCHEMA": "Effect.1",
                "metadata": {
                    "Resolve_OTIO": {
                        "Effect Name": "Fairlight Equaliser Band",
                        "Enabled": true,
                        "Name": "Equalizer Band",
                        "Parameters": [
                            {
                                "Default Parameter Value": 1,
                                "Parameter ID": "eq band index",
                                "Parameter Value": 1,
                                "Variant Type": "Int",
                                "maxValue": 5.0,
                                "minValue": -1.0
                            }
                        ],
                        "Type": 63
                    }
                },
                "name": "",
                "effect_name": "Resolve Effect"
            }
        ]
    }
    "#;

    let clip: Clip = serde_json::from_str(json_with_effects).expect("Failed to parse clip with effects");

    // Verify effects were parsed correctly
    assert_eq!(clip.effects.len(), 1);
    let effects = &clip.effects;

    let effect = &effects[0];
    assert_eq!(effect.otio_schema, "Effect.1");
    assert_eq!(effect.name, "");
    assert_eq!(effect.effect_name, "Resolve Effect");

    // Verify metadata was preserved
    assert!(effect.metadata.get("Resolve_OTIO").is_some());
    let resolve_metadata = effect.metadata.get("Resolve_OTIO").unwrap();
    assert_eq!(resolve_metadata.get("Effect Name").unwrap().as_str().unwrap(), "Fairlight Equaliser Band");
}

#[test]
fn test_parse_clip_without_effects() {
    let json_without_effects = r#"
    {
        "OTIO_SCHEMA": "Clip.1",
        "name": "Test Clip",
        "source_range": {
            "OTIO_SCHEMA": "TimeRange.1",
            "start_time": {
                "OTIO_SCHEMA": "RationalTime.1",
                "rate": 24.0,
                "value": 0.0
            },
            "duration": {
                "OTIO_SCHEMA": "RationalTime.1",
                "rate": 24.0,
                "value": 100.0
            }
        },
        "media_references": {},
        "active_media_reference_key": null,
        "metadata": {}
    }
    "#;

    let clip: Clip = serde_json::from_str(json_without_effects).expect("Failed to parse clip without effects");

    // Verify effects is empty when not present
    assert!(clip.effects.is_empty());
}

#[test]
fn test_parse_clip_with_empty_effects() {
    let json_with_empty_effects = r#"
    {
        "OTIO_SCHEMA": "Clip.1",
        "name": "Test Clip",
        "source_range": {
            "OTIO_SCHEMA": "TimeRange.1",
            "start_time": {
                "OTIO_SCHEMA": "RationalTime.1",
                "rate": 24.0,
                "value": 0.0
            },
            "duration": {
                "OTIO_SCHEMA": "RationalTime.1",
                "rate": 24.0,
                "value": 100.0
            }
        },
        "media_references": {},
        "active_media_reference_key": null,
        "metadata": {},
        "effects": []
    }
    "#;

    let clip: Clip = serde_json::from_str(json_with_empty_effects).expect("Failed to parse clip with empty effects");

    // Verify effects is empty
    assert!(clip.effects.is_empty());
}

#[test]
fn test_parse_clip_with_missing_effects_field() {
    // This JSON completely omits the effects field
    let json_without_effects_field = r#"
    {
        "OTIO_SCHEMA": "Clip.1",
        "name": "Test Clip",
        "source_range": {
            "OTIO_SCHEMA": "TimeRange.1",
            "start_time": {
                "OTIO_SCHEMA": "RationalTime.1",
                "rate": 24.0,
                "value": 0.0
            },
            "duration": {
                "OTIO_SCHEMA": "RationalTime.1",
                "rate": 24.0,
                "value": 100.0
            }
        },
        "media_references": {},
        "active_media_reference_key": null,
        "metadata": {}
    }
    "#;

    let clip: Clip = serde_json::from_str(json_without_effects_field).expect("Failed to parse clip with missing effects field");

    // Verify effects is empty when field is missing (serde default kicks in)
    assert!(clip.effects.is_empty());
}

#[test]
fn test_parse_video_effect() {
    let json_with_video_effect = r#"
    {
        "OTIO_SCHEMA": "Effect.1",
        "name": "",
        "effect_name": "",
        "metadata": {
            "Resolve_OTIO": {
                "Effect Name": "Transform",
                "Parameters": [
                    {
                        "Parameter ID": "transformationPan",
                        "Parameter Value": 0.1
                    },
                    {
                        "Parameter ID": "transformationTilt",
                        "Parameter Value": -0.2
                    },
                    {
                        "Parameter ID": "transformationZoomX",
                        "Parameter Value": 0.8
                    },
                    {
                        "Parameter ID": "transformationZoomY",
                        "Parameter Value": 0.9
                    },
                    {
                        "Parameter ID": "transformationFlipY",
                        "Parameter Value": false
                    }
                ]
            }
        }
    }
    "#;

    let effect: tellers_timeline_core::types::Effect = serde_json::from_str(json_with_video_effect)
        .expect("Failed to parse effect");

    let output = effect.parse_video_effect().expect("Should parse video effect");

    // Verify calculations:
    // x = pan - zoomX / 2 + 0.5 = 0.1 - 0.8/2 + 0.5 = 0.1 - 0.4 + 0.5 = 0.2
    // y = tilt - zoomY / 2 + 0.5 = -0.2 - 0.9/2 + 0.5 = -0.2 - 0.45 + 0.5 = -0.15
    assert!((output.x - 0.2).abs() < 0.0001);
    assert!((output.y - (-0.15)).abs() < 0.0001);
    assert!((output.width - 0.8).abs() < 0.0001);
    assert!((output.height - 0.9).abs() < 0.0001);
}

#[test]
fn test_parse_video_effect_defaults() {
    let json_with_empty_effect = r#"
    {
        "OTIO_SCHEMA": "Effect.1",
        "name": "",
        "effect_name": "",
        "metadata": {
            "Resolve_OTIO": {
                "Effect Name": "Transform",
                "Parameters": []
            }
        }
    }
    "#;

    let effect: tellers_timeline_core::types::Effect = serde_json::from_str(json_with_empty_effect)
        .expect("Failed to parse effect");

    let output = effect.parse_video_effect().expect("Should parse video effect with defaults");

    // Should use default values: pan=0, tilt=0, zoomX=1, zoomY=1
    // x = 0 - 1/2 + 0.5 = 0
    // y = 0 - 1/2 + 0.5 = 0
    assert!((output.x - 0.0).abs() < 0.0001);
    assert!((output.y - 0.0).abs() < 0.0001);
    assert!((output.width - 1.0).abs() < 0.0001);
    assert!((output.height - 1.0).abs() < 0.0001);
}

#[test]
fn test_parse_video_effect_missing_metadata() {
    let json_without_resolve = r#"
    {
        "OTIO_SCHEMA": "Effect.1",
        "name": "",
        "effect_name": "",
        "metadata": {}
    }
    "#;

    let effect: tellers_timeline_core::types::Effect = serde_json::from_str(json_without_resolve)
        .expect("Failed to parse effect");

    let output = effect.parse_video_effect();
    assert!(output.is_none(), "Should return None when Resolve_OTIO is missing");
}

#[test]
fn test_parse_audio_effect() {
    let json_with_audio_effect = r#"
    {
        "OTIO_SCHEMA": "Effect.1",
        "name": "",
        "effect_name": "",
        "metadata": {
            "Resolve_OTIO": {
                "Effect Name": "Fairlight Clip Volume and Fades",
                "Parameters": [
                    {
                        "Parameter ID": "volume",
                        "Parameter Value": -27.08759083332872
                    }
                ]
            }
        }
    }
    "#;

    let effect: tellers_timeline_core::types::Effect = serde_json::from_str(json_with_audio_effect)
        .expect("Failed to parse effect");

    let output = effect.parse_audio_effect().expect("Should parse audio effect");
    assert!(output.gain.is_some());
    assert!((output.gain.unwrap() - (-27.08759083332872)).abs() < 0.0001);
}

#[test]
fn test_parse_audio_effect_gain() {
    let json_with_gain = r#"
    {
        "OTIO_SCHEMA": "Effect.1",
        "name": "",
        "effect_name": "",
        "metadata": {
            "Resolve_OTIO": {
                "Effect Name": "Audio Gain",
                "Parameters": [
                    {
                        "Parameter ID": "gain",
                        "Parameter Value": 5.5
                    }
                ]
            }
        }
    }
    "#;

    let effect: tellers_timeline_core::types::Effect = serde_json::from_str(json_with_gain)
        .expect("Failed to parse effect");

    let output = effect.parse_audio_effect().expect("Should parse audio effect with gain");
    assert!(output.gain.is_some());
    assert!((output.gain.unwrap() - 5.5).abs() < 0.0001);
}

#[test]
fn test_parse_audio_effect_no_volume() {
    let json_without_volume = r#"
    {
        "OTIO_SCHEMA": "Effect.1",
        "name": "",
        "effect_name": "",
        "metadata": {
            "Resolve_OTIO": {
                "Effect Name": "Some Other Effect",
                "Parameters": [
                    {
                        "Parameter ID": "someOtherParam",
                        "Parameter Value": 123
                    }
                ]
            }
        }
    }
    "#;

    let effect: tellers_timeline_core::types::Effect = serde_json::from_str(json_without_volume)
        .expect("Failed to parse effect");

    let output = effect.parse_audio_effect();
    assert!(output.is_none(), "Should return None when no volume/gain parameter found");
}

#[test]
fn test_parse_text_effect() {
    let json_with_text_effect = r#"
    {
        "OTIO_SCHEMA": "Effect.1",
        "name": "",
        "effect_name": "",
        "metadata": {
            "Resolve_OTIO": {
                "Effect Name": "Text Transform",
                "Parameters": [
                    {
                        "Parameter ID": "position",
                        "Parameter Value": [0.3, 0.7]
                    },
                    {
                        "Parameter ID": "transformationZoomX",
                        "Parameter Value": 1.5
                    },
                    {
                        "Parameter ID": "transformationZoomY",
                        "Parameter Value": 1.2
                    },
                    {
                        "Parameter ID": "transformationRotationAngle",
                        "Parameter Value": 45.0
                    }
                ]
            }
        }
    }
    "#;

    let effect: tellers_timeline_core::types::Effect = serde_json::from_str(json_with_text_effect)
        .expect("Failed to parse effect");

    let params = effect.parse_text_effect();
    assert!((params.position[0] - 0.3).abs() < 0.0001);
    assert!((params.position[1] - 0.7).abs() < 0.0001);
    assert!((params.zoom_x - 1.5).abs() < 0.0001);
    assert!((params.zoom_y - 1.2).abs() < 0.0001);
    assert!((params.rotation - 45.0).abs() < 0.0001);
}

#[test]
fn test_parse_text_effect_defaults() {
    let json_without_text_params = r#"
    {
        "OTIO_SCHEMA": "Effect.1",
        "name": "",
        "effect_name": "",
        "metadata": {
            "Resolve_OTIO": {
                "Effect Name": "Some Effect",
                "Parameters": []
            }
        }
    }
    "#;

    let effect: tellers_timeline_core::types::Effect = serde_json::from_str(json_without_text_params)
        .expect("Failed to parse effect");

    let params = effect.parse_text_effect();
    // Should return default values
    assert!((params.position[0] - 0.5).abs() < 0.0001);
    assert!((params.position[1] - 0.5).abs() < 0.0001);
    assert!((params.zoom_x - 1.0).abs() < 0.0001);
    assert!((params.zoom_y - 1.0).abs() < 0.0001);
    assert!((params.rotation - 0.0).abs() < 0.0001);
}

#[test]
fn test_clip_get_video_effect_output() {
    let json_with_video_clip = r#"
    {
        "OTIO_SCHEMA": "Clip.2",
        "name": "Test Video Clip",
        "source_range": {
            "OTIO_SCHEMA": "TimeRange.1",
            "start_time": {
                "OTIO_SCHEMA": "RationalTime.1",
                "rate": 24.0,
                "value": 0.0
            },
            "duration": {
                "OTIO_SCHEMA": "RationalTime.1",
                "rate": 24.0,
                "value": 100.0
            }
        },
        "media_references": {},
        "active_media_reference_key": null,
        "metadata": {},
        "effects": [
            {
                "OTIO_SCHEMA": "Effect.1",
                "name": "",
                "effect_name": "",
                "metadata": {
                    "Resolve_OTIO": {
                        "Effect Name": "Transform",
                        "Parameters": [
                            {
                                "Parameter ID": "transformationZoomX",
                                "Parameter Value": 0.5
                            }
                        ]
                    }
                }
            }
        ]
    }
    "#;

    let clip: Clip = serde_json::from_str(json_with_video_clip)
        .expect("Failed to parse clip");

    let output = clip.get_video_effect_output();
    assert!((output.width - 0.5).abs() < 0.0001);
}

#[test]
fn test_clip_get_video_effect_output_default() {
    let json_without_effects = r#"
    {
        "OTIO_SCHEMA": "Clip.2",
        "name": "Test Video Clip",
        "source_range": {
            "OTIO_SCHEMA": "TimeRange.1",
            "start_time": {
                "OTIO_SCHEMA": "RationalTime.1",
                "rate": 24.0,
                "value": 0.0
            },
            "duration": {
                "OTIO_SCHEMA": "RationalTime.1",
                "rate": 24.0,
                "value": 100.0
            }
        },
        "media_references": {},
        "active_media_reference_key": null,
        "metadata": {},
        "effects": []
    }
    "#;

    let clip: Clip = serde_json::from_str(json_without_effects)
        .expect("Failed to parse clip");

    let output = clip.get_video_effect_output();
    // Should return default values
    assert!((output.x - 0.0).abs() < 0.0001);
    assert!((output.y - 0.0).abs() < 0.0001);
    assert!((output.width - 1.0).abs() < 0.0001);
    assert!((output.height - 1.0).abs() < 0.0001);
}

#[test]
fn test_clip_get_audio_effect_output() {
    let json_with_audio_clip = r#"
    {
        "OTIO_SCHEMA": "Clip.2",
        "name": "Test Audio Clip",
        "source_range": {
            "OTIO_SCHEMA": "TimeRange.1",
            "start_time": {
                "OTIO_SCHEMA": "RationalTime.1",
                "rate": 24.0,
                "value": 0.0
            },
            "duration": {
                "OTIO_SCHEMA": "RationalTime.1",
                "rate": 24.0,
                "value": 100.0
            }
        },
        "media_references": {},
        "active_media_reference_key": null,
        "metadata": {},
        "effects": [
            {
                "OTIO_SCHEMA": "Effect.1",
                "name": "",
                "effect_name": "",
                "metadata": {
                    "Resolve_OTIO": {
                        "Effect Name": "Volume",
                        "Parameters": [
                            {
                                "Parameter ID": "volume",
                                "Parameter Value": -10.5
                            }
                        ]
                    }
                }
            }
        ]
    }
    "#;

    let clip: Clip = serde_json::from_str(json_with_audio_clip)
        .expect("Failed to parse clip");

    let output = clip.get_audio_effect_output();
    assert!(output.is_some());
    let audio_output = output.unwrap();
    assert!(audio_output.gain.is_some());
    assert!((audio_output.gain.unwrap() - (-10.5)).abs() < 0.0001);
}
