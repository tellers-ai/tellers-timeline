mod common;
use common::*;

#[test]
fn insert_unsynced_clip_with_push_moves_later_synced_assets() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        2.0,
        clip(2.0, Some("linked-video")),
        vec![audio_clip(2.0, "file:///linked-audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();
    let primary_track_index = stack.get_item("linked-video").unwrap().0;

    let insert_result = stack.insert_item_at_time(
        primary_track_index,
        0.0,
        Item::Clip(clip(1.0, Some("inserted"))),
        OverlapPolicy::Push,
        InsertPolicy::SplitAndInsert,
        None,
    None,
    );

    assert!(matches!(insert_result, Some(InsertItemAtTimeResult::ItemId(_))));
    assert_eq!(
        stack.children[primary_track_index].start_time_of_item(
            stack.get_item("linked-video").unwrap().1
        ),
        3.0
    );
    let (audio_track_index, audio_item_index, audio_item) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[audio_track_index].start_time_of_item(audio_item_index),
        3.0
    );
    assert_eq!(audio_item.duration(), 2.0);
}

#[test]
fn insert_unsynced_clip_with_push_moves_synced_audio_below_video() {
    let mut stack = stack_with_synced_audio_below_video();
    let video_track_index = track_index_by_id(&stack, "v");

    let insert_result = stack.insert_item_at_time(
        video_track_index,
        0.0,
        Item::Clip(clip(1.0, Some("inserted"))),
        OverlapPolicy::Push,
        InsertPolicy::SplitAndInsert,
        None,
    None,
    );

    assert!(matches!(insert_result, Some(InsertItemAtTimeResult::ItemId(_))));
    let (video_track_index, video_item_index, _) = stack.get_item("linked-video").unwrap();
    let (audio_track_index, audio_item_index, audio_item) = stack.get_item("linked-audio").unwrap();
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        3.0
    );
    assert_eq!(
        stack.children[audio_track_index].start_time_of_item(audio_item_index),
        3.0
    );
    assert_eq!(audio_item.duration(), 2.0);
}

#[test]
fn insert_unsynced_clip_with_push_moves_synced_audio_below_video_for_time_policies() {
    for (insert_policy, dest_time, expected_start) in [
        (InsertPolicy::InsertBefore, 3.0, 3.0),
        (InsertPolicy::InsertAfter, 3.0, 2.0),
        (InsertPolicy::InsertBeforeOrAfter, 2.25, 3.0),
        (InsertPolicy::InsertBeforeOrAfter, 3.75, 2.0),
    ] {
        let mut stack = stack_with_synced_audio_below_video();
        let video_track_index = track_index_by_id(&stack, "v");

        let insert_result = stack.insert_item_at_time(
            video_track_index,
            dest_time,
            Item::Clip(clip(1.0, Some("inserted"))),
            OverlapPolicy::Push,
            insert_policy,
            None,
        None,
        );

        assert!(matches!(insert_result, Some(InsertItemAtTimeResult::ItemId(_))));
        let (video_track_index, video_item_index, _) = stack.get_item("linked-video").unwrap();
        let (audio_track_index, audio_item_index, _) = stack.get_item("linked-audio").unwrap();
        assert_eq!(
            stack.children[video_track_index].start_time_of_item(video_item_index),
            expected_start
        );
        assert_eq!(
            stack.children[audio_track_index].start_time_of_item(audio_item_index),
            expected_start
        );
    }
}

#[test]
fn insert_unsynced_clip_at_index_with_push_moves_synced_audio_below_video() {
    let mut stack = stack_with_synced_audio_below_video();

    let insert_result = stack.insert_item_at_index(
        "v",
        0,
        Item::Clip(clip(1.0, Some("inserted"))),
        OverlapPolicy::Push,
        None,
    None,
    );

    assert!(matches!(insert_result, Some(InsertItemAtTimeResult::ItemId(_))));
    let (video_track_index, video_item_index, _) = stack.get_item("linked-video").unwrap();
    let (audio_track_index, audio_item_index, audio_item) = stack.get_item("linked-audio").unwrap();
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        3.0
    );
    assert_eq!(
        stack.children[audio_track_index].start_time_of_item(audio_item_index),
        3.0
    );
    assert_eq!(audio_item.duration(), 2.0);
}

#[test]
fn insert_unsynced_clip_with_override_updates_synced_audio_below_video() {
    let mut stack = stack_with_synced_audio_below_video();
    let video_track_index = track_index_by_id(&stack, "v");

    let insert_result = stack.insert_item_at_time(
        video_track_index,
        3.0,
        Item::Clip(clip(1.0, Some("inserted"))),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        None,
    None,
    );

    assert!(matches!(insert_result, Some(InsertItemAtTimeResult::ItemId(_))));
    let (audio_track_index, _, _) = stack.get_item("linked-audio").unwrap();
    assert!(range_is_gap_backed_for_test(
        &stack.children[audio_track_index],
        3.0,
        4.0
    ));
}

#[test]
fn insert_unsynced_clip_at_index_with_push_moves_later_synced_assets() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        2.0,
        clip(2.0, Some("linked-video")),
        vec![audio_clip(2.0, "file:///linked-audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();
    let primary_track_index = stack.get_item("linked-video").unwrap().0;
    let primary_track_id = stack.children[primary_track_index].get_id().unwrap();

    let insert_result = stack.insert_item_at_index(
        &primary_track_id,
        0,
        Item::Clip(clip(1.0, Some("inserted"))),
        OverlapPolicy::Push,
        None,
    None,
    );

    assert!(matches!(insert_result, Some(InsertItemAtTimeResult::ItemId(_))));
    let (video_track_index, video_item_index, video_item) =
        stack.get_item("linked-video").unwrap();
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        3.0
    );
    let (audio_track_index, audio_item_index, audio_item) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[audio_track_index].start_time_of_item(audio_item_index),
        3.0
    );
    assert_eq!(video_item.duration(), 2.0);
    assert_eq!(audio_item.duration(), 2.0);
}

#[test]
fn insert_unsynced_clip_before_policy_pushes_synced_assets_from_boundary() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        2.0,
        clip(2.0, Some("linked-video")),
        vec![audio_clip(2.0, "file:///linked-audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();
    let primary_track_index = stack.get_item("linked-video").unwrap().0;

    let insert_result = stack.insert_item_at_time(
        primary_track_index,
        3.0,
        Item::Clip(clip(1.0, Some("inserted"))),
        OverlapPolicy::Push,
        InsertPolicy::InsertBefore,
        None,
    None,
    );

    assert!(matches!(insert_result, Some(InsertItemAtTimeResult::ItemId(_))));
    let (video_track_index, video_item_index, _) = stack.get_item("linked-video").unwrap();
    let (audio_track_index, audio_item_index, _) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        3.0
    );
    assert_eq!(
        stack.children[audio_track_index].start_time_of_item(audio_item_index),
        3.0
    );
}

#[test]
fn insert_unsynced_clip_after_policy_adds_boundary_gap_sync_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        2.0,
        clip(2.0, Some("linked-video")),
        vec![audio_clip(2.0, "file:///linked-audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();
    let primary_track_index = stack.get_item("linked-video").unwrap().0;

    let insert_result = stack.insert_item_at_time(
        primary_track_index,
        3.0,
        Item::Clip(clip(1.0, Some("inserted"))),
        OverlapPolicy::Push,
        InsertPolicy::InsertAfter,
        None,
    None,
    );

    assert!(matches!(insert_result, Some(InsertItemAtTimeResult::ItemId(_))));
    let (video_track_index, video_item_index, _) = stack.get_item("linked-video").unwrap();
    let (audio_track_index, audio_item_index, _) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        2.0
    );
    assert_eq!(
        stack.children[audio_track_index].start_time_of_item(audio_item_index),
        2.0
    );
    // The inserted clip lands after the synced group on the video track. The cluster's
    // audio track is padded with a gap spacer at the same start, but a trailing gap is
    // trimmed during sanitize, so the audio track stays at its original duration while
    // the synced group remains aligned (both members still start at 2.0).
    assert_eq!(stack.children[video_track_index].total_duration(), 5.0);
    assert_eq!(stack.children[audio_track_index].total_duration(), 4.0);
}

#[test]
fn insert_unsynced_clip_at_index_after_boundary_adds_gap_sync_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        2.0,
        clip(2.0, Some("linked-video")),
        vec![audio_clip(2.0, "file:///linked-audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();
    let primary_track_index = stack.get_item("linked-video").unwrap().0;
    let primary_track_id = stack.children[primary_track_index].get_id().unwrap();

    let insert_result = stack.insert_item_at_index(
        &primary_track_id,
        stack.children[primary_track_index].items.len(),
        Item::Clip(clip(1.0, Some("inserted"))),
        OverlapPolicy::Push,
        None,
    None,
    );

    assert!(matches!(insert_result, Some(InsertItemAtTimeResult::ItemId(_))));
    let (video_track_index, video_item_index, _) = stack.get_item("linked-video").unwrap();
    let (audio_track_index, audio_item_index, _) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        2.0
    );
    assert_eq!(
        stack.children[audio_track_index].start_time_of_item(audio_item_index),
        2.0
    );
    // Appending after the synced group: the video track grows to 5.0, the audio track's
    // padding gap is trailing and gets trimmed, so it stays at 4.0 while the group keeps
    // its alignment (both members still start at 2.0).
    assert_eq!(stack.children[video_track_index].total_duration(), 5.0);
    assert_eq!(stack.children[audio_track_index].total_duration(), 4.0);
}

#[test]
fn insert_unsynced_between_synced_clips_keeps_partial_boundary_tracks_aligned() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));

    let first = match stack.insert_item_at_time(
        0,
        0.0,
        Item::Clip(clip(2.0, Some("first-video"))),
        OverlapPolicy::Push,
        InsertPolicy::InsertBeforeOrAfter,
        Some(vec![
            audio_clip(2.0, "file:///first-a1.wav", None),
            audio_clip(2.0, "file:///first-a2.wav", None),
            audio_clip(2.0, "file:///first-a3.wav", None),
            audio_clip(2.0, "file:///first-a4.wav", None),
        ]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("first linked insert should succeed"),
    };
    let video_track_index = stack.get_item("first-video").unwrap().0;

    let second = match stack.insert_item_at_time(
        video_track_index,
        2.0,
        Item::Clip(clip(3.0, Some("second-video"))),
        OverlapPolicy::Push,
        InsertPolicy::InsertBeforeOrAfter,
        Some(vec![
            audio_clip(3.0, "file:///second-a1.wav", None),
            audio_clip(3.0, "file:///second-a2.wav", None),
        ]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("second linked insert should succeed"),
    };

    let inserted = stack.insert_item_at_time(
        video_track_index,
        2.0,
        Item::Clip(clip(1.0, Some("inserted-video"))),
        OverlapPolicy::Push,
        InsertPolicy::InsertBeforeOrAfter,
        None,
    None,
    );

    // No synced audio clips supplied -> plain ItemId. The cluster's always-pad still
    // inserts a gap spacer on every audio track in the cluster, keeping them aligned.
    assert!(matches!(
        inserted.as_ref(),
        Some(InsertItemAtTimeResult::ItemId(id))
            if *id == "inserted-video"
    ));
    assert_eq!(
        stack
            .children
            .iter()
            .filter(|track| track.kind == TrackKind::Audio)
            .count(),
        4
    );

    // The two audio tracks that carry the second group (the first two cluster audio
    // tracks) extend to hold it; the inserted unsynced clip pushes the second group from
    // 2..5 to 3..6. The remaining two audio tracks only ever held the first group's clip
    // (their padding gaps were trailing and trimmed), so they stay at 2.0. Both synced
    // groups remain internally aligned.
    let second_tracks: Vec<usize> = second.audio_clips.iter().map(|(_, t)| *t).collect();
    for (_, track_index) in &first.audio_clips {
        let expected = if second_tracks.contains(track_index) {
            6.0
        } else {
            2.0
        };
        assert_eq!(stack.children[*track_index].total_duration(), expected);
    }
    for (audio_id, _) in &second.audio_clips {
        let (track_index, item_index, item) = stack.get_item(audio_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            3.0
        );
        assert_eq!(item.duration(), 3.0);
    }
}

#[test]
fn insert_unsynced_clip_override_after_policy_adds_boundary_gap_sync_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        2.0,
        clip(2.0, Some("linked-video")),
        vec![audio_clip(2.0, "file:///linked-audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();
    let primary_track_index = stack.get_item("linked-video").unwrap().0;

    let insert_result = stack.insert_item_at_time(
        primary_track_index,
        3.0,
        Item::Clip(clip(1.0, Some("inserted"))),
        OverlapPolicy::Override,
        InsertPolicy::InsertAfter,
        None,
    None,
    );

    assert!(matches!(insert_result, Some(InsertItemAtTimeResult::ItemId(_))));
    let (video_track_index, video_item_index, video_item) =
        stack.get_item("linked-video").unwrap();
    let (audio_track_index, audio_item_index, audio_item) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        2.0
    );
    assert_eq!(
        stack.children[audio_track_index].start_time_of_item(audio_item_index),
        2.0
    );
    assert_eq!(video_item.duration(), 2.0);
    assert_eq!(audio_item.duration(), 2.0);
    // Inserted after the synced group on the video track; the audio padding gap is
    // trailing and trimmed, leaving the audio at its original duration while the group
    // remains aligned.
    assert_eq!(stack.children[video_track_index].total_duration(), 5.0);
    assert_eq!(stack.children[audio_track_index].total_duration(), 4.0);
}
