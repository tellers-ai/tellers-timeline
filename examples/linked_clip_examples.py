import argparse
import json

from tellers_timeline import Clip, Gap, Item, MediaReference, Stack, Track


def clip(duration, url="file:///video.mov", id=None):
    return Clip(duration, {"DEFAULT_MEDIA": MediaReference(url)}, id=id)


def audio(duration, url="file:///audio.wav", id=None):
    return Clip(duration, {"DEFAULT_MEDIA": MediaReference(url)}, id=id)


def link_group(item):
    metadata = json.loads(item.get_metadata_json())
    return metadata.get("Resolve_OTIO", {}).get("Link Group ID")


def print_layout(stack):
    for track_index, track in enumerate(stack.tracks()):
        items = []
        for item in track.items():
            kind = "clip" if item.is_clip() else "gap"
            items.append(f"{kind}:{item.get_id()}:{item.duration()}")
        print(f"  Track {track_index} {track.kind} {track.get_id()}: {items}")


def print_case(title, result, stack):
    print(f"\n--- {title} ---")
    print("result:", result)
    print("layout:")
    print_layout(stack)
    groups = [
        link_group(item)
        for track in stack.tracks()
        for item in track.items()
        if item.is_clip()
    ]
    print("link groups:", groups)


def insert_at_time_examples():
    stack = Stack([Track(kind="video")])
    result = stack.insert_item_at_time(
        0,
        0.0,
        clip(3.0, id="primary"),
        "override",
        "split_and_insert",
        [],
    )
    print_case("insert_at_time: primary only", result, stack)

    stack = Stack([Track(kind="video")])
    result = stack.insert_item_at_time(
        0,
        0.0,
        clip(3.0, id="primary"),
        "override",
        "split_and_insert",
        [audio(3.0, id="audio")],
    )
    print_case("insert_at_time: video + linked audio", result, stack)

    stack = Stack([Track(kind="video")])
    result = stack.insert_item_at_time(
        0,
        0.0,
        clip(3.0, id="primary"),
        "override",
        "split_and_insert",
        [audio(2.0, id="audio")],
    )
    print_case("insert_at_time: duration mismatch", result, stack)

    stack = Stack([Track(kind="video", children=[Item.from_clip(clip(5.0, id="existing"))])])
    result = stack.insert_item_at_time(
        0,
        1.0,
        clip(2.0, id="primary"),
        "override",
        "split_and_insert",
        [],
    )
    print_case("insert_at_time: normal primary insert over conflict", result, stack)

    stack = Stack(
        [
            Track(kind="video"),
            Track(kind="audio", id="blocked", children=[Item.from_clip(audio(4.0, id="unlinked"))]),
            Track(kind="audio", id="later", children=[Item.from_gap(Gap(10.0, id="later-gap"))]),
        ]
    )
    result = stack.insert_item_at_time(
        0,
        0.0,
        clip(3.0, id="primary"),
        "override",
        "split_and_insert",
        [audio(3.0, id="audio")],
    )
    print_case("insert_at_time: unrelated audio boundary", result, stack)

    stack = Stack([Track(kind="audio")])
    result = stack.insert_item_at_time(
        0,
        0.0,
        audio(4.0, id="audio"),
        "override",
        "split_and_insert",
        None,
        clip(4.0, id="video"),
    )
    print_case("insert_at_time: audio + linked video", result, stack)


def insert_at_index_examples():
    stack = Stack([Track(kind="video", id="v")])
    result = stack.insert_item_at_index("v", 0, clip(3.0, id="primary"), "override")
    print_case("insert_at_index: primary only", result, stack)

    stack = Stack([Track(kind="video", id="v", children=[Item.from_gap(Gap(5.0, id="gap"))])])
    result = stack.insert_item_at_index(
        "v",
        0,
        clip(3.0, id="primary"),
        "override",
        [audio(3.0, id="audio")],
    )
    print_case("insert_at_index: video + linked audio", result, stack)

    stack = Stack([Track(kind="video", id="v", children=[Item.from_gap(Gap(5.0, id="gap"))])])
    result = stack.insert_item_at_index(
        "v",
        0,
        clip(3.0, id="primary"),
        "override",
        [audio(2.0, id="audio")],
    )
    print_case("insert_at_index: duration mismatch", result, stack)

    stack = Stack([Track(kind="video", id="v", children=[Item.from_clip(clip(5.0, id="existing"))])])
    result = stack.insert_item_at_index("v", 0, clip(2.0, id="primary"), "override", [])
    print_case("insert_at_index: normal primary insert at existing clip", result, stack)

    stack = Stack([Track(kind="video", id="v", children=[Item.from_clip(clip(2.0, id="existing"))])])
    result = stack.insert_item_at_index("v", 99, clip(3.0, id="primary"), "override", [])
    print_case("insert_at_index: past end", result, stack)

    stack = Stack([Track(kind="audio", id="a")])
    result = stack.insert_item_at_index(
        "a",
        0,
        audio(4.0, id="audio"),
        "override",
        None,
        clip(4.0, id="video"),
    )
    print_case("insert_at_index: audio + linked video", result, stack)


def replace_examples():
    stack = Stack([Track(kind="video", id="v", children=[Item.from_clip(clip(3.0, id="primary"))])])
    result = stack.replace_item("primary", clip(5.0, id="replacement"))
    print_case("replace_item: simple unlinked replace", result, stack)
    print("get_item('replacement'):", stack.get_item("replacement"))

    stack = Stack([Track(kind="video", id="v")])
    stack.insert_item_at_time(
        0,
        0.0,
        clip(3.0, id="primary"),
        "override",
        "split_and_insert",
        [audio(3.0, id="audio")],
    )
    result = stack.replace_item("primary", clip(5.0, id="replacement"))
    print_case("replace_item: selected linked clip", result, stack)

    stack = Stack([Track(kind="video", id="v", children=[Item.from_clip(clip(3.0, id="primary"))])])
    result = stack.replace_item(
        "primary",
        clip(3.0, id="replacement"),
        [audio(3.0, id="audio")],
    )
    print_case("replace_item: add linked audio", result, stack)

    stack = Stack([Track(kind="audio", id="a", children=[Item.from_clip(audio(4.0, id="audio"))])])
    result = stack.replace_item(
        "audio",
        audio(4.0, id="replacement"),
        None,
        clip(4.0, id="video"),
    )
    print_case("replace_item: add linked video", result, stack)

    stack = Stack([Track(kind="video", id="v", children=[Item.from_clip(clip(3.0, id="primary"))])])
    result = stack.replace_item(
        "primary",
        clip(3.0, id="replacement"),
        [audio(2.0, id="audio")],
    )
    print_case("replace_item: duration mismatch", result, stack)

    stack = Stack(
        [
            Track(kind="video", id="v", children=[Item.from_clip(clip(3.0, id="primary"))]),
            Track(kind="audio", id="blocked", children=[Item.from_clip(audio(4.0, id="unlinked"))]),
            Track(kind="audio", id="later", children=[Item.from_gap(Gap(10.0, id="later-gap"))]),
        ]
    )
    result = stack.replace_item(
        "primary",
        clip(3.0, id="replacement"),
        [audio(3.0, id="audio")],
    )
    print_case("replace_item: unrelated audio boundary", result, stack)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "example",
        choices=["all", "insert-time", "insert-index", "replace"],
        default="all",
        nargs="?",
    )
    args = parser.parse_args()

    if args.example in ("all", "insert-time"):
        insert_at_time_examples()
    if args.example in ("all", "insert-index"):
        insert_at_index_examples()
    if args.example in ("all", "replace"):
        replace_examples()


if __name__ == "__main__":
    main()
