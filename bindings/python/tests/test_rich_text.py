"""Tests for rich text getter/setter methods."""
import json
from tellers_timeline import MediaReference, MediaReferencePosition


def test_get_rich_text():
    """Test getting rich text from a GeneratorReference."""
    json_generator_ref = """
    {
        "OTIO_SCHEMA": "GeneratorReference.1",
        "metadata": {
            "Resolve_OTIO": {
                "Generator Type": "Rich"
            }
        },
        "name": "Text",
        "available_range": null,
        "available_image_bounds": null,
        "generator_kind": "Rich",
        "parameters": {
            "Resolve_OTIO": [
                {
                    "Effect Name": "Rich Text",
                    "Enabled": true,
                    "Name": "Rich Text",
                    "Parameters": [
                        {
                            "Parameter ID": "title blob",
                            "Title HTML": "<p>Test HTML Content</p>"
                        }
                    ],
                    "Type": 24
                }
            ]
        }
    }
    """

    media_ref = MediaReference.parse_json(json_generator_ref)
    html = media_ref.get_rich_text()
    assert html is not None
    assert html == "<p>Test HTML Content</p>"


def test_get_rich_text_not_found():
    """Test getting rich text when it doesn't exist."""
    json_generator_ref = """
    {
        "OTIO_SCHEMA": "GeneratorReference.1",
        "metadata": {
            "Resolve_OTIO": {
                "Generator Type": "Rich"
            }
        },
        "name": "Text",
        "available_range": null,
        "available_image_bounds": null,
        "generator_kind": "Rich",
        "parameters": {
            "Resolve_OTIO": []
        }
    }
    """

    media_ref = MediaReference.parse_json(json_generator_ref)
    html = media_ref.get_rich_text()
    assert html is None


def test_get_rich_text_external_reference():
    """Test getting rich text from ExternalReference (should return None)."""
    media_ref = MediaReference("file:///test.mp4")
    html = media_ref.get_rich_text()
    assert html is None


def test_set_rich_text():
    """Test setting rich text on a GeneratorReference."""
    json_generator_ref = """
    {
        "OTIO_SCHEMA": "GeneratorReference.1",
        "metadata": {
            "Resolve_OTIO": {
                "Generator Type": "Rich"
            }
        },
        "name": "Text",
        "available_range": null,
        "available_image_bounds": null,
        "generator_kind": "Rich",
        "parameters": {
            "Resolve_OTIO": []
        }
    }
    """

    media_ref = MediaReference.parse_json(json_generator_ref)
    media_ref.set_rich_text("<p>New HTML Content</p>")

    # Verify it was set
    html = media_ref.get_rich_text()
    assert html is not None
    assert html == "<p>New HTML Content</p>"


def test_set_rich_text_with_position():
    """Test setting rich text with position."""
    json_generator_ref = """
    {
        "OTIO_SCHEMA": "GeneratorReference.1",
        "metadata": {
            "Resolve_OTIO": {
                "Generator Type": "Rich"
            }
        },
        "name": "Text",
        "available_range": null,
        "available_image_bounds": null,
        "generator_kind": "Rich",
        "parameters": {
            "Resolve_OTIO": []
        }
    }
    """

    media_ref = MediaReference.parse_json(json_generator_ref)
    pos = MediaReferencePosition(x=0.3, y=0.7)
    media_ref.set_rich_text("<p>HTML with Position</p>", pos)

    # Verify HTML was set
    html = media_ref.get_rich_text()
    assert html is not None
    assert html == "<p>HTML with Position</p>"

    # Verify position was set by checking JSON
    json_str = str(media_ref)
    data = json.loads(json_str)
    assert "parameters" in data
    assert "Resolve_OTIO" in data["parameters"]
    resolve_otio = data["parameters"]["Resolve_OTIO"]
    assert len(resolve_otio) > 0

    # Find Rich Text effect
    rich_text_effect = None
    for effect in resolve_otio:
        if effect.get("Effect Name") == "Rich Text" and effect.get("Type") == 24:
            rich_text_effect = effect
            break

    assert rich_text_effect is not None
    assert "Parameters" in rich_text_effect

    # Find position parameter
    position_param = None
    for param in rich_text_effect["Parameters"]:
        if param.get("Parameter ID") == "position":
            position_param = param
            break

    assert position_param is not None
    assert "Parameter Value" in position_param
    assert position_param["Parameter Value"] == [0.3, 0.7]


def test_set_rich_text_default_position():
    """Test that default position is set when position is None and no position exists."""
    json_generator_ref = """
    {
        "OTIO_SCHEMA": "GeneratorReference.1",
        "metadata": {
            "Resolve_OTIO": {
                "Generator Type": "Rich"
            }
        },
        "name": "Text",
        "available_range": null,
        "available_image_bounds": null,
        "generator_kind": "Rich",
        "parameters": {
            "Resolve_OTIO": []
        }
    }
    """

    media_ref = MediaReference.parse_json(json_generator_ref)
    media_ref.set_rich_text("<p>HTML with Default Position</p>")

    # Verify position was set to default by checking JSON
    json_str = str(media_ref)
    data = json.loads(json_str)
    assert "parameters" in data
    assert "Resolve_OTIO" in data["parameters"]
    resolve_otio = data["parameters"]["Resolve_OTIO"]
    assert len(resolve_otio) > 0

    # Find Rich Text effect
    rich_text_effect = None
    for effect in resolve_otio:
        if effect.get("Effect Name") == "Rich Text" and effect.get("Type") == 24:
            rich_text_effect = effect
            break

    assert rich_text_effect is not None
    assert "Parameters" in rich_text_effect

    # Find position parameter
    position_param = None
    for param in rich_text_effect["Parameters"]:
        if param.get("Parameter ID") == "position":
            position_param = param
            break

    assert position_param is not None
    assert "Parameter Value" in position_param
    assert position_param["Parameter Value"] == [0.5, 0.5]


def test_set_rich_text_update_existing():
    """Test updating existing rich text."""
    json_generator_ref = """
    {
        "OTIO_SCHEMA": "GeneratorReference.1",
        "metadata": {
            "Resolve_OTIO": {
                "Generator Type": "Rich"
            }
        },
        "name": "Text",
        "available_range": null,
        "available_image_bounds": null,
        "generator_kind": "Rich",
        "parameters": {
            "Resolve_OTIO": [
                {
                    "Effect Name": "Rich Text",
                    "Enabled": true,
                    "Name": "Rich Text",
                    "Parameters": [
                        {
                            "Parameter ID": "title blob",
                            "Title HTML": "<p>Old HTML</p>"
                        }
                    ],
                    "Type": 24
                }
            ]
        }
    }
    """

    media_ref = MediaReference.parse_json(json_generator_ref)
    media_ref.set_rich_text("<p>Updated HTML</p>")

    # Verify it was updated
    html = media_ref.get_rich_text()
    assert html is not None
    assert html == "<p>Updated HTML</p>"


def test_set_rich_text_external_reference_error():
    """Test that set_rich_text raises an error on ExternalReference."""
    media_ref = MediaReference("file:///test.mp4")
    try:
        media_ref.set_rich_text("<p>Test</p>")
        assert False, "Expected ValueError"
    except ValueError as e:
        assert "set_rich_text can only be called on GeneratorReference" in str(e)


def test_rich_text_roundtrip():
    """Test that rich text survives JSON serialization/deserialization."""
    json_generator_ref = """
    {
        "OTIO_SCHEMA": "GeneratorReference.1",
        "metadata": {
            "Resolve_OTIO": {
                "Generator Type": "Rich"
            }
        },
        "name": "Text",
        "available_range": null,
        "available_image_bounds": null,
        "generator_kind": "Rich",
        "parameters": {
            "Resolve_OTIO": []
        }
    }
    """

    media_ref = MediaReference.parse_json(json_generator_ref)
    pos = MediaReferencePosition(x=0.25, y=0.75)
    media_ref.set_rich_text("<p>Roundtrip Test</p>", pos)

    # Serialize to JSON
    json_str = str(media_ref)

    # Deserialize
    media_ref2 = MediaReference.parse_json(json_str)

    # Verify rich text is preserved
    html = media_ref2.get_rich_text()
    assert html is not None
    assert html == "<p>Roundtrip Test</p>"
