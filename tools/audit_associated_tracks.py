#!/usr/bin/env python3
"""Audit project timelines against the associated-track rules.

Reads project documents (one JSON per line: {"id": ..., "updated_at": ...,
"data": <OTIO timeline>}, or a directory of *.otio / *.json files) and reports,
per project and in total, what the new rules would see:

- groups derived from the sync clips (Link Group IDs), and whether a track
  would be claimed by two owners (exclusivity conflict, auto-resolved by
  keeping the owner sharing the most columns);
- plain audio clips (no sync partner) sitting on a video group's audio track
  (allowed to stay; new inserts there are now refused);
- audio-only sync columns on a video group's audio track (mixing);
- whether `normalize_track_order` would reorder the tracks (groups not
  contiguous / owner not on top), i.e. what the host sees after one
  normalization;
- video and audio tracks interleaved (informational).

Usage:
    python tools/audit_associated_tracks.py projects.jsonl
    python tools/audit_associated_tracks.py some/dir/with/otio/files
    python tools/audit_associated_tracks.py projects.jsonl --details 20

Requires the Python bindings (`maturin develop` in bindings/python).
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path
from typing import Any, Iterable

from tellers_timeline import Timeline


def iter_documents(source: Path) -> Iterable[tuple[str, dict[str, Any]]]:
    if source.is_dir():
        for path in sorted(source.rglob("*")):
            if path.suffix in {".otio", ".json"}:
                yield path.name, json.loads(path.read_text())
        return
    with source.open() as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            data = row.get("data", row)
            if isinstance(data, str):
                data = json.loads(data)
            yield str(row.get("id", "?")), data


def tracks_of(data: dict[str, Any]) -> list[dict[str, Any]]:
    tracks = data.get("tracks") or {}
    return list(tracks.get("children") or [])


def sync_id_of(item: dict[str, Any]) -> int | None:
    meta = item.get("metadata") or {}
    for key in ("Resolve_OTIO", "resolve"):
        value = (meta.get(key) or {}).get("Link Group ID")
        if value is not None:
            try:
                return int(value)
            except (TypeError, ValueError):
                return None
    return None


def audit_one(data: dict[str, Any]) -> dict[str, Any]:
    tracks = tracks_of(data)
    kinds = [str(t.get("kind", "")).lower() for t in tracks]
    report: dict[str, Any] = {
        "tracks": len(tracks),
        "video_tracks": kinds.count("video"),
        "audio_tracks": kinds.count("audio"),
        "sync_columns": 0,
        "shared_track_conflicts": 0,
        "plain_audio_on_video_group_track": 0,
        "audio_only_columns_on_video_group_track": 0,
        "interleaved": False,
        "normalize_reorders": False,
        "stored_lists": 0,
        "parse_error": None,
    }
    # Interleaving: an audio track above (after) a video track in the list.
    first_video = next((i for i, k in enumerate(kinds) if k == "video"), None)
    if first_video is not None:
        report["interleaved"] = any(k == "audio" for k in kinds[first_video + 1 :])

    # Sync groups straight from the JSON.
    members: dict[int, set[int]] = {}
    for index, track in enumerate(tracks):
        for item in track.get("children") or []:
            sync_id = sync_id_of(item)
            if sync_id is not None:
                members.setdefault(sync_id, set()).add(index)
    columns = {sid: m for sid, m in members.items() if len(m) > 1}
    report["sync_columns"] = len(columns)
    owners: dict[int, Counter[int]] = {}
    for sid, member_set in columns.items():
        videos = [i for i in member_set if kinds[i] == "video"]
        owner = max(videos) if videos else max(member_set)
        for partner in member_set:
            if partner != owner:
                owners.setdefault(partner, Counter())[owner] += 1
    report["shared_track_conflicts"] = sum(1 for c in owners.values() if len(c) > 1)
    partner_owner = {p: c.most_common(1)[0][0] for p, c in owners.items()}
    video_group_audio = {p for p, o in partner_owner.items() if kinds[o] == "video" and kinds[p] == "audio"}

    for index in video_group_audio:
        for item in tracks[index].get("children") or []:
            if item.get("OTIO_SCHEMA", "").startswith("Gap"):
                continue
            sid = sync_id_of(item)
            if sid is None or sid not in columns:
                report["plain_audio_on_video_group_track"] += 1
            elif all(kinds[m] == "audio" for m in columns[sid]):
                report["audio_only_columns_on_video_group_track"] += 1

    try:
        timeline = Timeline.parse_json(json.dumps(data))
        stack = timeline.get_stack()
        report["stored_lists"] = sum(1 for t in stack.tracks() if t.get_associated_track_ids())
        report["normalize_reorders"] = stack.normalize_track_order()
    except Exception as error:  # noqa: BLE001 - report, don't stop
        report["parse_error"] = str(error)[:200]
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("source", type=Path, help="JSONL file or directory of OTIO files")
    parser.add_argument("--details", type=int, default=0, help="print the first N projects needing attention")
    args = parser.parse_args()

    totals: Counter[str] = Counter()
    attention: list[tuple[str, dict[str, Any]]] = []
    count = 0
    for project_id, data in iter_documents(args.source):
        count += 1
        report = audit_one(data)
        totals["projects"] += 1
        totals["with_sync_columns"] += report["sync_columns"] > 0
        totals["with_shared_track_conflicts"] += report["shared_track_conflicts"] > 0
        totals["with_plain_audio_on_video_group_track"] += report["plain_audio_on_video_group_track"] > 0
        totals["with_audio_only_columns_on_video_group_track"] += (
            report["audio_only_columns_on_video_group_track"] > 0
        )
        totals["interleaved"] += report["interleaved"]
        totals["normalize_would_reorder"] += report["normalize_reorders"]
        totals["with_stored_lists"] += report["stored_lists"] > 0
        totals["parse_errors"] += report["parse_error"] is not None
        if (
            report["shared_track_conflicts"]
            or report["audio_only_columns_on_video_group_track"]
            or report["parse_error"]
        ):
            attention.append((project_id, report))

    print(f"projects scanned: {count}")
    for key, value in sorted(totals.items()):
        print(f"  {key}: {value}")
    print()
    print("auto-repair verdict:")
    print("  - normalize_would_reorder: fixed by one normalize_track_order() call (no data loss).")
    print("  - with_plain_audio_on_video_group_track: existing clips stay; only new inserts there are refused.")
    print("  - with_shared_track_conflicts: resolved by exclusivity (owner with most columns wins); check details.")
    print("  - with_audio_only_columns_on_video_group_track: NOT auto-repaired; needs a migration or manual fix.")
    print("  - parse_errors: documents the library cannot read; inspect.")
    if args.details:
        print()
        print(f"first {min(args.details, len(attention))} projects needing attention:")
        for project_id, report in attention[: args.details]:
            print(f"  {project_id}: {json.dumps(report)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
