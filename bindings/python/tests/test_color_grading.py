"""Tests for the `.cube` LUT list methods on the timeline and its clips."""
import json

import pytest

from tellers_timeline import Clip, Gap, Item, MediaReference, Timeline, Track

LOOK = "https://cdn.example.test/film_look.cube"


def make_clip(id="c1"):
    ref = MediaReference("file:///test.mp4")
    return Clip(10.0, {"DEFAULT_MEDIA": ref}, "DEFAULT_MEDIA", id=id)


def make_timeline():
    track = Track(
        kind="video",
        id="t1",
        children=[Item.from_clip(make_clip("c1")), Item.from_gap(Gap(1.0, id="g1"))],
    )
    return Timeline([track])


def lut(url, name=None):
    return {"url": url, "name": name}


def test_timeline_luts_round_trip():
    tl = Timeline()
    assert tl.get_color_luts() == []
    assert tl.push_color_lut(LOOK, name="film_look.cube")

    assert tl.get_color_luts() == [lut(LOOK, "film_look.cube")]
    metadata = json.loads(tl.get_metadata_json())
    assert metadata["tellers.ai"]["color_grading"] == [
        {"cube": LOOK, "name": "film_look.cube"}
    ]

    reparsed = Timeline.parse_json(tl.to_json())
    assert reparsed.get_color_luts() == [lut(LOOK, "film_look.cube")]

    assert tl.clear_color_luts()
    assert tl.get_color_luts() == []
    assert not tl.clear_color_luts()


def test_clip_list_operations():
    clip = make_clip()
    assert clip.push_color_lut("warm.cube")
    assert clip.insert_color_lut_at(0, "slog3_to_709.cube", "slog3_to_709.cube")
    assert not clip.insert_color_lut_at(5, "x.cube")
    assert clip.get_color_luts() == [
        lut("slog3_to_709.cube", "slog3_to_709.cube"),
        lut("warm.cube"),
    ]
    metadata = json.loads(clip.get_metadata_json())
    assert metadata["tellers.ai"]["color_grading"] == [
        {"cube": "slog3_to_709.cube", "name": "slog3_to_709.cube"},
        {"cube": "warm.cube"},
    ]

    assert clip.replace_color_lut_at(1, "cool.cube") == lut("warm.cube")
    assert clip.replace_color_lut_at(9, "x.cube") is None
    assert clip.remove_color_lut_at(0) == lut("slog3_to_709.cube", "slog3_to_709.cube")
    assert clip.remove_color_lut_at(9) is None
    assert clip.get_color_luts() == [lut("cool.cube")]


def test_set_color_luts_replaces_and_validates():
    clip = make_clip()
    assert clip.set_color_luts([{"url": "a.cube"}, {"url": " b.cube ", "name": "b"}])
    assert clip.get_color_luts() == [lut("a.cube"), lut("b.cube", "b")]

    assert not clip.set_color_luts([{"url": "ok.cube"}, {"url": "  "}])
    assert clip.get_color_luts() == [lut("a.cube"), lut("b.cube", "b")]

    with pytest.raises(ValueError):
        clip.set_color_luts([{"name": "no-url"}])
    with pytest.raises(TypeError):
        clip.set_color_luts(["a.cube"])

    assert clip.set_color_luts([])
    assert clip.get_color_luts() == []


def test_empty_url_is_rejected():
    tl = Timeline()
    assert not tl.push_color_lut("   ")
    assert tl.get_color_luts() == []


def test_item_luts_and_gap():
    item = Item.from_clip(make_clip())
    assert item.push_color_lut("clip.cube")
    assert item.get_color_luts() == [lut("clip.cube")]

    gap = Item.from_gap(Gap(1.0))
    assert not gap.push_color_lut("clip.cube")
    assert gap.get_color_luts() == []
    assert gap.remove_color_lut_at(0) is None


def test_timeline_edits_clips_by_id():
    tl = make_timeline()
    assert tl.push_item_color_lut("c1", "warm.cube")
    assert tl.insert_item_color_lut_at("c1", 0, "log.cube")
    tl.push_color_lut("look.cube")

    assert tl.get_item_color_luts("c1") == [lut("log.cube"), lut("warm.cube")]
    assert tl.color_lut_urls() == ["look.cube", "log.cube", "warm.cube"]

    assert tl.replace_item_color_lut_at("c1", 1, "cool.cube") == lut("warm.cube")
    assert tl.remove_item_color_lut_at("c1", 0) == lut("log.cube")

    assert tl.get_item_color_luts("missing") is None
    assert tl.get_item_color_luts("g1") is None
    assert not tl.push_item_color_lut("missing", "x.cube")
    assert not tl.push_item_color_lut("g1", "x.cube")

    assert tl.set_item_color_luts("c1", [{"url": "a.cube"}])
    assert tl.clear_item_color_luts("c1")
    assert tl.get_item_color_luts("c1") == []
    assert tl.color_lut_urls() == ["look.cube"]
