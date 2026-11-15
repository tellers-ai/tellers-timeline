// Editing a composite item as the player's `editCompositeItem` does
// (video-player-js #837): box and crop, Transforms (3D tilt included, #838),
// filters, and Text / Shape patches, written where the player reads them.

mod common;

use common::*;
use serde_json::{json, Value};
use tellers_timeline_core::{
    CompositeItemEdit, CompositeItemEditError, CompositeStack, TellersEffect, TextParameters,
};

fn text_clip(id: &str) -> Clip {
    let parameters: TextParameters = serde_json::from_value(json!({
        "text": "Hello", "words": [{ "text": "Hello", "start": 0.0 }],
        "style": { "fontSize": 64, "color": "#ffffff" }
    }))
    .unwrap();
    Clip::new_text(2.0, &parameters, None, Some(id.to_string())).unwrap()
}

fn shape_clip(id: &str) -> Clip {
    let parameters = serde_json::from_value(json!({ "shape": "rectangle", "fill": "#ff0000" })).unwrap();
    Clip::new_shape(2.0, &parameters, None, Some(id.to_string())).unwrap()
}

fn video_clip(id: &str) -> Clip {
    Clip::new_single_media_reference(range(2.0), media_ref("https://example.com/a.mp4", None), None, Some(id.to_string()))
}

/// V1: [a 1s][comp: inner [title][shape][video]]
fn timeline() -> Timeline {
    let mut inner = Track::new(TrackKind::Video, Some("inner".to_string()));
    inner.items = vec![
        Item::Clip(text_clip("title")),
        Item::Clip(shape_clip("bar")),
        Item::Clip(video_clip("shot")),
    ];
    let comp = CompositeStack::new(vec![inner], Some("Comp".into()), Some("comp".into()));
    let mut v1 = Track::new(TrackKind::Video, Some("v1".to_string()));
    v1.items = vec![Item::Clip(video_clip("a")), Item::Composite(comp)];
    let mut timeline = Timeline::default();
    timeline.tracks.children = vec![v1];
    timeline
}

fn edit(value: Value) -> CompositeItemEdit {
    serde_json::from_value(value).unwrap()
}

fn clip<'a>(timeline: &'a Timeline, id: &str) -> &'a Clip {
    match timeline.tracks.get_nested_item(id) {
        Some(Item::Clip(clip)) => clip,
        other => panic!("{id}: {other:?}"),
    }
}

fn names(timeline: &Timeline, id: &str) -> Vec<String> {
    timeline.get_item_tellers_effects(id).unwrap().into_iter().map(|e| e.effect_name).collect()
}

#[test]
fn item_source_names_the_item() {
    let timeline = timeline();
    assert_eq!(timeline.tracks.composite_item_id("comp", 0, 1).as_deref(), Some("bar"));
    assert_eq!(timeline.tracks.composite_item_id("comp", 0, 9), None);
    assert_eq!(timeline.tracks.composite_item_id("a", 0, 0), None);
}

#[test]
fn box_and_crop_go_to_the_resolve_transform_and_cropping() {
    let mut timeline = timeline();
    timeline
        .edit_composite_item("shot", &edit(json!({ "box": { "x": 0.1, "width": 0.5, "cropLeft": 0.2 } })))
        .unwrap();
    let shot = clip(&timeline, "shot");
    let placement = shot.resolve_placement();
    assert_eq!((placement.x, placement.y, placement.zoom_x, placement.zoom_y), (0.1, 0.0, 0.5, 1.0));
    assert_eq!(shot.get_crop().crop_left, 0.2);
    // Merged: a later edit keeps what it does not name.
    timeline.edit_composite_item("shot", &edit(json!({ "box": { "rotation": 15 } }))).unwrap();
    let placement = clip(&timeline, "shot").resolve_placement();
    assert_eq!((placement.x, placement.zoom_x, placement.rotation), (0.1, 0.5, 15.0));

    // A Shape item's box is its Resolve Transform too, not a tellers Transform.
    timeline.edit_composite_item("bar", &edit(json!({ "box": { "y": -0.3, "height": 0.2 } }))).unwrap();
    let placement = clip(&timeline, "bar").resolve_placement();
    assert_eq!((placement.y, placement.zoom_y), (-0.3, 0.2));
    assert!(names(&timeline, "bar").is_empty());
}

#[test]
fn box_and_crop_are_refused_where_the_player_does_not_read_them() {
    let mut timeline = timeline();
    let before = timeline.clone();
    assert_eq!(
        timeline.edit_composite_item("title", &edit(json!({ "box": { "x": 0.1 } }))),
        Err(CompositeItemEditError::BoxOnText("title".into()))
    );
    assert_eq!(
        timeline.edit_composite_item("bar", &edit(json!({ "box": { "x": 0.1, "cropTop": 0.1 } }))),
        Err(CompositeItemEditError::CropNotOnMedia("bar".into()))
    );
    assert_eq!(timeline, before, "all or nothing");
    assert_eq!(
        timeline.edit_composite_item("a", &edit(json!({ "box": { "x": 0.1 } }))),
        Err(CompositeItemEditError::NotInComposite("a".into()))
    );
    assert_eq!(
        timeline.edit_composite_item("nope", &edit(json!({}))),
        Err(CompositeItemEditError::NoSuchItem("nope".into()))
    );
}

#[test]
fn transforms_replace_its_own_and_keep_stabilize_and_3d_tilt() {
    let mut timeline = timeline();
    assert!(timeline.set_item_tellers_effects(
        "shot",
        vec![
            TellersEffect::new("Stabilize"),
            TellersEffect::new("Blur").with_parameter("strength", 4.0),
            TellersEffect::new("Transform").with_parameter("x", 0.2),
        ],
    ));
    timeline
        .edit_composite_item(
            "shot",
            &edit(json!({ "transforms": [
                { "rotationX": 20, "rotationY": { "keyframes": [{ "t": 0, "v": 0 }, { "t": 1, "v": 30 }] } },
                { "scale": 1.2 }
            ] })),
        )
        .unwrap();
    let effects = timeline.get_item_tellers_effects("shot").unwrap();
    let listed: Vec<_> = effects.iter().map(|e| e.effect_name.as_str()).collect();
    assert_eq!(listed, ["Stabilize", "Blur", "Transform", "Transform"]);
    assert_eq!(effects[2].parameters["rotationX"], json!(20));
    assert_eq!(effects[2].parameters["rotationY"]["keyframes"][1]["v"], json!(30));
    assert_eq!(effects[3].parameters["scale"], json!(1.2));

    timeline.edit_composite_item("shot", &edit(json!({ "transforms": [] }))).unwrap();
    assert_eq!(names(&timeline, "shot"), ["Stabilize", "Blur"]);

    assert!(matches!(
        timeline.edit_composite_item("shot", &edit(json!({ "transforms": [{ "rotationX": "tilted" }] }))),
        Err(CompositeItemEditError::InvalidEffect { .. })
    ));
}

#[test]
fn effects_replace_its_filters_only() {
    let mut timeline = timeline();
    assert!(timeline.set_item_tellers_effects(
        "shot",
        vec![
            TellersEffect::new("Transform").with_parameter("x", 0.2),
            TellersEffect::new("Blur"),
            TellersEffect::new("Mask"),
            TellersEffect::new("Pixelate"),
        ],
    ));
    timeline
        .edit_composite_item(
            "shot",
            &edit(json!({ "effects": [
                { "name": "Glow", "parameters": { "distance": 10 } },
                { "name": "Adjustment", "parameters": {} }
            ] })),
        )
        .unwrap();
    assert_eq!(names(&timeline, "shot"), ["Transform", "Glow", "Adjustment", "Mask"]);

    // Not drawn on an item.
    for name in ["Border", "MotionBlur", "Transform", "Volume"] {
        assert_eq!(
            timeline.edit_composite_item("shot", &edit(json!({ "effects": [{ "name": name }] }))),
            Err(CompositeItemEditError::NotAnItemFilter { effect: name.into() })
        );
    }
}

#[test]
fn text_patch_restyles_a_text_item() {
    let mut timeline = timeline();
    assert!(timeline.set_item_tellers_effects(
        "title",
        vec![TellersEffect::new("TextShadow").with_parameter("style", "pulse").with_parameter("colors", json!(["#f00"]))],
    ));
    timeline
        .edit_composite_item(
            "title",
            &edit(json!({ "textPatch": {
                "text": "Bye", "fontSize": 80, "color": null, "wrap": true,
                "animation": "TextTypewriter", "shadowAnimation": "sweep"
            } })),
        )
        .unwrap();
    let title = clip(&timeline, "title");
    let parameters = serde_json::to_value(title.text_parameters().unwrap()).unwrap();
    assert_eq!(parameters["text"], json!("Bye"));
    assert!(parameters.get("words").is_none(), "a new string drops the word timings");
    assert_eq!(parameters["wrap"], json!(true));
    assert_eq!(parameters["style"]["fontSize"].as_f64(), Some(80.0));
    assert!(parameters["style"].get("color").is_none());
    let effects = timeline.get_item_tellers_effects("title").unwrap();
    assert_eq!(effects[0].effect_name, "TextTypewriter");
    assert_eq!(effects[1].effect_name, "TextShadow");
    assert_eq!(effects[1].parameters["style"], json!("sweep"));
    assert_eq!(effects[1].parameters["colors"], json!(["#f00"]), "keeps its colors");

    timeline
        .edit_composite_item("title", &edit(json!({ "textPatch": { "animation": "none", "shadowAnimation": "none" } })))
        .unwrap();
    assert!(names(&timeline, "title").is_empty());

    assert_eq!(
        timeline.edit_composite_item("shot", &edit(json!({ "textPatch": { "text": "x" } }))),
        Err(CompositeItemEditError::NotText("shot".into()))
    );
    assert_eq!(
        timeline.edit_composite_item("title", &edit(json!({ "textPatch": { "animation": "Spin" } }))),
        Err(CompositeItemEditError::UnknownAnimation("Spin".into()))
    );
}

#[test]
fn shape_patch_merges_into_its_parameters() {
    let mut timeline = timeline();
    timeline
        .edit_composite_item(
            "bar",
            &edit(json!({ "shapePatch": { "fill": "#00ff00", "cornerRadius": 12, "animation": "ShapeHandwrite" } })),
        )
        .unwrap();
    let parameters = serde_json::to_value(clip(&timeline, "bar").shape_parameters().unwrap()).unwrap();
    assert_eq!(parameters["fill"], json!("#00ff00"));
    assert_eq!(parameters["shape"], json!("rectangle"));
    assert_eq!(names(&timeline, "bar"), ["ShapeHandwrite"]);

    timeline.edit_composite_item("bar", &edit(json!({ "shapePatch": { "cornerRadius": null } }))).unwrap();
    let parameters = serde_json::to_value(clip(&timeline, "bar").shape_parameters().unwrap()).unwrap();
    assert!(parameters.get("cornerRadius").is_none());

    assert_eq!(
        timeline.edit_composite_item("title", &edit(json!({ "shapePatch": { "fill": "#fff" } }))),
        Err(CompositeItemEditError::NotShape("title".into()))
    );
}

#[test]
fn an_edit_round_trips_through_otio_and_keeps_lengths() {
    let mut timeline = timeline();
    let duration = timeline.tracks.get_nested_item("comp").unwrap().duration();
    timeline
        .edit_composite_item("shot", &edit(json!({ "box": { "x": 0.25 }, "transforms": [{ "rotationY": 12 }] })))
        .unwrap();
    let reread: Timeline = serde_json::from_str(&serde_json::to_string(&timeline).unwrap()).unwrap();
    assert_eq!(clip(&reread, "shot").resolve_placement().x, 0.25);
    assert_eq!(reread.get_item_tellers_effects("shot").unwrap()[0].parameters["rotationY"], json!(12));
    assert_eq!(reread.tracks.get_nested_item("comp").unwrap().duration(), duration);
}
