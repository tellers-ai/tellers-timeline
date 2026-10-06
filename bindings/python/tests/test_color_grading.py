"""Tests for the `.cube` LUT methods at the timeline, clip and asset stages."""
import json

from tellers_timeline import Clip, Gap, Item, MediaReference, Timeline, Track

LOOK = "https://cdn.example.test/look.cube"


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


def test_timeline_lut_round_trips():
    tl = Timeline()
    assert tl.get_color_lut() is None
    assert tl.set_color_lut(LOOK, name="look.cube")

    assert tl.get_color_lut() == {"url": LOOK, "name": "look.cube"}
    metadata = json.loads(tl.get_metadata_json())
    assert metadata["tellers.ai"]["color_grading"] == {"cube": LOOK, "name": "look.cube"}

    reparsed = Timeline.parse_json(tl.to_json())
    assert reparsed.get_color_lut() == {"url": LOOK, "name": "look.cube"}

    assert tl.remove_color_lut()
    assert tl.get_color_lut() is None
    assert not tl.remove_color_lut()


def test_set_trims_and_rejects_empty_url():
    tl = Timeline()
    assert tl.set_color_lut("  /luts/warm.cube ")
    assert tl.get_color_lut() == {"url": "/luts/warm.cube", "name": None}
    assert not tl.set_color_lut("   ")
    assert tl.get_color_lut() == {"url": "/luts/warm.cube", "name": None}


def test_clip_and_asset_stages():
    clip = make_clip()
    assert clip.set_color_lut("clip.cube")
    assert clip.set_asset_color_lut("asset.cube", "log.cube")

    assert clip.get_color_lut() == {"url": "clip.cube", "name": None}
    assert clip.get_asset_color_lut() == {"url": "asset.cube", "name": "log.cube"}
    assert clip.color_lut_urls() == ["asset.cube", "clip.cube"]

    ref = clip.get_media_references()["DEFAULT_MEDIA"]
    assert ref.get_color_lut() == {"url": "asset.cube", "name": "log.cube"}

    assert clip.remove_asset_color_lut()
    assert clip.get_asset_color_lut() is None
    assert clip.color_lut_urls() == ["clip.cube"]


def test_media_reference_lut():
    ref = MediaReference("file:///test.mp4")
    assert ref.set_color_lut("asset.cube")
    assert json.loads(ref.get_metadata_json())["tellers.ai"]["color_grading"] == {
        "cube": "asset.cube"
    }
    assert ref.remove_color_lut()
    assert ref.get_color_lut() is None


def test_item_lut_and_gap():
    item = Item.from_clip(make_clip())
    assert item.set_color_lut("clip.cube")
    assert item.get_color_lut() == {"url": "clip.cube", "name": None}

    gap = Item.from_gap(Gap(1.0))
    assert not gap.set_color_lut("clip.cube")
    assert gap.get_color_lut() is None
    assert gap.get_asset_color_lut() is None


def test_timeline_writes_items_by_id():
    tl = make_timeline()
    assert tl.set_item_color_lut("c1", "clip.cube")
    assert tl.set_item_asset_color_lut("c1", "asset.cube", name="log.cube")
    tl.set_color_lut("look.cube")

    assert tl.get_item_color_lut("c1") == {"url": "clip.cube", "name": None}
    assert tl.get_item_asset_color_lut("c1") == {"url": "asset.cube", "name": "log.cube"}
    assert tl.color_lut_urls() == ["look.cube", "asset.cube", "clip.cube"]

    assert not tl.set_item_color_lut("missing", "x.cube")
    assert not tl.set_item_color_lut("g1", "x.cube")

    assert tl.remove_item_color_lut("c1")
    assert tl.remove_item_asset_color_lut("c1")
    assert tl.color_lut_urls() == ["look.cube"]
