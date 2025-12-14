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


def test_create_rich_text_reference():
    """Test creating rich text GeneratorReference."""
    media_ref = MediaReference.create_rich_text_reference("<p>New HTML Content</p>")

    # Verify it's a GeneratorReference
    assert media_ref is not None

    # Verify HTML was set
    html = media_ref.get_rich_text()
    assert html is not None
    assert html == "<p>New HTML Content</p>"


def test_create_rich_text_reference_default_position():
    """Test that default position [0.5, 0.5] is set."""
    media_ref = MediaReference.create_rich_text_reference(
        "<p>HTML with Default Position</p>"
    )

    # Verify HTML was set
    html = media_ref.get_rich_text()
    assert html is not None
    assert html == "<p>HTML with Default Position</p>"

    # Verify position was set to default [0.5, 0.5] by checking JSON
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


def test_create_rich_text_reference_creates_new():
    """Test that create_rich_text_reference creates a new GeneratorReference."""
    media_ref = MediaReference.create_rich_text_reference("<p>New HTML</p>")

    # Verify it creates a new one with the HTML
    html = media_ref.get_rich_text()
    assert html is not None
    assert html == "<p>New HTML</p>"


def test_rich_text_roundtrip():
    """Test that rich text survives JSON serialization/deserialization."""
    media_ref = MediaReference.create_rich_text_reference("<p>Roundtrip Test</p>")

    # Serialize to JSON
    json_str = str(media_ref)

    # Deserialize
    media_ref2 = MediaReference.parse_json(json_str)

    # Verify rich text is preserved
    html = media_ref2.get_rich_text()
    assert html is not None
    assert html == "<p>Roundtrip Test</p>"
