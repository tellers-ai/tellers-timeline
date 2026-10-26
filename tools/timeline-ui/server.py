#!/usr/bin/env python3
"""Visual test UI for tellers-timeline.

Serves a small browser timeline (index.html) backed by the Python bindings.
Every action in the UI is turned into a real call on a `Stack` held in memory,
so what you see is what the library actually did. No media data is needed:
clips just carry a placeholder URL.

Run:  python tools/timeline-ui/server.py [--port 8765] [--no-browser]  (PORT or TIMELINE_UI_PORT sets the default)
Or:   VS Code -> F5 -> "Timeline test UI"
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import threading
import traceback
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any, Callable, Optional

from tellers_timeline import Clip, Gap, Item, MediaReference, Stack, Timeline, Track, Transition, tellers_effect_specs

HERE = os.path.dirname(os.path.abspath(__file__))
EFFECT_SPECS = tellers_effect_specs()
MAX_HISTORY = 100
MAX_LOG = 200


# --------------------------------------------------------------------------- #
# Helpers to build items from the JSON the UI sends
# --------------------------------------------------------------------------- #


def _fmt(value: Any) -> str:
    """Python-literal-ish formatting for the call log."""
    if isinstance(value, bool) or value is None:
        return repr(value)
    if isinstance(value, float):
        return f"{value:g}"
    if isinstance(value, (list, tuple)):
        return "[" + ", ".join(_fmt(v) for v in value) + "]"
    return repr(value)


def make_media_reference(spec: dict) -> MediaReference:
    url = spec.get("url") or "file:///placeholder.mov"
    media_duration = spec.get("media_duration")
    if media_duration in ("", None):
        media_duration = None
    else:
        media_duration = float(media_duration)
    media_start = spec.get("media_start")
    # Note: passing media_start without media_duration creates a zero-length
    # available_range, which sanitize() clamps to nothing. Only pass what was
    # actually set so a plain clip keeps an unbounded available range.
    kwargs: dict[str, Any] = {}
    if media_start not in ("", None):
        kwargs["media_start"] = float(media_start)
    if media_duration is not None:
        kwargs["media_duration"] = media_duration
    return MediaReference(url, name=spec.get("name"), **kwargs)


def make_item(spec: dict):
    """Build a Clip or Gap from {kind:'clip'|'gap', duration, name, id, url, ...}."""
    duration = float(spec.get("duration", 1.0))
    item_id = spec.get("id") or None
    if spec.get("kind") == "gap":
        gap = Gap(duration, id=item_id)
        if spec.get("name"):
            gap.set_name(spec["name"])
        return gap
    if spec.get("text"):
        # A plain Text clip (the player's own text); rich text is no longer created.
        clip = Clip.new_text(
            duration,
            {"text": spec.get("text_plain") or spec.get("name") or "Text"},
            spec.get("name") or None,
            item_id,
        )
    else:
        clip = Clip(
            duration,
            {"DEFAULT_MEDIA": make_media_reference(spec)},
            active_key="DEFAULT_MEDIA",
            name=spec.get("name") or None,
            id=item_id,
        )
    if spec.get("enabled") is False:
        clip.set_enabled(False)
    if spec.get("source_start"):
        item = Item.from_clip(clip)
        tr = item.get_source_range()
        tr.set_start_time(float(spec["source_start"]))
        item.set_source_range(tr)
        return item
    return clip


def describe_item(spec: dict) -> str:
    """How the item shows up in the call log."""
    duration = float(spec.get("duration", 1.0))
    if spec.get("kind") == "gap":
        return f"Gap({duration:g})"
    parts = [f"{duration:g}"]
    if spec.get("text"):
        parts.append(f'{{"text": {_fmt(spec.get("text_plain") or spec.get("name") or "Text")}}}')
    else:
        parts.append(f'{{"DEFAULT_MEDIA": MediaReference({_fmt(spec.get("url") or "file:///placeholder.mov")})}}')
    if spec.get("name"):
        parts.append(f"name={_fmt(spec['name'])}")
    return "Clip(" + ", ".join(parts) + ")"


# --------------------------------------------------------------------------- #
# State
# --------------------------------------------------------------------------- #


class State:
    def __init__(self) -> None:
        self.stack = Stack()
        self.history: list[str] = []
        self.log: list[dict] = []
        # Open compounds, outermost first. While one is open, self.stack holds
        # its tracks (see open_compound).
        self.frames: list[dict] = []
        self.lock = threading.Lock()
        self.reset()

    # -- snapshots --------------------------------------------------------- #
    def snapshot(self) -> str:
        return Timeline(self.stack).to_json(pretty=False)

    def restore(self, js: str) -> None:
        self.stack = Timeline.parse_json(js).get_stack()

    def push_history(self) -> None:
        self.history.append(self.snapshot())
        del self.history[:-MAX_HISTORY]

    # -- view model -------------------------------------------------------- #
    def view(self, result: Any = None, call: Optional[str] = None, error: Optional[str] = None) -> dict:
        timeline = Timeline(self.stack)
        tracks = []
        for track_index, track in enumerate(self.stack.tracks()):
            start = 0.0
            items = []
            for item_index, item in enumerate(track.items()):
                duration = item.duration()
                meta = json.loads(item.get_metadata_json() or "{}")
                entry: dict[str, Any] = {
                    "id": item.get_id(),
                    "index": item_index,
                    "kind": _item_kind(item),
                    "effects": item.get_tellers_effects() if item.is_clip() or item.is_composite() else [],
                    "start": start,
                    "duration": duration,
                    "enabled": item.get_enabled(),
                    "link_group_id": (meta.get("Resolve_OTIO") or {}).get("Link Group ID"),
                    "group_id": (meta.get("tellers.ai") or {}).get("Tellers Group ID"),
                    "name": None,
                    "source_start": item.get_source_range().get_start_time(),
                }
                if item.is_clip():
                    refs = item.get_media_references()
                    key = item.get_active_media_reference_key() or "DEFAULT_MEDIA"
                    ref = refs.get(key) or (next(iter(refs.values())) if refs else None)
                    if ref is not None:
                        entry["text"] = ref.get_rich_text()
                        try:
                            entry["url"] = ref.get_url()
                        except ValueError:  # generator references have no target_url
                            entry["url"] = None
                        entry["media_start"] = ref.get_media_start()
                        entry["media_duration"] = ref.get_media_duration()
                    clip = _as_clip(item)
                    entry["name"] = clip.get_name() if clip else None
                    entry["text_clip"] = clip.is_text_clip() if clip else False
                    entry["speed"] = clip.get_speed() if clip else None
                    entry["source_duration"] = clip.get_source_duration() if clip else None
                elif item.is_composite():
                    entry["name"] = json.loads(str(item)).get("name")
                    entry["children"] = [_child_track_view(child) for child in item.get_children()]
                elif item.is_transition():
                    transition = item.as_transition()
                    entry.update(
                        name=transition.get_name(),
                        transition_type=transition.get_transition_type(),
                        transition_kind=transition.get_kind(),
                        audio_kind=transition.get_audio_kind(),
                        direction=transition.get_direction(),
                        color=transition.get_color(),
                        in_offset=transition.get_in_offset(),
                        out_offset=transition.get_out_offset(),
                    )
                items.append(entry)
                start += duration
            tracks.append(
                {
                    "id": track.get_id(),
                    "index": track_index,
                    "kind": track.kind,
                    "name": track.get_name(),
                    "enabled": track.get_enabled(),
                    "duration": track.total_duration(),
                    "effects": track.get_tellers_effects(),
                    "items": items,
                }
            )
        try:
            sync_info = self.stack.sync_track_info()
        except Exception as exc:  # pragma: no cover - defensive
            sync_info = [{"error": str(exc)}]
        try:
            track_groups = self.stack.track_groups()
        except Exception as exc:  # pragma: no cover - defensive
            track_groups = [{"error": str(exc)}]
        return {
            "tracks": tracks,
            "timeline": json.loads(timeline.to_json()),
            "validation": timeline.validate(),
            "sync_track_info": sync_info,
            "track_groups": track_groups,
            "markers": timeline.get_markers(),
            "timeline_effects": timeline.get_tellers_effects(),
            "effect_specs": EFFECT_SPECS,
            "result": result,
            "call": call,
            "error": error,
            "log": self.log[-MAX_LOG:],
            "can_undo": bool(self.history),
            "compound_path": [{"id": f["id"], "name": f["name"]} for f in self.frames],
        }

    # -- compounds ----------------------------------------------------------- #
    # Opening a composite clip makes a copy of its tracks the working stack
    # (stack.composite_stack), so every op applies to them; closing writes them
    # back with stack.set_composite_tracks, which ripples the enclosing track
    # when an auto-length composite changes length. Timeline-level data made
    # while inside (stack markers, clip groups) has nowhere to go and is
    # dropped on close.
    def open_compound(self, item_id: str) -> str:
        found = self.stack.get_item(item_id)
        if found is None or not found[2].is_composite():
            raise ValueError(f"{item_id!r} is not a composite clip at this level")
        inner = self.stack.composite_stack(item_id)
        name = inner.get_name() or item_id
        self.frames.append({"id": item_id, "name": name, "outer": self.snapshot(), "history": self.history})
        self.history = []
        self.stack = inner
        return name

    def close_compound(self) -> str:
        frame = self.frames.pop()
        tracks = self.stack.tracks()
        self.restore(frame["outer"])
        if not self.stack.set_composite_tracks(frame["id"], tracks):
            raise ValueError(f"composite {frame['id']!r} is gone")
        self.history = frame["history"]
        self.history.append(frame["outer"])  # undo the whole compound edit at once
        del self.history[:-MAX_HISTORY]
        return frame["name"]

    def close_all_compounds(self) -> None:
        """Close every open compound, keeping its edits as the undo point."""
        if not self.frames:
            return
        while self.frames:
            self.close_compound()
        self.push_history()

    # -- ops --------------------------------------------------------------- #
    def reset(self) -> None:
        self.stack = Stack()
        self.stack.add_track(Track(kind="video"))

    def load_sample(self) -> None:
        """Video tracks with 0, 1, 2 and 3 associated audio tracks, video clips
        with 0 to 3 synced audio clips, an audio-led column (an audio clip
        linked to a video clip), a clip played at 2x speed, a compound clip
        (lower third), a standalone music track and a track of transitions."""
        stack = self.stack = Stack()

        def clip(name: str, duration: float, ext: str = "mov", **spec: Any):
            url = f"file:///{name.lower().replace(' ', '_')}.{ext}"
            return make_item({"duration": duration, "name": name, "url": url, **spec})

        def channels(name: str, duration: float, count: int) -> list:
            names = [f"{name} audio {n + 1}" for n in range(count)] if count > 1 else [f"{name} audio"]
            return [clip(n, duration, "wav") for n in names]

        def add_track(kind: str, name: str, index: Optional[int] = None) -> int:
            track = Track(kind=kind)
            track.set_name(name)
            track_id = stack.add_track(track, len(stack.tracks()) if index is None else index)
            return stack.get_track_by_id(track_id)[0]

        def insert(track_name: str, time: float, item, linked_audio=None) -> None:
            index = next(i for i, t in enumerate(stack.tracks()) if t.get_name() == track_name)
            stack.insert_item_at_time(index, time, item, "override", "split_and_insert", linked_audio)

        # V1: two partner tracks (A1, A2), from clips with 0, 1 and 2 channels.
        add_track("video", "V1")
        insert("V1", 0.0, clip("A", 3))
        insert("V1", 4.0, clip("B", 2, media_duration=5), channels("B", 2, 1))
        insert("V1", 7.0, clip("C", 2.5), channels("C", 2.5, 2))
        # V2: three partner tracks (A3-A5), and a clip without sound.
        add_track("video", "V2")
        insert("V2", 2.0, clip("D", 2), channels("D", 2, 3))
        insert("V2", 6.0, clip("E", 1.5))
        # A speed change: 2s on the timeline playing 4s of its 6s media, with
        # its sync audio clip sped up too (set_item_speed follows the column).
        insert("V2", 8.5, clip("G 2x", 2, media_duration=6), channels("G 2x", 2, 1))
        g = next(it for t in stack.tracks() for it in t.items() if _clip_name(it) == "G 2x")
        stack.set_item_speed(g.get_id(), 2.0)
        # Cutaway: one partner track, from an audio-led column. The library
        # refuses an insert with a linked video, so it is linked afterwards.
        add_track("video", "Cutaway")
        insert("Cutaway", 5.0, clip("F", 2))
        add_track("audio", "VO", 0)
        insert("VO", 5.0, clip("F voice", 2, "wav"))
        ids = {item.get_id(): item for track in stack.tracks() for item in track.items()}
        by_name = {_clip_name(item): item_id for item_id, item in ids.items()}
        stack.link_item([by_name["F"], by_name["F voice"]])
        # Music: an audio track no video track owns. Text: no partner track.
        add_track("audio", "Music", 0)
        insert("Music", 0.0, clip("Music", 6, "wav", media_duration=8))
        insert("Music", 6.0, clip("Music 2", 4, "wav", media_duration=8, source_start=2))
        add_track("video", "Text")
        insert("Text", 1.0, make_item({"duration": 2, "name": "Title", "text": True, "text_html": "<p>Hello</p>"}))
        # A plain Text clip (a Text generator), the only clip text effects go on.
        insert("Text", 8.0, Item.from_clip(Clip.new_text(2.0, {"text": "Caption"}, "Caption")))
        # A compound (composite clip, an OTIO Stack.1 in the track): a lower
        # third made of a background bar and, a second later, a name on top.
        bar = Track(kind="video", children=[Item.from_clip(clip("Bar", 3))])
        bar.set_name("bar")
        name = Track(kind="video", children=[Item.from_gap(Gap(1.0)), Item.from_clip(make_item(
            {"duration": 2, "name": "Name", "text": True, "text_html": "<p>Jane Doe</p>"}))])
        name.set_name("name")
        insert("Text", 4.0, Item.from_composite([bar, name], name="Lower third"))
        # Transitions: two clips sharing one, and a clip with one before and
        # one after it (a move carries only the first). Spliced in below. Each
        # clip has 3s of media on either side so both edges can be dragged.
        add_track("video", "Transitions")
        for start, label in [(0.0, "T1"), (2.0, "T2"), (5.0, "X"), (7.0, "Y"), (9.0, "Z")]:
            insert("Transitions", start, clip(label, 2, media_duration=8, source_start=3))
        stack.normalize_track_order()
        # One marker of each kind: timeline-wide (a range), on a track, on a clip.
        v1 = next(t for t in self.stack.tracks() if t.get_name() == "V1")
        clip_b = next(it for it in v1.items() if it.is_clip() and it.duration() == 2)
        timeline = Timeline(self.stack)
        timeline.add_marker(0.5, name="Intro", comment="Timeline marker", color="BLUE", duration=1.0)
        timeline.add_marker(6.5, name="Track note", comment="Marker on V1", color="GREEN", owner_id=v1.get_id())
        timeline.add_marker(5.0, name="Fix B", comment="Clip marker: moves with B", owner_id=clip_b.get_id())
        self.stack = timeline.get_stack()
        # Items are [T1, T2, gap, X, Y, Z]: cuts T1|T2, X|Y and Y|Z, from the end.
        track_index = next(i for i, t in enumerate(self.stack.tracks()) if t.get_name() == "Transitions")
        cuts = [(5, {"kind": "Wipe", "direction": "right"}), (4, {"kind": "DipToColor", "color": "#ffffff"}), (1, {"kind": "Dissolve"})]
        _splice_track(self, track_index, lambda children: [
            children.insert(index, json.loads(_make_transition(spec).to_json())) for index, spec in cuts
        ])
        # Music: a constant-power crossfade between its two clips.
        music_index = next(i for i, t in enumerate(self.stack.tracks()) if t.get_name() == "Music")
        _splice_track(self, music_index, lambda children: children.insert(1, json.loads(_make_transition({"audio_kind": "constant-power"}).to_json())))

    def load_basic_sample(self) -> None:
        """One video track with one synced clip, and a text track."""
        self.stack = Stack()
        video = Track(kind="video")
        video.set_name("V1")
        self.stack.add_track(video)
        self.stack.insert_item_at_time(0, 0.0, make_item({"duration": 3, "name": "A", "url": "file:///a.mov"}), "override", "split_and_insert")
        self.stack.insert_item_at_time(0, 4.0, make_item({"duration": 2, "name": "B", "url": "file:///b.mov", "media_duration": 5}), "override", "split_and_insert")
        self.stack.insert_item_at_time(
            0,
            7.0,
            make_item({"duration": 2.5, "name": "C", "url": "file:///c.mov"}),
            "override",
            "split_and_insert",
            [make_item({"duration": 2.5, "name": "C audio", "url": "file:///c.wav"})],
        )
        overlay = Track(kind="video")
        overlay.set_name("Text")
        text_id = self.stack.add_track(overlay, len(self.stack.tracks()))
        text_index = self.stack.get_track_by_id(text_id)[0]
        self.stack.insert_item_at_time(text_index, 1.0, make_item({"duration": 2, "name": "Title", "text": True, "text_html": "<p>Hello</p>"}), "override", "split_and_insert")


def _item_kind(item: Item) -> str:
    if item.is_clip():
        return "clip"
    if item.is_composite():
        return "composite"
    return "transition" if item.is_transition() else "gap"


def _child_track_view(track: Track) -> dict:
    """A composite's inner track, flattened for the UI (one level deep)."""
    start, items = 0.0, []
    for item in track.items():
        items.append({"kind": _item_kind(item), "name": _clip_name(item) if item.is_clip() else None, "start": start, "duration": item.duration()})
        start += item.duration()
    return {"name": track.get_name(), "kind": track.kind, "items": items}


def _as_clip(item: Item) -> Optional[Clip]:
    # Item has no get_name / get_speed; read them from the serialized clip.
    try:
        return Clip.parse_json(str(item))
    except Exception:
        return None


def _clip_name(item: Item) -> Optional[str]:
    clip = _as_clip(item)
    return clip.get_name() if clip else None


# --------------------------------------------------------------------------- #
# Operations (name -> handler). Each handler returns (result, call_string).
# --------------------------------------------------------------------------- #

Handler = Callable[[State, dict], tuple[Any, Optional[str]]]
OPS: dict[str, Handler] = {}


def op(name: str, mutates: bool = True):
    def deco(fn: Handler) -> Handler:
        fn.mutates = mutates  # type: ignore[attr-defined]
        OPS[name] = fn
        return fn

    return deco


@op("state", mutates=False)
def op_state(state: State, p: dict):
    return None, None


@op("reset")
def op_reset(state: State, p: dict):
    state.close_all_compounds()
    state.reset()
    return True, "Stack(); stack.add_track(Track(kind='video'))"


@op("sample")
def op_sample(state: State, p: dict):
    state.close_all_compounds()
    state.load_sample()
    return True, "load_sample()"


@op("load_json")
def op_load_json(state: State, p: dict):
    state.close_all_compounds()
    state.restore(p["json"] if isinstance(p["json"], str) else json.dumps(p["json"]))
    return True, "Timeline.parse_json(...).get_stack()"


@op("open_compound", mutates=False)
def op_open_compound(state: State, p: dict):
    name = state.open_compound(p["id"])
    return name, f"stack = stack.composite_stack({_fmt(p['id'])})  # open {name!r}: ops now edit its tracks"


@op("close_compound", mutates=False)
def op_close_compound(state: State, p: dict):
    if not state.frames:
        return False, "close_compound (no compound open)"
    name = state.close_compound()
    return name, f"outer.set_composite_tracks(<id>, stack.tracks())  # close {name!r}, back to the outer stack"


@op("undo", mutates=False)
def op_undo(state: State, p: dict):
    if not state.history:
        return False, "undo (nothing to undo)"
    state.restore(state.history.pop())
    return True, "undo"


@op("sanitize")
def op_sanitize(state: State, p: dict):
    state.stack.sanitize()
    return True, "stack.sanitize()"


@op("normalize_tracks")
def op_normalize_tracks(state: State, p: dict):
    changed = state.stack.normalize_track_order()
    return changed, "stack.normalize_track_order()"


@op("prune_tracks")
def op_prune_tracks(state: State, p: dict):
    removed = state.stack.prune_empty_partner_tracks()
    return removed, "stack.prune_empty_partner_tracks()"


@op("add_track")
def op_add_track(state: State, p: dict):
    kind = p.get("kind") or "video"
    # The library's default insertion_index=-1 means "before the last track";
    # the UI passes an explicit index (default: append).
    index = int(p.get("index", len(state.stack.tracks())))
    track = Track(kind=kind)
    if p.get("name"):
        track.set_name(p["name"])
    track_id = state.stack.add_track(track, index)
    return track_id, f"stack.add_track(Track(kind={_fmt(kind)}), {index})"


@op("delete_track")
def op_delete_track(state: State, p: dict):
    removed = state.stack.delete_track(p["id"])
    return removed is not None, f"stack.delete_track({_fmt(p['id'])})"


@op("reorder_track")
def op_reorder_track(state: State, p: dict):
    index = int(p["index"])
    ok = state.stack.reorder_track(p["id"], index)
    return ok, f"stack.reorder_track({_fmt(p['id'])}, {index})"


@op("rename_track")
def op_rename_track(state: State, p: dict):
    tracks = state.stack.tracks()
    for track in tracks:
        if track.get_id() == p["id"]:
            track.set_name(p.get("name") or None)
            if "enabled" in p:
                track.set_enabled(bool(p["enabled"]))
    state.stack.set_tracks(tracks)
    return True, f"track.set_name({_fmt(p.get('name'))}); stack.set_tracks(tracks)"


@op("insert_item")
def op_insert_item(state: State, p: dict):
    item_spec = p["item"]
    item = make_item(item_spec)
    overlap = p.get("overlap_policy", "override")
    insert_policy = p.get("insert_policy", "split_and_insert")
    linked_audio_specs = p.get("linked_audio") or None
    linked_video_spec = p.get("linked_video") or None
    linked_audio = [make_item(s) for s in linked_audio_specs] if linked_audio_specs else None
    linked_video = make_item(linked_video_spec) if linked_video_spec else None
    extra = ""
    if linked_audio_specs:
        extra += ", linked_audio_clips=[" + ", ".join(describe_item(s) for s in linked_audio_specs) + "]"
    if linked_video_spec:
        extra += ", linked_video_clip=" + describe_item(linked_video_spec)
    if p.get("mode") == "index":
        track_id = p["track_id"]
        index = int(p["index"])
        result = state.stack.insert_item_at_index(track_id, index, item, overlap, linked_audio, linked_video)
        call = f"stack.insert_item_at_index({_fmt(track_id)}, {index}, {describe_item(item_spec)}, {_fmt(overlap)}{extra})"
    else:
        track_index = int(p["track_index"])
        time = float(p["time"])
        result = state.stack.insert_item_at_time(track_index, time, item, overlap, insert_policy, linked_audio, linked_video)
        call = f"stack.insert_item_at_time({track_index}, {time:g}, {describe_item(item_spec)}, {_fmt(overlap)}, {_fmt(insert_policy)}{extra})"
    return result, call


@op("delete_item")
def op_delete_item(state: State, p: dict):
    replace_with_gap = bool(p.get("replace_with_gap", True))
    with_sync_clips = bool(p.get("with_sync_clips", True))
    removed = state.stack.delete_item(p["id"], replace_with_gap, with_sync_clips)
    result = [{"track_index": ti, "item_id": it.get_id(), "duration": it.duration()} for ti, it in removed]
    return result, f"stack.delete_item({_fmt(p['id'])}, replace_with_gap={replace_with_gap}, with_sync_clips={with_sync_clips})"


@op("move_item")
def op_move_item(state: State, p: dict):
    replace_with_gap = bool(p.get("replace_with_gap", True))
    overlap = p.get("overlap_policy", "override")
    insert_policy = p.get("insert_policy", "insert_before_or_after")
    if p.get("mode") == "index":
        index = int(p["index"])
        ok = state.stack.move_item_at_index(p["id"], p["track_id"], index, replace_with_gap, overlap)
        call = f"stack.move_item_at_index({_fmt(p['id'])}, {_fmt(p['track_id'])}, {index}, {replace_with_gap}, {_fmt(overlap)})"
    elif p.get("mode") == "timeline":
        time = float(p["time"])
        timeline = Timeline(state.stack)
        ok = timeline.move_item(p["id"], p["track_id"], time)
        state.stack = timeline.get_stack()
        call = f"timeline.move_item({_fmt(p['id'])}, {_fmt(p['track_id'])}, {time:g})"
    elif p.get("others"):
        # A multi-selection drag: the others shift by the dragged clip's delta.
        time = float(p["time"])
        others = [str(i) for i in p["others"]]
        ok = state.stack.move_items_at_time(p["id"], others, p["track_id"], time, replace_with_gap, overlap, insert_policy)
        call = f"stack.move_items_at_time({_fmt(p['id'])}, {_fmt(others)}, {_fmt(p['track_id'])}, {time:g}, {replace_with_gap}, {_fmt(overlap)}, {_fmt(insert_policy)})"
    else:
        time = float(p["time"])
        ok = state.stack.move_item_at_time(p["id"], p["track_id"], time, replace_with_gap, overlap, insert_policy)
        call = f"stack.move_item_at_time({_fmt(p['id'])}, {_fmt(p['track_id'])}, {time:g}, {replace_with_gap}, {_fmt(overlap)}, {_fmt(insert_policy)})"
    return ok, call


@op("split_item")
def op_split_item(state: State, p: dict):
    time = float(p["time"])
    ok = state.stack.split_item_at_time(p["id"], time)
    return ok, f"stack.split_item_at_time({_fmt(p['id'])}, {time:g})"


@op("resize_item")
def op_resize_item(state: State, p: dict):
    start = float(p["start"])
    duration = float(p["duration"])
    overlap = p.get("overlap_policy", "override")
    clamp = bool(p.get("clamp_to_media", False))
    ok = state.stack.resize_item(p["id"], start, duration, overlap, clamp)
    return ok, f"stack.resize_item({_fmt(p['id'])}, {start:g}, {duration:g}, {_fmt(overlap)}, clamp_to_media={clamp})"


@op("modify_item")
def op_modify_item(state: State, p: dict):
    source_start = float(p["source_start"])
    duration = float(p["duration"])
    clamp = bool(p.get("clamp_to_media", True))
    from_start = bool(p.get("resize_from_start", False))
    push = bool(p.get("push_following", False))
    ok = state.stack.modify_item(p["id"], source_start, duration, clamp, from_start, push)
    return ok, (
        f"stack.modify_item({_fmt(p['id'])}, {source_start:g}, {duration:g}, clamp_to_media={clamp}, "
        f"resize_from_start={from_start}, push_following={push})"
    )


@op("replace_item")
def op_replace_item(state: State, p: dict):
    item_spec = p["item"]
    linked_specs = p.get("linked_audio") or None
    linked = [make_item(s) for s in linked_specs] if linked_specs else None
    ok = state.stack.replace_item(p["id"], make_item(item_spec), linked)
    extra = ""
    if linked_specs:
        extra = ", linked_audio_clips=[" + ", ".join(describe_item(s) for s in linked_specs) + "]"
    return ok, f"stack.replace_item({_fmt(p['id'])}, {describe_item(item_spec)}{extra})"


@op("set_item_enabled")
def op_set_item_enabled(state: State, p: dict):
    found = state.stack.get_item(p["id"])
    if found is None:
        return False, f"stack.get_item({_fmt(p['id'])}) -> None"
    _, _, item = found
    item.set_enabled(bool(p["enabled"]))
    ok = state.stack.replace_item(p["id"], item)
    return ok, f"item.set_enabled({bool(p['enabled'])}); stack.replace_item({_fmt(p['id'])}, item)"


@op("link_items")
def op_link_items(state: State, p: dict):
    ids = list(p["ids"])
    return state.stack.link_item(ids), f"stack.link_item({_fmt(ids)})"


@op("unlink_items")
def op_unlink_items(state: State, p: dict):
    ids = list(p["ids"])
    return state.stack.unlink_item(ids), f"stack.unlink_item({_fmt(ids)})"


@op("group_items")
def op_group_items(state: State, p: dict):
    ids = list(p["ids"])
    return state.stack.group_item(ids), f"stack.group_item({_fmt(ids)})"


@op("ungroup_items")
def op_ungroup_items(state: State, p: dict):
    ids = list(p["ids"])
    return state.stack.ungroup_item(ids), f"stack.ungroup_item({_fmt(ids)})"


def _on_timeline(state: State, fn: Callable[[Timeline], Any]) -> Any:
    """Marker methods live on Timeline: run ``fn`` on a timeline wrapping the
    stack, then keep the stack it leaves."""
    timeline = Timeline(state.stack)
    result = fn(timeline)
    state.stack = timeline.get_stack()
    return result


@op("add_marker")
def op_add_marker(state: State, p: dict):
    kwargs = {
        "name": p.get("name") or "",
        "comment": p.get("comment") or "",
        "color": p.get("color") or None,
        "duration": float(p.get("duration") or 0.0),
        "owner_id": p.get("owner_id") or None,
    }
    time = float(p["time"])
    marker_id = _on_timeline(state, lambda tl: tl.add_marker(time, **kwargs))
    if marker_id is None:
        raise ValueError(f"add_marker refused: owner {kwargs['owner_id']!r} not found or is a gap")
    args = ", ".join(f"{k}={_fmt(v)}" for k, v in kwargs.items() if v not in (None, "", 0.0))
    return marker_id, f"timeline.add_marker({_fmt(time)}{', ' + args if args else ''})"


@op("update_marker")
def op_update_marker(state: State, p: dict):
    kwargs = {k: p[k] for k in ("name", "comment", "color") if p.get(k) is not None}
    for k in ("timeline_start", "duration"):
        if p.get(k) is not None:
            kwargs[k] = float(p[k])
    ok = _on_timeline(state, lambda tl: tl.update_marker(p["id"], **kwargs))
    args = ", ".join(f"{k}={_fmt(v)}" for k, v in kwargs.items())
    return ok, f"timeline.update_marker({_fmt(p['id'])}, {args})"


@op("delete_marker")
def op_delete_marker(state: State, p: dict):
    ok = _on_timeline(state, lambda tl: tl.delete_marker(p["id"]))
    return ok, f"timeline.delete_marker({_fmt(p['id'])})"


# -- tellers effects ------------------------------------------------------- #
# target: {"item": id} | {"track": id} | {} (the timeline). Each op is one
# Timeline call on the matching *_tellers_effect* method.


def _effect_call(state: State, p: dict, method: str, *args: Any) -> tuple[Any, str]:
    if p.get("item_id"):
        name, args = f"{method.replace('tellers', 'item_tellers', 1)}", (p["item_id"], *args)
    elif p.get("track_id"):
        name, args = f"{method.replace('tellers', 'track_tellers', 1)}", (p["track_id"], *args)
    else:
        name = method
    result = _on_timeline(state, lambda tl: getattr(tl, name)(*args))
    return result, f"timeline.{name}({', '.join(_fmt(a) for a in args)})"


@op("push_effect")
def op_push_effect(state: State, p: dict):
    effect = {"effect_name": p["effect_name"], "parameters": p.get("parameters") or {}}
    ok, call = _effect_call(state, p, "push_tellers_effect", effect)
    if ok is False:
        raise ValueError(f"push refused: {call}")
    return ok, call


@op("remove_effect")
def op_remove_effect(state: State, p: dict):
    return _effect_call(state, p, "remove_tellers_effect_at", int(p["index"]))


@op("move_effect")
def op_move_effect(state: State, p: dict):
    """Reorder by remove + insert (the library has no move)."""
    src, dst = int(p["index"]), int(p["to"])
    removed, call1 = _effect_call(state, p, "remove_tellers_effect_at", src)
    if removed is None:
        return None, call1
    ok, call2 = _effect_call(state, p, "insert_tellers_effect_at", dst, removed)
    return ok, f"{call1}; {call2}"


@op("set_effect_enabled")
def op_set_effect_enabled(state: State, p: dict):
    return _effect_call(state, p, "set_tellers_effect_enabled", int(p["index"]), bool(p["enabled"]))


@op("set_effect_parameter")
def op_set_effect_parameter(state: State, p: dict):
    return _effect_call(state, p, "set_tellers_effect_parameter", int(p["index"]), p["key"], p.get("value"))


@op("clear_effects")
def op_clear_effects(state: State, p: dict):
    return _effect_call(state, p, "clear_tellers_effects")


# -- speed ------------------------------------------------------------------ #


@op("set_speed")
def op_set_speed(state: State, p: dict):
    """1.0 removes the time warp, 0 is a freeze frame, negative plays backwards."""
    speed = float(p["speed"])
    preserve = p.get("preserve_pitch")
    sync = bool(p.get("with_sync_clips", True))
    n = state.stack.set_item_speed(p["id"], speed, preserve, sync)
    return n, f"stack.set_item_speed({_fmt(p['id'])}, {speed:g}, preserve_pitch={_fmt(preserve)}, with_sync_clips={sync})"


# -- transitions ------------------------------------------------------------ #
# The stack API has no transition insert (insert_item_at_index and
# replace_item refuse a Transition), so these splice Transition.1 into the
# track's children through the timeline JSON and parse it back.


def _make_transition(p: dict, transition: Optional[Transition] = None) -> Transition:
    t = transition or Transition(float(p.get("in_offset", 0.5)), float(p.get("out_offset", 0.5)))
    if "in_offset" in p:
        t.set_in_offset(float(p["in_offset"]))
    if "out_offset" in p:
        t.set_out_offset(float(p["out_offset"]))
    if p.get("kind"):
        t.set_kind(p["kind"])
    if p.get("audio_kind"):
        t.set_audio_kind(p["audio_kind"])
    if "direction" in p:
        t.set_direction(p["direction"] or None)
    if "color" in p:
        t.set_color(p["color"] or None)
    if "name" in p:
        t.set_name(p["name"] or None)
    return t


def _check_kind(t: Transition, track_kind: str, p: dict) -> None:
    # Audio transitions (fade curves) go on audio tracks only, picture ones
    # on any other. Only a kind being written is checked, so an audio track's
    # "auto" transition (any other type) can still be edited.
    if (p.get("kind") or p.get("audio_kind")) and not t.allowed_on(track_kind):
        kind = t.get_audio_kind() or t.get_kind()
        raise ValueError(f"{kind} transitions do not go on {track_kind} tracks")


def _splice_track(state: State, track_index: int, fn: Callable[[list], None]) -> None:
    doc = json.loads(Timeline(state.stack).to_json())
    fn(doc["tracks"]["children"][track_index]["children"])
    state.restore(json.dumps(doc))


@op("add_transition")
def op_add_transition(state: State, p: dict):
    found = state.stack.get_track_by_id(p["track_id"])
    if found is None:
        raise ValueError(f"no track {p['track_id']!r}")
    track_index, track = found
    index = int(p["index"])
    items = track.items()
    if not (0 < index < len(items)):
        raise ValueError(f"a transition goes between two items: index {index} of {len(items)}")
    t = _make_transition(p)
    _check_kind(t, track.kind, p)
    _splice_track(state, track_index, lambda children: children.insert(index, json.loads(t.to_json())))
    args = f"{t.get_in_offset():g}, {t.get_out_offset():g}, {_fmt(t.get_transition_type())}"
    return t.get_id(), f"track.children.insert({index}, Transition({args}))  # via JSON, no stack API"


@op("update_transition")
def op_update_transition(state: State, p: dict):
    found = state.stack.get_item(p["id"])
    if found is None or not found[2].is_transition():
        raise ValueError(f"no transition {p['id']!r}")
    track_index, item_index, item = found
    t = _make_transition(p, item.as_transition())
    _check_kind(t, state.stack.tracks()[track_index].kind, p)

    def replace(children: list) -> None:
        children[item_index] = json.loads(t.to_json())

    _splice_track(state, track_index, replace)
    fields = ", ".join(f"{k}={_fmt(p[k])}" for k in ("kind", "audio_kind", "direction", "color", "in_offset", "out_offset", "name") if k in p)
    return True, f"transition.set_*({fields})  # {_fmt(p['id'])}, via JSON"


@op("get_item", mutates=False)
def op_get_item(state: State, p: dict):
    found = state.stack.get_item(p["id"])
    if found is None:
        return None, f"stack.get_item({_fmt(p['id'])})"
    ti, ii, item = found
    return {"track_index": ti, "item_index": ii, "item": json.loads(str(item))}, f"stack.get_item({_fmt(p['id'])})"


# --------------------------------------------------------------------------- #
# HTTP
# --------------------------------------------------------------------------- #


STATE = State()


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt: str, *args: Any) -> None:  # quieter console
        if os.environ.get("TIMELINE_UI_VERBOSE"):
            super().log_message(fmt, *args)

    def _send(self, status: int, body: bytes, content_type: str) -> None:
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def _send_json(self, status: int, payload: Any) -> None:
        self._send(status, json.dumps(payload).encode("utf-8"), "application/json")

    def do_GET(self) -> None:  # noqa: N802
        if self.path in ("/", "/index.html"):
            with open(os.path.join(HERE, "index.html"), "rb") as fh:
                self._send(200, fh.read(), "text/html; charset=utf-8")
            return
        if self.path == "/api/state":
            with STATE.lock:
                self._send_json(200, STATE.view())
            return
        self._send(404, b"not found", "text/plain")

    def do_POST(self) -> None:  # noqa: N802
        if not self.path.startswith("/api/"):
            self._send(404, b"not found", "text/plain")
            return
        name = self.path[len("/api/") :]
        handler = OPS.get(name)
        if handler is None:
            self._send_json(404, {"error": f"unknown op {name!r}", "ops": sorted(OPS)})
            return
        length = int(self.headers.get("Content-Length") or 0)
        raw = self.rfile.read(length) if length else b"{}"
        try:
            params = json.loads(raw or b"{}")
        except json.JSONDecodeError as exc:
            self._send_json(400, {"error": f"bad JSON body: {exc}"})
            return
        with STATE.lock:
            mutates = getattr(handler, "mutates", True)
            if mutates:
                STATE.push_history()
            try:
                result, call = handler(STATE, params)
            except Exception as exc:  # report library errors to the UI
                if mutates:
                    STATE.restore(STATE.history.pop())
                err = f"{type(exc).__name__}: {exc}"
                traceback.print_exc()
                STATE.log.append({"call": f"{name}({json.dumps(params)[:200]})", "result": None, "error": err})
                self._send_json(400, STATE.view(error=err))
                return
            if call:
                STATE.log.append({"call": call, "result": _jsonable(result), "error": None})
                del STATE.log[:-MAX_LOG]
            self._send_json(200, STATE.view(result=_jsonable(result), call=call))


def _jsonable(value: Any) -> Any:
    try:
        json.dumps(value)
        return value
    except TypeError:
        return str(value)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument(
        "--port",
        type=int,
        default=int(os.environ.get("PORT", os.environ.get("TIMELINE_UI_PORT", "8765"))),
    )
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--no-browser", action="store_true")
    parser.add_argument("--sample", action="store_true", help="start with a sample layout")
    args = parser.parse_args()

    if args.sample:
        STATE.load_sample()

    try:
        server = ThreadingHTTPServer((args.host, args.port), Handler)
    except OSError as exc:
        if exc.errno != 48:  # Address already in use
            raise
        # F5 may be pressed while a prior UI terminal is still running. Use an
        # available local port rather than failing to open the test UI.
        server = ThreadingHTTPServer((args.host, 0), Handler)
        print(f"Port {args.port} is already in use; using port {server.server_address[1]} instead.")
    url = f"http://{args.host}:{server.server_address[1]}/"
    print(f"tellers-timeline test UI: {url}  (Ctrl+C to stop)")
    sys.stdout.flush()
    if not args.no_browser:
        threading.Timer(0.4, lambda: webbrowser.open(url)).start()
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
