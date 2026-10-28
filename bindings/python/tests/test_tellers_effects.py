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
    assert tl.push_tellers_effect({"effect_name": "tellers.ai.MotionBlur", "parameters": {"shutterAngle": 90}})
    assert tl.get_tellers_effects()[1]["effect_name"] == "MotionBlur"
    # Speed goes on clips only: the player ignores it on the timeline.
    assert not tl.push_tellers_effect({"effect_name": "tellers.ai.TimeWarp", "parameters": {"speed": 2}})


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
    data = json.loads(make_clip().to_json())
    time_warp = {"OTIO_SCHEMA": "LinearTimeWarp.1", "name": "", "effect_name": "", "time_scalar": 2.0, "metadata": {}}
    data["effects"] = [time_warp]
    clip = Clip.parse_json(json.dumps(data))
    clip.push_tellers_effect({"effect_name": "Transform", "parameters": {"x": 0.1}})
    assert clip.clear_tellers_effects()
    assert json.loads(clip.to_json())["effects"] == [time_warp]


def test_color_lut_and_volume_are_tellers_effects():
    clip = make_clip()
    assert clip.push_tellers_effect({"effect_name": "ColorLut", "parameters": {"asset_id": "look"}})
    assert clip.push_tellers_effect({"effect_name": "Volume", "parameters": {"volume": -6.0}})
    assert [e["effect_name"] for e in clip.get_tellers_effects()] == ["ColorLut", "Volume"]
    assert clip.get_tellers_effects()[1]["parameters"]["volume"] == pytest.approx(-6.0)
    assert clip.clear_tellers_effects()
    assert clip.get_tellers_effects() == []


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
    assert specs["Equalizer"]["target"] == "audio"
    assert specs["Equalizer"]["parameters"][0] == {"name": "bands", "kind": "bands"}
    assert [p["name"] for p in specs["Compressor"]["parameters"]] == ["threshold", "ratio", "knee", "attack", "release", "makeup"]
    assert [p["name"] for p in specs["Limiter"]["parameters"]] == ["ceiling", "release"]
    assert specs["Pitch"]["target"] == "audio"
    assert specs["Pitch"]["parameters"] == [{"name": "semitones", "kind": "animatable"}]
    assert {"name": "corner_radius", "kind": "animatable"} in specs["Border"]["parameters"]
    assert validate_tellers_effect({"effect_name": "Border", "parameters": {"corner_radius": 24}}) is None
    bands = [{"type": "highpass", "frequency": FADE_IN}, {"type": "peaking", "frequency": 3500, "gain": 4, "q": 1}]
    assert validate_tellers_effect({"effect_name": "Equalizer", "parameters": {"bands": bands}}) is None
    assert "bands" in validate_tellers_effect({"effect_name": "Equalizer", "parameters": {"bands": [{"type": "notch"}]}})

    assert validate_tellers_effect({"effect_name": "Blur", "parameters": {"strength": FADE_IN}}) is None
    assert "unknown" in validate_tellers_effect({"effect_name": "Sparkles"})
    assert validate_tellers_effect({"effect_name": "Blur", "parameters": {"size": 2}}) is not None

    assert evaluate_animatable(3, 10.0) == 3
    assert evaluate_animatable(FADE_IN, -1.0) == 0.0
    assert evaluate_animatable(FADE_IN, 2.0) == 8.0
    assert evaluate_animatable(FADE_IN, 0.5) == pytest.approx(8.0 * (1 - 0.5**3))
    assert evaluate_animatable({"keyframes": []}, 0.0) is None


def test_effects_only_go_on_their_kind_of_track():
    video = Track(kind="video", id="v", children=[Item.from_clip(make_clip("vc"))])
    audio = Track(kind="audio", id="a", children=[Item.from_clip(make_clip("ac"))])
    tl = Timeline([video, audio])
    volume = {"effect_name": "Volume", "parameters": {"volume": -6.0}}
    blur = {"effect_name": "Blur", "parameters": {"strength": 2}}

    # Sound effects on audio tracks and their clips only.
    assert not tl.push_track_tellers_effect("v", volume)
    assert not tl.push_item_tellers_effect("vc", volume)
    assert tl.push_track_tellers_effect("a", volume)
    assert tl.push_item_tellers_effect("ac", {"effect_name": "Pan", "parameters": {"pan": 0.5}})
    # Picture effects on video tracks and their clips only.
    assert not tl.push_track_tellers_effect("a", blur)
    assert not tl.set_item_tellers_effects("ac", [blur])
    assert tl.replace_item_tellers_effect_at("ac", 0, blur) is None
    assert tl.push_item_tellers_effect("vc", blur)
    # Speed goes anywhere.
    assert tl.push_item_tellers_effect("ac", {"effect_name": "TimeWarp", "parameters": {"speed": 2}})

    assert [e["effect_name"] for e in tl.get_item_tellers_effects("ac")] == ["Pan", "TimeWarp"]
    assert [e["effect_name"] for e in tl.get_item_tellers_effects("vc")] == ["Blur"]
    assert tl.get_track_tellers_effects("v") == []


def test_text_effects_only_go_on_text_clips():
    text = Clip.new_text(2.0, {"text": "Caption"}, id="tc")
    video = Track(kind="video", id="v", children=[Item.from_clip(make_clip("vc")), Item.from_clip(text)])
    tl = Timeline([video])
    wave = {"effect_name": "TextWave", "parameters": {"amplitude": 4}}

    assert not tl.push_item_tellers_effect("vc", wave)
    assert tl.push_item_tellers_effect("tc", wave)
    # Picture effects go on text clips too.
    assert tl.push_item_tellers_effect("tc", {"effect_name": "DropShadow", "parameters": {}})
    # A track applies its text effects to its text clips.
    assert tl.push_track_tellers_effect("v", wave)
    assert [e["effect_name"] for e in tl.get_item_tellers_effects("tc")] == ["TextWave", "DropShadow"]
    assert tl.get_item_tellers_effects("vc") == []


def test_item_follow_round_trips_through_the_transform():
    video = Track(kind="video", id="v", children=[Item.from_clip(make_clip("vc"))])
    audio = Track(kind="audio", id="a", children=[Item.from_clip(make_clip("ac"))])
    tl = Timeline([video, audio])
    region = {"shape": "rectangle", "width": 0.1, "height": 0.4,
              "x": {"keyframes": [{"t": 0, "v": 0.4}, {"t": 7, "v": -0.4}]}}
    follow = {"region": region, "target": "people", "panSmoothing": 0.8}
    baked = {"x": {"keyframes": [{"t": 0, "v": -1.5}, {"t": 7, "v": 1.5}]}, "scale": 4.1}

    assert tl.get_item_follow("vc") is None
    assert tl.set_item_follow("vc", follow, baked)
    assert tl.get_item_follow("vc") == follow
    (transform,) = tl.get_item_tellers_effects("vc")
    assert transform["effect_name"] == "Transform" and transform["name"] == "Follow"
    assert transform["parameters"]["scale"] == 4.1
    assert transform["parameters"]["follow"] == follow

    assert not tl.set_item_follow("ac", follow, baked)
    assert not tl.set_item_follow("vc", {"fill": 0.6})
    assert not tl.set_item_follow("vc", {**follow, "target": "cars"})
    assert tl.set_item_follow("vc", {**follow, "target": "dog"})
    assert tl.get_item_follow("vc")["target"] == "dog"
    assert tl.set_item_follow("vc", follow)
    assert not tl.set_item_follow("vc", follow, {"rotation": 3})

    assert tl.clear_item_follow("vc")
    assert tl.get_item_follow("vc") is None
    assert tl.get_item_tellers_effects("vc")[0]["parameters"]["scale"] == 4.1
    kinds = {p["name"]: p["kind"] for s in tellers_effect_specs() if s["effect_name"] == "Transform" for p in s["parameters"]}
    assert kinds["follow"] == "follow"


def test_declared_follow_is_resolved_by_the_player_and_saved():
    video = Track(kind="video", id="v", children=[Item.from_clip(make_clip("vc"))])
    tl = Timeline([video])
    pick = {"t": 2.4, "x": 0.18, "y": -0.05}
    assert not tl.set_item_follow("vc", {"target": "people"})
    assert not tl.set_item_follow("vc", {"pick": pick})
    assert tl.set_item_follow("vc", {"target": "people", "pick": pick})
    assert tl.unresolved_follows() == [("vc", {"target": "people", "pick": pick})]

    region = {"shape": "rectangle", "width": 0.1, "height": 0.4, "x": 0.18}
    resolved = {"target": "people", "pick": pick, "region": region}
    parameters = {"x": -0.4, "y": 0.0, "scale": 2.5, "opacity": 0.3, "follow": resolved}
    # Re-picked during detection: the stale result is dropped.
    assert tl.set_item_follow("vc", {"target": "people", "pick": {**pick, "x": -0.3}})
    assert not tl.apply_follow_resolution("vc", parameters)
    assert tl.set_item_follow("vc", {"target": "people", "pick": pick})
    assert tl.apply_follow_resolution("vc", parameters)
    assert tl.unresolved_follows() == []
    assert tl.item_follows() == [("vc", resolved)]
    (transform,) = tl.get_item_tellers_effects("vc")
    assert transform["parameters"]["scale"] == 2.5
    assert "opacity" not in transform["parameters"]


def test_follow_detect_settings_round_trip():
    video = Track(kind="video", id="v", children=[Item.from_clip(make_clip("vc"))])
    tl = Timeline([video])
    follow = {"target": "car", "pick": {"t": 1.0, "x": 0.0, "y": 0.0}, "detect": {"padX": 1.2}}
    assert tl.set_item_follow("vc", follow)
    assert tl.get_item_follow("vc") == follow
    # How well subjects are found is the player's setting (video-player-js #690).
    assert not tl.set_item_follow("vc", {**follow, "detect": {"objectModel": "accurate"}})
    assert not tl.set_item_follow("vc", {**follow, "tolerance": 0.002})
    assert not tl.set_item_follow("vc", {**follow, "detect": {"delegate": "GPU"}})


def test_attach_keeps_a_clip_on_a_subject_of_another_track():
    base = Track(kind="video", id="v", children=[Item.from_clip(make_clip("vc"))])
    labels = Track(kind="video", id="l", children=[Item.from_clip(make_clip("label"))])
    tl = Timeline([base, labels])
    attach = {"track": "v", "target": "people", "pick": {"t": 1.0, "x": 0.2, "y": 0.0}, "anchorY": -0.7, "hide": True}
    assert not tl.set_item_attach("label", {"target": "people"})
    assert not tl.set_item_attach("label", {"track": "v"})
    assert tl.set_item_attach("label", attach)
    assert tl.get_item_attach("label") == attach
    assert tl.unresolved_attaches() == [("label", attach)]
    (transform,) = tl.get_item_tellers_effects("label")
    assert transform["name"] == "Attach"

    region = {"shape": "rectangle", "width": 0.1, "height": 0.4, "x": 0.2}
    resolved = {**attach, "region": region, "opacity": 1}
    parameters = {"x": 0.2, "y": -0.18, "opacity": 0.0, "scale": 3, "attach": resolved}
    assert not tl.apply_attach_resolution("label", {**parameters, "attach": {**resolved, "anchorY": 0.5}})
    assert tl.apply_attach_resolution("label", parameters)
    assert tl.unresolved_attaches() == []
    assert tl.item_attaches() == [("label", resolved)]
    (transform,) = tl.get_item_tellers_effects("label")
    assert transform["parameters"]["opacity"] == 0.0
    assert "scale" not in transform["parameters"]
    assert tl.clear_item_attach("label")
    assert tl.get_item_attach("label") is None
    kinds = {p["name"]: p["kind"] for s in tellers_effect_specs() if s["effect_name"] == "Transform" for p in s["parameters"]}
    assert kinds["attach"] == "attach"


def test_a_detect_can_read_another_track():
    video = Track(kind="video", id="v", children=[Item.from_clip(make_clip("vc"))])
    tl = Timeline([video])
    detect = {"target": "people", "track": "Base"}
    assert tl.set_item_tellers_effects("vc", [{"effect_name": "Mask", "parameters": {"invert": True, "detect": detect}}])
    (unresolved,) = tl.unresolved_detects()
    assert unresolved["detect"] == detect
    assert validate_tellers_effect({"effect_name": "Blur", "parameters": {"detect": {"target": "faces", "fps": 8}}}) is not None


def test_detect_on_filters_and_masks_is_resolved_by_the_player_and_saved():
    video = Track(kind="video", id="v", children=[Item.from_clip(make_clip("vc"))])
    tl = Timeline([video])
    faces = {"target": "faces"}
    walker = {"target": "people", "pick": {"t": 1.0, "x": 0.31, "y": -0.16}, "shape": "outline"}
    assert validate_tellers_effect({"effect_name": "Blur", "parameters": {"detect": {"target": "ghost"}}}) is not None
    assert tl.set_item_tellers_effects("vc", [
        {"effect_name": "Blur", "parameters": {"strength": 40, "detect": faces}},
        {"effect_name": "Mask", "parameters": {"feather": 4, "detect": walker}},
    ])
    assert [(d["effect_index"], d["effect_name"], d["detect"]) for d in tl.unresolved_detects()] == [
        (0, "Blur", faces),
        (1, "Mask", walker),
    ]

    regions = [{"shape": "ellipse", "x": 0.1, "y": 0.0, "width": 0.2, "height": 0.3}]
    assert not tl.apply_detect_resolution("vc", 0, "Blur", {"detect": {"target": "people"}, "regions": regions})
    assert tl.apply_detect_resolution("vc", 0, "Blur", {"strength": 1, "detect": faces, "regions": regions})
    assert tl.apply_detect_resolution("vc", 1, "Mask", {"detect": walker, "shape": "rectangle", "x": 0.3})
    assert tl.unresolved_detects() == []
    assert [d["resolved"] for d in tl.item_detects()] == [True, True]
    blur, mask = tl.get_item_tellers_effects("vc")
    assert blur["parameters"]["strength"] == 40
    assert blur["parameters"]["regions"] == regions
    assert mask["parameters"]["feather"] == 4
    assert mask["parameters"]["x"] == 0.3


def test_a_filter_detect_can_pick_several_subjects():
    video = Track(kind="video", id="v", children=[Item.from_clip(make_clip("vc"))])
    tl = Timeline([video])
    picks = [{"t": 1.0, "x": -0.2, "y": -0.1}, {"t": 3.2, "x": 0.25, "y": -0.15}]
    detect = {"target": "faces", "pick": picks}
    assert tl.set_item_tellers_effects("vc", [{"effect_name": "Blur", "parameters": {"detect": detect}}])
    (unresolved,) = tl.unresolved_detects()
    assert unresolved["detect"] == detect
    assert validate_tellers_effect({"effect_name": "Blur", "parameters": {"detect": {"target": "faces", "pick": []}}}) is not None


def test_track_references_follow_a_rename_and_a_delete():
    base = Track(kind="video", id="v1", children=[Item.from_clip(make_clip("vc"))])
    base.set_name("Base")
    overlay = Track(kind="video", id="v2", children=[Item.from_clip(make_clip("label"))])
    tl = Timeline([base, overlay])
    assert tl.set_track_tellers_effects("v2", [{"effect_name": "TrackMatte", "parameters": {"mode": "alpha", "track": "Base"}}])
    assert tl.set_item_attach("label", {"track": "Base", "target": "people"}, {"x": 0.1, "y": 0.2})
    refs = tl.track_references()
    assert [(r["owner_id"], r["kind"], r["track"], r["matches"]) for r in refs] == [
        ("v2", "track_matte", "Base", ["v1"]),
        ("label", "attach", "Base", ["v1"]),
    ]
    assert tl.dangling_track_references() == [] and tl.validate() == []

    assert tl.rename_track("v1", "Interview")
    assert [r["track"] for r in tl.track_references()] == ["Interview", "Interview"]

    assert tl.set_item_tellers_effects("vc", [{"effect_name": "TrackMatte", "parameters": {"track": "nowhere"}}])
    (dangling,) = tl.dangling_track_references()
    assert dangling["owner_id"] == "vc" and dangling["matches"] == []
    assert len(tl.validate()) == 1 and "nowhere" in tl.validate()[0]

    assert tl.delete_track("v1") is not None
    assert tl.track_references() == []
    assert tl.get_track_tellers_effects("v2") == []
    assert tl.get_item_attach("label") is None
    assert tl.get_item_tellers_effects("label")[0]["parameters"] == {"x": 0.1, "y": 0.2}
