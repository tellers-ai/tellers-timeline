use tellers_timeline_core::{
    Clip, ClampPolicy, Gap, IdMetadataExt, Item, MediaReference, OverlapPolicy, RationalTime,
    Seconds, Stack, TimeRange, Track, TrackKind,
};

// ── helpers ──────────────────────────────────────────────────────────────────

fn range(duration: f64) -> TimeRange {
    TimeRange {
        otio_schema: "TimeRange.1".to_string(),
        start_time: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: 1.0,
            value: 0.0,
        },
        duration: RationalTime {
            otio_schema: "RationalTime.1".to_string(),
            rate: 1.0,
            value: duration,
        },
    }
}

fn make_clip(id: &str, duration: f64) -> Item {
    let mut refs = std::collections::HashMap::new();
    refs.insert(
        "DEFAULT_MEDIA".to_string(),
        MediaReference::ExternalReference {
            target_url: "mem://".to_string(),
            available_range: None,
            name: None,
            available_image_bounds: None,
            metadata: serde_json::Value::Null,
        },
    );
    let mut item = Item::Clip(Clip {
        otio_schema: "Clip.2".to_string(),
        enabled: true,
        name: None,
        source_range: range(duration),
        media_references: refs,
        active_media_reference_key: Some("DEFAULT_MEDIA".to_string()),
        metadata: serde_json::Value::Null,
        effects: Vec::new(),
    });
    item.set_id(Some(id.to_string()));
    item
}

fn make_gap(duration: f64) -> Item {
    Item::Gap(Gap::make_gap(duration))
}

fn synced_clip(id: &str, duration: f64, sync_id: i64) -> Item {
    let mut item = make_clip(id, duration);
    if let Item::Clip(clip) = &mut item {
        // Merge sync id into existing metadata so the item id set by make_clip is preserved.
        clip.metadata["Resolve_OTIO"] = serde_json::json!({ "Link Group ID": sync_id });
    }
    item
}

/// Returns the (start, duration) of the item with the given id.
fn item_range(stack: &Stack, id: &str) -> (Seconds, Seconds) {
    let (ti, ii, item) = stack.get_item(id).expect("item not found");
    let start = stack.children[ti].start_time_of_item(ii);
    (start, item.duration())
}

/// Returns true if every item at the given track index is either all-clips or
/// the track exactly matches the provided (is_gap, duration) sequence.
fn track_layout(stack: &Stack, track_index: usize) -> Vec<(bool, Seconds)> {
    stack.children[track_index]
        .items
        .iter()
        .map(|i| (matches!(i, Item::Gap(_)), i.duration()))
        .collect()
}

// ── set_item_start_time ───────────────────────────────────────────────────────

// [A:5][B:5]  →  move B start from 5 to 7 (prev is clip A, ReplaceGap)
// Expected: [A:5][gap:2][B:3]
#[test]
fn set_start_replace_gap_creates_gap_when_prev_is_clip() {
    let mut track = Track { kind: TrackKind::Video, ..Track::default() };
    track.items.push(make_clip("a", 5.0));
    track.items.push(make_clip("b", 5.0));
    let mut stack = Stack { children: vec![track], ..Stack::default() };

    let ok = stack.set_item_start_time("b", 7.0, OverlapPolicy::Override, ClampPolicy::ReplaceGap);
    assert!(ok);

    let (start, dur) = item_range(&stack, "b");
    assert!((start - 7.0).abs() < 1e-9, "b should start at 7, got {start}");
    assert!((dur - 3.0).abs() < 1e-9, "b duration should be 3 (right edge fixed), got {dur}");

    let layout = track_layout(&stack, 0);
    // [A:5][gap:2][B:3]
    assert_eq!(layout.len(), 3);
    assert!(!layout[0].0); // A is clip
    assert!(layout[1].0);  // gap
    assert!((layout[1].1 - 2.0).abs() < 1e-9, "gap should be 2, got {}", layout[1].1);
    assert!(!layout[2].0); // B is clip
}

// [A:5][B:5]  →  move B start from 5 to 7 (prev is clip A, ClampByPulling)
// ClampByPulling: B cannot move away from A → clamped to A's end (5.0), no change.
#[test]
fn set_start_clamp_by_pulling_stops_at_prev_clip() {
    let mut track = Track { kind: TrackKind::Video, ..Track::default() };
    track.items.push(make_clip("a", 5.0));
    track.items.push(make_clip("b", 5.0));
    let mut stack = Stack { children: vec![track], ..Stack::default() };

    let ok = stack.set_item_start_time("b", 8.0, OverlapPolicy::Override, ClampPolicy::ClampByPulling);
    assert!(ok);

    let (start, dur) = item_range(&stack, "b");
    // B must not move away from A — stays flush at 5.0
    assert!((start - 5.0).abs() < 1e-9, "b should be clamped at 5.0, got {start}");
    assert!((dur - 5.0).abs() < 1e-9, "duration unchanged, got {dur}");

    // No gap between A and B
    let layout = track_layout(&stack, 0);
    assert_eq!(layout.len(), 2, "no gap should be inserted");
}

// [A:5][gap:3][B:5]  →  move B start from 8 to 10 (prev is gap, ClampByPulling)
// Prev is a gap → clamping does NOT apply; gap just grows.
#[test]
fn set_start_clamp_by_pulling_no_effect_when_prev_is_gap() {
    let mut track = Track { kind: TrackKind::Video, ..Track::default() };
    track.items.push(make_clip("a", 5.0));
    track.items.push(make_gap(3.0));
    track.items.push(make_clip("b", 5.0));
    let mut stack = Stack { children: vec![track], ..Stack::default() };

    // B starts at 8, move to 10
    let ok = stack.set_item_start_time("b", 10.0, OverlapPolicy::Override, ClampPolicy::ClampByPulling);
    assert!(ok);

    let (start, dur) = item_range(&stack, "b");
    assert!((start - 10.0).abs() < 1e-9, "b should move to 10, got {start}");
    assert!((dur - 3.0).abs() < 1e-9, "duration = 13 - 10 = 3, got {dur}");
}

// [A:5][gap:3][B:5]  →  move B start from 8 to 6 (moving LEFT, growing into gap)
// ClampByPulling only applies when moving right → left move is always free.
#[test]
fn set_start_moving_left_always_free() {
    let mut track = Track { kind: TrackKind::Video, ..Track::default() };
    track.items.push(make_clip("a", 5.0));
    track.items.push(make_gap(3.0));
    track.items.push(make_clip("b", 5.0));
    let mut stack = Stack { children: vec![track], ..Stack::default() };

    let ok = stack.set_item_start_time("b", 6.0, OverlapPolicy::Override, ClampPolicy::ClampByPulling);
    assert!(ok);

    let (start, dur) = item_range(&stack, "b");
    assert!((start - 6.0).abs() < 1e-9, "b should start at 6, got {start}");
    assert!((dur - 7.0).abs() < 1e-9, "right edge stays at 13, duration = 7, got {dur}");
}

// Right edge is always fixed: moving start changes duration, not end position.
#[test]
fn set_start_right_edge_stays_fixed() {
    let mut track = Track { kind: TrackKind::Video, ..Track::default() };
    track.items.push(make_gap(2.0));
    track.items.push(make_clip("c", 8.0)); // occupies [2, 10]
    let mut stack = Stack { children: vec![track], ..Stack::default() };

    let ok = stack.set_item_start_time("c", 5.0, OverlapPolicy::Override, ClampPolicy::ReplaceGap);
    assert!(ok);

    let (start, dur) = item_range(&stack, "c");
    assert!((start - 5.0).abs() < 1e-9, "start should be 5, got {start}");
    // right edge was 10, so duration = 10 - 5 = 5
    assert!((dur - 5.0).abs() < 1e-9, "duration should be 5, got {dur}");
}

// ── set_item_duration ─────────────────────────────────────────────────────────

// [A:5][B:5]  →  shrink A to 3 (next is clip B, ReplaceGap)
// Expected: [A:3][gap:2][B:5]
#[test]
fn set_duration_replace_gap_creates_gap_when_next_is_clip() {
    let mut track = Track { kind: TrackKind::Video, ..Track::default() };
    track.items.push(make_clip("a", 5.0));
    track.items.push(make_clip("b", 5.0));
    let mut stack = Stack { children: vec![track], ..Stack::default() };

    let ok = stack.set_item_duration("a", 3.0, OverlapPolicy::Override, ClampPolicy::ReplaceGap);
    assert!(ok);

    let (start, dur) = item_range(&stack, "a");
    assert!((start - 0.0).abs() < 1e-9, "a should still start at 0");
    assert!((dur - 3.0).abs() < 1e-9, "a duration should be 3, got {dur}");

    let layout = track_layout(&stack, 0);
    // [A:3][gap:2][B:5]
    assert_eq!(layout.len(), 3);
    assert!(!layout[0].0); // A clip
    assert!(layout[1].0);  // gap
    assert!((layout[1].1 - 2.0).abs() < 1e-9, "gap should be 2, got {}", layout[1].1);
    assert!(!layout[2].0); // B clip
}

// [A:5][B:5]  →  shrink A to 3 (next is clip B, ClampByPulling)
// ClampByPulling: A cannot move away from B → clamped, no change.
#[test]
fn set_duration_clamp_by_pulling_stops_at_next_clip() {
    let mut track = Track { kind: TrackKind::Video, ..Track::default() };
    track.items.push(make_clip("a", 5.0));
    track.items.push(make_clip("b", 5.0));
    let mut stack = Stack { children: vec![track], ..Stack::default() };

    let ok = stack.set_item_duration("a", 2.0, OverlapPolicy::Override, ClampPolicy::ClampByPulling);
    assert!(ok);

    let (_, dur) = item_range(&stack, "a");
    // A must not shrink away from B — stays at 5.0
    assert!((dur - 5.0).abs() < 1e-9, "a should be clamped at 5.0, got {dur}");

    let layout = track_layout(&stack, 0);
    assert_eq!(layout.len(), 2, "no gap should be inserted");
}

// [A:5][gap:3][B:5]  →  shrink A to 3 (next is gap, ClampByPulling)
// Prev is a gap → clamping does NOT apply; gap just grows.
#[test]
fn set_duration_clamp_by_pulling_no_effect_when_next_is_gap() {
    let mut track = Track { kind: TrackKind::Video, ..Track::default() };
    track.items.push(make_clip("a", 5.0));
    track.items.push(make_gap(3.0));
    track.items.push(make_clip("b", 5.0));
    let mut stack = Stack { children: vec![track], ..Stack::default() };

    let ok = stack.set_item_duration("a", 3.0, OverlapPolicy::Override, ClampPolicy::ClampByPulling);
    assert!(ok);

    let (_, dur) = item_range(&stack, "a");
    assert!((dur - 3.0).abs() < 1e-9, "a should shrink to 3, got {dur}");
    // Gap grows from 3 to 5
    let layout = track_layout(&stack, 0);
    assert!(layout[1].0, "index 1 should be a gap");
    assert!((layout[1].1 - 5.0).abs() < 1e-9, "gap should be 5, got {}", layout[1].1);
}

// Growing duration always free regardless of ClampByPulling.
#[test]
fn set_duration_growing_always_free() {
    let mut track = Track { kind: TrackKind::Video, ..Track::default() };
    track.items.push(make_clip("a", 5.0));
    track.items.push(make_gap(5.0));
    let mut stack = Stack { children: vec![track], ..Stack::default() };

    let ok = stack.set_item_duration("a", 8.0, OverlapPolicy::Override, ClampPolicy::ClampByPulling);
    assert!(ok);

    let (_, dur) = item_range(&stack, "a");
    assert!((dur - 8.0).abs() < 1e-9, "a should grow to 8, got {dur}");
}

// Left edge is always fixed: only the right edge moves.
#[test]
fn set_duration_left_edge_stays_fixed() {
    let mut track = Track { kind: TrackKind::Video, ..Track::default() };
    track.items.push(make_gap(3.0));
    track.items.push(make_clip("c", 6.0)); // starts at 3
    track.items.push(make_gap(5.0));
    let mut stack = Stack { children: vec![track], ..Stack::default() };

    let ok = stack.set_item_duration("c", 4.0, OverlapPolicy::Override, ClampPolicy::ReplaceGap);
    assert!(ok);

    let (start, dur) = item_range(&stack, "c");
    assert!((start - 3.0).abs() < 1e-9, "left edge should stay at 3, got {start}");
    assert!((dur - 4.0).abs() < 1e-9, "duration should be 4, got {dur}");
}

// ── gap items ────────────────────────────────────────────────────────────────

// [clip:5][gap:5][clip:5] — shrink gap to 2 via set_item_duration
#[test]
fn set_duration_works_on_gap() {
    let mut track = Track { kind: TrackKind::Video, ..Track::default() };
    track.items.push(make_clip("a", 5.0));
    let mut gap = make_gap(5.0);
    gap.set_id(Some("g".to_string()));
    track.items.push(gap);
    track.items.push(make_clip("b", 5.0));
    let mut stack = Stack { children: vec![track], ..Stack::default() };

    let ok = stack.set_item_duration("g", 2.0, OverlapPolicy::Override, ClampPolicy::ReplaceGap);
    assert!(ok);

    let (_, dur) = item_range(&stack, "g");
    assert!((dur - 2.0).abs() < 1e-9, "gap duration should be 2, got {dur}");
    // b shifts left
    let (b_start, _) = item_range(&stack, "b");
    assert!((b_start - 7.0).abs() < 1e-9, "b should start at 7, got {b_start}");
}

// [clip:5][gap:5][clip:5] — move gap start right via set_item_start_time
#[test]
fn set_start_works_on_gap() {
    let mut track = Track { kind: TrackKind::Video, ..Track::default() };
    track.items.push(make_clip("a", 5.0));
    let mut gap = make_gap(5.0); // occupies [5, 10]
    gap.set_id(Some("g".to_string()));
    track.items.push(gap);
    track.items.push(make_clip("b", 5.0));
    let mut stack = Stack { children: vec![track], ..Stack::default() };

    // Move left edge from 5 to 7 — gap shrinks to 3 (right edge stays at 10)
    let ok = stack.set_item_start_time("g", 7.0, OverlapPolicy::Override, ClampPolicy::ReplaceGap);
    assert!(ok);

    // Gap shrinks in place — no compensating gap is inserted on the left (unlike clips).
    // Total duration decreases by the 2 units trimmed from the gap's left edge.
    let (b_start, _) = item_range(&stack, "b");
    assert!((b_start - 8.0).abs() < 1e-9, "b should start at 8 after gap shrinks, got {b_start}");
    let total: Seconds = stack.children[0].items.iter().map(|i| i.duration().max(0.0)).sum();
    assert!((total - 13.0).abs() < 1e-9, "total duration should be 13, got {total}");
}

// ── synced clips ──────────────────────────────────────────────────────────────

// Video [V:10] / Audio [A:10] (same sync group)
// set_item_start_time on V → A should resize by the same delta.
#[test]
fn set_start_propagates_to_synced_clips() {
    let sync_id = 42_i64;
    let mut video_track = Track { kind: TrackKind::Video, ..Track::default() };
    video_track.items.push(synced_clip("v", 10.0, sync_id));

    let mut audio_track = Track { kind: TrackKind::Other, ..Track::default() };
    audio_track.items.push(synced_clip("a", 10.0, sync_id));

    let mut stack = Stack {
        children: vec![video_track, audio_track],
        ..Stack::default()
    };

    // Move V's left edge from 0 to 3 (ReplaceGap — gap forms on both tracks).
    let ok = stack.set_item_start_time("v", 3.0, OverlapPolicy::Override, ClampPolicy::ReplaceGap);
    assert!(ok);

    let (v_start, v_dur) = item_range(&stack, "v");
    let (a_start, a_dur) = item_range(&stack, "a");

    assert!((v_start - 3.0).abs() < 1e-9, "v start should be 3, got {v_start}");
    assert!((v_dur - 7.0).abs() < 1e-9, "v duration should be 7, got {v_dur}");
    assert!((a_start - 3.0).abs() < 1e-9, "a should move in sync, got {a_start}");
    assert!((a_dur - 7.0).abs() < 1e-9, "a duration should match, got {a_dur}");
}

// Video [V:10] / Audio [A:10] (same sync group)
// set_item_duration on V → A should shrink by the same amount.
#[test]
fn set_duration_propagates_to_synced_clips() {
    let sync_id = 99_i64;
    let mut video_track = Track { kind: TrackKind::Video, ..Track::default() };
    video_track.items.push(synced_clip("v", 10.0, sync_id));

    let mut audio_track = Track { kind: TrackKind::Other, ..Track::default() };
    audio_track.items.push(synced_clip("a", 10.0, sync_id));

    let mut stack = Stack {
        children: vec![video_track, audio_track],
        ..Stack::default()
    };

    let ok = stack.set_item_duration("v", 6.0, OverlapPolicy::Override, ClampPolicy::ReplaceGap);
    assert!(ok);

    let (_, v_dur) = item_range(&stack, "v");
    let (_, a_dur) = item_range(&stack, "a");

    assert!((v_dur - 6.0).abs() < 1e-9, "v duration should be 6, got {v_dur}");
    assert!((a_dur - 6.0).abs() < 1e-9, "a duration should match, got {a_dur}");
}
