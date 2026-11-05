//! A split edit (J / L cut): `modify_item` with `trim_sync_partners = false`
//! trims one clip of a sync column and keeps it synced, its start's move
//! recorded as its Sync Offset.
mod common;
use common::*;
use tellers_timeline_core::metadata::resolve_sync_offset;

/// Audio track 0, video track 1. Columns 1 (0-5 s) and 2 (5-10 s), every
/// clip 5 s from source 2 s, so heads can be pulled back 2 s.
fn two_columns() -> Stack {
    let mut audio = Track::new(TrackKind::Audio, Some("a".into()));
    audio.items = vec![
        synced_clip_item_with_source_start(5.0, 2.0, "a1", 1),
        synced_clip_item_with_source_start(5.0, 2.0, "a2", 2),
    ];
    let mut video = Track::new(TrackKind::Video, Some("v".into()));
    video.items = vec![
        synced_clip_item_with_source_start(5.0, 2.0, "v1", 1),
        synced_clip_item_with_source_start(5.0, 2.0, "v2", 2),
    ];
    let mut stack = Stack::default();
    stack.children.push(audio);
    stack.children.push(video);
    stack
}

/// (start, duration, source start, sync id, sync offset) of `id`.
fn state(stack: &Stack, id: &str) -> (f64, f64, f64, Option<i64>, f64) {
    let (ti, ii, item) = stack.get_item(id).expect(id);
    let offset = match item {
        Item::Clip(clip) => resolve_sync_offset(&clip.metadata),
        _ => 0.0,
    };
    (
        stack.children[ti].start_time_of_item(ii),
        item.duration(),
        source_start(item),
        sync_clips_id(item),
        offset,
    )
}

/// Pull a2's head 1 s before v2: a J cut.
fn j_cut(stack: &mut Stack) -> bool {
    stack.modify_item("a2", 1.0, 6.0, false, true, false, false)
}

#[test]
fn a_j_cut_trims_only_the_audio_and_keeps_it_synced() {
    let mut stack = two_columns();

    assert!(j_cut(&mut stack));

    assert_eq!(state(&stack, "a2"), (4.0, 6.0, 1.0, Some(2), -1.0));
    // The video is untouched; the audio before it gave up its last second.
    assert_eq!(state(&stack, "v2"), (5.0, 5.0, 2.0, Some(2), 0.0));
    assert_eq!(state(&stack, "a1"), (0.0, 4.0, 2.0, Some(1), 0.0));
    assert_eq!(state(&stack, "v1"), (0.0, 5.0, 2.0, Some(1), 0.0));
}

#[test]
fn trimming_with_partners_still_moves_the_whole_column() {
    let mut stack = two_columns();

    assert!(stack.modify_item("a2", 1.0, 6.0, false, true, false, true));

    assert_eq!(state(&stack, "a2").0, 4.0);
    assert_eq!(state(&stack, "v2").0, 4.0);
    assert_eq!(state(&stack, "a2").4, 0.0);
}

#[test]
fn trimming_the_audio_back_to_the_video_clears_the_offset() {
    let mut stack = two_columns();
    assert!(j_cut(&mut stack));

    assert!(stack.modify_item("a2", 2.0, 5.0, false, true, false, false));

    assert_eq!(state(&stack, "a2"), (5.0, 5.0, 2.0, Some(2), 0.0));
}

#[test]
fn an_l_cut_into_free_space_needs_no_offset() {
    let mut stack = two_columns();
    stack.children[0].items.pop();
    stack.children[1].items.pop();

    // a1's tail runs 1 s past v1.
    assert!(stack.modify_item("a1", 2.0, 6.0, false, false, false, false));

    assert_eq!(state(&stack, "a1"), (0.0, 6.0, 2.0, Some(1), 0.0));
    assert_eq!(state(&stack, "v1"), (0.0, 5.0, 2.0, Some(1), 0.0));
}

#[test]
fn an_l_cut_over_the_next_audio_trims_only_its_head() {
    // a1's tail runs 1 s under v2: a2 gives up its first second, on the
    // audio track only, and stays in sync with v2.
    let mut stack = two_columns();

    assert!(stack.modify_item("a1", 2.0, 6.0, false, false, false, false));

    assert_eq!(state(&stack, "a1"), (0.0, 6.0, 2.0, Some(1), 0.0));
    assert_eq!(state(&stack, "a2"), (6.0, 4.0, 3.0, Some(2), 1.0));
    assert_eq!(state(&stack, "v1"), (0.0, 5.0, 2.0, Some(1), 0.0));
    assert_eq!(state(&stack, "v2"), (5.0, 5.0, 2.0, Some(2), 0.0));
}

#[test]
fn an_l_cut_after_moving_the_next_audio_head_works() {
    let mut stack = two_columns();
    // a2 starts 1 s after v2, then a1 runs into the space.
    assert!(stack.modify_item("a2", 3.0, 4.0, false, true, false, false));
    assert_eq!(state(&stack, "a2"), (6.0, 4.0, 3.0, Some(2), 1.0));

    assert!(stack.modify_item("a1", 2.0, 6.0, false, false, false, false));

    assert_eq!(state(&stack, "a1"), (0.0, 6.0, 2.0, Some(1), 0.0));
    assert_eq!(state(&stack, "v2").0, 5.0);
}

#[test]
fn a_rippling_split_edit_that_desyncs_later_columns_is_refused() {
    let mut stack = two_columns();
    let before = stack.clone();

    // Extending a1 with push would shift a2 alone, 1 s after v2.
    assert!(!stack.modify_item("a1", 2.0, 6.0, false, false, true, false));
    assert_eq!(stack, before);
}

#[test]
fn splitting_a_j_cut_column_keeps_the_lead_in_on_the_left_piece() {
    let mut stack = two_columns();
    assert!(j_cut(&mut stack));

    assert!(stack.split_item_at_time("v2", 7.0));

    assert_eq!(state(&stack, "a2"), (4.0, 3.0, 1.0, Some(2), -1.0));
    let right: Vec<_> = stack
        .children
        .iter()
        .map(|track| {
            let index = track.get_item_at_time(7.5).unwrap();
            let item = &track.items[index];
            let offset = match item {
                Item::Clip(clip) => resolve_sync_offset(&clip.metadata),
                _ => f64::NAN,
            };
            (track.start_time_of_item(index), sync_clips_id(item), offset)
        })
        .collect();
    assert_eq!(right[0].0, 7.0);
    assert_eq!(right[0], right[1]);
    assert_ne!(right[0].1, Some(2));
    assert_eq!(right[0].2, 0.0);
}

#[test]
fn a_column_trim_after_a_j_cut_keeps_the_lead_in() {
    let mut stack = two_columns();
    assert!(j_cut(&mut stack));

    // Trim the whole column's head 1 s later from the video.
    assert!(stack.modify_item("v2", 3.0, 4.0, false, true, false, true));

    assert_eq!(state(&stack, "v2").0, 6.0);
    assert_eq!(state(&stack, "a2").0, 5.0);
    assert_eq!(state(&stack, "a2").4, -1.0);
}

#[test]
fn a_split_edit_on_an_unsynced_clip_is_an_ordinary_trim() {
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("v".into()));
    video.items = vec![Item::Clip(clip(5.0, Some("free")))];
    stack.children.push(video);

    assert!(stack.modify_item("free", 0.0, 3.0, false, false, false, false));
    assert_eq!(state(&stack, "free"), (0.0, 3.0, 0.0, None, 0.0));
}

/// [`two_columns`] with track ids "a" and "v", and room after column 2.
fn two_columns_with_ids() -> Stack {
    let mut stack = two_columns();
    stack.children[0].set_id(Some("a".into()));
    stack.children[1].set_id(Some("v".into()));
    stack
}

/// Where each clip of column `sync_id` starts, by name of its partner role.
fn column(stack: &Stack, audio: &str, video: &str) -> (f64, f64, Option<i64>, Option<i64>) {
    let (a, v) = (state(stack, audio), state(stack, video));
    (a.0, v.0, a.3, v.3)
}

#[test]
fn moving_the_video_of_a_j_cut_column_brings_its_audio() {
    // The audio clip used to be left out of the column and dropped.
    let mut stack = two_columns_with_ids();
    assert!(j_cut(&mut stack));

    for policy in [
        InsertPolicy::SplitAndInsert,
        InsertPolicy::InsertBeforeOrAfter,
    ] {
        let mut moved = stack.clone();
        assert!(moved.move_item_at_time("v2", "v", 12.0, true, policy, OverlapPolicy::Override));
        let (audio_start, video_start, audio_id, video_id) = column(&moved, "a2", "v2");
        assert_eq!(video_start, 12.0);
        assert_eq!(audio_start, 11.0, "the 1 s lead-in is kept");
        assert!(audio_id.is_some());
        assert_eq!(audio_id, video_id);
        assert_eq!(state(&moved, "a2").4, -1.0);
    }
}

#[test]
fn moving_the_audio_of_a_j_cut_column_brings_its_video() {
    let mut stack = two_columns_with_ids();
    assert!(j_cut(&mut stack));

    assert!(stack.move_item_at_time(
        "a2",
        "a",
        11.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override
    ));

    let (audio_start, video_start, audio_id, video_id) = column(&stack, "a2", "v2");
    assert_eq!((audio_start, video_start), (11.0, 12.0));
    assert!(audio_id.is_some());
    assert_eq!(audio_id, video_id);
}

#[test]
fn moving_an_l_cut_column_keeps_the_longer_audio() {
    let mut stack = two_columns_with_ids();
    // a2's tail runs 1 s past v2 (into free space).
    assert!(stack.modify_item("a2", 2.0, 6.0, false, false, false, false));

    assert!(stack.move_item_at_time(
        "v2",
        "v",
        12.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override
    ));

    let (audio_start, video_start, audio_id, video_id) = column(&stack, "a2", "v2");
    assert_eq!((audio_start, video_start), (12.0, 12.0));
    assert_eq!(state(&stack, "a2").1, 6.0);
    assert!(audio_id.is_some());
    assert_eq!(audio_id, video_id);
}

/// [`two_columns_with_ids`] with a third column (a3 / v3, sync id 3) at
/// 10-15 s, so a ripple has something to close up.
fn three_columns() -> Stack {
    let mut stack = two_columns_with_ids();
    stack.children[0]
        .items
        .push(synced_clip_item_with_source_start(5.0, 2.0, "a3", 3));
    stack.children[1]
        .items
        .push(synced_clip_item_with_source_start(5.0, 2.0, "v3", 3));
    stack
}

/// Every column in step: each clip at its partner's start, less its offset.
fn assert_columns_in_step(stack: &Stack, pairs: &[(&str, &str)]) {
    for (audio, video) in pairs {
        let (a, v) = (state(stack, audio), state(stack, video));
        assert!(a.3.is_some(), "{audio} lost its sync: {a:?}");
        assert_eq!(a.3, v.3, "{audio} / {video} not in one column");
        assert!(
            (a.0 - a.4 - v.0).abs() < 1e-9,
            "{audio} out of step with {video}: {a:?} vs {v:?}"
        );
    }
}

#[test]
fn moving_a_j_cut_column_in_a_multi_selection_brings_its_audio() {
    let mut stack = three_columns();
    assert!(j_cut(&mut stack));

    // v2 dragged with v3 selected too, both 10 s right.
    assert!(stack.move_items_at_time(
        "v2",
        &["v3".to_string()],
        "v",
        15.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override
    ));

    assert_eq!(state(&stack, "v2").0, 15.0);
    assert_eq!(state(&stack, "a2").0, 14.0);
    assert_eq!(state(&stack, "v3").0, 20.0);
    assert_columns_in_step(&stack, &[("a1", "v1"), ("a2", "v2"), ("a3", "v3")]);
}

#[test]
fn moving_the_j_cut_audio_in_a_multi_selection_brings_its_video() {
    let mut stack = three_columns();
    assert!(j_cut(&mut stack));

    assert!(stack.move_items_at_time(
        "a2",
        &["a3".to_string()],
        "a",
        14.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override
    ));

    assert_eq!(state(&stack, "a2").0, 14.0);
    assert_eq!(state(&stack, "v2").0, 15.0);
    assert_eq!(state(&stack, "v3").0, 20.0);
    assert_columns_in_step(&stack, &[("a1", "v1"), ("a2", "v2"), ("a3", "v3")]);
}

#[test]
fn moving_a_j_cut_column_by_index_brings_its_audio() {
    let mut stack = three_columns();
    assert!(j_cut(&mut stack));

    // v2 to the end of the video track (after v3).
    assert!(stack.move_item_at_index("v2", "v", 3, true, OverlapPolicy::Override));

    let (_, _, a2_id, v2_id) = column(&stack, "a2", "v2");
    assert!(v2_id.is_some());
    assert_eq!(a2_id, v2_id);
    assert_eq!(state(&stack, "v2").0, 15.0);
    assert_eq!(state(&stack, "a2").0, 14.0);
    assert_columns_in_step(&stack, &[("a1", "v1"), ("a2", "v2"), ("a3", "v3")]);
}

#[test]
fn ripple_moving_a_j_cut_column_keeps_every_column_in_step() {
    let mut stack = three_columns();
    assert!(j_cut(&mut stack));

    // Without a gap left behind: column 3 closes up.
    assert!(stack.move_item_at_time(
        "v2",
        "v",
        20.0,
        false,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override
    ));

    assert_columns_in_step(&stack, &[("a1", "v1"), ("a2", "v2"), ("a3", "v3")]);
    assert_eq!(state(&stack, "a2").4, -1.0);
}
