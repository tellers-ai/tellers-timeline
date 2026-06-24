mod common;
use common::*;

#[test]
fn insert_into_synced_clip_adds_spacer_gap_on_same_sync_clips_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("a".to_string())));
    let first = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("primary")),
        vec![audio_clip(4.0, "file:///audio.wav", None)],
    )
    .unwrap();

    let primary_track_index = stack.get_item("primary").unwrap().0;
    let result = stack.insert_item_at_time(
        primary_track_index,
        1.0,
        Item::Clip(clip(1.0, Some("inserted"))),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        None,
    None,
    );

    // No synced audio clips were supplied, so the result is a plain ItemId. The cluster
    // still pads the audio track with a gap spacer to stay aligned.
    assert!(matches!(result, Some(InsertItemAtTimeResult::ItemId(_))));
    assert_eq!(stack.get_item("inserted").unwrap().0, primary_track_index);
    let audio_track = &stack.children[first.audio_clips[0].1];
    let spacer_index = audio_track.get_item_at_time(1.0).unwrap();
    assert!(matches!(audio_track.items[spacer_index], Item::Gap(_)));
    assert_eq!(audio_track.items[spacer_index].duration(), 1.0);
}

#[test]
fn insert_into_synced_clip_updates_every_same_sync_clips_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("a1".to_string())));
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("a2".to_string())));
    let first = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("primary")),
        vec![
            audio_clip(4.0, "file:///audio-1.wav", None),
            audio_clip(4.0, "file:///audio-2.wav", None),
        ],
    )
    .unwrap();

    let primary_track_index = stack.get_item("primary").unwrap().0;
    let result = stack.insert_item_at_time(
        primary_track_index,
        1.0,
        Item::Clip(clip(1.0, Some("inserted"))),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        None,
    None,
    );

    // No synced audio clips supplied -> plain ItemId, but the cluster still pads every
    // audio track with a gap spacer.
    assert!(matches!(result, Some(InsertItemAtTimeResult::ItemId(_))));
    for (_, track_index) in first.audio_clips {
        let track = &stack.children[track_index];
        let spacer_index = track.get_item_at_time(1.0).unwrap();
        assert!(matches!(track.items[spacer_index], Item::Gap(_)));
        assert_eq!(track.items[spacer_index].duration(), 1.0);
    }
}

#[test]
fn insert_synced_clip_after_synced_clip_uses_same_audio_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let first = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(2.0, Some("first-video")),
        vec![audio_clip(2.0, "file:///first-audio.wav", None)],
    )
    .unwrap();
    let first_audio_id = first.audio_clips[0].0.clone();
    let first_audio_track = first.audio_clips[0].1;
    let video_track_index = stack.get_item("first-video").unwrap().0;

    let second = match stack.insert_item_at_time(
        video_track_index,
        1.0,
        Item::Clip(clip(3.0, Some("second-video"))),
        OverlapPolicy::Push,
        InsertPolicy::InsertAfter,
        Some(vec![audio_clip(3.0, "file:///second-audio.wav", None)]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("linked insert after linked clip should stay linked"),
    };
    let second_audio_id = second.audio_clips[0].0.clone();

    assert_eq!(second.audio_clips[0].1, first_audio_track);
    assert_eq!(stack.get_item(&first_audio_id).unwrap().0, first_audio_track);
    assert_eq!(stack.get_item(&second_audio_id).unwrap().0, first_audio_track);
    assert_eq!(
        stack.children[first_audio_track]
            .items
            .iter()
            .filter(|item| matches!(item, Item::Clip(_)))
            .count(),
        2
    );
}

#[test]
fn insert_synced_clip_at_end_of_synced_clip_uses_same_audio_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let first = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(2.0, Some("first-video")),
        vec![audio_clip(2.0, "file:///first-audio.wav", None)],
    )
    .unwrap();
    let first_audio_track = first.audio_clips[0].1;
    let video_track_index = stack.get_item("first-video").unwrap().0;

    let second = match stack.insert_item_at_time(
        video_track_index,
        2.0,
        Item::Clip(clip(3.0, Some("second-video"))),
        OverlapPolicy::Push,
        InsertPolicy::SplitAndInsert,
        Some(vec![audio_clip(3.0, "file:///second-audio.wav", None)]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("linked insert at end should stay linked"),
    };

    assert_eq!(second.audio_clips[0].1, first_audio_track);
    assert_eq!(stack.children.iter().filter(|t| t.kind == TrackKind::Audio).count(), 1);
    assert_eq!(stack.children[first_audio_track].items.len(), 2);
    assert_eq!(stack.children[first_audio_track].items[0].duration(), 2.0);
    assert_eq!(stack.children[first_audio_track].items[1].duration(), 3.0);
}

#[test]
fn insert_synced_clip_with_fewer_audio_links_fills_remaining_audio_track_with_gap() {
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

    let audio_track_count = stack
        .children
        .iter()
        .filter(|track| track.kind == TrackKind::Audio)
        .count();

    // The second group only has 2 audio links but the cluster has 3 audio tracks. The
    // two links reuse the first two audio tracks; no new track is created. The third
    // audio track is padded with a gap spacer at the insertion point, but because it
    // lands at the end of that track the trailing gap is trimmed during sanitize, so the
    // track keeps just its original first-group clip and the groups stay aligned.
    assert_eq!(audio_track_count, 3);
    assert!(second.created_track_indices.is_empty());
    assert_eq!(second.audio_clips.len(), 2);
    assert_eq!(first.audio_clips[0].1, second.audio_clips[0].1);
    assert_eq!(first.audio_clips[1].1, second.audio_clips[1].1);

    // The extra (third) audio track was not given a second-group clip; it still holds
    // only its original first-group audio clip.
    let extra_track_index = first.audio_clips[2].1;
    assert!(!second
        .audio_clips
        .iter()
        .any(|(_, track_index)| *track_index == extra_track_index));
    let extra_track = &stack.children[extra_track_index];
    assert_eq!(extra_track.items.len(), 1);
    assert!(matches!(extra_track.items[0], Item::Clip(_)));
    assert_eq!(extra_track.total_duration(), 2.0);
}

#[test]
fn insert_synced_clip_with_more_audio_links_creates_additional_audio_track() {
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
            audio_clip(3.0, "file:///second-a3.wav", None),
        ]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("second linked insert should succeed"),
    };

    assert_eq!(
        stack
            .children
            .iter()
            .filter(|track| track.kind == TrackKind::Audio)
            .count(),
        3
    );
    assert_eq!(second.audio_clips.len(), 3);
    // The new audio track is created directly below the video (nearest-first), so it
    // takes the first of the second group's clips; the two existing cluster tracks are
    // reused for the remaining clips. Both of the first group's tracks reappear among the
    // second group's tracks, plus exactly one brand-new track.
    let first_tracks: std::collections::HashSet<usize> =
        first.audio_clips.iter().map(|(_, t)| *t).collect();
    let second_tracks: std::collections::HashSet<usize> =
        second.audio_clips.iter().map(|(_, t)| *t).collect();
    assert!(first_tracks.is_subset(&second_tracks));
    let new_tracks: Vec<usize> = second
        .audio_clips
        .iter()
        .map(|(_, t)| *t)
        .filter(|t| !first_tracks.contains(t))
        .collect();
    assert_eq!(new_tracks.len(), 1);
    // The newly created track is nearest the video (the first assigned clip).
    assert_eq!(second.audio_clips[0].1, new_tracks[0]);
    for (audio_id, _) in &second.audio_clips {
        let (track_index, item_index, item) = stack.get_item(audio_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            2.0
        );
        assert_eq!(item.duration(), 3.0);
    }
}

#[test]
fn insert_synced_clip_with_more_audio_links_reuses_pushable_boundary_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));

    let first = match stack.insert_item_at_time(
        0,
        2.0,
        Item::Clip(clip(2.0, Some("first-video"))),
        OverlapPolicy::Push,
        InsertPolicy::SplitAndInsert,
        Some(vec![audio_clip(2.0, "file:///first-a1.wav", None)]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("first linked insert should succeed"),
    };
    let first_audio_id = first.audio_clips[0].0.clone();
    let first_audio_track = first.audio_clips[0].1;
    let video_track_index = stack.get_item("first-video").unwrap().0;

    let second = match stack.insert_item_at_time(
        video_track_index,
        0.0,
        Item::Clip(clip(3.0, Some("second-video"))),
        OverlapPolicy::Push,
        InsertPolicy::SplitAndInsert,
        Some(vec![
            audio_clip(3.0, "file:///second-a1.wav", None),
            audio_clip(3.0, "file:///second-a2.wav", None),
        ]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("second linked insert should succeed"),
    };

    assert_eq!(
        stack
            .children
            .iter()
            .filter(|track| track.kind == TrackKind::Audio)
            .count(),
        2
    );
    assert_eq!(second.created_track_indices.len(), 1);
    assert!(second
        .audio_clips
        .iter()
        .any(|(_, track_index)| *track_index == first_audio_track));

    let (first_audio_track_after, first_audio_index, first_audio_item) =
        stack.get_item(&first_audio_id).unwrap();
    assert_eq!(first_audio_track_after, first_audio_track);
    assert_eq!(
        stack.children[first_audio_track_after].start_time_of_item(first_audio_index),
        5.0
    );
    assert_eq!(first_audio_item.duration(), 2.0);

    let (first_video_track, first_video_index, first_video_item) =
        stack.get_item("first-video").unwrap();
    assert_eq!(
        stack.children[first_video_track].start_time_of_item(first_video_index),
        5.0
    );
    assert_eq!(first_video_item.duration(), 2.0);
}

#[test]
fn insert_synced_clip_four_then_two_then_four_reuses_full_boundary() {
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

    let third = match stack.insert_item_at_time(
        video_track_index,
        5.0,
        Item::Clip(clip(4.0, Some("third-video"))),
        OverlapPolicy::Push,
        InsertPolicy::InsertBeforeOrAfter,
        Some(vec![
            audio_clip(4.0, "file:///third-a1.wav", None),
            audio_clip(4.0, "file:///third-a2.wav", None),
            audio_clip(4.0, "file:///third-a3.wav", None),
            audio_clip(4.0, "file:///third-a4.wav", None),
        ]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("third linked insert should succeed"),
    };

    assert_eq!(
        stack
            .children
            .iter()
            .filter(|track| track.kind == TrackKind::Audio)
            .count(),
        4
    );
    assert_eq!(second.audio_clips.len(), 2);
    assert_eq!(third.audio_clips.len(), 4);
    assert_eq!(third.audio_clips[0].1, first.audio_clips[0].1);
    assert_eq!(third.audio_clips[1].1, first.audio_clips[1].1);
    assert_eq!(third.audio_clips[2].1, first.audio_clips[2].1);
    assert_eq!(third.audio_clips[3].1, first.audio_clips[3].1);

    for (audio_id, track_index) in &third.audio_clips {
        let (actual_track_index, item_index, item) = stack.get_item(audio_id).unwrap();
        assert_eq!(actual_track_index, *track_index);
        assert_eq!(
            stack.children[actual_track_index].start_time_of_item(item_index),
            5.0
        );
        assert_eq!(item.duration(), 4.0);
    }
}

#[test]
fn insert_synced_clip_at_index_after_synced_clip_uses_same_audio_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let first = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(2.0, Some("first-video")),
        vec![audio_clip(2.0, "file:///first-audio.wav", None)],
    )
    .unwrap();
    let first_audio_track = first.audio_clips[0].1;
    let video_track_index = stack.get_item("first-video").unwrap().0;
    let video_track_id = stack.children[video_track_index].get_id().unwrap();

    let second = match stack.insert_item_at_index(
        &video_track_id,
        1,
        Item::Clip(clip(3.0, Some("second-video"))),
        OverlapPolicy::Push,
        Some(vec![audio_clip(3.0, "file:///second-audio.wav", None)]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("linked index insert after linked clip should stay linked"),
    };

    assert_eq!(second.audio_clips[0].1, first_audio_track);
    assert_eq!(stack.children.iter().filter(|t| t.kind == TrackKind::Audio).count(), 1);
    assert_eq!(stack.children[first_audio_track].items.len(), 2);
    assert_eq!(stack.children[first_audio_track].items[0].duration(), 2.0);
    assert_eq!(stack.children[first_audio_track].items[1].duration(), 3.0);
}

#[test]
fn append_synced_clip_at_index_with_override_uses_same_audio_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let first = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(2.0, Some("first-video")),
        vec![audio_clip(2.0, "file:///first-audio.wav", None)],
    )
    .unwrap();
    let first_audio_track = first.audio_clips[0].1;
    let video_track_index = stack.get_item("first-video").unwrap().0;
    let video_track_id = stack.children[video_track_index].get_id().unwrap();
    let dest_index = stack.children[video_track_index].items.len();

    let second = match stack.insert_item_at_index(
        &video_track_id,
        dest_index,
        Item::Clip(clip_with_media_range(219.2, 0.0, 0.0, 300.0)),
        OverlapPolicy::Override,
        Some(vec![audio_clip_with_available_duration(
            219.2,
            "file:///second-audio.wav",
            300.0,
        )]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("linked index append with override should stay linked"),
    };

    assert_eq!(second.audio_clips[0].1, first_audio_track);
    assert_eq!(stack.children.iter().filter(|t| t.kind == TrackKind::Audio).count(), 1);
    assert_eq!(stack.children[first_audio_track].items.len(), 2);
    assert_eq!(stack.children[first_audio_track].items[0].duration(), 2.0);
    assert_eq!(stack.children[first_audio_track].items[1].duration(), 219.2);
}

#[test]
fn insert_into_synced_clip_propagates_across_empty_track_boundary() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let first = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("primary")),
        vec![audio_clip(4.0, "file:///audio.wav", None)],
    )
    .unwrap();
    // After the synced insert the layout is [A1 (idx0), v (idx1)] — audio below the
    // video. Add a free "empty" audio track just below the synced audio so it neighbours
    // the group without separating the synced video from its synced audio.
    let audio_track_index = first.audio_clips[0].1;
    let mut empty = Track::new(TrackKind::Audio, Some("empty".to_string()));
    empty.items.push(Item::Gap(Gap::make_gap(4.0)));
    stack.children.insert(audio_track_index, empty);
    // Inserting below the synced audio shifts it (and the video) up by one.
    let audio_track_index = audio_track_index + 1;
    let primary_track_index = stack.get_item("primary").unwrap().0;

    let result = stack.insert_item_at_time(
        primary_track_index,
        1.0,
        Item::Clip(clip(1.0, Some("inserted"))),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        None,
    None,
    );

    // No synced audio clips supplied -> plain ItemId. The synced audio track is adjacent
    // to the destination video (they share a boundary group), so the cluster's gap-spacer
    // padding reaches it: it is split too (gap at 1..2) and the group keeps one shared
    // duration instead of diverging.
    assert!(matches!(result, Some(InsertItemAtTimeResult::ItemId(_))));
    let sync_track = &stack.children[audio_track_index];
    let spacer_index = sync_track.get_item_at_time(1.0).unwrap();
    assert!(matches!(sync_track.items[spacer_index], Item::Gap(_)));
    assert_eq!(sync_track.items[spacer_index].duration(), 1.0);
    // Override keeps the track length unchanged; video and audio stay aligned.
    assert_eq!(
        stack.children[primary_track_index].total_duration(),
        sync_track.total_duration()
    );
    // The neighbouring empty track (below the synced audio) holds no real content.
    assert!(stack.children[audio_track_index - 1]
        .items
        .iter()
        .all(|item| matches!(item, Item::Gap(_))));
}

#[test]
fn insert_into_synced_clip_replaces_same_sync_clips_spacer_with_new_synced_audio() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("a".to_string())));
    insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("primary")),
        vec![audio_clip(4.0, "file:///audio.wav", None)],
    )
    .unwrap();

    let result = match stack.insert_item_at_time(
        0,
        1.0,
        Item::Clip(clip(1.0, Some("inserted"))),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        Some(vec![audio_clip(1.0, "file:///new-audio.wav", None)]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("expected linked result"),
    };

    let (audio_id, audio_track_index) = &result.audio_clips[0];
    assert_eq!(stack.get_item(audio_id).unwrap().0, *audio_track_index);
    let audio_track = &stack.children[*audio_track_index];
    let audio_index = audio_track.get_item_at_time(1.0).unwrap();
    assert_eq!(
        audio_track.items[audio_index].get_id().as_ref(),
        Some(audio_id)
    );
    assert_eq!(audio_track.items[audio_index].duration(), 1.0);
    assert_eq!(
        sync_clips_id(&audio_track.items[audio_index]),
        result.sync_clips_id
    );
}
