#!/usr/bin/env python3
"""Visual test UI for tellers-timeline.

Serves a small browser timeline (index.html) backed by the Python bindings.
Every action in the UI is turned into a real call on a `Stack` held in memory,
so what you see is what the library actually did. No media data is needed:
clips just carry a placeholder URL.

Run:  python tools/timeline-ui/server.py [--port 8765] [--no-browser]
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

from tellers_timeline import Clip, Gap, Item, MediaReference, Stack, Timeline, Track

HERE = os.path.dirname(os.path.abspath(__file__))
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
    if spec.get("text"):
        return MediaReference.create_rich_text_reference(spec.get("text_html") or "<p>Text</p>")
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
        parts.append('{"DEFAULT_MEDIA": MediaReference.create_rich_text_reference(...)}')
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
                    "kind": "clip" if item.is_clip() else "gap",
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
                    entry["name"] = _clip_name(item)
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
                    "items": items,
                }
            )
        try:
            sync_info = self.stack.sync_track_info()
        except Exception as exc:  # pragma: no cover - defensive
            sync_info = [{"error": str(exc)}]
        return {
            "tracks": tracks,
            "timeline": json.loads(timeline.to_json()),
            "validation": timeline.validate(),
            "sync_track_info": sync_info,
            "result": result,
            "call": call,
            "error": error,
            "log": self.log[-MAX_LOG:],
            "can_undo": bool(self.history),
        }

    # -- ops --------------------------------------------------------------- #
    def reset(self) -> None:
        self.stack = Stack()
        self.stack.add_track(Track(kind="video"))

    def load_sample(self) -> None:
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


def _clip_name(item: Item) -> Optional[str]:
    # Item has no get_name; read it from the serialized clip.
    try:
        clip = Clip.parse_json(str(item))
        return clip.get_name()
    except Exception:
        return None


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
    state.reset()
    return True, "Stack(); stack.add_track(Track(kind='video'))"


@op("sample")
def op_sample(state: State, p: dict):
    state.load_sample()
    return True, "load_sample()"


@op("load_json")
def op_load_json(state: State, p: dict):
    state.restore(p["json"] if isinstance(p["json"], str) else json.dumps(p["json"]))
    return True, "Timeline.parse_json(...).get_stack()"


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
    removed = state.stack.delete_item(p["id"], replace_with_gap)
    result = [{"track_index": ti, "item_id": it.get_id(), "duration": it.duration()} for ti, it in removed]
    return result, f"stack.delete_item({_fmt(p['id'])}, replace_with_gap={replace_with_gap})"


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
    parser.add_argument("--port", type=int, default=int(os.environ.get("TIMELINE_UI_PORT", "8765")))
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--no-browser", action="store_true")
    parser.add_argument("--sample", action="store_true", help="start with a sample layout")
    args = parser.parse_args()

    if args.sample:
        STATE.load_sample()

    server = ThreadingHTTPServer((args.host, args.port), Handler)
    url = f"http://{args.host}:{args.port}/"
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
