mod common;
use common::*;

#[test]
fn add_unsynced_clip_into_two_synced_clips_all_positions_and_policies() {
    let times = [0.0_f64, 1.0, 2.0, 4.0, 5.0, 6.0, 8.0];
    let overlaps = [OverlapPolicy::Override, OverlapPolicy::Push];
    let insert_policies = [
        InsertPolicy::SplitAndInsert,
        InsertPolicy::InsertBefore,
        InsertPolicy::InsertAfter,
        InsertPolicy::InsertBeforeOrAfter,
    ];
    for &t in &times {
        for &op in &overlaps {
            for &ip in &insert_policies {
                let mut stack = two_synced_clips();
                let video_track_index = track_index_by_id(&stack, "v");
                let label = format!("t={t} op={op:?} ip={ip:?}");
                let result = stack.insert_item_at_time(
                    video_track_index,
                    t,
                    Item::Clip(clip(1.0, Some("unsynced"))),
                    op,
                    ip,
                    None,
                None,
                );
                assert!(result.is_some(), "{label}: insert returned None");
                // The inserted clip exists and is not part of any sync clips.
                let (_, _, inserted) = stack.get_item("unsynced").expect("inserted clip present");
                assert_eq!(
                    sync_clips_id(inserted),
                    None,
                    "{label}: inserted clip must stay unsynced"
                );
                // Every surviving sync clips stays duration-aligned across its tracks.
                assert_sync_clips_track_aligned(&stack, &label);
            }
        }
    }
}

#[test]
fn add_unsynced_clip_between_two_synced_clips_pushes_second_group_aligned() {
    let mut stack = two_synced_clips();
    let video_track_index = track_index_by_id(&stack, "v");
    let result = stack.insert_item_at_time(
        video_track_index,
        4.0,
        Item::Clip(clip(1.0, Some("unsynced"))),
        OverlapPolicy::Push,
        InsertPolicy::SplitAndInsert,
        None,
    None,
    );
    assert!(result.is_some());
    assert_eq!(sync_clips_id(stack.get_item("unsynced").unwrap().2), None);
    assert_sync_clips_track_aligned(&stack, "between-push");

    // Group A is untouched at 0..4; group B is pushed to 5..9 on BOTH tracks.
    let (va_t, va_i, _) = stack.get_item("vA").unwrap();
    let (aa_t, aa_i, _) = stack.get_item("aA").unwrap();
    assert_eq!(stack.children[va_t].start_time_of_item(va_i), 0.0);
    assert_eq!(stack.children[aa_t].start_time_of_item(aa_i), 0.0);
    let (vb_t, vb_i, _) = stack.get_item("vB").unwrap();
    let (ab_t, ab_i, _) = stack.get_item("aB").unwrap();
    assert_eq!(stack.children[vb_t].start_time_of_item(vb_i), 5.0);
    assert_eq!(
        stack.children[vb_t].start_time_of_item(vb_i),
        stack.children[ab_t].start_time_of_item(ab_i),
    );
}

#[test]
fn add_unsynced_clip_over_synced_clip_override_splits_group_aligned() {
    let mut stack = two_synced_clips();
    let video_track_index = track_index_by_id(&stack, "v");
    // Drop a 1.0 unsynced clip in the middle of synced clips A (over vA at 1..2).
    let result = stack.insert_item_at_time(
        video_track_index,
        1.0,
        Item::Clip(clip(1.0, Some("unsynced"))),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        None,
    None,
    );
    assert!(result.is_some());
    assert_eq!(sync_clips_id(stack.get_item("unsynced").unwrap().2), None);
    // Group A's video split (vA -> 0..1 and 2..4); the audio must split the same way
    // so the group keeps one shared duration footprint.
    assert_sync_clips_track_aligned(&stack, "override-split");
    // The audio track got a 1.0 spacer at 1..2 mirroring the video split.
    let audio = &stack.children[track_index_by_id(&stack, "a")];
    let spacer = audio.get_item_at_time(1.0).unwrap();
    assert!(matches!(audio.items[spacer], Item::Gap(_)));
    assert_eq!(audio.items[spacer].duration(), 1.0);
}

#[test]
fn add_synced_clip_with_multiple_audio_into_two_synced_clips() {
    let times = [0.0_f64, 2.0, 3.0, 4.0, 6.0, 8.0];
    let overlaps = [OverlapPolicy::Override, OverlapPolicy::Push];
    let insert_policies = [InsertPolicy::SplitAndInsert, InsertPolicy::InsertBefore];
    for &t in &times {
        for &op in &overlaps {
            for &ip in &insert_policies {
                let mut stack = two_synced_clips();
                let video_track_index = track_index_by_id(&stack, "v");
                let label = format!("multi-audio t={t} op={op:?} ip={ip:?}");
                let result = stack.insert_item_at_time(
                    video_track_index,
                    t,
                    Item::Clip(clip(2.0, Some("new-v"))),
                    op,
                    ip,
                    Some(vec![
                        audio_clip(2.0, "file:///na1.wav", None),
                        audio_clip(2.0, "file:///na2.wav", None),
                    ]),
                None,
                );
                let Some(InsertItemAtTimeResult::Synced(r)) = result else {
                    panic!("{label}: expected a synced insert result");
                };
                // The new clip and its two audio companions share one sync id.
                let primary_group = sync_clips_id(stack.get_item(&r.primary_clip_id).unwrap().2);
                assert!(primary_group.is_some(), "{label}: new clip should be synced");
                assert_eq!(r.audio_clips.len(), 2, "{label}");
                for (audio_id, _) in &r.audio_clips {
                    assert_eq!(
                        sync_clips_id(stack.get_item(audio_id).unwrap().2),
                        primary_group,
                        "{label}: audio companion must join the new sync clips"
                    );
                }
                // Every group (the new one and the two pre-existing) stays aligned.
                assert_sync_clips_track_aligned(&stack, &label);
            }
        }
    }
}

#[test]
fn add_unsynced_clip_at_index_into_two_synced_clips() {
    let overlaps = [OverlapPolicy::Override, OverlapPolicy::Push];
    for dest_index in 0..=2usize {
        for &op in &overlaps {
            let mut stack = two_synced_clips();
            let label = format!("at-index idx={dest_index} op={op:?}");
            let result = stack.insert_item_at_index(
                "v",
                dest_index,
                Item::Clip(clip(1.0, Some("unsynced"))),
                op,
                None,
            None,
            );
            assert!(result.is_some(), "{label}: insert returned None");
            assert_eq!(
                sync_clips_id(stack.get_item("unsynced").unwrap().2),
                None,
                "{label}: inserted clip must stay unsynced"
            );
            assert_sync_clips_track_aligned(&stack, &label);
        }
    }
}
