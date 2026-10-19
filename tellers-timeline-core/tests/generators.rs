// Tests for the player's Text and Shape generator clips: a
// `GeneratorReference.1` with `generator_kind` "Text" or "Shape" whose
// `parameters` follow `definitions.MediaReference` in video-player-js's
// `schema/timeline.schema.json`. The JSON shapes here are that contract.

use serde_json::json;
use tellers_timeline_core::*;

fn clip_json(media_reference: serde_json::Value) -> serde_json::Value {
    json!({
        "OTIO_SCHEMA": "Clip.2",
        "name": "c",
        "source_range": {
            "OTIO_SCHEMA": "TimeRange.1",
            "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 1.0, "value": 4.0},
            "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 1.0, "value": 0.0}
        },
        "media_references": {"DEFAULT_MEDIA": media_reference},
        "active_media_reference_key": "DEFAULT_MEDIA",
        "metadata": {},
        "effects": []
    })
}

fn full_text_parameters() -> serde_json::Value {
    json!({
        "text": "Hello world",
        "maxWidth": 0.9,
        "wrap": true,
        "style": {
            "fontFamily": "Inter",
            "fontSize": 72.0,
            "fontWeight": 700,
            "fontStyle": "italic",
            "fill": "#ffd400",
            "align": "left",
            "lineHeight": 1.2,
            "letterSpacing": 2.0,
            "stroke": {"color": "#000", "width": 4.0},
            "dropShadow": {"color": "#000", "alpha": 0.5, "blur": 4.0, "distance": 6.0, "angle": 0.8},
            "background": {"mode": "word", "color": "#222", "opacity": 0.8, "padding": [12.0, 6.0], "radius": 8.0}
        }
    })
}

#[test]
fn reads_every_text_parameter_of_the_player_schema() {
    let clip: Clip = serde_json::from_value(clip_json(json!({
        "OTIO_SCHEMA": "GeneratorReference.1",
        "generator_kind": "Text",
        "parameters": full_text_parameters()
    })))
    .unwrap();

    assert!(clip.is_text_clip());
    assert!(!clip.is_shape_clip());
    let params = clip.text_parameters().unwrap();
    assert_eq!(params.text.as_deref(), Some("Hello world"));
    assert_eq!(params.max_width, Some(0.9));
    assert_eq!(params.wrap, Some(true));
    let style = params.style.as_ref().unwrap();
    assert_eq!(style.font_family.as_deref(), Some("Inter"));
    assert_eq!(style.font_size, Some(72.0));
    assert_eq!(style.font_weight, Some(TextFontWeight::Number(700.into())));
    assert_eq!(style.font_style, Some(TextFontStyle::Italic));
    assert_eq!(style.align, Some(TextAlign::Left));
    assert_eq!(style.stroke.as_ref().unwrap().width, Some(4.0));
    assert_eq!(style.drop_shadow.as_ref().unwrap().angle, Some(0.8));
    let background = style.background.as_ref().unwrap();
    assert_eq!(background.mode, Some(TextBackgroundMode::Word));
    assert_eq!(background.padding, Some(TextPadding::Axes(vec![12.0, 6.0])));

    // Unchanged parameters write back the same JSON.
    let mut copy = clip.clone();
    copy.set_text_parameters(&params).unwrap();
    let written = serde_json::to_value(&copy).unwrap();
    assert_eq!(
        written["media_references"]["DEFAULT_MEDIA"]["parameters"],
        full_text_parameters()
    );
}

#[test]
fn text_parameters_keep_unknown_keys_at_every_level() {
    let mut clip: Clip = serde_json::from_value(clip_json(json!({
        "OTIO_SCHEMA": "GeneratorReference.1",
        "generator_kind": "Text",
        "parameters": {
            "text": "Hi",
            "futureTop": 1,
            "words": [{"text": "Hi", "start": 0.0, "end": 0.5, "confidence": 0.9}],
            "style": {
                "fontWeight": "bold",
                "textTransform": "uppercase",
                "background": {"padding": 10.0, "blendMode": "multiply"}
            }
        }
    })))
    .unwrap();

    let mut params = clip.text_parameters().unwrap();
    assert_eq!(params.other["futureTop"], json!(1));
    assert_eq!(params.words.as_ref().unwrap()[0].other["confidence"], json!(0.9));
    let style = params.style.as_mut().unwrap();
    assert_eq!(style.font_weight, Some(TextFontWeight::Name("bold".to_string())));
    assert_eq!(style.other["textTransform"], json!("uppercase"));
    assert_eq!(style.background.as_ref().unwrap().padding, Some(TextPadding::Uniform(10.0)));
    style.fill = Some("#fff".to_string());
    clip.set_text_parameters(&params).unwrap();

    let written = serde_json::to_value(&clip).unwrap();
    let parameters = &written["media_references"]["DEFAULT_MEDIA"]["parameters"];
    assert_eq!(parameters["futureTop"], json!(1));
    assert_eq!(parameters["words"][0]["confidence"], json!(0.9));
    assert_eq!(parameters["style"]["textTransform"], json!("uppercase"));
    assert_eq!(parameters["style"]["background"]["blendMode"], json!("multiply"));
    assert_eq!(parameters["style"]["fill"], json!("#fff"));
    assert!(parameters.get("Resolve_OTIO").is_none());
}

#[test]
fn new_text_clip_serializes_like_the_player_expects() {
    let mut params = TextParameters::with_text("Title");
    params.max_width = Some(0.8);
    params.style = Some(TextClipStyle {
        font_size: Some(64.0),
        fill: Some("#ffffff".to_string()),
        ..Default::default()
    });
    let clip = Clip::new_text(3.0, &params, Some("Title".to_string()), Some("abc".to_string())).unwrap();

    assert_eq!(clip.get_id().as_deref(), Some("abc"));
    assert_eq!(clip.source_range.get_duration(), 3.0);
    assert_eq!(clip.get_text().as_deref(), Some("Title"));
    let value = serde_json::to_value(&clip).unwrap();
    let reference = &value["media_references"]["DEFAULT_MEDIA"];
    assert_eq!(reference["OTIO_SCHEMA"], json!("GeneratorReference.1"));
    assert_eq!(reference["generator_kind"], json!("Text"));
    assert_eq!(
        reference["parameters"],
        json!({"text": "Title", "maxWidth": 0.8, "style": {"fontSize": 64.0, "fill": "#ffffff"}})
    );
    assert_eq!(serde_json::from_value::<Clip>(value).unwrap(), clip);
}

#[test]
fn set_text_replaces_words_and_keeps_style() {
    let mut clip: Clip = serde_json::from_value(clip_json(json!({
        "OTIO_SCHEMA": "GeneratorReference.1",
        "generator_kind": "Text",
        "parameters": {
            "words": [{"text": "one", "start": 0.0}, {"text": "two", "start": 0.4, "end": 0.9}],
            "style": {"fill": "#f00"}
        }
    })))
    .unwrap();

    assert_eq!(clip.get_text().as_deref(), Some("one two"));
    clip.set_text("three").unwrap();
    let params = clip.text_parameters().unwrap();
    assert_eq!(params.text.as_deref(), Some("three"));
    assert!(params.words.is_none());
    assert_eq!(params.style.unwrap().fill.as_deref(), Some("#f00"));
}

#[test]
fn text_parameters_reject_out_of_range_values() {
    let mut params = TextParameters::with_text("x");
    params.max_width = Some(0.0);
    assert!(matches!(Clip::new_text(1.0, &params, None, None), Err(GeneratorError::Invalid(_))));

    let mut params = TextParameters::with_text("x");
    params.style = Some(TextClipStyle {
        font_size: Some(-1.0),
        ..Default::default()
    });
    assert!(matches!(MediaReference::new_text(&params, None), Err(GeneratorError::Invalid(_))));

    let mut params = TextParameters::with_text("x");
    params.style = Some(TextClipStyle {
        background: Some(TextBackground {
            padding: Some(TextPadding::Axes(vec![1.0, 2.0, 3.0])),
            ..Default::default()
        }),
        ..Default::default()
    });
    assert!(params.validate().is_err());
}

#[test]
fn malformed_text_parameters_still_load_but_do_not_parse() {
    let clip: Clip = serde_json::from_value(clip_json(json!({
        "OTIO_SCHEMA": "GeneratorReference.1",
        "generator_kind": "Text",
        "parameters": {"text": "x", "style": {"align": "justify"}}
    })))
    .unwrap();
    assert!(matches!(clip.text_parameters(), Err(GeneratorError::Invalid(_))));
    assert_eq!(clip.get_text(), None);
}

#[test]
fn shape_parameters_round_trip() {
    let parameters = json!({
        "shape": "line",
        "stroke": "#ff0000",
        "stroke_width": 6.0,
        "x1": 0.0, "y1": 0.2, "x2": 1.0, "y2": 0.8,
        "dash": [4, 2]
    });
    let mut clip: Clip = serde_json::from_value(clip_json(json!({
        "OTIO_SCHEMA": "GeneratorReference.1",
        "generator_kind": "Shape",
        "parameters": parameters
    })))
    .unwrap();

    assert!(clip.is_shape_clip());
    let mut shape = clip.shape_parameters().unwrap();
    assert_eq!(shape.shape, ShapeKind::Line);
    assert_eq!(shape.stroke_width, Some(6.0));
    assert_eq!(shape.y2, Some(0.8));
    assert_eq!(shape.other["dash"], json!([4, 2]));

    clip.set_shape_parameters(&shape).unwrap();
    let written = serde_json::to_value(&clip).unwrap();
    assert_eq!(written["media_references"]["DEFAULT_MEDIA"]["parameters"], parameters);

    shape.shape = ShapeKind::Rectangle;
    shape.fill = Some("#00ff00".to_string());
    shape.corner_radius = Some(24.0);
    clip.set_shape_parameters(&shape).unwrap();
    let written = serde_json::to_value(&clip).unwrap();
    let parameters = &written["media_references"]["DEFAULT_MEDIA"]["parameters"];
    assert_eq!(parameters["shape"], json!("rectangle"));
    assert_eq!(parameters["corner_radius"], json!(24.0));
    assert_eq!(parameters["dash"], json!([4, 2]));
}

#[test]
fn new_shape_clip_and_missing_shape() {
    let mut shape = ShapeParameters::new(ShapeKind::Ellipse);
    shape.fill = Some("#fff".to_string());
    let clip = Clip::new_shape(2.0, &shape, None, None).unwrap();
    let value = serde_json::to_value(&clip).unwrap();
    let reference = &value["media_references"]["DEFAULT_MEDIA"];
    assert_eq!(reference["generator_kind"], json!("Shape"));
    assert_eq!(reference["parameters"], json!({"shape": "ellipse", "fill": "#fff"}));
    assert!(clip.get_id().is_some());

    // `shape` is required by the schema.
    let clip: Clip = serde_json::from_value(clip_json(json!({
        "OTIO_SCHEMA": "GeneratorReference.1",
        "generator_kind": "Shape",
        "parameters": {"fill": "#fff"}
    })))
    .unwrap();
    assert!(matches!(clip.shape_parameters(), Err(GeneratorError::Invalid(_))));
}

#[test]
fn accessors_refuse_the_wrong_kind() {
    let mut text = Clip::new_text(1.0, &TextParameters::with_text("x"), None, None).unwrap();
    assert!(matches!(
        text.shape_parameters(),
        Err(GeneratorError::WrongKind { expected: "Shape", found: Some(ref kind) }) if kind == "Text"
    ));
    assert!(text.set_shape_parameters(&ShapeParameters::new(ShapeKind::Line)).is_err());
    assert!(text.text_parameters().is_ok());

    let rich = MediaReference::create_rich_text_reference("<p>x</p>".to_string());
    assert!(matches!(rich.text_parameters(), Err(GeneratorError::WrongKind { .. })));

    let video = MediaReference::ExternalReference {
        target_url: "mem://".to_string(),
        available_range: None,
        name: None,
        available_image_bounds: None,
        metadata: json!({}),
    };
    assert!(matches!(
        video.text_parameters(),
        Err(GeneratorError::WrongKind { found: None, .. })
    ));
}

#[test]
fn text_and_shape_clips_are_positioned_by_the_transform_effect() {
    for mut clip in [
        Clip::new_text(1.0, &TextParameters::with_text("x"), None, None).unwrap(),
        Clip::new_shape(1.0, &ShapeParameters::new(ShapeKind::Rectangle), None, None).unwrap(),
    ] {
        let before = serde_json::to_value(clip.media_references["DEFAULT_MEDIA"].clone()).unwrap();
        clip.set_position(MediaReferencePosition {
            x: 0.1,
            y: -0.2,
            rotation: 15.0,
            zoom_x: 1.5,
            zoom_y: 1.5,
        });

        // The parameters stay the player's, with no Resolve "Rich Text" block.
        let after = serde_json::to_value(clip.media_references["DEFAULT_MEDIA"].clone()).unwrap();
        assert_eq!(before, after);
        assert_eq!(clip.effects.len(), 1);

        let position = clip.get_position();
        assert!((position.x - 0.1).abs() < 1e-9);
        assert!((position.y + 0.2).abs() < 1e-9);
        assert!((position.rotation - 15.0).abs() < 1e-9);
        assert!((position.zoom_x - 1.5).abs() < 1e-9);
    }
}
