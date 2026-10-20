"""Tests for the tellers effect list methods on the timeline, its tracks and clips.

Tellers effects are the player's effects (filters, Transform, Mask, Volume,
Text effects, ...): `Effect.1` entries whose parameters live under
`metadata["tellers.ai"].parameters`, as numbers or keyframes.
"""
import json

import pytest

from tellers_timeline import (
    Clip,
    Gap,
    Item,
    MediaReference,
    Timeline,
    Track,
    evaluate_animatable,
    tellers_effect_specs,
    validate_tellers_effect,
)


def make_clip(id="c1"):
    ref = MediaReference("file:///test.mp4")
    return Clip(10.0, {"DEFAULT_MEDIA": ref}, "DEFAULT_MEDIA", id=id)


def make_timeline():
    track = Track(
        kind="video",
        id="t1",
        children=[
            Item.from_clip(make_clip("c1")),
            Item.from_gap(Gap(1.0, id="g1")),
            Item.from_clip(make_clip("c2")),
        ],
    )
    return Timeline([track])


FADE_IN = {"keyframes": [{"t": 0.0, "v": 0.0, "ease": "easeOut"}, {"t": 1.0, "v": 8.0}]}


def test_clip_effects_round_trip():
    clip = make_clip()
    assert clip.get_tellers_effects() == []
    assert clip.push_tellers_effect(
        {"effect_name": "Blur", "parameters": {"strength": FADE_IN}}
    )
    assert clip.push_tellers_effect(
        {"effect_name": "ChromaKey", "name": "green", "parameters": {"color": "#00ff00"}}
    )
    assert clip.get_tellers_effects() == [
        {"effect_name": "Blur", "name": None, "enabled": True, "parameters": {"strength": FADE_IN}},
        {"effect_name": "ChromaKey", "name": "green", "enabled": True, "parameters": {"color": "#00ff00"}},
    ]
    raw = json.loads(str(clip))["effects"][0]
    assert raw["OTIO_SCHEMA"] == "Effect.1"
    assert raw["effect_name"] == "tellers.ai.Blur"
    assert raw["metadata"]["tellers.ai"]["parameters"]["strength"] == FADE_IN


def test_bare_names_are_read_and_rewritten_prefixed():
    tl = Timeline.parse_json(make_timeline().to_json())
    data = json.loads(tl.to_json())
    data["tracks"]["effects"] = [
        {"OTIO_SCHEMA": "Effect.1", "effect_name": "Pan", "metadata": {"tellers.ai": {"parameters": {"pan": -1}}}}
    ]
    tl = Timeline.parse_json(json.dumps(data))
    assert tl.get_tellers_effects()[0]["effect_name"] == "Pan"
    assert tl.set_tellers_effect_parameter(0, "pan", 1)
    assert json.loads(tl.to_json())["tracks"]["effects"][0]["effect_name"] == "tellers.ai.Pan"
    assert tl.push_tellers_effect({"effect_name": "tellers.ai.TimeWarp", "parameters": {"preserve_pitch": False}})
    assert tl.get_tellers_effects()[1]["effect_name"] == "TimeWarp"


def test_unknown_effect_names_are_refused():
    clip = make_clip()
    assert not clip.push_tellers_effect({"effect_name": "Sparkles"})
    assert not clip.set_tellers_effects([{"effect_name": "Blur"}, {"effect_name": "Sparkles"}])
    assert clip.get_tellers_effects() == []
    with pytest.raises(TypeError):
        clip.push_tellers_effect("Blur")


def test_enable_parameter_replace_remove():
    clip = make_clip()
    clip.set_tellers_effects([{"effect_name": "Blur"}, {"effect_name": "Pixelate"}])
    assert clip.set_tellers_effect_enabled(1, False)
    assert clip.get_tellers_effects()[1]["enabled"] is False
    assert json.loads(str(clip))["effects"][1]["enabled"] is False

    assert clip.set_tellers_effect_parameter(0, "strength", 4)
    assert clip.get_tellers_effects()[0]["parameters"] == {"strength": 4}
    assert clip.set_tellers_effect_parameter(0, "strength")
    assert clip.get_tellers_effects()[0]["parameters"] == {}
    assert not clip.set_tellers_effect_parameter(5, "strength", 1)

    previous = clip.replace_tellers_effect_at(0, {"effect_name": "Noise", "parameters": {"amount": 0.2}})
    assert previous["effect_name"] == "Blur"
    assert [e["effect_name"] for e in clip.get_tellers_effects()] == ["Noise", "Pixelate"]

    removed = clip.remove_tellers_effect_at(1)
    assert removed["effect_name"] == "Pixelate"
    assert clip.remove_tellers_effect_at(3) is None
    assert clip.clear_tellers_effects()
    assert not clip.clear_tellers_effects()


def test_other_effects_are_kept():
    clip = make_clip()
    clip.push_color_lut("look")
    clip.push_tellers_effect({"effect_name": "Transform", "parameters": {"x": 0.1}})
    clip.clear_tellers_effects()
    assert clip.get_color_luts()[0]["asset_id"] == "look"


def test_volume_is_a_tellers_effect():
    clip = make_clip()
    clip.set_volume(-6.0)
    assert [e["effect_name"] for e in clip.get_tellers_effects()] == ["Volume"]
    assert clip.get_tellers_effects()[0]["parameters"]["volume"] == pytest.approx(-6.0)


def test_timeline_and_by_id_stages():
    tl = make_timeline()
    assert tl.push_tellers_effect({"effect_name": "Noise"})
    assert tl.push_track_tellers_effect("t1", {"effect_name": "Blend", "parameters": {"mode": "screen"}})
    assert tl.push_item_tellers_effect("c2", {"effect_name": "Mask", "parameters": {"shape": "ellipse"}})
    assert not tl.push_item_tellers_effect("g1", {"effect_name": "Blur"})
    assert not tl.push_item_tellers_effect("missing", {"effect_name": "Blur"})
    assert tl.get_item_tellers_effects("missing") is None

    reparsed = Timeline.parse_json(tl.to_json())
    assert reparsed.get_tellers_effects()[0]["effect_name"] == "Noise"
    assert reparsed.get_track_tellers_effects("t1")[0]["parameters"] == {"mode": "screen"}
    assert reparsed.get_item_tellers_effects("c2")[0]["parameters"] == {"shape": "ellipse"}
    assert reparsed.get_item_tellers_effects("c1") == []

    assert reparsed.set_item_tellers_effect_parameter("c2", 0, "feather", {"keyframes": [{"t": 0, "v": 0.1}]})
    assert reparsed.set_track_tellers_effect_enabled("t1", 0, False)
    assert reparsed.get_track_tellers_effects("t1")[0]["enabled"] is False
    assert reparsed.clear_item_tellers_effects("c2")


def test_specs_validate_and_evaluate():
    specs = {spec["effect_name"]: spec for spec in tellers_effect_specs()}
    assert specs["Volume"]["target"] == "audio"
    assert specs["TextWave"]["target"] == "text"
    blend_mode = specs["Blend"]["parameters"][0]
    assert blend_mode["kind"] == "choice" and "multiply" in blend_mode["options"]

    assert validate_tellers_effect({"effect_name": "Blur", "parameters": {"strength": FADE_IN}}) is None
    assert "unknown" in validate_tellers_effect({"effect_name": "Sparkles"})
    assert validate_tellers_effect({"effect_name": "Blur", "parameters": {"size": 2}}) is not None

    assert evaluate_animatable(3, 10.0) == 3
    assert evaluate_animatable(FADE_IN, -1.0) == 0.0
    assert evaluate_animatable(FADE_IN, 2.0) == 8.0
    assert evaluate_animatable(FADE_IN, 0.5) == pytest.approx(8.0 * (1 - 0.5**3))
    assert evaluate_animatable({"keyframes": []}, 0.0) is None
