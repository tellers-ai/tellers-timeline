import json

import pytest

from tellers_timeline import Clip, Item, MediaReference, MediaReferencePosition, Timeline, Track


def media_ref(clip: Clip) -> dict:
    return json.loads(clip.to_json())["media_references"]["DEFAULT_MEDIA"]


def test_new_text_clip_writes_the_player_shape():
    params = {
        "text": "Hello world",
        "maxWidth": 0.9,
        "wrap": True,
        "style": {
            "fontFamily": "Inter",
            "fontSize": 72,
            "fontWeight": 700,
            "fontStyle": "italic",
            "fill": "#ffd400",
            "align": "center",
            "lineHeight": 1.2,
            "letterSpacing": 2,
            "stroke": {"color": "#000", "width": 4},
            "dropShadow": {"color": "#000", "alpha": 0.5, "blur": 4, "distance": 6, "angle": 0.8},
            "background": {"mode": "line", "color": "#222", "opacity": 0.8, "padding": [12, 6], "radius": 8},
        },
    }
    clip = Clip.new_text(3.0, params, name="Title", id="t1")

    assert clip.get_id() == "t1"
    assert clip.get_duration() == 3.0
    assert clip.is_text_clip() and not clip.is_shape_clip()
    assert clip.get_text() == "Hello world"
    assert clip.get_text_parameters() == params
    ref = media_ref(clip)
    assert ref["OTIO_SCHEMA"] == "GeneratorReference.1"
    assert ref["generator_kind"] == "Text"
    assert ref["parameters"] == params


def test_text_parameters_keep_unknown_keys():
    clip = Clip.new_text(1.0, {"text": "Hi", "future": {"a": 1}, "style": {"textTransform": "uppercase"}})
    params = clip.get_text_parameters()
    params["style"]["fill"] = "#fff"
    clip.set_text_parameters(params)

    stored = media_ref(clip)["parameters"]
    assert stored["future"] == {"a": 1}
    assert stored["style"] == {"textTransform": "uppercase", "fill": "#fff"}


def test_words_and_set_text():
    clip = Clip.new_text(
        2.0,
        {"words": [{"text": "one", "start": 0.0}, {"text": "two", "start": 0.5, "end": 1.0}], "style": {"fill": "#f00"}},
    )
    assert clip.get_text() == "one two"
    clip.set_text("three")
    assert clip.get_text_parameters() == {"text": "three", "style": {"fill": "#f00"}}


def test_invalid_text_parameters_raise():
    with pytest.raises(ValueError):
        Clip.new_text(1.0, {"text": "x", "maxWidth": 0})
    with pytest.raises(ValueError):
        Clip.new_text(1.0, {"text": "x", "style": {"align": "justify"}})
    with pytest.raises(ValueError):
        Clip.new_text(1.0, {"words": [{"text": "no start"}]})


def test_shape_clip_round_trip():
    clip = Clip.new_shape(2.0, {"shape": "rectangle", "fill": "#00ff00", "corner_radius": 24})
    assert clip.is_shape_clip()
    assert clip.get_text() is None
    ref = media_ref(clip)
    assert ref["generator_kind"] == "Shape"
    assert ref["parameters"] == {"shape": "rectangle", "fill": "#00ff00", "corner_radius": 24.0}

    params = clip.get_shape_parameters()
    params.update({"shape": "line", "stroke": "#f00", "stroke_width": 6, "x1": 0, "y1": 0.2, "x2": 1, "y2": 0.8})
    clip.set_shape_parameters(params)
    assert clip.get_shape_parameters()["shape"] == "line"
    assert media_ref(clip)["parameters"]["y2"] == 0.8

    with pytest.raises(ValueError):
        Clip.new_shape(1.0, {"fill": "#fff"})
    with pytest.raises(ValueError):
        Clip.new_shape(1.0, {"shape": "triangle"})


def test_wrong_kind_raises():
    text = Clip.new_text(1.0, {"text": "x"})
    with pytest.raises(ValueError):
        text.get_shape_parameters()
    with pytest.raises(ValueError):
        text.set_shape_parameters({"shape": "line"})

    rich = MediaReference.create_rich_text_reference("<p>x</p>")
    assert rich.get_generator_kind() == "Rich"
    with pytest.raises(ValueError):
        rich.get_text_parameters()

    video = MediaReference("https://example.test/a.mp4")
    assert video.get_generator_kind() is None
    assert not video.is_text_generator()


def test_media_reference_constructors():
    text = MediaReference.create_text_reference({"text": "x"})
    assert text.is_text_generator()
    assert text.get_text_parameters() == {"text": "x"}
    text.set_text_parameters({"text": "y", "wrap": False})
    assert json.loads(str(text))["parameters"] == {"text": "y", "wrap": False}

    shape = MediaReference.create_shape_reference({"shape": "ellipse"}, name="Dot")
    assert shape.is_shape_generator()
    assert json.loads(str(shape))["name"] == "Dot"

    clip = Clip(1.0, {"DEFAULT_MEDIA": shape})
    assert clip.get_shape_parameters() == {"shape": "ellipse"}


def test_text_clip_position_uses_transform_effect_and_survives_timeline_round_trip():
    clip = Clip.new_text(1.0, {"text": "x"})
    clip.set_position(MediaReferencePosition(0.1, -0.2, 0.0, 1.0, 1.0))
    assert media_ref(clip)["parameters"] == {"text": "x"}
    pos = clip.get_position()
    assert pos.get_x() == pytest.approx(0.1) and pos.get_y() == pytest.approx(-0.2)

    timeline = Timeline([Track(children=[Item.from_clip(clip)])])
    reparsed = Timeline.parse_json(timeline.to_json())
    item = json.loads(reparsed.to_json())["tracks"]["children"][0]["children"][0]
    assert item["media_references"]["DEFAULT_MEDIA"]["parameters"] == {"text": "x"}
