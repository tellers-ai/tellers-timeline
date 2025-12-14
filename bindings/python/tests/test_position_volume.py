"""Tests for position and volume getter/setter methods."""
import json
from tellers_timeline import Clip, MediaReference, MediaReferencePosition, Timeline


def test_get_set_position():
    """Test getting and setting position on a clip."""
    # Create a clip with a media reference
    ref = MediaReference("file:///test.mp4")
    clip = Clip(10.0, {"DEFAULT_MEDIA": ref}, "DEFAULT_MEDIA")

    # Get initial position (should have defaults)
    pos = clip.get_position()
    assert pos.get_x() == 0.5
    assert pos.get_y() == 0.5
    assert pos.get_rotation() == 0.0
    assert pos.get_zoom_x() == 1.0
    assert pos.get_zoom_y() == 1.0

    # Set a new position
    new_pos = MediaReferencePosition(x=0.3, y=0.7, rotation=45.0, zoom_x=1.5, zoom_y=1.2)
    clip.set_position(new_pos)

    # Verify the position was set
    pos_after = clip.get_position()
    assert abs(pos_after.get_x() - 0.3) < 0.001
    assert abs(pos_after.get_y() - 0.7) < 0.001
    assert abs(pos_after.get_rotation() - 45.0) < 0.001
    assert abs(pos_after.get_zoom_x() - 1.5) < 0.001
    assert abs(pos_after.get_zoom_y() - 1.2) < 0.001


def test_get_set_volume():
    """Test getting and setting volume on a clip."""
    # Create a clip
    ref = MediaReference("file:///test.mp4")
    clip = Clip(10.0, {"DEFAULT_MEDIA": ref}, "DEFAULT_MEDIA")

    # Get initial volume (should be 1.0 default)
    volume = clip.get_volume()
    assert abs(volume - 1.0) < 0.001

    # Set a new volume
    clip.set_volume(-12.5)

    # Verify the volume was set
    volume_after = clip.get_volume()
    assert abs(volume_after - (-12.5)) < 0.001

    # Set another volume
    clip.set_volume(0.0)
    assert abs(clip.get_volume() - 0.0) < 0.001


def test_position_serialization_roundtrip():
    """Test that position survives JSON serialization/deserialization."""
    # Create a clip and set position
    ref = MediaReference("file:///test.mp4")
    clip = Clip(10.0, {"DEFAULT_MEDIA": ref}, "DEFAULT_MEDIA")

    # Set position
    pos = MediaReferencePosition(x=0.25, y=0.75, rotation=30.0, zoom_x=2.0, zoom_y=1.8)
    clip.set_position(pos)

    # Serialize to JSON
    json_str = clip.to_json()
    clip_dict = json.loads(json_str)

    # Verify effects are present in JSON
    assert "effects" in clip_dict
    assert len(clip_dict["effects"]) > 0

    # Find the Resolve Effect
    resolve_effect = None
    for effect in clip_dict["effects"]:
        if effect.get("effect_name") == "Resolve Effect":
            resolve_effect = effect
            break

    assert resolve_effect is not None
    assert "metadata" in resolve_effect
    assert "Resolve_OTIO" in resolve_effect["metadata"]

    resolve_otio = resolve_effect["metadata"]["Resolve_OTIO"]
    assert resolve_otio["Name"] == "Transform"
    assert "Parameters" in resolve_otio

    # Check for transformation parameters
    params = resolve_otio["Parameters"]
    param_ids = [p["Parameter ID"] for p in params]
    assert "transformationPan" in param_ids
    assert "transformationTilt" in param_ids
    assert "transformationZoomX" in param_ids
    assert "transformationZoomY" in param_ids
    assert "transformationRotationAngle" in param_ids

    # Deserialize and verify position is preserved
    clip2 = Clip.parse_json(json_str)
    pos2 = clip2.get_position()

    assert abs(pos2.get_x() - 0.25) < 0.001
    assert abs(pos2.get_y() - 0.75) < 0.001
    assert abs(pos2.get_rotation() - 30.0) < 0.001
    assert abs(pos2.get_zoom_x() - 2.0) < 0.001
    assert abs(pos2.get_zoom_y() - 1.8) < 0.001


def test_volume_serialization_roundtrip():
    """Test that volume survives JSON serialization/deserialization."""
    # Create a clip and set volume
    ref = MediaReference("file:///test.mp4")
    clip = Clip(10.0, {"DEFAULT_MEDIA": ref}, "DEFAULT_MEDIA")

    # Set volume
    clip.set_volume(-15.3)

    # Serialize to JSON
    json_str = clip.to_json()
    clip_dict = json.loads(json_str)

    # Verify effects are present in JSON
    assert "effects" in clip_dict
    assert len(clip_dict["effects"]) > 0

    # Find the Resolve Effect with Volume
    volume_effect = None
    for effect in clip_dict["effects"]:
        if effect.get("effect_name") == "Resolve Effect":
            metadata = effect.get("metadata", {})
            resolve_otio = metadata.get("Resolve_OTIO")
            if resolve_otio and resolve_otio.get("Name") == "Volume":
                volume_effect = resolve_otio
                break

    assert volume_effect is not None
    assert "Parameters" in volume_effect

    # Check for volume parameter
    params = volume_effect["Parameters"]
    volume_param = None
    for param in params:
        if param.get("Parameter ID") == "volume":
            volume_param = param
            break

    assert volume_param is not None
    assert abs(volume_param["Parameter Value"] - (-15.3)) < 0.001

    # Deserialize and verify volume is preserved
    clip2 = Clip.parse_json(json_str)
    volume2 = clip2.get_volume()

    assert abs(volume2 - (-15.3)) < 0.001


def test_position_and_volume_together():
    """Test setting both position and volume on the same clip."""
    ref = MediaReference("file:///test.mp4")
    clip = Clip(10.0, {"DEFAULT_MEDIA": ref}, "DEFAULT_MEDIA")

    # Set both position and volume
    pos = MediaReferencePosition(x=0.4, y=0.6, rotation=90.0, zoom_x=1.3, zoom_y=1.1)
    clip.set_position(pos)
    clip.set_volume(-20.0)

    # Verify both are set
    pos_after = clip.get_position()
    assert abs(pos_after.get_x() - 0.4) < 0.001
    assert abs(pos_after.get_rotation() - 90.0) < 0.001

    volume_after = clip.get_volume()
    assert abs(volume_after - (-20.0)) < 0.001

    # Serialize and deserialize
    json_str = clip.to_json()
    clip2 = Clip.parse_json(json_str)

    # Verify both are preserved
    pos2 = clip2.get_position()
    assert abs(pos2.get_x() - 0.4) < 0.001
    assert abs(pos2.get_rotation() - 90.0) < 0.001

    volume2 = clip2.get_volume()
    assert abs(volume2 - (-20.0)) < 0.001


def test_position_with_existing_effects():
    """Test setting position when clip already has effects."""
    ref = MediaReference("file:///test.mp4")
    clip = Clip(10.0, {"DEFAULT_MEDIA": ref}, "DEFAULT_MEDIA")

    # Set volume first (creates an effect)
    clip.set_volume(-10.0)

    # Then set position (should add another effect)
    pos = MediaReferencePosition(x=0.1, y=0.9, rotation=180.0, zoom_x=0.8, zoom_y=0.9)
    clip.set_position(pos)

    # Both should work
    assert abs(clip.get_volume() - (-10.0)) < 0.001
    pos_after = clip.get_position()
    assert abs(pos_after.get_x() - 0.1) < 0.001
    assert abs(pos_after.get_rotation() - 180.0) < 0.001


def test_parse_json_with_position_and_volume():
    """Test parsing JSON that already contains position and volume effects."""
    # Create JSON with position and volume effects
    json_data = {
        "OTIO_SCHEMA": "Clip.2",
        "name": "test_clip",
        "source_range": {
            "OTIO_SCHEMA": "TimeRange.1",
            "duration": {"OTIO_SCHEMA": "RationalTime.1", "rate": 1.0, "value": 10.0},
            "start_time": {"OTIO_SCHEMA": "RationalTime.1", "rate": 1.0, "value": 0.0}
        },
        "media_references": {
            "DEFAULT_MEDIA": {
                "OTIO_SCHEMA": "ExternalReference.1",
                "target_url": "file:///test.mp4",
                "name": "test"
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
                        "Effect Name": "Transform",
                        "Enabled": True,
                        "Name": "Transform",
                        "Parameters": [
                            {
                                "Variant Type": "Double",
                                "Parameter ID": "transformationPan",
                                "Parameter Value": 0.3,
                                "Default Parameter Value": 0.0,
                                "maxValue": 4.0,
                                "minValue": -4.0
                            },
                            {
                                "Variant Type": "Double",
                                "Parameter ID": "transformationTilt",
                                "Parameter Value": 0.7,
                                "Default Parameter Value": 0.0,
                                "maxValue": 4.0,
                                "minValue": -4.0
                            },
                            {
                                "Variant Type": "Double",
                                "Parameter ID": "transformationZoomX",
                                "Parameter Value": 1.5,
                                "Default Parameter Value": 1.0,
                                "maxValue": 100.0,
                                "minValue": 0.0
                            },
                            {
                                "Variant Type": "Double",
                                "Parameter ID": "transformationZoomY",
                                "Parameter Value": 1.2,
                                "Default Parameter Value": 1.0,
                                "maxValue": 100.0,
                                "minValue": 0.0
                            },
                            {
                                "Variant Type": "Double",
                                "Parameter ID": "transformationRotationAngle",
                                "Parameter Value": 45.0,
                                "Default Parameter Value": 0.0,
                                "maxValue": 100000.0,
                                "minValue": -100000.0
                            }
                        ],
                        "Type": 2
                    }
                }
            },
            {
                "OTIO_SCHEMA": "Effect.1",
                "effect_name": "Resolve Effect",
                "name": "",
                "metadata": {
                    "Resolve_OTIO": {
                        "Effect Name": "Fairlight Clip Volume and Fades",
                        "Enabled": True,
                        "Name": "Volume",
                        "Parameters": [
                            {
                                "Variant Type": "Double",
                                "Parameter ID": "volume",
                                "Parameter Value": -12.5,
                                "Default Parameter Value": 0.0,
                                "maxValue": 30.0,
                                "minValue": -100.0
                            }
                        ],
                        "Type": 62
                    }
                }
            }
        ]
    }

    # Parse JSON
    json_str = json.dumps(json_data)
    clip = Clip.parse_json(json_str)

    # Verify position and volume are parsed correctly
    pos = clip.get_position()
    assert abs(pos.get_x() - 0.3) < 0.001
    assert abs(pos.get_y() - 0.7) < 0.001
    assert abs(pos.get_rotation() - 45.0) < 0.001
    assert abs(pos.get_zoom_x() - 1.5) < 0.001
    assert abs(pos.get_zoom_y() - 1.2) < 0.001

    volume = clip.get_volume()
    assert abs(volume - (-12.5)) < 0.001

    # Serialize back and verify roundtrip
    json_str2 = clip.to_json()
    clip2 = Clip.parse_json(json_str2)

    pos2 = clip2.get_position()
    assert abs(pos2.get_x() - 0.3) < 0.001
    assert abs(pos2.get_rotation() - 45.0) < 0.001

    volume2 = clip2.get_volume()
    assert abs(volume2 - (-12.5)) < 0.001
