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
