use tellers_timeline_core::Stack;

fn clip(url: &str, media_id: Option<&str>) -> String {
    let refs = match media_id {
        Some(id) => format!(
            r#"{{ "OTIO_SCHEMA": "ExternalReference.1", "target_url": "{url}",
                  "metadata": {{ "tellers.ai": {{ "media_id": "{id}" }} }} }}"#
        ),
        None => format!(r#"{{ "OTIO_SCHEMA": "ExternalReference.1", "target_url": "{url}" }}"#),
    };
    format!(
        r#"{{
            "OTIO_SCHEMA": "Clip.2",
            "active_media_reference_key": "DEFAULT_MEDIA",
            "media_references": {{ "DEFAULT_MEDIA": {refs} }},
            "source_range": {{
                "OTIO_SCHEMA": "TimeRange.1",
                "start_time": {{ "OTIO_SCHEMA": "RationalTime.1", "rate": 1, "value": 0 }},
                "duration": {{ "OTIO_SCHEMA": "RationalTime.1", "rate": 1, "value": 2 }}
            }}
        }}"#
    )
}

#[test]
fn stack_clear_asset_backed_only_clears_refs_with_media_id() {
    let stack_json = format!(
        r#"{{
            "OTIO_SCHEMA": "Stack.1",
            "children": [{{
                "OTIO_SCHEMA": "Track.1",
                "kind": "Video",
                "children": [{asset}, {raw}]
            }}]
        }}"#,
        asset = clip("https://asset.example/a", Some("m1")),
        raw = clip("https://raw.example/b", None),
    );
    let mut stack: Stack = serde_json::from_str(&stack_json).unwrap();

    stack.clear_asset_backed_target_urls();

    let json = serde_json::to_string(&stack).unwrap();
    // Asset-backed URL cleared; genuine non-asset URL preserved.
    assert!(!json.contains("asset.example"));
    assert!(json.contains("raw.example"));
}
