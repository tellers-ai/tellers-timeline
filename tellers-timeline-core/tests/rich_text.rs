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
fn test_rich_text_roundtrip() {
    // Rich text survives JSON serialization/deserialization.
    let media_ref: MediaReference = serde_json::from_str(r##"{
        "OTIO_SCHEMA": "GeneratorReference.1",
        "generator_kind": "Rich",
        "parameters": { "Resolve_OTIO": [ {
            "Effect Name": "Rich Text", "Enabled": true, "Name": "Rich Text", "Type": 24,
            "Parameters": [ { "Parameter ID": "title blob", "Title HTML": "<p>Roundtrip Test</p>" } ]
        } ] }
    }"##).expect("Failed to parse");
    assert_eq!(media_ref.get_rich_text().as_deref(), Some("<p>Roundtrip Test</p>"));

    let json_str = serde_json::to_string(&media_ref).expect("Failed to serialize");
    let media_ref2: MediaReference = serde_json::from_str(&json_str).expect("Failed to deserialize");
    assert_eq!(media_ref2.get_rich_text().as_deref(), Some("<p>Roundtrip Test</p>"));
}
