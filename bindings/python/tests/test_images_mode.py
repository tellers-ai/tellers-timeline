"""Tests for images timelines: edit_mode, compounds() and pages() (video-player-js #835, #854)."""
import pathlib

import pytest

from tellers_timeline import Timeline, Track

FIXTURE = pathlib.Path(__file__).parents[3] / "tellers-timeline-core/tests/fixtures/images_pages.otio"


def test_edit_mode_defaults_to_video_and_round_trips():
    tl = Timeline()
    assert tl.get_edit_mode() == "video"
    tl.set_edit_mode("images")
    assert tl.get_edit_mode() == "images"
    tl.set_edit_mode("video")
    assert tl.get_edit_mode() == "video"
    with pytest.raises(ValueError):
        tl.set_edit_mode("static")


def test_compounds_and_pages_of_the_players_example():
    tl = Timeline.parse_json(FIXTURE.read_text())
    assert tl.get_edit_mode() == "images"
    assert [(p["name"], p["start"], p["duration"]) for p in tl.compounds()] == [
        ("Cover", 0.0, 1.5),
        ("Offer", 1.5, 1.5),
        ("Call to action", 3.0, 2.5),
    ]
    assert tl.pages() == [
        {"time": 0.0, "duration": 1.5},
        {"time": 1.5, "duration": 1.5},
        {"time": 3.0, "duration": 2.5},
    ]


def test_an_images_timeline_refuses_audio():
    tl = Timeline([Track(kind="video", id="v1"), Track(kind="audio", id="a1")])
    assert tl.accepts_audio()
    tl.set_edit_mode("images")
    assert not tl.accepts_audio()
    assert [t.kind for t in tl.get_stack().children()] == ["video"]
    tl.add_track(Track(kind="audio", id="a2"))
    tl.sanitize()
    assert [t.kind for t in tl.get_stack().children()] == ["video"]
    assert not tl.remove_audio()
