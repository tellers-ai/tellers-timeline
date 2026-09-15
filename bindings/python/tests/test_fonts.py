"""Tests for the timeline custom-font methods (get/add/remove)."""
import json

from tellers_timeline import Timeline


def test_timeline_without_fonts_has_none():
    assert Timeline().get_fonts() == []


def test_add_font_then_get_fonts_round_trips():
    tl = Timeline()
    assert tl.add_font(
        "Brand Sans",
        "https://cdn.example.test/brand-sans.woff2",
        weight="400 700",
        style="normal",
    )

    assert tl.get_fonts() == [
        {
            "family": "Brand Sans",
            "url": "https://cdn.example.test/brand-sans.woff2",
            "weight": "400 700",
            "style": "normal",
        }
    ]
    metadata = json.loads(tl.get_metadata_json())
    assert metadata["tellers.ai"]["availableFonts"] == [
        {
            "family": "Brand Sans",
            "url": "https://cdn.example.test/brand-sans.woff2",
            "weight": "400 700",
            "style": "normal",
        }
    ]


def test_add_font_without_weight_or_style():
    tl = Timeline()
    assert tl.add_font("Plain", "https://cdn.example.test/plain.woff2")

    assert tl.get_fonts() == [
        {
            "family": "Plain",
            "url": "https://cdn.example.test/plain.woff2",
            "weight": None,
            "style": None,
        }
    ]


def test_add_font_replaces_the_same_family():
    tl = Timeline()
    assert tl.add_font("Brand Sans", "https://cdn.example.test/v1.woff2")
    assert tl.add_font("brand sans", "https://cdn.example.test/v2.woff2")

    fonts = tl.get_fonts()
    assert len(fonts) == 1
    assert fonts[0]["url"] == "https://cdn.example.test/v2.woff2"


def test_add_font_rejects_an_unusable_font():
    tl = Timeline()
    assert not tl.add_font("", "https://cdn.example.test/nameless.woff2")
    assert not tl.add_font("Ftp", "ftp://cdn.example.test/ftp.woff2")
    assert tl.get_fonts() == []


def test_remove_font():
    tl = Timeline()
    assert tl.add_font("Brand Sans", "https://cdn.example.test/brand-sans.woff2")
    assert tl.add_font("Other", "https://cdn.example.test/other.woff2")

    assert tl.remove_font("BRAND sans")
    assert [font["family"] for font in tl.get_fonts()] == ["Other"]
    assert not tl.remove_font("Brand Sans")


def test_fonts_survive_a_json_round_trip():
    tl = Timeline()
    assert tl.add_font("Brand Sans", "https://cdn.example.test/brand-sans.woff2")

    parsed = Timeline.parse_json(tl.to_json())
    assert parsed.get_fonts() == tl.get_fonts()
