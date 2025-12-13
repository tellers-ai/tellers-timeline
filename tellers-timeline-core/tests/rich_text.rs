use tellers_timeline_core::types::MediaReference;

#[test]
fn test_get_rich_text() {
    // Test getting rich text from a GeneratorReference
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
                            "Parameter ID": "title blob",
                            "Title HTML": "<p>Test HTML Content</p>"
                        }
                    ],
                    "Type": 24
                }
            ]
        }
    }
    "##;

    let media_ref: MediaReference = serde_json::from_str(json_generator_ref)
        .expect("Failed to parse GeneratorReference");

    let html = media_ref.get_rich_text();
    assert!(html.is_some());
    assert_eq!(html.unwrap(), "<p>Test HTML Content</p>");
}

#[test]
fn test_get_rich_text_not_found() {
    // Test getting rich text when it doesn't exist
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
            "Resolve_OTIO": []
        }
    }
    "##;

    let media_ref: MediaReference = serde_json::from_str(json_generator_ref)
        .expect("Failed to parse GeneratorReference");

    let html = media_ref.get_rich_text();
    assert!(html.is_none());
}

#[test]
fn test_get_rich_text_external_reference() {
    // Test getting rich text from ExternalReference (should return None)
    let media_ref = MediaReference::ExternalReference {
        target_url: "file:///test.mp4".to_string(),
        available_range: None,
        name: None,
        available_image_bounds: None,
        metadata: serde_json::Value::Object(serde_json::Map::new()),
    };

    let html = media_ref.get_rich_text();
    assert!(html.is_none());
}

#[test]
fn test_set_rich_text() {
    // Test setting rich text on a GeneratorReference
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
            "Resolve_OTIO": []
        }
    }
    "##;

    let mut media_ref: MediaReference = serde_json::from_str(json_generator_ref)
        .expect("Failed to parse GeneratorReference");

    // Set rich text
    media_ref.set_rich_text("<p>New HTML Content</p>".to_string(), None);

    // Verify it was set
    let html = media_ref.get_rich_text();
    assert!(html.is_some());
    assert_eq!(html.unwrap(), "<p>New HTML Content</p>");
}

#[test]
fn test_set_rich_text_with_position() {
    // Test setting rich text with position
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
            "Resolve_OTIO": []
        }
    }
    "##;

    let mut media_ref: MediaReference = serde_json::from_str(json_generator_ref)
        .expect("Failed to parse GeneratorReference");

    // Set rich text with position
    media_ref.set_rich_text("<p>HTML with Position</p>".to_string(), Some([0.3, 0.7]));

    // Verify HTML was set
    let html = media_ref.get_rich_text();
    assert!(html.is_some());
    assert_eq!(html.unwrap(), "<p>HTML with Position</p>");

    // Verify position was set by checking the parameters
    if let MediaReference::GeneratorReference { parameters, .. } = &media_ref {
        if let Some(resolve_otio_effects) = &parameters.resolve_otio {
            for effect in resolve_otio_effects {
                if effect.effect_name == "Rich Text" && effect.effect_type == 24 {
                    for parameter in &effect.parameters {
                        if let tellers_timeline_core::types::ResolveOTIOParameter::PointF(param) = parameter {
                            if param.parameter_id == "position" {
                                assert_eq!(param.parameter_value, Some([0.3, 0.7]));
                                return;
                            }
                        }
                    }
                }
            }
        }
        panic!("Position parameter not found");
    } else {
        panic!("Expected GeneratorReference");
    }
}

#[test]
fn test_set_rich_text_default_position() {
    // Test that default position is set when position is None and no position exists
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
            "Resolve_OTIO": []
        }
    }
    "##;

    let mut media_ref: MediaReference = serde_json::from_str(json_generator_ref)
        .expect("Failed to parse GeneratorReference");

    // Set rich text without position (should create default position)
    media_ref.set_rich_text("<p>HTML with Default Position</p>".to_string(), None);

    // Verify position was set to default
    if let MediaReference::GeneratorReference { parameters, .. } = &media_ref {
        if let Some(resolve_otio_effects) = &parameters.resolve_otio {
            for effect in resolve_otio_effects {
                if effect.effect_name == "Rich Text" && effect.effect_type == 24 {
                    for parameter in &effect.parameters {
                        if let tellers_timeline_core::types::ResolveOTIOParameter::PointF(param) = parameter {
                            if param.parameter_id == "position" {
                                assert_eq!(param.parameter_value, Some([0.5, 0.5]));
                                return;
                            }
                        }
                    }
                }
            }
        }
        panic!("Position parameter not found");
    } else {
        panic!("Expected GeneratorReference");
    }
}

#[test]
fn test_set_rich_text_update_existing() {
    // Test updating existing rich text
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
                            "Parameter ID": "title blob",
                            "Title HTML": "<p>Old HTML</p>"
                        }
                    ],
                    "Type": 24
                }
            ]
        }
    }
    "##;

    let mut media_ref: MediaReference = serde_json::from_str(json_generator_ref)
        .expect("Failed to parse GeneratorReference");

    // Update rich text
    media_ref.set_rich_text("<p>Updated HTML</p>".to_string(), None);

    // Verify it was updated
    let html = media_ref.get_rich_text();
    assert!(html.is_some());
    assert_eq!(html.unwrap(), "<p>Updated HTML</p>");
}

#[test]
fn test_rich_text_roundtrip() {
    // Test that rich text survives JSON serialization/deserialization
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
            "Resolve_OTIO": []
        }
    }
    "##;

    let mut media_ref: MediaReference = serde_json::from_str(json_generator_ref)
        .expect("Failed to parse GeneratorReference");

    // Set rich text
    media_ref.set_rich_text("<p>Roundtrip Test</p>".to_string(), Some([0.25, 0.75]));

    // Serialize to JSON
    let json_str = serde_json::to_string(&media_ref).expect("Failed to serialize");

    // Deserialize
    let media_ref2: MediaReference = serde_json::from_str(&json_str)
        .expect("Failed to deserialize");

    // Verify rich text is preserved
    let html = media_ref2.get_rich_text();
    assert!(html.is_some());
    assert_eq!(html.unwrap(), "<p>Roundtrip Test</p>");
}
