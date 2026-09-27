"""Tests for the timeline text-style methods (get/set/remove, CSS)."""
import json

from tellers_timeline import Timeline, Track


def test_timeline_without_styles_has_none():
    tl = Timeline()
    assert tl.get_text_styles() == []
    assert tl.get_text_style("subtitle") is None
    assert tl.text_styles_css() == ""


def test_set_text_style_then_get_round_trips():
    tl = Timeline()
    assert tl.set_text_style("subtitle", "font-size:56px;color:#fff")

    assert tl.get_text_styles() == [
        {"name": "subtitle", "declarations": "font-size:56px;color:#fff"}
    ]
    assert tl.get_text_style("subtitle") == "font-size:56px;color:#fff"
    metadata = json.loads(tl.get_metadata_json())
    assert metadata["tellers.ai"]["textStyles"] == {
        "subtitle": "font-size:56px;color:#fff"
    }


def test_set_text_style_replaces_the_same_name():
    tl = Timeline()
    assert tl.set_text_style("subtitle", "color:#fff")
    assert tl.set_text_style("subtitle", "color:#ffd400")

    assert tl.get_text_styles() == [{"name": "subtitle", "declarations": "color:#ffd400"}]


def test_set_text_style_rejects_unusable_input():
    tl = Timeline()
    assert not tl.set_text_style("sub title", "color:#fff")
    assert not tl.set_text_style("1st", "color:#fff")
    assert not tl.set_text_style("subtitle", "")
    assert not tl.set_text_style("subtitle", "color:#fff } body { display:none")
    assert tl.get_text_styles() == []


def test_remove_text_style():
    tl = Timeline()
    assert tl.set_text_style("subtitle", "color:#fff")
    assert tl.remove_text_style("subtitle")
    assert not tl.remove_text_style("subtitle")
    assert tl.get_text_styles() == []
    assert "textStyles" not in json.loads(tl.get_metadata_json())["tellers.ai"]


def test_text_styles_css_builds_rules():
    tl = Timeline()
    assert tl.set_text_style("subtitle", "font-size:56px;color:#fff")
    assert tl.set_text_style("accent", "color:#ffd400")

    assert tl.text_styles_css() == (
        ".accent { color:#ffd400 }\n.subtitle { font-size:56px;color:#fff }"
    )


def test_text_styles_survive_metadata_round_trip():
    tl = Timeline()
    tl.set_metadata_json(json.dumps({
        "tellers.ai": {
            "textStyles": {
                "subtitle": " color:#fff ",
                "bad name": "color:#000",
                "empty": "",
            }
        }
    }))

    assert tl.get_text_styles() == [{"name": "subtitle", "declarations": "color:#fff"}]


def _timeline_with_tracks():
    return Timeline([Track(kind="video", id="subs"), Track(kind="video", id="titles")])


def test_track_styles_merge_over_timeline_styles():
    tl = _timeline_with_tracks()
    assert tl.set_text_style("subtitle", "font-size:56px;color:#fff")
    assert tl.set_text_style("accent", "color:#ffd400")
    assert tl.set_track_text_style("subs", "subtitle", "font-size:40px;color:#fff")
    assert tl.set_track_text_style("subs", "speaker", "font-style:italic")

    assert tl.get_track_text_styles("subs") == [
        {"name": "accent", "declarations": "color:#ffd400"},
        {"name": "subtitle", "declarations": "font-size:40px;color:#fff"},
        {"name": "speaker", "declarations": "font-style:italic"},
    ]
    assert tl.get_track_text_styles("titles") == tl.get_text_styles()
    assert tl.get_track_text_styles("missing") is None
    assert tl.track_text_styles_css("subs") == (
        ".accent { color:#ffd400 }\n"
        ".subtitle { font-size:40px;color:#fff }\n"
        ".speaker { font-style:italic }"
    )


def test_track_styles_live_on_the_track_metadata():
    tl = _timeline_with_tracks()
    assert tl.set_track_text_style("subs", "subtitle", "font-size:40px")

    _, track = tl.get_stack().get_track_by_id("subs")
    assert track.get_text_styles() == [{"name": "subtitle", "declarations": "font-size:40px"}]
    assert json.loads(track.get_metadata_json())["tellers.ai"]["textStyles"] == {
        "subtitle": "font-size:40px"
    }
    assert tl.get_text_styles() == []


def test_removing_a_track_override_restores_the_timeline_class():
    tl = _timeline_with_tracks()
    assert tl.set_text_style("subtitle", "font-size:56px")
    assert tl.set_track_text_style("subs", "subtitle", "font-size:40px")

    assert tl.remove_track_text_style("subs", "subtitle")
    assert not tl.remove_track_text_style("subs", "subtitle")
    assert tl.get_track_text_styles("subs") == [
        {"name": "subtitle", "declarations": "font-size:56px"}
    ]


def test_track_style_writes_fail_for_unknown_tracks_or_bad_styles():
    tl = _timeline_with_tracks()
    assert not tl.set_track_text_style("missing", "subtitle", "color:#fff")
    assert not tl.set_track_text_style("subs", "bad name", "color:#fff")
    assert tl.get_track_text_styles("subs") == []


def test_standalone_track_styles_travel_with_add_track():
    track = Track(kind="video", id="subs")
    assert track.set_text_style("subtitle", "font-size:40px")
    assert track.get_text_style("subtitle") == "font-size:40px"

    tl = Timeline()
    tl.set_text_style("subtitle", "font-size:56px")
    tl.add_track(track)
    assert tl.get_track_text_styles("subs") == [
        {"name": "subtitle", "declarations": "font-size:40px"}
    ]
    assert track.remove_text_style("subtitle")
    assert track.get_text_styles() == []
