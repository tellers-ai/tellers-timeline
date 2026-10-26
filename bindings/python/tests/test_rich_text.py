"""Tests for reading rich text (Rich Text clips are read, never created)."""
from tellers_timeline import MediaReference


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


def test_rich_text_roundtrip():
    """Rich text survives JSON serialization/deserialization."""
    media_ref = MediaReference.parse_json("""{
        "OTIO_SCHEMA": "GeneratorReference.1",
        "generator_kind": "Rich",
        "parameters": {"Resolve_OTIO": [{
            "Effect Name": "Rich Text", "Enabled": true, "Name": "Rich Text", "Type": 24,
            "Parameters": [{"Parameter ID": "title blob", "Title HTML": "<p>Roundtrip Test</p>"}]
        }]}
    }""")
    assert media_ref.get_rich_text() == "<p>Roundtrip Test</p>"

    media_ref2 = MediaReference.parse_json(str(media_ref))
    assert media_ref2.get_rich_text() == "<p>Roundtrip Test</p>"
