"""Tests for the clip composite mode (DaVinci Resolve "Composite Mode") accessors."""
import json

import pytest
from tellers_timeline import (
    COMPOSITE_MODES,
    MASK_COMPOSITE_MODES,
    Clip,
    Gap,
    Item,
    MediaReference,
    MediaReferenceCrop,
    composite_mode_from_resolve_code,
    composite_mode_resolve_code,
)


def make_clip():
    ref = MediaReference("file:///test.mp4")
    return Clip(10.0, {"DEFAULT_MEDIA": ref}, "DEFAULT_MEDIA")


def composite_effect(clip_dict):
    for effect in clip_dict["effects"]:
        resolve_otio = effect.get("metadata", {}).get("Resolve_OTIO")
        if resolve_otio and resolve_otio.get("Effect Name") == "Composite":
            return resolve_otio
    return None


def test_mode_table_matches_resolve_enum():
    assert len(COMPOSITE_MODES) == 32
    assert COMPOSITE_MODES[0] == "normal"
    assert COMPOSITE_MODES[16] == "color"
    assert COMPOSITE_MODES[28] == "alpha"
    assert COMPOSITE_MODES[31] == "inverted-luminosity"
    assert MASK_COMPOSITE_MODES == [
        "luma-mask", "alpha", "inverted-alpha", "luminosity", "inverted-luminosity",
    ]
    for code, name in enumerate(COMPOSITE_MODES):
        assert composite_mode_from_resolve_code(code) == name
        assert composite_mode_resolve_code(name) == code
    assert composite_mode_from_resolve_code(32) is None
    assert composite_mode_resolve_code("Hard Light") == 7
    with pytest.raises(ValueError):
        composite_mode_resolve_code("not a mode")


def test_default_is_normal():
    assert make_clip().get_composite_mode() == "normal"


def test_get_set_composite_mode():
    clip = make_clip()
    clip.set_composite_mode("multiply")
    assert clip.get_composite_mode() == "multiply"
    clip.set_composite_mode("Inverted Alpha")
    assert clip.get_composite_mode() == "inverted-alpha"
    clip.set_composite_mode("normal")
    assert clip.get_composite_mode() == "normal"


def test_unknown_mode_raises():
    clip = make_clip()
    with pytest.raises(ValueError):
        clip.set_composite_mode("sparkle")
    assert clip.get_composite_mode() == "normal"


def test_serialization_writes_resolve_effect():
    clip = make_clip()
    clip.set_composite_mode("alpha")

    clip_dict = json.loads(clip.to_json())
    effect = composite_effect(clip_dict)
    assert effect is not None
    assert effect["Enabled"] is True
    assert effect["Type"] == 1
    param_map = {p["Parameter ID"]: p for p in effect["Parameters"]}
    assert param_map["composite mode"]["Parameter Value"] == 28
    assert param_map["composite mode"]["Variant Type"] == "UInt"

    clip2 = Clip.parse_json(clip.to_json())
    assert clip2.get_composite_mode() == "alpha"


def test_parse_resolve_export():
    """The Composite effect exactly as DaVinci Resolve writes it."""
    clip_json = {
        "OTIO_SCHEMA": "Clip.2",
        "name": "happy_horse_cheerful_meadow.mp4",
        "source_range": {
            "OTIO_SCHEMA": "TimeRange.1",
            "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 30.0, "value": 75.0},
            "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 30.0, "value": 0.0},
        },
        "media_references": {
            "DEFAULT_MEDIA": {
                "OTIO_SCHEMA": "ExternalReference.1",
                "metadata": {},
                "name": "happy_horse_cheerful_meadow.mp4",
                "available_range": None,
                "available_image_bounds": None,
                "target_url": "/Users/me/happy_horse_cheerful_meadow.mp4",
            }
        },
        "active_media_reference_key": "DEFAULT_MEDIA",
        "metadata": {"Resolve_OTIO": {"Link Group ID": 1}},
        "effects": [
            {
                "OTIO_SCHEMA": "Effect.1",
                "metadata": {
                    "Resolve_OTIO": {
                        "Display Type": 1,
                        "Effect Name": "Composite",
                        "Enabled": True,
                        "Name": "Composite",
                        "Parameters": [
                            {
                                "Default Parameter Value": 0,
                                "Parameter ID": "composite mode",
                                "Parameter Value": 16,
                                "Variant Type": "UInt",
                            }
                        ],
                        "Type": 1,
                    }
                },
                "name": "",
                "effect_name": "Resolve Effect",
            }
        ],
    }
    clip = Clip.parse_json(json.dumps(clip_json))
    assert clip.get_composite_mode() == "color"

    # Resolve exports Normal with an empty parameter list.
    clip_json["effects"][0]["metadata"]["Resolve_OTIO"]["Parameters"] = []
    assert Clip.parse_json(json.dumps(clip_json)).get_composite_mode() == "normal"


def test_composite_mode_keeps_other_effects():
    clip = make_clip()
    clip.set_crop(MediaReferenceCrop(crop_left=0.2))
    clip.set_composite_mode("screen")
    clip.set_composite_mode("overlay")

    assert clip.get_composite_mode() == "overlay"
    assert abs(clip.get_crop().get_crop_left() - 0.2) < 1e-9
    clip_dict = json.loads(clip.to_json())
    names = [e["metadata"]["Resolve_OTIO"]["Effect Name"] for e in clip_dict["effects"]]
    assert sorted(names) == ["Composite", "Cropping"]


def test_item_accessors():
    item = Item.from_clip(make_clip())
    assert item.get_composite_mode() == "normal"
    item.set_composite_mode("luminosity")
    assert item.get_composite_mode() == "luminosity"

    gap = Item.from_gap(Gap(2.0))
    gap.set_composite_mode("luminosity")
    assert gap.get_composite_mode() == "normal"
