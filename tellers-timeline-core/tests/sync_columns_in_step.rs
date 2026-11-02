//! An edit never leaves a sync column out of step: its clips start together
//! on every track. Found on a project whose linked audio drifted behind its
//! video after a run of edits.
mod common;
use common::*;
use std::collections::HashMap;

/// Every sync column starts at the same times on each track it sits on.
fn assert_columns_start_together(stack: &Stack) {
    let mut starts: HashMap<i64, HashMap<usize, Vec<f64>>> = HashMap::new();
    for (track_index, track) in stack.children.iter().enumerate() {
        for (item_index, item) in track.items.iter().enumerate() {
            if let Some(id) = sync_clips_id(item) {
                starts
                    .entry(id)
                    .or_default()
                    .entry(track_index)
                    .or_default()
                    .push(track.start_time_of_item(item_index));
            }
        }
    }
    for (id, per_track) in starts {
        let mut lists: Vec<Vec<f64>> = per_track.into_values().collect();
        for list in &mut lists {
            list.sort_by(f64::total_cmp);
        }
        for list in &lists[1..] {
            assert_eq!(list, &lists[0], "sync column {id} is out of step");
        }
    }
}

fn gap(duration: f64) -> Item {
    Item::Gap(Gap::make_gap(duration))
}

/// A video track and its audio track: columns 1 and 2 cut to 2.5 s, an
/// unsynced clip at 7.5 s over a gap in the audio, then columns 3 to 5.
fn stack_with_unsynced_clip_between_columns() -> Stack {
    let mut audio = Track::new(TrackKind::Audio, Some("a".into()));
    let mut video = Track::new(TrackKind::Video, Some("v".into()));
    audio.items = vec![
        synced_clip_item(5.0, "a1", 1),
        synced_clip_item(2.5, "a2", 2),
        gap(5.0),
        synced_clip_item(2.5, "a3", 3),
        synced_clip_item(5.0, "a4", 4),
        synced_clip_item(5.0, "a5", 5),
    ];
    video.items = vec![
        synced_clip_item(5.0, "v1", 1),
        synced_clip_item(2.5, "v2", 2),
        Item::Clip(clip(5.0, Some("free"))),
        synced_clip_item(2.5, "v3", 3),
        synced_clip_item(5.0, "v4", 4),
        synced_clip_item(5.0, "v5", 5),
    ];
    let mut stack = Stack::default();
    stack.children.push(audio);
    stack.children.push(video);
    assert_columns_start_together(&stack);
    stack
}

#[test]
fn resizing_an_unsynced_clip_over_a_column_keeps_the_columns_in_step() {
    let mut stack = stack_with_unsynced_clip_between_columns();

    // Pull the free clip back over column 2. Before the guard this shifted
    // every later audio clip 2.5 s ahead of its video.
    stack.resize_item("free", 5.0, 5.0, OverlapPolicy::Override, false);

    assert_columns_start_together(&stack);
}

#[test]
fn a_refused_edit_leaves_the_stack_unchanged() {
    let mut stack = stack_with_unsynced_clip_between_columns();
    let before = stack.clone();

    if !stack.resize_item("free", 5.0, 5.0, OverlapPolicy::Override, false) {
        assert_eq!(stack, before);
    }
}

#[test]
fn edits_still_work_on_a_project_already_out_of_step() {
    // An older project: column 1's audio already sits 1 s late.
    let mut stack = Stack::default();
    let mut audio = Track::new(TrackKind::Audio, Some("a".into()));
    audio.items = vec![gap(1.0), synced_clip_item(5.0, "a1", 1)];
    let mut video = Track::new(TrackKind::Video, Some("v".into()));
    video.items = vec![
        synced_clip_item(5.0, "v1", 1),
        Item::Clip(clip(3.0, Some("free"))),
    ];
    stack.children.push(audio);
    stack.children.push(video);

    assert!(stack.split_item_at_time("free", 6.0));
}

#[test]
fn splitting_a_column_by_an_insert_gives_the_right_piece_a_fresh_id() {
    // Column 1 on V1/A1, column 2 on V2/A2. A column inserted inside column 1
    // splits it; the right piece used to take id 1 + 1 = 2, joining column 2.
    let mut stack = Stack::default();
    for (kind, name) in [
        (TrackKind::Audio, "A2"),
        (TrackKind::Audio, "A1"),
        (TrackKind::Video, "V1"),
        (TrackKind::Video, "V2"),
    ] {
        let mut track = Track::new(kind, Some(name.into()));
        track.set_id(Some(name.into()));
        stack.children.push(track);
    }
    stack.children[1].items = vec![synced_clip_item(4.0, "c1-a", 1)];
    stack.children[2].items = vec![synced_clip_item(4.0, "c1", 1)];
    stack.children[0].items = vec![synced_clip_item(4.0, "c2-a", 2)];
    stack.children[3].items = vec![synced_clip_item(4.0, "c2", 2)];

    let inserted = stack.insert_item_at_time_by_id(
        "V1",
        1.0,
        Item::Clip(clip(1.0, Some("new"))),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        Some(vec![audio_clip(1.0, "file:///new.wav", None)]),
        None,
    );

    assert!(inserted.is_some());
    assert_columns_start_together(&stack);
    let (_, _, c2) = stack.get_item("c2").unwrap();
    let column_2: Vec<_> = stack
        .children
        .iter()
        .flat_map(|track| &track.items)
        .filter(|item| sync_clips_id(item) == sync_clips_id(c2))
        .collect();
    assert_eq!(column_2.len(), 2, "only c2 and its audio share its id");
}

fn start_of(stack: &Stack, id: &str) -> f64 {
    let (track_index, item_index, _) = stack.get_item(id).unwrap();
    stack.children[track_index].start_time_of_item(item_index)
}

/// A video track owning an audio track, with the given (id, duration, sync
/// id) items on each; `None` ids are gaps.
fn video_and_audio(
    video: &[(&str, f64, Option<i64>)],
    audio: &[(&str, f64, Option<i64>)],
) -> Stack {
    let build = |kind, name: &str, items: &[(&str, f64, Option<i64>)]| {
        let mut track = Track::new(kind, Some(name.into()));
        track.items = items
            .iter()
            .map(|(id, duration, sync)| match (id.is_empty(), sync) {
                (true, _) => gap(*duration),
                (false, Some(sync)) => synced_clip_item(*duration, id, *sync),
                (false, None) => Item::Clip(clip(*duration, Some(id))),
            })
            .collect();
        track
    };
    let mut stack = Stack::default();
    stack.children.push(build(TrackKind::Audio, "a", audio));
    stack.children.push(build(TrackKind::Video, "v", video));
    assert_columns_start_together(&stack);
    stack
}

#[test]
fn resizing_a_column_to_start_past_its_track_end_moves_every_channel_there() {
    // The video track runs on after the column; its audio track ends with it.
    let mut stack = video_and_audio(
        &[("v1", 5.0, Some(1)), ("free", 5.0, None)],
        &[("a1", 5.0, Some(1))],
    );

    assert!(stack.resize_item("v1", 15.0, 2.0, OverlapPolicy::Override, false));

    assert_eq!(start_of(&stack, "v1"), 15.0);
    assert_eq!(start_of(&stack, "a1"), 15.0);
    assert_eq!(start_of(&stack, "free"), 5.0);
}

#[test]
fn shortening_a_clip_with_push_pulls_each_later_column_back_once() {
    // Free clips around two columns on the video track; the audio track has
    // only the columns. Shrinking the second free clip with push pulls both
    // columns back by the same 4 s on both tracks; the second column's audio
    // used to be pulled once per column (8 s).
    let mut stack = video_and_audio(
        &[
            ("f1", 5.0, None),
            ("f2", 5.0, None),
            ("f3", 5.0, None),
            ("v1", 5.0, Some(1)),
            ("f4", 5.0, None),
            ("f5", 5.0, None),
            ("v2", 5.0, Some(2)),
            ("f6", 5.0, None),
        ],
        &[
            ("", 15.0, None),
            ("a1", 5.0, Some(1)),
            ("", 10.0, None),
            ("a2", 5.0, Some(2)),
        ],
    );

    assert!(stack.modify_item("f2", 0.0, 1.0, true, true, true));

    assert_eq!(start_of(&stack, "v1"), 11.0);
    assert_eq!(start_of(&stack, "a1"), 11.0);
    assert_eq!(start_of(&stack, "v2"), 26.0);
    assert_eq!(start_of(&stack, "a2"), 26.0);
}

#[test]
fn a_head_trim_that_shortens_more_than_it_slides_keeps_what_follows_in_place() {
    // Slide the head by 1 s but keep only 2 s: the last 2 s of the old span
    // stay as a gap, so the column after it doesn't move.
    let mut stack = video_and_audio(
        &[("free", 5.0, None), ("v1", 5.0, Some(1))],
        &[("", 5.0, None), ("a1", 5.0, Some(1))],
    );

    assert!(stack.modify_item("free", 1.0, 2.0, true, true, false));

    assert_eq!(start_of(&stack, "free"), 1.0);
    assert_eq!(start_of(&stack, "v1"), 5.0);
    assert_eq!(start_of(&stack, "a1"), 5.0);
}
