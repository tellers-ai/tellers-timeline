// Tests for the public group-id metadata accessors (`item_link_group_id`,
// `item_tellers_group_id`, `set_item_tellers_group_id`, and the underlying
// `resolve/set/remove_tellers_group_id` helpers). These read/write the
// `tellers.ai` and `Resolve_OTIO` metadata namespaces and are consumed by the
// flutter_rust_bridge editor layer, so they are part of the public surface.

use tellers_timeline_core::{
    item_link_group_id, item_tellers_group_id, remove_tellers_group_id, resolve_tellers_group_id,
    set_item_tellers_group_id, set_tellers_group_id, Clip, Gap, Item, MediaReference, RationalTime,
    TimeRange,
};

fn range(duration: f64) -> TimeRange {
    TimeRange {
        otio_schema: "TimeRange.1".to_string(),
        start_time: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: 1.0,
            value: 0.0,
        },
        duration: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: 1.0,
            value: duration,
        },
    }
}

fn media_ref() -> MediaReference {
    MediaReference::ExternalReference {
        target_url: "file:///video.mov".to_string(),
        available_range: Some(range(100.0)),
        name: None,
        available_image_bounds: Some(serde_json::Value::Null),
        metadata: serde_json::json!({}),
    }
}

fn clip(id: &str) -> Item {
    Item::Clip(Clip::new_single_media_reference(
        range(5.0),
        media_ref(),
        None,
        Some(id.to_string()),
    ))
}

fn gap() -> Item {
    Item::Gap(Gap {
        otio_schema: "Gap.1".to_string(),
        name: None,
        source_range: range(2.0),
        metadata: serde_json::json!({}),
        effects: Vec::new(),
    })
}

#[test]
fn tellers_group_id_round_trips_and_clears() {
    let mut item = clip("clip-1");
    assert_eq!(item_tellers_group_id(&item), None);

    set_item_tellers_group_id(&mut item, Some(42));
    assert_eq!(item_tellers_group_id(&item), Some(42));

    set_item_tellers_group_id(&mut item, None);
    assert_eq!(item_tellers_group_id(&item), None);
}

#[test]
fn tellers_group_id_coerces_uint_and_string() {
    let mut item = clip("clip-1");
    // A stringified id (as Resolve/legacy data can carry) still reads back.
    if let Item::Clip(c) = &mut item {
        c.metadata = serde_json::json!({ "tellers.ai": { "Tellers Group ID": "7" } });
    }
    assert_eq!(item_tellers_group_id(&item), Some(7));
}

#[test]
fn link_group_id_reads_resolve_otio_and_ignores_gaps() {
    let mut item = clip("clip-1");
    if let Item::Clip(c) = &mut item {
        c.metadata = serde_json::json!({ "Resolve_OTIO": { "Link Group ID": 9 } });
    }
    assert_eq!(item_link_group_id(&item), Some(9));

    // Gaps never carry a group id.
    assert_eq!(item_link_group_id(&gap()), None);
    assert_eq!(item_tellers_group_id(&gap()), None);
}

#[test]
fn metadata_level_helpers_operate_on_raw_value() {
    let mut metadata = serde_json::json!({});
    assert_eq!(resolve_tellers_group_id(&metadata), None);

    set_tellers_group_id(&mut metadata, 3);
    assert_eq!(resolve_tellers_group_id(&metadata), Some(3));

    assert!(remove_tellers_group_id(&mut metadata));
    assert_eq!(resolve_tellers_group_id(&metadata), None);
    // Removing again reports nothing was present.
    assert!(!remove_tellers_group_id(&mut metadata));
}
