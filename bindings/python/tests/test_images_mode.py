"""Tests for images timelines: edit_mode, compounds() and frame_delay (video-player-js #835)."""
import pathlib

import pytest

from tellers_timeline import Timeline

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


def test_compounds_and_frame_delay_of_the_players_example():
    tl = Timeline.parse_json(FIXTURE.read_text())
    assert tl.get_edit_mode() == "images"
    pages = tl.compounds()
    assert [(p["name"], p["start"], p["frame_delay"]) for p in pages] == [
        ("Cover", 0.0, 1.5),
        ("Offer", 2.0, 1.5),
        ("Call to action", 4.0, 2.5),
    ]
    cover = pages[0]["id"]
    assert tl.set_frame_delay(cover, 0.5)
    assert tl.get_frame_delay(cover) == 0.5
    assert not tl.set_frame_delay(cover, 0)
    assert tl.set_frame_delay(cover)
    assert tl.get_frame_delay(cover) is None
    assert tl.compounds()[0]["frame_delay"] == 1.0
