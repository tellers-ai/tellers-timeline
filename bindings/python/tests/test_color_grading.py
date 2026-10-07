"""Tests for the `.cube` LUT list methods on the timeline, its tracks and clips.

LUTs are stored by asset id; the `.cube` URL the player reads is resolved
from the asset id with `set_color_lut_url(s)` and stripped with
`clear_color_lut_urls`.
"""
import json

import pytest

from tellers_timeline import Clip, Gap, Item, MediaReference, Timeline, Track


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


def lut(asset_id, name=None, url=None):
    return {"asset_id": asset_id, "name": name, "url": url}


def test_timeline_luts_round_trip():
    tl = Timeline()
    assert tl.get_color_luts() == []
    assert tl.push_color_lut("look", name="film_look.cube")

    assert tl.get_color_luts() == [lut("look", "film_look.cube")]
    metadata = json.loads(tl.get_metadata_json())
    assert metadata["tellers.ai"]["color_grading"] == [
        {"asset_id": "look", "name": "film_look.cube"}
    ]

    reparsed = Timeline.parse_json(tl.to_json())
    assert reparsed.get_color_luts() == [lut("look", "film_look.cube")]

    assert tl.clear_color_luts()
    assert tl.get_color_luts() == []
    assert not tl.clear_color_luts()


def test_clip_list_operations():
    clip = make_clip()
    assert clip.push_color_lut("warm")
    assert clip.insert_color_lut_at(0, "slog3", "slog3_to_709.cube")
    assert not clip.insert_color_lut_at(5, "x")
    assert clip.get_color_luts() == [lut("slog3", "slog3_to_709.cube"), lut("warm")]
    metadata = json.loads(clip.get_metadata_json())
    assert metadata["tellers.ai"]["color_grading"] == [
        {"asset_id": "slog3", "name": "slog3_to_709.cube"},
        {"asset_id": "warm"},
    ]

    assert clip.replace_color_lut_at(1, "cool") == lut("warm")
    assert clip.replace_color_lut_at(9, "x") is None
    assert clip.remove_color_lut_at(0) == lut("slog3", "slog3_to_709.cube")
    assert clip.remove_color_lut_at(9) is None
    assert clip.get_color_luts() == [lut("cool")]


def test_set_color_luts_replaces_and_validates():
    clip = make_clip()
    assert clip.set_color_luts(
        [{"asset_id": "a"}, {"asset_id": " b ", "name": "b.cube", "url": "https://cdn/b.cube"}]
    )
    assert clip.get_color_luts() == [lut("a"), lut("b", "b.cube", "https://cdn/b.cube")]

    assert not clip.set_color_luts([{"asset_id": "ok"}, {"asset_id": "  "}])
    assert len(clip.get_color_luts()) == 2

    with pytest.raises(ValueError):
        clip.set_color_luts([{"url": "https://cdn/no-id.cube"}])
    with pytest.raises(TypeError):
        clip.set_color_luts(["a"])

    assert clip.set_color_luts([])
    assert clip.get_color_luts() == []


def test_empty_asset_id_is_rejected():
    tl = Timeline()
    assert not tl.push_color_lut("   ")
    assert tl.get_color_luts() == []


def test_item_luts_and_gap():
    item = Item.from_clip(make_clip())
    assert item.push_color_lut("clip")
    assert item.get_color_luts() == [lut("clip")]

    gap = Item.from_gap(Gap(1.0))
    assert not gap.push_color_lut("clip")
    assert gap.get_color_luts() == []
    assert gap.remove_color_lut_at(0) is None


def test_timeline_edits_clips_by_id():
    tl = make_timeline()
    assert tl.push_item_color_lut("c1", "warm")
    assert tl.insert_item_color_lut_at("c1", 0, "log")
    assert tl.get_item_color_luts("c1") == [lut("log"), lut("warm")]

    assert tl.replace_item_color_lut_at("c1", 1, "cool") == lut("warm")
    assert tl.remove_item_color_lut_at("c1", 0) == lut("log")

    assert tl.get_item_color_luts("missing") is None
    assert tl.get_item_color_luts("g1") is None
    assert not tl.push_item_color_lut("missing", "x")
    assert not tl.push_item_color_lut("g1", "x")

    assert tl.set_item_color_luts("c1", [{"asset_id": "a"}])
    assert tl.clear_item_color_luts("c1")
    assert tl.get_item_color_luts("c1") == []


def test_set_urls_from_asset_ids():
    tl = make_timeline()
    tl.push_color_lut("look")
    tl.set_item_color_luts("c1", [{"asset_id": "log"}, {"asset_id": "warm"}])
    tl.push_item_color_lut("c2", "log")

    assert tl.color_lut_asset_ids() == ["look", "log", "warm"]

    # One asset id, every entry that uses it.
    assert tl.set_color_lut_url("log", "https://cdn/log.cube") == 2
    assert tl.set_color_lut_url("log", "") == 0
    # Many at once; unknown ids are ignored.
    assert tl.set_color_lut_urls(
        {"look": "https://cdn/look.cube", "warm": "https://cdn/warm.cube", "zzz": "x"}
    ) == 2

    assert tl.get_color_luts() == [lut("look", url="https://cdn/look.cube")]
    assert tl.get_item_color_luts("c1") == [
        lut("log", url="https://cdn/log.cube"),
        lut("warm", url="https://cdn/warm.cube"),
    ]
    metadata = json.loads(tl.to_json())
    track_items = metadata["tracks"]["children"][0]["children"]
    assert track_items[2]["metadata"]["tellers.ai"]["color_grading"] == [
        {"asset_id": "log", "cube": "https://cdn/log.cube"}
    ]

    assert tl.clear_color_lut_urls() == 4
    assert tl.get_item_color_luts("c1") == [lut("log"), lut("warm")]
    assert tl.color_lut_asset_ids() == ["look", "log", "warm"]


def test_clip_url_setters():
    clip = make_clip()
    clip.push_color_lut("a")
    clip.push_color_lut("b")
    assert clip.set_color_lut_urls({"a": "https://cdn/a.cube"}) == 1
    assert clip.get_color_luts() == [lut("a", url="https://cdn/a.cube"), lut("b")]
    assert clip.clear_color_lut_urls() == 1
    assert clip.get_color_luts() == [lut("a"), lut("b")]


def test_unknown_asset_ids_are_ignored_without_error():
    tl = Timeline()
    assert tl.set_color_lut_url("missing", "https://cdn/a.cube") == 0
    assert tl.set_color_lut_urls({"missing": "https://cdn/a.cube"}) == 0
    assert tl.set_color_lut_urls({}) == 0

    tl.push_color_lut("a1")
    assert tl.set_color_lut_url("b2", "https://cdn/b.cube") == 0
    assert tl.get_color_luts() == [lut("a1")]


def make_mixed_timeline():
    """A video clip and a text clip on a video track, a media clip on an audio
    track; each already carries a LUT `<id>-lut`."""
    video = make_clip("video")
    text = Clip(
        10.0,
        {"DEFAULT_MEDIA": MediaReference.create_rich_text_reference("<div>Hi</div>")},
        "DEFAULT_MEDIA",
        id="text",
    )
    audio = make_clip("audio")
    for c, id in [(video, "video"), (text, "text"), (audio, "audio")]:
        c.push_color_lut(f"{id}-lut")
    return Timeline(
        [
            Track(kind="video", id="v", children=[Item.from_clip(video), Item.from_clip(text)]),
            Track(kind="audio", id="a", children=[Item.from_clip(audio)]),
        ]
    )


def test_only_media_clips_on_video_tracks_are_gradable():
    tl = make_mixed_timeline()
    assert tl.is_item_color_gradable("video")
    assert not tl.is_item_color_gradable("text")
    assert not tl.is_item_color_gradable("audio")
    assert not tl.is_item_color_gradable("missing")

    assert make_clip().is_color_gradable()
    text = Clip(1.0, {"DEFAULT_MEDIA": MediaReference.create_rich_text_reference("x")})
    assert not text.is_color_gradable()
    assert not Item.from_gap(Gap(1.0)).is_color_gradable()


def test_timeline_writers_refuse_ungraded_clips():
    tl = make_mixed_timeline()
    for id in ["text", "audio"]:
        assert not tl.push_item_color_lut(id, "x")
        assert not tl.insert_item_color_lut_at(id, 0, "x")
        assert tl.replace_item_color_lut_at(id, 0, "x") is None
        assert not tl.set_item_color_luts(id, [{"asset_id": "x"}])
        # Still readable and removable, so stale LUTs can be cleaned up.
        assert tl.get_item_color_luts(id) == [lut(f"{id}-lut")]
    assert tl.remove_item_color_lut_at("text", 0) == lut("text-lut")
    assert tl.clear_item_color_luts("audio")
    assert tl.push_item_color_lut("video", "x")


def test_url_resolution_skips_ungraded_clips():
    tl = make_mixed_timeline()
    tl.push_color_lut("look")
    assert tl.color_lut_asset_ids() == ["look", "video-lut"]

    assert tl.set_color_lut_urls(
        {
            "video-lut": "https://cdn/v.cube",
            "text-lut": "https://cdn/t.cube",
            "audio-lut": "https://cdn/a.cube",
        }
    ) == 1
    assert tl.set_color_lut_url("audio-lut", "https://cdn/a.cube") == 0
    assert tl.get_item_color_luts("video") == [lut("video-lut", url="https://cdn/v.cube")]
    assert tl.get_item_color_luts("text") == [lut("text-lut")]


def test_track_luts_on_a_track_object():
    track = Track(kind="video", id="t")
    assert track.is_color_gradable()
    assert track.push_color_lut("warm", name="warm.cube")
    assert track.insert_color_lut_at(0, "film")
    assert track.get_color_luts() == [lut("film"), lut("warm", name="warm.cube")]
    assert json.loads(track.get_metadata_json())["tellers.ai"]["color_grading"] == [
        {"asset_id": "film"},
        {"asset_id": "warm", "name": "warm.cube"},
    ]
    assert track.set_color_lut_urls({"film": "https://cdn/f.cube"}) == 1
    assert track.clear_color_lut_urls() == 1
    assert track.remove_color_lut_at(0) == lut("film")
    assert track.clear_color_luts()
    assert not Track(kind="audio").is_color_gradable()


def test_timeline_edits_tracks_by_id():
    tl = make_mixed_timeline()
    assert tl.is_track_color_gradable("v")
    assert not tl.is_track_color_gradable("a")
    assert not tl.is_track_color_gradable("missing")

    assert tl.push_track_color_lut("v", "warm")
    assert tl.insert_track_color_lut_at("v", 0, "film")
    assert tl.replace_track_color_lut_at("v", 1, "cool") == lut("warm")
    assert tl.get_track_color_luts("v") == [lut("film"), lut("cool")]
    assert tl.remove_track_color_lut_at("v", 0) == lut("film")
    assert tl.set_track_color_luts("v", [{"asset_id": "video-track"}])
    # The track list is separate from its clips' lists.
    assert tl.get_item_color_luts("video") == [lut("video-lut")]

    assert not tl.push_track_color_lut("a", "x")
    assert not tl.insert_track_color_lut_at("a", 0, "x")
    assert tl.replace_track_color_lut_at("a", 0, "x") is None
    assert not tl.set_track_color_luts("a", [{"asset_id": "x"}])
    assert tl.get_track_color_luts("a") == []
    assert tl.get_track_color_luts("missing") is None

    tl.push_color_lut("look")
    assert tl.color_lut_asset_ids() == ["look", "video-lut", "video-track"]
    assert tl.set_color_lut_urls(
        {"video-track": "https://cdn/vt.cube", "video-lut": "https://cdn/v.cube"}
    ) == 2
    assert tl.get_track_color_luts("v") == [lut("video-track", url="https://cdn/vt.cube")]
    assert tl.clear_color_lut_urls() == 2
    assert tl.clear_track_color_luts("v")
    assert not tl.clear_track_color_luts("missing")
