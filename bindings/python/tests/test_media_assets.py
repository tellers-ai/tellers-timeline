"""Tests for resolving a media reference's `target_url` from its
`metadata["tellers.ai"]["media_id"]`, clips inside composite clips included:
`media_asset_ids`, `set_media_target_url(s)` and
`clear_media_asset_target_urls`.
"""
import json

from tellers_timeline import Timeline


def clip(media_id=None, url=""):
    metadata = {"tellers.ai": {"media_id": media_id}} if media_id else {}
    return {
        "OTIO_SCHEMA": "Clip.2",
        "source_range": {
            "OTIO_SCHEMA": "TimeRange.1",
            "start_time": {"OTIO_SCHEMA": "RationalTime.1", "value": 0, "rate": 1},
            "duration": {"OTIO_SCHEMA": "RationalTime.1", "value": 2, "rate": 1},
        },
        "active_media_reference_key": "DEFAULT_MEDIA",
        "media_references": {
            "DEFAULT_MEDIA": {
                "OTIO_SCHEMA": "ExternalReference.1",
                "target_url": url,
                "metadata": metadata,
            }
        },
    }


def composite(*children):
    return {
        "OTIO_SCHEMA": "Stack.1",
        "children": [{"OTIO_SCHEMA": "Track.1", "kind": "Video", "children": list(children)}],
    }


def make_timeline():
    return Timeline.parse_json(
        json.dumps(
            {
                "OTIO_SCHEMA": "Timeline.1",
                "tracks": {
                    "OTIO_SCHEMA": "Stack.1",
                    "children": [
                        {
                            "OTIO_SCHEMA": "Track.1",
                            "kind": "Video",
                            "children": [
                                clip("a"),
                                composite(clip("b"), composite(clip("c"))),
                                clip(url="https://raw.example/plain.mp4"),
                            ],
                        }
                    ],
                },
            }
        )
    )


def target_urls(tl):
    urls = []

    def walk(tracks):
        for track in tracks:
            for item in track["children"]:
                if item["OTIO_SCHEMA"] == "Stack.1":
                    walk(item["children"])
                elif item["OTIO_SCHEMA"].startswith("Clip"):
                    urls.append(item["media_references"]["DEFAULT_MEDIA"]["target_url"])

    walk(json.loads(tl.to_json())["tracks"]["children"])
    return urls


def test_media_asset_ids_include_composites():
    assert make_timeline().media_asset_ids() == ["a", "b", "c"]


def test_set_and_clear_media_target_urls():
    tl = make_timeline()
    urls = {id: f"https://cdn.example/{id}" for id in ("a", "b", "c")}
    assert tl.set_media_target_urls(urls) == 3
    assert target_urls(tl) == [
        "https://cdn.example/a",
        "https://cdn.example/b",
        "https://cdn.example/c",
        "https://raw.example/plain.mp4",
    ]

    assert tl.clear_media_asset_target_urls() == 3
    assert target_urls(tl) == ["", "", "", "https://raw.example/plain.mp4"]


def test_set_media_target_url_single():
    tl = make_timeline()
    assert tl.set_media_target_url("c", "https://cdn.example/c") == 1
    assert target_urls(tl)[2] == "https://cdn.example/c"
