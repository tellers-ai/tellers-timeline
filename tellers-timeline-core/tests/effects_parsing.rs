use tellers_timeline_core::types::{Clip, Effect, ResolveOTIOEffect, ResolveOTIOParameter, MediaReference};
use serde_json;

// Test-only types
/// Video transformation output coordinates
#[derive(Debug, Clone, PartialEq)]
pub struct VideoEffectOutput {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Audio effect output (gain/volume)
#[derive(Debug, Clone, PartialEq)]
pub struct AudioEffectOutput {
    pub gain: Option<f64>,
}

/// Text effect parameters
#[derive(Debug, Clone, PartialEq)]
pub struct TextEffectParams {
    pub position: [f64; 2],
    pub zoom_x: f64,
    pub zoom_y: f64,
    pub rotation: f64,
}

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
    assert!(effect.metadata.resolve_otio.is_some());
    let resolve_metadata = effect.metadata.resolve_otio.as_ref().unwrap();
    assert_eq!(resolve_metadata.effect_name, "Fairlight Equaliser Band");
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

    let output = parse_video_effect(&effect).expect("Should parse video effect");

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

    let output = parse_video_effect(&effect).expect("Should parse video effect with defaults");

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

    let output = parse_video_effect(&effect);
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

    let output = parse_audio_effect(&effect).expect("Should parse audio effect");
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

    let output = parse_audio_effect(&effect).expect("Should parse audio effect with gain");
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

    let output = parse_audio_effect(&effect);
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

    let params = parse_text_effect(&effect);
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

    let params = parse_text_effect(&effect);
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

    let output = get_video_effect_output(&clip);
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

    let output = get_video_effect_output(&clip);
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

    let output = get_audio_effect_output(&clip);
    assert!(output.is_some());
    let audio_output = output.unwrap();
    assert!(audio_output.gain.is_some());
    assert!((audio_output.gain.unwrap() - (-10.5)).abs() < 0.0001);
}

#[test]
fn test_parse_color_parameter() {
    let json_with_color = r##"
    {
        "OTIO_SCHEMA": "Effect.1",
        "name": "",
        "effect_name": "",
        "metadata": {
            "Resolve_OTIO": {
                "Effect Name": "Shadow",
                "Parameters": [
                    {
                        "Default Parameter Value": "#000000",
                        "Parameter ID": "shadow color",
                        "Parameter Value": "#000000",
                        "Variant Type": "Color"
                    }
                ]
            }
        }
    }
    "##;

    let effect: tellers_timeline_core::types::Effect = serde_json::from_str(json_with_color)
        .expect("Failed to parse effect");

    let resolve_data = effect.metadata.resolve_otio.as_ref()
        .expect("Should have Resolve_OTIO metadata");

    let parameters = &resolve_data.parameters;
    assert_eq!(parameters.len(), 1);

    let param = &parameters[0];
    match param {
        tellers_timeline_core::types::ResolveOTIOParameter::Color(v) => {
            assert_eq!(v.parameter_value, "#000000".to_string());
        }
        _ => panic!("Expected Color variant, got {:?}", param),
    }

    // Verify we can get the color string directly from the variant
    if let tellers_timeline_core::types::ResolveOTIOParameter::Color(v) = param {
        assert_eq!(v.parameter_value, "#000000".to_string());
    }
}

#[test]
fn test_parse_pointf_parameter() {
    let json_with_pointf = r#"
    {
        "OTIO_SCHEMA": "Effect.1",
        "name": "",
        "effect_name": "",
        "metadata": {
            "Resolve_OTIO": {
                "Effect Name": "Shadow",
                "Parameters": [
                    {
                        "Default Parameter Value": [0, 0],
                        "Key Frames": {},
                        "Parameter ID": "shadow offset",
                        "Parameter Value": [0, 0],
                        "Variant Type": "POINTF"
                    }
                ]
            }
        }
    }
    "#;

    let effect: tellers_timeline_core::types::Effect = serde_json::from_str(json_with_pointf)
        .expect("Failed to parse effect");

    let resolve_data = effect.metadata.resolve_otio.as_ref()
        .expect("Should have Resolve_OTIO metadata");

    let parameters = &resolve_data.parameters;
    assert_eq!(parameters.len(), 1);

    let param = &parameters[0];
    match param {
        tellers_timeline_core::types::ResolveOTIOParameter::PointF(v) => {
            assert_eq!(v.parameter_value, Some([0.0, 0.0]));
        }
        _ => panic!("Expected PointF variant, got {:?}", param),
    }

    // Verify we can get the PointF value directly from the variant
    if let tellers_timeline_core::types::ResolveOTIOParameter::PointF(v) = param {
        assert_eq!(v.parameter_value.as_ref(), Some(&[0.0, 0.0]));
    }
}

#[test]
fn test_parse_uint_parameter() {
    let json_with_uint = r#"
    {
        "OTIO_SCHEMA": "Effect.1",
        "name": "",
        "effect_name": "",
        "metadata": {
            "Resolve_OTIO": {
                "Effect Name": "Anchor",
                "Parameters": [
                    {
                        "Default Parameter Value": 4,
                        "Parameter ID": "anchor",
                        "Parameter Value": 4,
                        "Variant Type": "UInt"
                    }
                ]
            }
        }
    }
    "#;

    let effect: tellers_timeline_core::types::Effect = serde_json::from_str(json_with_uint)
        .expect("Failed to parse effect");

    let resolve_data = effect.metadata.resolve_otio.as_ref()
        .expect("Should have Resolve_OTIO metadata");

    let parameters = &resolve_data.parameters;
    assert_eq!(parameters.len(), 1);

    let param = &parameters[0];
    match param {
        tellers_timeline_core::types::ResolveOTIOParameter::UInt(v) => {
            assert_eq!(v.parameter_value, 4u64);
            assert_eq!(v.default_parameter_value, Some(4u64));
        }
        _ => panic!("Expected UInt variant, got {:?}", param),
    }

    // Verify we can get the UInt value directly from the variant
    if let tellers_timeline_core::types::ResolveOTIOParameter::UInt(v) = param {
        assert_eq!(v.parameter_value, 4u64);
        // UInt can be converted to f64
        assert_eq!(v.parameter_value as f64, 4.0);
    }
}

#[test]
fn test_parse_title_blob_parameter() {
    let json_with_title_blob = r##"
    {
        "OTIO_SCHEMA": "Effect.1",
        "name": "",
        "effect_name": "",
        "metadata": {
            "Resolve_OTIO": {
                "Effect Name": "Rich Text",
                "Parameters": [
                    {
                        "Parameter ID": "title blob",
                        "Title HTML": "<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.0//EN\" \"http://www.w3.org/TR/REC-html40/strict.dtd\">\n<html><head><meta name=\"qrichtext\" content=\"1\" /><style type=\"text/css\">\np, li { white-space: pre-wrap; }\n</style></head><body style=\" font-family:'.AppleSystemUIFont'; font-size:13pt; font-weight:400; font-style:normal;\">\n<p align=\"center\" style=\" margin-top:0px; margin-bottom:0px; margin-left:0px; margin-right:0px; -qt-block-indent:0; text-indent:0px; line-height:0; -qt-line-height-type: line-distance;\"><span style=\" font-family:'Open Sans'; font-size:96pt; font-weight:504; color:#ffffff;\">Basic </span><span style=\" font-family:'Open Sans'; font-size:96pt; font-weight:504; color:#ff6b81;\">Title<br />yo</span></p></body></html>"
                    }
                ]
            }
        }
    }
    "##;

    let effect: tellers_timeline_core::types::Effect = serde_json::from_str(json_with_title_blob)
        .expect("Failed to parse effect");

    let resolve_data = effect.metadata.resolve_otio.as_ref()
        .expect("Should have Resolve_OTIO metadata");

    let parameters = &resolve_data.parameters;
    assert_eq!(parameters.len(), 1);

    let param = &parameters[0];
    match param {
        tellers_timeline_core::types::ResolveOTIOParameter::String(v) => {
            assert!(v.parameter_value.contains("Basic"));
            assert!(v.parameter_value.contains("Title"));
        }
        _ => panic!("Expected String variant for title blob, got {:?}", param),
    }

    // Verify we can get the HTML directly from the variant
    if let tellers_timeline_core::types::ResolveOTIOParameter::String(v) = param {
        assert!(v.parameter_value.contains("Basic"));
    }

    // Verify parameter_id is accessible
    let param_id = param.parameter_id();
    assert_eq!(param_id, &"title blob".to_string());
}

#[test]
fn test_parse_generator_reference() {
    let json_generator_ref = r##"
    {
        "OTIO_SCHEMA": "GeneratorReference.1",
        "metadata": {
            "Resolve_OTIO": {
                "Generator Type": "Rich"
            }
        },
        "name": "Text",
        "available_range": null,
        "available_image_bounds": null,
        "generator_kind": "Rich",
        "parameters": {
            "Resolve_OTIO": [
                {
                    "Effect Name": "Rich Text",
                    "Enabled": true,
                    "Name": "Rich Text",
                    "Parameters": [
                        {
                            "Default Parameter Value": "Title",
                            "Parameter ID": "rich text",
                            "Parameter Value": "Title",
                            "Variant Type": "String"
                        },
                        {
                            "Parameter ID": "title blob",
                            "Title HTML": "<!DOCTYPE HTML PUBLIC \"-//W3C//DTD HTML 4.0//EN\" \"http://www.w3.org/TR/REC-html40/strict.dtd\">\n<html><head><meta name=\"qrichtext\" content=\"1\" /><style type=\"text/css\">\np, li { white-space: pre-wrap; }\n</style></head><body style=\" font-family:'.AppleSystemUIFont'; font-size:13pt; font-weight:400; font-style:normal;\">\n<p align=\"center\" style=\" margin-top:0px; margin-bottom:0px; margin-left:0px; margin-right:0px; -qt-block-indent:0; text-indent:0px; line-height:0; -qt-line-height-type: line-distance;\"><span style=\" font-family:'Open Sans'; font-size:96pt; font-weight:504; color:#ffffff;\">Basic </span><span style=\" font-family:'Open Sans'; font-size:96pt; font-weight:504; color:#ff6b81;\">Title<br />yo</span></p></body></html>"
                        }
                    ],
                    "Type": 24
                }
            ]
        }
    }
    "##;

    let media_ref: tellers_timeline_core::types::MediaReference = serde_json::from_str(json_generator_ref)
        .expect("Failed to parse GeneratorReference");

    match media_ref {
        tellers_timeline_core::types::MediaReference::GeneratorReference {
            generator_kind,
            name,
            parameters,
            ..
        } => {
            assert_eq!(generator_kind, "Rich");
            assert_eq!(name, Some("Text".to_string()));

            // Verify parameters structure - now it's a typed GeneratorParameters
            if let Some(resolve_otio) = &parameters.resolve_otio {
                assert_eq!(resolve_otio.len(), 1);
                assert_eq!(resolve_otio[0].effect_name, "Rich Text");
            } else {
                panic!("Expected resolve_otio to be Some");
            }
        }
        _ => panic!("Expected GeneratorReference variant"),
    }

    // Test that we can extract text data from a clip with GeneratorReference
    let clip_json = r##"
    {
        "OTIO_SCHEMA": "Clip.1",
        "name": "Text Clip",
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
        "media_references": {
            "DEFAULT_MEDIA": {
                "OTIO_SCHEMA": "GeneratorReference.1",
                "metadata": {
                    "Resolve_OTIO": {
                        "Generator Type": "Rich"
                    }
                },
                "name": "Text",
                "available_range": null,
                "available_image_bounds": null,
                "generator_kind": "Rich",
                "parameters": {
                    "Resolve_OTIO": [
                        {
                            "Effect Name": "Rich Text",
                            "Enabled": true,
                            "Name": "Rich Text",
                            "Parameters": [
                                {
                                    "Parameter ID": "title blob",
                                    "Title HTML": "<p>Test HTML</p>"
                                }
                            ],
                            "Type": 24
                        }
                    ]
                }
            }
        },
        "active_media_reference_key": "DEFAULT_MEDIA",
        "metadata": {},
        "effects": []
    }
    "##;

    let clip: Clip = serde_json::from_str(clip_json)
        .expect("Failed to parse clip with GeneratorReference");

    let (html, _text_params) = extract_text_data(&clip);
    assert!(html.is_some());
    assert_eq!(html.unwrap(), "<p>Test HTML</p>");
}

// Test-only helper function to extract text data from clip
fn extract_text_data(clip: &Clip) -> (Option<String>, TextEffectParams) {
    let mut result = TextEffectParams {
        position: [0.5, 0.5],
        zoom_x: 1.0,
        zoom_y: 1.0,
        rotation: 0.0,
    };
    let mut html: Option<String> = None;

    // Get active media reference
    let media_ref_key = clip
        .active_media_reference_key
        .as_deref()
        .unwrap_or("DEFAULT_MEDIA");
    if let Some(media_ref) = clip.media_references.get(media_ref_key) {
        // Check for Resolve_OTIO parameters in media reference
        // For GeneratorReference, check the parameters field; for ExternalReference, check metadata
        let resolve_array_opt: Option<Vec<ResolveOTIOEffect>> = match media_ref {
            MediaReference::GeneratorReference { parameters, .. } => {
                // For GeneratorReference, parameters is a typed GeneratorParameters struct
                parameters.resolve_otio.clone()
            }
            MediaReference::ExternalReference { .. } => {
                // For ExternalReference, check metadata for parameters
                if let Some(parameters) = media_ref.metadata().get("parameters") {
                    if let Some(params_obj) = parameters.as_object() {
                        if let Some(resolve_otio) = params_obj.get("Resolve_OTIO") {
                            // Try to deserialize from JSON value
                            serde_json::from_value::<Vec<ResolveOTIOEffect>>(resolve_otio.clone()).ok()
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        };
        if let Some(resolve_array) = resolve_array_opt {
            for effect in resolve_array {
                if effect.effect_name == "Rich Text" {
                    for param in &effect.parameters {
                        match param.parameter_id().as_str() {
                            "title blob" => {
                                // title_html is only in Unknown variant
                                if let ResolveOTIOParameter::Unknown(v) = &param {
                                    if let Some(title_html) = &v.title_html {
                                        html = Some(title_html.clone());
                                    }
                                }
                            }
                            "position" => {
                                if let tellers_timeline_core::types::ResolveOTIOParameter::PointF(v) = param {
                                    if let Some(pos) = v.parameter_value {
                                        result.position = pos;
                                    }
                                }
                            }
                            "transformationZoomX" => {
                                if let tellers_timeline_core::types::ResolveOTIOParameter::Double(v) = param {
                                    result.zoom_x = v.parameter_value;
                                }
                            }
                            "transformationZoomY" => {
                                if let tellers_timeline_core::types::ResolveOTIOParameter::Double(v) = param {
                                    result.zoom_y = v.parameter_value;
                                }
                            }
                            "transformationRotationAngle" => {
                                if let tellers_timeline_core::types::ResolveOTIOParameter::Double(v) = param {
                                    result.rotation = v.parameter_value;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    return (html, result);
}

// Test-only helper functions for parsing effects
/// Parse Resolve_OTIO video transformation effects and convert to output coordinates.
/// Returns None if the effect doesn't contain valid video transformation parameters.
/// This function is defensive and won't panic on unexpected data structures.
fn parse_video_effect(effect: &Effect) -> Option<VideoEffectOutput> {
    // Get Resolve_OTIO metadata
    let resolve_data = effect.metadata.resolve_otio.as_ref()?;
    let parameters = &resolve_data.parameters;

    // Initialize with default values
    let mut pan = 0.0;      // OTIO: -0.5 to 0.5, where 0 is center
    let mut tilt = 0.0;     // OTIO: -0.5 to 0.5, where 0 is center
    let mut zoom_x = 1.0;    // OTIO: normalized 0-1
    let mut zoom_y = 1.0;    // OTIO: normalized 0-1
    let mut _flip_y = false;

    // Collect all parameters
    for param in parameters {
        match param.parameter_id().as_str() {
            "transformationPan" => {
                if let ResolveOTIOParameter::Double(v) = &param {
                    pan = v.parameter_value;
                }
            }
            "transformationTilt" => {
                if let ResolveOTIOParameter::Double(v) = &param {
                    tilt = v.parameter_value;
                }
            }
            "transformationZoomX" => {
                if let ResolveOTIOParameter::Double(v) = &param {
                    zoom_x = v.parameter_value;
                }
            }
            "transformationZoomY" => {
                if let ResolveOTIOParameter::Double(v) = &param {
                    zoom_y = v.parameter_value;
                }
            }
            "transformationFlipY" => {
                if let ResolveOTIOParameter::Bool(v) = &param {
                    _flip_y = v.parameter_value;
                }
            }
            _ => {}
        }
    }

    Some(VideoEffectOutput {
        x: pan - zoom_x / 2.0 + 0.5,
        y: tilt - zoom_y / 2.0 + 0.5,
        width: zoom_x,
        height: zoom_y,
    })
}


/// Parse Resolve_OTIO text transformation parameters.
/// Returns default values if parameters are not found.
/// This function is defensive and won't panic on unexpected data structures.
fn parse_text_effect(effect: &Effect) -> TextEffectParams {
    let mut result = TextEffectParams {
        position: [0.5, 0.5],
        zoom_x: 1.0,
        zoom_y: 1.0,
        rotation: 0.0,
    };

    // Get Resolve_OTIO metadata
    let resolve_data = match &effect.metadata.resolve_otio {
        Some(d) => d,
        None => return result,
    };

    let parameters = &resolve_data.parameters;

    // Parse parameters
    for param in parameters {
        match param.parameter_id().as_str() {
            "position" => {
                match &param {
                    ResolveOTIOParameter::PointF(v) => {
                        if let Some(arr) = v.parameter_value {
                            result.position = arr;
                        }
                    }
                    ResolveOTIOParameter::Unknown(v) => {
                        if let Some(val) = &v.parameter_value {
                            if let Some(arr) = val.as_array() {
                                if arr.len() >= 2 {
                                    if let (Some(x), Some(y)) = (arr[0].as_f64(), arr[1].as_f64()) {
                                        result.position = [x, y];
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            "transformationZoomX" => {
                if let ResolveOTIOParameter::Double(v) = &param {
                    result.zoom_x = v.parameter_value;
                }
            }
            "transformationZoomY" => {
                if let ResolveOTIOParameter::Double(v) = &param {
                    result.zoom_y = v.parameter_value;
                }
            }
            "transformationRotationAngle" => {
                if let ResolveOTIOParameter::Double(v) = &param {
                    result.rotation = v.parameter_value;
                }
            }
            _ => {
                // Ignore unknown parameters
            }
        }
    }

    result
}

// Test-only helper functions
fn get_video_effect_output(clip: &Clip) -> VideoEffectOutput {
    for effect in &clip.effects {
        if let Some(output) = parse_video_effect(effect) {
            return output;
        }
    }
    // Default output
    VideoEffectOutput {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
    }
}

fn get_audio_effect_output(clip: &Clip) -> Option<AudioEffectOutput> {
    for effect in &clip.effects {
        if let Some(output) = parse_audio_effect(effect) {
            return Some(output);
        }
    }
    None
}

/// Parse Resolve_OTIO audio effects (volume/gain).
/// Returns None if the effect doesn't contain valid audio parameters.
fn parse_audio_effect(effect: &Effect) -> Option<AudioEffectOutput> {
    // Get Resolve_OTIO metadata
    let resolve_data = effect.metadata.resolve_otio.as_ref()?;
    let parameters = &resolve_data.parameters;

    // Look for volume or gain parameter
    for param in parameters {
        match param.parameter_id().as_str() {
            "volume" | "gain" => {
                if let ResolveOTIOParameter::Double(v) = param {
                    return Some(AudioEffectOutput {
                        gain: Some(v.parameter_value),
                    });
                }
            }
            _ => {}
        }
    }

    None
}
