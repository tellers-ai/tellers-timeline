"""Tests for the timeline text-style methods (get/set/remove, CSS)."""
import json

from tellers_timeline import Timeline


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
