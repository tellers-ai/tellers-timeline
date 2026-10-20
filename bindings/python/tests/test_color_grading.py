"""Tests for `.cube` LUTs, ordinary `ColorLut` tellers effects, and for
resolving an effect's `target_url` from its `asset_id`.

A LUT is edited with the tellers effect methods like any effect; its URL
is set with `set_effect_target_url(s)` and stripped with
`clear_effect_target_urls` (or `clear_target_urls`), on every stage.
"""
import json

from tellers_timeline import Clip, Gap, Item, MediaReference, Timeline, Track


def make_clip(id="c1"):
    ref = MediaReference("file:///test.mp4")
    return Clip(10.0, {"DEFAULT_MEDIA": ref}, "DEFAULT_MEDIA", id=id)


def make_timeline():
    video = Track(
        kind="video",
        id="t1",
        children=[
            Item.from_clip(make_clip("c1")),
            Item.from_gap(Gap(1.0, id="g1")),
            Item.from_clip(make_clip("c2")),
        ],
    )
    audio = Track(kind="audio", id="a1", children=[Item.from_clip(make_clip("a"))])
    return Timeline([video, audio])


def lut(asset_id, name=None):
    effect = {"effect_name": "ColorLut", "parameters": {"asset_id": asset_id}}
    if name is not None:
        effect["name"] = name
    return effect


def lut_effect(asset_id, name="", url=None):
    parameters = {"asset_id": asset_id}
    if url is not None:
        parameters["target_url"] = url
    return {
        "OTIO_SCHEMA": "Effect.1",
        "name": name,
        "effect_name": "tellers.ai.ColorLut",
        "metadata": {"tellers.ai": {"parameters": parameters}},
    }


def asset_ids(effects):
    return [e["parameters"]["asset_id"] for e in effects if e["effect_name"] == "ColorLut"]


def test_luts_are_tellers_effects():
    tl = make_timeline()
    assert tl.push_tellers_effect(lut("look", "film_look.cube"))
    assert tl.push_track_tellers_effect("t1", lut("track_look"))
    assert tl.push_item_tellers_effect("c1", lut("log"))
    assert tl.push_item_tellers_effect("c1", {"effect_name": "Blur", "parameters": {"strength": 2}})
    assert tl.insert_item_tellers_effect_at("c1", 1, lut("warm"))

    # The timeline's LUTs are effects of its root stack.
    assert json.loads(tl.to_json())["tracks"]["effects"] == [lut_effect("look", "film_look.cube")]

    reparsed = Timeline.parse_json(tl.to_json())
    assert asset_ids(reparsed.get_tellers_effects()) == ["look"]
    assert asset_ids(reparsed.get_track_tellers_effects("t1")) == ["track_look"]
    effects = reparsed.get_item_tellers_effects("c1")
    assert [e["effect_name"] for e in effects] == ["ColorLut", "ColorLut", "Blur"]
    assert asset_ids(effects) == ["log", "warm"]

    assert tl.remove_item_tellers_effect_at("c1", 0)["parameters"] == {"asset_id": "log"}


def test_set_and_clear_effect_target_urls():
    tl = make_timeline()
    tl.push_tellers_effect(lut("look"))
    tl.set_item_tellers_effects("c1", [lut("log"), lut("warm")])
    tl.push_item_tellers_effect("c2", lut("log"))
    tl.push_item_tellers_effect("a", lut("audio_lut"))

    assert tl.effect_asset_ids() == ["look", "log", "warm", "audio_lut"]

    # One asset id, every effect that uses it.
    assert tl.set_effect_target_url("log", "https://cdn/log.cube") == 2
    assert tl.set_effect_target_url("log", "") == 0
    # Many at once; unknown ids are ignored.
    assert tl.set_effect_target_urls(
        {"look": "https://cdn/look.cube", "warm": "https://cdn/warm.cube", "zzz": "x"}
    ) == 2

    assert tl.get_tellers_effects()[0]["parameters"] == {
        "asset_id": "look",
        "target_url": "https://cdn/look.cube",
    }
    items = json.loads(tl.to_json())["tracks"]["children"][0]["children"]
    assert items[2]["effects"] == [lut_effect("log", url="https://cdn/log.cube")]

    assert tl.clear_effect_target_urls() == 4
    assert [e["parameters"] for e in tl.get_item_tellers_effects("c1")] == [
        {"asset_id": "log"},
        {"asset_id": "warm"},
    ]
    assert tl.effect_asset_ids() == ["look", "log", "warm", "audio_lut"]


def test_clear_target_urls_strips_media_and_effect_urls():
    tl = make_timeline()
    tl.push_item_tellers_effect("c1", lut("log"))
    tl.set_effect_target_urls({"log": "https://cdn/log.cube"})

    tl.clear_target_urls()

    data = json.loads(tl.to_json())
    clip = data["tracks"]["children"][0]["children"][0]
    assert clip["effects"] == [lut_effect("log")]
    assert clip["media_references"]["DEFAULT_MEDIA"]["target_url"] == ""


def test_sanitize_upgrades_legacy_luts():
    legacy = {
        "OTIO_SCHEMA": "Effect.1",
        "name": "look.cube",
        "effect_name": "ColorLut",
        "metadata": {"tellers.ai": {"asset_id": "a", "cube_url": "https://old/a.cube"}},
    }
    data = json.loads(make_timeline().to_json())
    data["tracks"]["effects"] = [legacy]
    tl = Timeline.parse_json(json.dumps(data))

    tl.sanitize()

    assert json.loads(tl.to_json())["tracks"]["effects"] == [
        lut_effect("a", "look.cube", url="https://old/a.cube")
    ]
    assert tl.effect_asset_ids() == ["a"]
