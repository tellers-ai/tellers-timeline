"""Tests for crop getter/setter methods."""
import json
from tellers_timeline import (
    Clip,
    Item,
    MediaReference,
    MediaReferenceCrop,
    MediaReferencePosition,
)


def test_get_set_crop():
    """Test getting and setting crop on a clip."""
    ref = MediaReference("file:///test.mp4")
    clip = Clip(10.0, {"DEFAULT_MEDIA": ref}, "DEFAULT_MEDIA")

    crop = clip.get_crop()
    assert crop.get_crop_left() == 0.0
    assert crop.get_crop_right() == 0.0
    assert crop.get_crop_top() == 0.0
    assert crop.get_crop_bottom() == 0.0

    new_crop = MediaReferenceCrop(
        crop_left=0.1, crop_right=0.2, crop_top=0.3, crop_bottom=0.4
    )
    clip.set_crop(new_crop)

    crop_after = clip.get_crop()
    assert abs(crop_after.get_crop_left() - 0.1) < 0.001
    assert abs(crop_after.get_crop_right() - 0.2) < 0.001
    assert abs(crop_after.get_crop_top() - 0.3) < 0.001
    assert abs(crop_after.get_crop_bottom() - 0.4) < 0.001


def test_crop_serialization_roundtrip():
    """Test that crop survives JSON serialization/deserialization."""
    ref = MediaReference("file:///test.mp4")
    clip = Clip(10.0, {"DEFAULT_MEDIA": ref}, "DEFAULT_MEDIA")

    crop = MediaReferenceCrop(
        crop_left=0.05, crop_right=0.15, crop_top=0.25, crop_bottom=0.35
    )
    clip.set_crop(crop)

    json_str = clip.to_json()
    clip_dict = json.loads(json_str)

    assert "effects" in clip_dict
    assert len(clip_dict["effects"]) > 0

    cropping_effect = None
    for effect in clip_dict["effects"]:
        if effect.get("effect_name") == "Resolve Effect":
            metadata = effect.get("metadata", {})
            resolve_otio = metadata.get("Resolve_OTIO")
            if resolve_otio and resolve_otio.get("Name") == "Cropping":
                cropping_effect = resolve_otio
                break

    assert cropping_effect is not None
    assert "Parameters" in cropping_effect

    params = cropping_effect["Parameters"]
    param_map = {p["Parameter ID"]: p["Parameter Value"] for p in params}
    assert abs(param_map["cropLeft"] - 0.05) < 0.001
    assert abs(param_map["cropRight"] - 0.15) < 0.001
    assert abs(param_map["cropTop"] - 0.25) < 0.001
    assert abs(param_map["cropBottom"] - 0.35) < 0.001

    clip2 = Clip.parse_json(json_str)
    crop2 = clip2.get_crop()
    assert abs(crop2.get_crop_left() - 0.05) < 0.001
    assert abs(crop2.get_crop_right() - 0.15) < 0.001
    assert abs(crop2.get_crop_top() - 0.25) < 0.001
    assert abs(crop2.get_crop_bottom() - 0.35) < 0.001


def test_crop_with_position_and_volume():
    """Test setting crop alongside position and volume on the same clip."""
    ref = MediaReference("file:///test.mp4")
    clip = Clip(10.0, {"DEFAULT_MEDIA": ref}, "DEFAULT_MEDIA")

    pos = MediaReferencePosition(x=0.4, y=0.6, rotation=90.0, zoom_x=1.3, zoom_y=1.1)
    crop = MediaReferenceCrop(
        crop_left=0.1, crop_right=0.2, crop_top=0.3, crop_bottom=0.4
    )
    clip.set_position(pos)
    clip.set_volume(-20.0)
    clip.set_crop(crop)

    pos_after = clip.get_position()
    assert abs(pos_after.get_x() - 0.4) < 0.001
    assert abs(pos_after.get_rotation() - 90.0) < 0.001

    assert abs(clip.get_volume() - (-20.0)) < 0.001

    crop_after = clip.get_crop()
    assert abs(crop_after.get_crop_left() - 0.1) < 0.001
    assert abs(crop_after.get_crop_bottom() - 0.4) < 0.001

    json_str = clip.to_json()
    clip2 = Clip.parse_json(json_str)

    pos2 = clip2.get_position()
    assert abs(pos2.get_x() - 0.4) < 0.001
    assert abs(clip2.get_volume() - (-20.0)) < 0.001
    crop2 = clip2.get_crop()
    assert abs(crop2.get_crop_left() - 0.1) < 0.001
    assert abs(crop2.get_crop_bottom() - 0.4) < 0.001


def test_crop_clamping():
    """Test that crop insets are clamped to [0, 1]."""
    ref = MediaReference("file:///test.mp4")
    clip = Clip(10.0, {"DEFAULT_MEDIA": ref}, "DEFAULT_MEDIA")

    crop = MediaReferenceCrop(
        crop_left=-0.5, crop_right=1.5, crop_top=float("inf"), crop_bottom=float("nan")
    )
    clip.set_crop(crop)

    crop_after = clip.get_crop()
    assert abs(crop_after.get_crop_left() - 0.0) < 0.001
    assert abs(crop_after.get_crop_right() - 1.0) < 0.001
    assert abs(crop_after.get_crop_top() - 0.0) < 0.001
    assert abs(crop_after.get_crop_bottom() - 0.0) < 0.001


def test_parse_json_with_crop():
    """Test parsing JSON that already contains a crop effect."""
    json_data = {
        "OTIO_SCHEMA": "Clip.2",
        "name": "test_clip",
        "source_range": {
            "OTIO_SCHEMA": "TimeRange.1",
            "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 1.0, "value": 10.0},
            "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 1.0, "value": 0.0},
        },
        "media_references": {
            "DEFAULT_MEDIA": {
                "OTIO_SCHEMA": "ExternalReference.1",
                "target_url": "file:///test.mp4",
                "name": "test",
            }
        },
        "active_media_reference_key": "DEFAULT_MEDIA",
        "effects": [
            {
                "OTIO_SCHEMA": "Effect.1",
                "effect_name": "Resolve Effect",
                "name": "",
                "metadata": {
                    "Resolve_OTIO": {
                        "Effect Name": "Cropping",
                        "Enabled": True,
                        "Name": "Cropping",
                        "Parameters": [
                            {
                                "Variant Type": "Double",
                                "Parameter ID": "cropLeft",
                                "Parameter Value": 0.12,
                                "Default Parameter Value": 0.0,
                                "maxValue": 1.0,
                                "minValue": 0.0,
                            },
                            {
                                "Variant Type": "Double",
                                "Parameter ID": "cropRight",
                                "Parameter Value": 0.22,
                                "Default Parameter Value": 0.0,
                                "maxValue": 1.0,
                                "minValue": 0.0,
                            },
                            {
                                "Variant Type": "Double",
                                "Parameter ID": "cropTop",
                                "Parameter Value": 0.32,
                                "Default Parameter Value": 0.0,
                                "maxValue": 1.0,
                                "minValue": 0.0,
                            },
                            {
                                "Variant Type": "Double",
                                "Parameter ID": "cropBottom",
                                "Parameter Value": 0.42,
                                "Default Parameter Value": 0.0,
                                "maxValue": 1.0,
                                "minValue": 0.0,
                            },
                        ],
                        "Type": 3,
                    }
                },
            }
        ],
    }

    clip = Clip.parse_json(json.dumps(json_data))
    crop = clip.get_crop()
    assert abs(crop.get_crop_left() - 0.12) < 0.001
    assert abs(crop.get_crop_right() - 0.22) < 0.001
    assert abs(crop.get_crop_top() - 0.32) < 0.001
    assert abs(crop.get_crop_bottom() - 0.42) < 0.001

    json_str2 = clip.to_json()
    clip2 = Clip.parse_json(json_str2)
    crop2 = clip2.get_crop()
    assert abs(crop2.get_crop_left() - 0.12) < 0.001
    assert abs(crop2.get_crop_bottom() - 0.42) < 0.001


def test_item_get_set_crop():
    """Test crop getters/setters on Item wrapper."""
    ref = MediaReference("file:///test.mp4")
    clip = Clip(10.0, {"DEFAULT_MEDIA": ref}, "DEFAULT_MEDIA")
    item = Item.from_clip(clip)

    crop = MediaReferenceCrop(
        crop_left=0.08, crop_right=0.18, crop_top=0.28, crop_bottom=0.38
    )
    item.set_crop(crop)

    crop_after = item.get_crop()
    assert abs(crop_after.get_crop_left() - 0.08) < 0.001
    assert abs(crop_after.get_crop_bottom() - 0.38) < 0.001
