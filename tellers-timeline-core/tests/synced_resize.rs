mod common;
use common::*;

#[test]
fn modify_unsynced_insert_in_cluster_updates_padding_and_sync_assets() {
    let mut stack = Stack::default();

    for id in ["A1", "A2", "A3"] {
        let mut audio = Track::new(TrackKind::Audio, Some(id.to_string()));
        audio.items.push(synced_clip_item_with_rate(
            62.88,
            0.0,
            &format!("{id}-g3"),
            3,
            25.0,
        ));
        stack.children.push(audio);
    }

    let mut video = Track::new(TrackKind::Video, Some("video".to_string()));
    video.items.push(synced_clip_item_with_rate(62.88, 0.0, "v-g3", 3, 25.0));
    stack.children.push(video);

    let video_track_index = stack.children.len() - 1;
    let audio_track_indices: Vec<_> = (0..video_track_index).collect();

    stack
        .insert_item_at_time(
            video_track_index,
            2.0,
            Item::Clip(clip(1.5, Some("inserted-at-2"))),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            None,
            None,
        )
        .expect("insert should succeed");

    assert!(stack.modify_item("inserted-at-2", 0.0, 2.0, false, false, false));

    let (_, _, inserted) = stack.get_item("inserted-at-2").unwrap();
    assert_eq!(inserted.duration(), 2.0);

    let video_track = &stack.children[video_track_index];
    let tail_index = video_track.get_item_at_time(4.0).unwrap();
    assert_eq!(video_track.start_time_of_item(tail_index), 4.0);

    for &audio_track_index in &audio_track_indices {
        let audio_track = &stack.children[audio_track_index];
        let spacer_index = audio_track.get_item_at_time(2.0).unwrap();
        assert!(matches!(audio_track.items[spacer_index], Item::Gap(_)));
        assert_eq!(audio_track.items[spacer_index].duration(), 2.0);
        let tail_index = audio_track.get_item_at_time(4.0).unwrap();
        assert_eq!(audio_track.start_time_of_item(tail_index), 4.0);
    }

    let groups = stack.sync_track_info();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].track_indices, vec![0, 1, 2, 3]);
}

#[test]
fn modify_cluster_padding_gap_updates_primary_clip_and_sync_assets() {
    let mut stack = Stack::default();

    for id in ["A1", "A2"] {
        let mut audio = Track::new(TrackKind::Audio, Some(id.to_string()));
        audio.items.push(synced_clip_item_with_rate(
            62.88,
            0.0,
            &format!("{id}-g3"),
            3,
            25.0,
        ));
        stack.children.push(audio);
    }

    let mut video = Track::new(TrackKind::Video, Some("video".to_string()));
    video.items.push(synced_clip_item_with_rate(62.88, 0.0, "v-g3", 3, 25.0));
    stack.children.push(video);

    let video_track_index = stack.children.len() - 1;

    stack
        .insert_item_at_time(
            video_track_index,
            2.0,
            Item::Clip(clip(1.5, Some("inserted-at-2"))),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            None,
            None,
        )
        .expect("insert should succeed");

    let audio_track = &stack.children[0];
    let gap_index = audio_track.get_item_at_time(2.0).unwrap();
    let gap_id = audio_track.items[gap_index].get_id().unwrap();

    assert!(stack.modify_item(&gap_id, 0.0, 1.0, false, false, false));

    let (_, _, inserted) = stack.get_item("inserted-at-2").unwrap();
    assert_eq!(inserted.duration(), 1.0);

    let video_track = &stack.children[video_track_index];
    let tail_index = video_track.get_item_at_time(3.0).unwrap();
    assert_eq!(video_track.start_time_of_item(tail_index), 3.0);

    let audio_track = &stack.children[0];
    let spacer_index = audio_track.get_item_at_time(2.0).unwrap();
    assert_eq!(audio_track.items[spacer_index].duration(), 1.0);
    let tail_index = audio_track.get_item_at_time(3.0).unwrap();
    assert_eq!(audio_track.start_time_of_item(tail_index), 3.0);
}

#[test]
fn modify_item_in_cluster_preserves_sync_track_info() {
    let mut stack = Stack::default();

    for id in ["A1", "A2", "A3"] {
        let mut audio = Track::new(TrackKind::Audio, Some(id.to_string()));
        audio.items.push(synced_clip_item_with_rate(
            62.88,
            0.0,
            &format!("{id}-g3"),
            3,
            25.0,
        ));
        stack.children.push(audio);
    }

    let mut video = Track::new(TrackKind::Video, Some("video".to_string()));
    video.items.push(synced_clip_item_with_rate(62.88, 0.0, "v-g3", 3, 25.0));
    stack.children.push(video);

    let video_track_index = stack.children.len() - 1;

    stack
        .insert_item_at_time(
            video_track_index,
            2.0,
            Item::Clip(clip(1.5, Some("inserted-at-2"))),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            None,
            None,
        )
        .expect("insert should succeed");

    let groups_after_insert = stack.sync_track_info();
    assert_eq!(groups_after_insert.len(), 1);
    assert_eq!(groups_after_insert[0].track_indices, vec![0, 1, 2, 3]);

    assert!(stack.modify_item("inserted-at-2", 0.0, 2.0, false, false, false));
    assert_sync_track_info_unchanged(&groups_after_insert, &stack.sync_track_info());

    let audio_track = &stack.children[0];
    let gap_index = audio_track.get_item_at_time(2.0).unwrap();
    let gap_id = audio_track.items[gap_index].get_id().unwrap();

    assert!(stack.modify_item(&gap_id, 0.0, 1.0, false, false, false));
    assert_sync_track_info_unchanged(&groups_after_insert, &stack.sync_track_info());
}

#[test]
fn resize_item_updates_synced_clips() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("primary")),
        vec![audio_clip(4.0, "file:///a1.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.resize_item(&audio_id, 1.0, 2.0, OverlapPolicy::Override, false));

    let (video_track_index, video_item_index, video_item) = stack.get_item("primary").unwrap();
    let (audio_track_index, audio_item_index, audio_item) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        1.0
    );
    assert_eq!(
        stack.children[audio_track_index].start_time_of_item(audio_item_index),
        1.0
    );
    assert_eq!(video_item.duration(), 2.0);
    assert_eq!(audio_item.duration(), 2.0);
}

#[test]
fn resize_primary_clip_over_itself_keeps_synced_clips() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("primary")),
        vec![audio_clip(4.0, "file:///a1.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.resize_item("primary", 1.0, 2.0, OverlapPolicy::Override, false));

    let (video_track_index, video_item_index, video_item) = stack.get_item("primary").unwrap();
    let (audio_track_index, audio_item_index, audio_item) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        1.0
    );
    assert_eq!(
        stack.children[audio_track_index].start_time_of_item(audio_item_index),
        1.0
    );
    assert_eq!(video_item.duration(), 2.0);
    assert_eq!(audio_item.duration(), 2.0);
    assert_eq!(sync_clips_id(video_item), result.sync_clips_id);
    assert_eq!(sync_clips_id(audio_item), result.sync_clips_id);
    assert_eq!(
        stack
            .children
            .iter()
            .flat_map(|track| track.items.iter())
            .filter(|item| matches!(item, Item::Clip(_)))
            .count(),
        2
    );
}

#[test]
fn resize_item_moves_selected_split_synced_clips_by_selected_delta() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("primary")),
        vec![audio_clip(4.0, "file:///a1.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();
    assert!(stack.split_item_at_time("primary", 2.0));
    let right_video_id = stack.children[stack.get_item("primary").unwrap().0].items[1]
        .get_id()
        .unwrap();
    let right_audio_id = stack.children[stack.get_item(&audio_id).unwrap().0].items[1]
        .get_id()
        .unwrap();
    let right_sync_clips_id = sync_clips_id(stack.get_item(&right_video_id).unwrap().2);
    assert_eq!(
        right_sync_clips_id,
        sync_clips_id(stack.get_item(&right_audio_id).unwrap().2)
    );
    assert_ne!(right_sync_clips_id, result.sync_clips_id);

    assert!(stack.resize_item("primary", 1.0, 1.0, OverlapPolicy::Override, false));

    for item_id in [&"primary".to_string(), &audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            1.0
        );
        assert_eq!(item.duration(), 1.0);
    }
    // Trimming the left fragment from the left happens in place: the right
    // fragments do not move or shrink.
    for item_id in [&right_video_id, &right_audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(stack.children[track_index].start_time_of_item(item_index), 2.0);
        assert_eq!(item.duration(), 2.0);
        assert_eq!(sync_clips_id(item), right_sync_clips_id);
    }
}

#[test]
fn resize_item_push_updates_synced_assets_of_pushed_clip() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(2.0, Some("unlinked"))));
    let audio = Track::new(TrackKind::Audio, Some("a".to_string()));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(audio);
    let result = insert_with_audio(
        &mut stack,
        0,
        2.0,
        clip(2.0, Some("linked-video")),
        vec![audio_clip(2.0, "file:///linked-audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.resize_item("unlinked", 0.0, 3.0, OverlapPolicy::Push, false));

    let (video_track_index, video_item_index, video_item) = stack.get_item("linked-video").unwrap();
    let (audio_track_index, audio_item_index, audio_item) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        3.0
    );
    assert_eq!(
        stack.children[audio_track_index].start_time_of_item(audio_item_index),
        3.0
    );
    assert_eq!(video_item.duration(), 2.0);
    assert_eq!(audio_item.duration(), 2.0);
}

#[test]
fn resize_synced_item_push_updates_synced_assets_of_pushed_group() {
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
    let video_track_index = stack.get_item("first-video").unwrap().0;
    let second = insert_with_audio(
        &mut stack,
        video_track_index,
        2.0,
        clip(2.0, Some("second-video")),
        vec![audio_clip(2.0, "file:///second-audio.wav", None)],
    )
    .unwrap();
    let first_audio_id = first.audio_clips[0].0.clone();
    let second_audio_id = second.audio_clips[0].0.clone();

    assert!(stack.resize_item("first-video", 0.0, 3.0, OverlapPolicy::Push, false));

    for item_id in ["first-video", &first_audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            0.0
        );
        assert_eq!(item.duration(), 3.0);
    }
    for item_id in ["second-video", &second_audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            3.0
        );
        assert_eq!(item.duration(), 2.0);
    }
}

#[test]
fn resize_audio_synced_item_push_updates_video_and_following_group() {
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
    let video_track_index = stack.get_item("first-video").unwrap().0;
    let second = insert_with_audio(
        &mut stack,
        video_track_index,
        2.0,
        clip(2.0, Some("second-video")),
        vec![audio_clip(2.0, "file:///second-audio.wav", None)],
    )
    .unwrap();
    let first_audio_id = first.audio_clips[0].0.clone();
    let second_audio_id = second.audio_clips[0].0.clone();

    assert!(stack.resize_item(&first_audio_id, 0.0, 3.0, OverlapPolicy::Push, false));

    for item_id in ["first-video", &first_audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            0.0
        );
        assert_eq!(item.duration(), 3.0);
    }
    for item_id in ["second-video", &second_audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            3.0
        );
        assert_eq!(item.duration(), 2.0);
    }
}

#[test]
fn resize_audio_synced_item_override_trims_following_group_when_start_is_unchanged() {
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
    let video_track_index = stack.get_item("first-video").unwrap().0;
    let second = insert_with_audio(
        &mut stack,
        video_track_index,
        2.0,
        clip(2.0, Some("second-video")),
        vec![audio_clip(2.0, "file:///second-audio.wav", None)],
    )
    .unwrap();
    let first_audio_id = first.audio_clips[0].0.clone();
    let second_audio_id = second.audio_clips[0].0.clone();

    assert!(stack.resize_item(&first_audio_id, 0.0, 3.0, OverlapPolicy::Override, false));

    for item_id in ["first-video", &first_audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            0.0
        );
        assert_eq!(item.duration(), 3.0);
    }
    // The second column lost its first second but keeps its ids: a trimmed
    // clip is the same clip, shorter, with its media offset moved on.
    for item_id in ["second-video", &second_audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(stack.children[track_index].start_time_of_item(item_index), 3.0);
        assert_eq!(item.duration(), 1.0);
        assert_eq!(source_start(item), 1.0);
    }
    for track in &stack.children {
        assert_eq!(track.items.len(), 2);
        assert_eq!(track.start_time_of_item(1), 3.0);
        assert_eq!(track.items[1].duration(), 1.0);
    }
}

#[test]
fn resize_gap_shrink_updates_following_synced_assets() {
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio
        .items
        .push(Item::Gap(Gap::new(5.0, Some("audio-gap".to_string()))));
    audio.items.push(synced_clip_item(2.0, "audio", 1));

    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video
        .items
        .push(Item::Gap(Gap::new(5.0, Some("video-gap".to_string()))));
    video.items.push(synced_clip_item(2.0, "video", 1));

    let mut stack = Stack::default();
    stack.children.push(audio);
    stack.children.push(video);

    assert!(stack.resize_item("audio-gap", 0.0, 3.0, OverlapPolicy::Override, false));

    for item_id in ["audio", "video"] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            3.0
        );
        assert_eq!(item.duration(), 2.0);
    }
}

#[test]
fn resize_item_override_updates_synced_assets_of_trimmed_clip() {
    let video = Track::new(TrackKind::Video, Some("v".to_string()));
    let audio = Track::new(TrackKind::Audio, Some("a".to_string()));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(audio);
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(2.0, Some("linked-video")),
        vec![audio_clip(2.0, "file:///linked-audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();
    stack.children[0]
        .items
        .push(Item::Clip(clip(2.0, Some("unlinked"))));

    assert!(stack.resize_item("unlinked", 1.0, 2.0, OverlapPolicy::Override, false));

    let (video_track_index, video_item_index, video_item) = stack.get_item("linked-video").unwrap();
    let (audio_track_index, audio_item_index, audio_item) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        0.0
    );
    assert_eq!(
        stack.children[audio_track_index].start_time_of_item(audio_item_index),
        0.0
    );
    assert_eq!(video_item.duration(), 1.0);
    assert_eq!(audio_item.duration(), 1.0);
}

#[test]
fn resize_synced_item_override_updates_synced_assets_of_trimmed_group() {
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
    let video_track_index = stack.get_item("first-video").unwrap().0;
    let second = insert_with_audio(
        &mut stack,
        video_track_index,
        2.0,
        clip(2.0, Some("second-video")),
        vec![audio_clip(2.0, "file:///second-audio.wav", None)],
    )
    .unwrap();
    let first_audio_id = first.audio_clips[0].0.clone();
    let second_audio_id = second.audio_clips[0].0.clone();

    assert!(stack.resize_item("first-video", 1.0, 2.0, OverlapPolicy::Override, false));

    for item_id in ["first-video", &first_audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            1.0
        );
        assert_eq!(item.duration(), 2.0);
    }
    // The first column moved right by 1 and overrode the head of the second:
    // the second keeps its place and ids, 1s shorter, media offset moved on.
    for item_id in ["second-video", &second_audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            3.0
        );
        assert_eq!(item.duration(), 1.0);
        assert_eq!(source_start(item), 1.0);
    }
}

#[test]
fn modify_synced_item_left_extension_preserves_following_group() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(5.0)));
    video.items.push(synced_clip_item_with_source_start(
        5.0,
        3.0,
        "first-video",
        1,
    ));
    video.items.push(synced_clip_item(3.0, "second-video", 2));

    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(5.0)));
    audio.items.push(synced_clip_item_with_source_start(
        5.0,
        3.0,
        "first-audio",
        1,
    ));
    audio.items.push(synced_clip_item(3.0, "second-audio", 2));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(audio);

    assert!(stack.modify_item("first-video", 0.0, 8.0, false, true, false));

    for item_id in ["first-video", "first-audio"] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            2.0
        );
        assert_eq!(source_start(item), 0.0);
        assert_eq!(item.duration(), 8.0);
        assert_eq!(sync_clips_id(item), Some(1));
    }
    for item_id in ["second-video", "second-audio"] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            10.0
        );
        assert_eq!(item.duration(), 3.0);
        assert_eq!(sync_clips_id(item), Some(2));
    }
}

#[test]
fn modify_synced_item_left_extension_with_push_keeps_start_and_pushes_following_group() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(5.0)));
    video.items.push(synced_clip_item_with_source_start(
        5.0,
        3.0,
        "first-video",
        1,
    ));
    video.items.push(synced_clip_item(3.0, "second-video", 2));

    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(5.0)));
    audio.items.push(synced_clip_item_with_source_start(
        5.0,
        3.0,
        "first-audio",
        1,
    ));
    audio.items.push(synced_clip_item(3.0, "second-audio", 2));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(audio);

    assert!(stack.modify_item("first-video", 0.0, 8.0, false, true, true));

    for item_id in ["first-video", "first-audio"] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            5.0
        );
        assert_eq!(source_start(item), 0.0);
        assert_eq!(item.duration(), 8.0);
        assert_eq!(sync_clips_id(item), Some(1));
    }
    for item_id in ["second-video", "second-audio"] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            13.0
        );
        assert_eq!(item.duration(), 3.0);
        assert_eq!(sync_clips_id(item), Some(2));
    }
}

#[test]
fn resize_audio_synced_item_override_updates_video_and_trims_following_group() {
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
    let video_track_index = stack.get_item("first-video").unwrap().0;
    let second = insert_with_audio(
        &mut stack,
        video_track_index,
        2.0,
        clip(2.0, Some("second-video")),
        vec![audio_clip(2.0, "file:///second-audio.wav", None)],
    )
    .unwrap();
    let first_audio_id = first.audio_clips[0].0.clone();
    let second_audio_id = second.audio_clips[0].0.clone();

    assert!(stack.resize_item(
        &first_audio_id,
        1.0,
        2.0,
        OverlapPolicy::Override,
        false
    ));

    for item_id in ["first-video", &first_audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            1.0
        );
        assert_eq!(item.duration(), 2.0);
    }
    // The first column moved right by 1 and overrode the head of the second:
    // the second keeps its place and ids, 1s shorter, media offset moved on.
    for item_id in ["second-video", &second_audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            3.0
        );
        assert_eq!(item.duration(), 1.0);
        assert_eq!(source_start(item), 1.0);
    }
}

#[test]
fn modify_item_right_shrink_removes_trailing_gap_on_synced_clips() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(5.0, Some("video")),
        vec![audio_clip(5.0, "file:///audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.modify_item("video", 0.0, 3.0, false, false, false));

    for item_id in ["video", &audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            0.0
        );
        assert_eq!(item.duration(), 3.0);
        assert_eq!(item_index + 1, stack.children[track_index].items.len());
    }
}

#[test]
fn modify_item_left_shrink_leaves_gap_on_synced_clips() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(5.0, Some("video")),
        vec![audio_clip(5.0, "file:///audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.modify_item("video", 2.0, 3.0, false, true, false));

    for item_id in ["video", &audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(item_index, 1);
        assert!(matches!(stack.children[track_index].items[0], Item::Gap(_)));
        assert_eq!(stack.children[track_index].items[0].duration(), 2.0);
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            2.0
        );
        assert_eq!(source_start(item), 2.0);
        assert_eq!(item.duration(), 3.0);
    }
}

#[test]
fn modify_gap_shrink_updates_following_synced_assets() {
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio
        .items
        .push(Item::Gap(Gap::new(5.0, Some("audio-gap".to_string()))));
    audio.items.push(synced_clip_item(2.0, "audio", 1));

    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video
        .items
        .push(Item::Gap(Gap::new(5.0, Some("video-gap".to_string()))));
    video.items.push(synced_clip_item(2.0, "video", 1));

    let mut stack = Stack::default();
    stack.children.push(audio);
    stack.children.push(video);

    assert!(stack.modify_item("audio-gap", 0.0, 3.0, false, false, false));

    for item_id in ["audio", "video"] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            3.0
        );
        assert_eq!(item.duration(), 2.0);
    }
}

#[test]
fn modify_item_left_shrink_with_push_updates_synced_source_starts() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(5.0, Some("video")),
        vec![audio_clip(5.0, "file:///audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.modify_item("video", 2.0, 3.0, false, true, true));

    for item_id in ["video", &audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(item_index, 0);
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            0.0
        );
        assert_eq!(source_start(item), 2.0);
        assert_eq!(item.duration(), 3.0);
    }
}

#[test]
fn modify_item_from_audio_updates_synced_video() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(5.0, Some("video")),
        vec![audio_clip(5.0, "file:///audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.modify_item(&audio_id, 1.0, 3.0, false, true, false));

    for item_id in ["video", &audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(item_index, 1);
        assert!(matches!(stack.children[track_index].items[0], Item::Gap(_)));
        assert_eq!(stack.children[track_index].items[0].duration(), 1.0);
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            1.0
        );
        assert_eq!(source_start(item), 1.0);
        assert_eq!(item.duration(), 3.0);
    }
}

#[test]
fn modify_item_negative_source_start_clamps_synced_clips() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(5.0, Some("video")),
        vec![audio_clip(5.0, "file:///audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.modify_item("video", -1.0, 5.0, false, true, true));

    for item_id in ["video", &audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            0.0
        );
        assert_eq!(source_start(item), 0.0);
        assert_eq!(item.duration(), 4.0);
    }
}

#[test]
fn modify_item_extend_updates_synced_clips_duration() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("video")),
        vec![audio_clip(3.0, "file:///audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.modify_item("video", 0.0, 5.0, false, false, true));

    for item_id in ["video", &audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            0.0
        );
        assert_eq!(source_start(item), 0.0);
        assert_eq!(item.duration(), 5.0);
    }
}

#[test]
fn modify_item_negative_duration_deletes_synced_clips() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("video")),
        vec![audio_clip(3.0, "file:///audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.modify_item("video", 0.0, -1.0, false, false, false));

    assert!(stack.get_item("video").is_none());
    assert!(stack.get_item(&audio_id).is_none());
    assert!(stack
        .children
        .iter()
        .all(|track| track.items.iter().all(|item| matches!(item, Item::Gap(_)))));
}

#[test]
fn override_resize_that_trims_a_synced_clip_trims_its_partners_too() {
    // V1 owns A1 and A2. Clip 2 (plain, 2..3) sits right before a synced column
    // (clip1 + two audio, 3..5.5). Extending Clip 2 to 2..4 with Override cuts
    // the head of clip1: the audio partners must follow (start 4, 1.5s, media
    // offset +1s), not keep their old place.
    let mut stack = Stack::default();
    for id in ["A2", "A1"] {
        stack.children.push(Track::new(TrackKind::Audio, Some(id.into())));
    }
    stack.children.push(Track::new(TrackKind::Video, Some("V1".into())));
    assert!(stack.associate_tracks("V1", &["A1".into(), "A2".into()]));
    let v1 = track_index_by_id(&stack, "V1");
    assert!(stack
        .insert_item_at_time(
            v1,
            2.0,
            Item::Clip(clip(1.0, Some("clip2"))),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            None,
            None,
        )
        .is_some());
    let mut left = audio_clip(2.5, "file:///left.wav", None);
    left.set_id(Some("left".into()));
    let mut right = audio_clip(2.5, "file:///right.wav", None);
    right.set_id(Some("right".into()));
    assert!(stack
        .insert_item_at_time(
            v1,
            3.0,
            Item::Clip(clip(2.5, Some("clip1"))),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            Some(vec![left, right]),
            None,
        )
        .is_some());

    assert!(stack.resize_item("clip2", 2.0, 2.0, OverlapPolicy::Override, false));

    // The trimmed clip keeps its id.
    let (track_index, fragment_index, fragment) = stack.get_item("clip1").expect("clip1 keeps its id");
    let video = &stack.children[track_index];
    assert_eq!(video.get_id().as_deref(), Some("V1"));
    assert_eq!(video.start_time_of_item(fragment_index), 4.0);
    assert_eq!(fragment.duration(), 1.5);
    assert_eq!(source_start(fragment), 1.0);
    let group = sync_clips_id(fragment);
    assert!(group.is_some());
    for id in ["left", "right"] {
        let (track_index, item_index, item) = stack.get_item(id).unwrap();
        assert_eq!(stack.children[track_index].start_time_of_item(item_index), 4.0, "{id}");
        assert_eq!(item.duration(), 1.5, "{id}");
        assert_eq!(source_start(item), 1.0, "{id} media offset follows the trim");
        assert_eq!(sync_clips_id(item), group, "{id}");
    }
    assert_sync_clips_track_aligned(&stack, "override resize trims the column");
}

/// V1 owns A1 and A2. Column C at 0..2, plain clip2 at 2..3, column clip1 at
/// 3..4.5 (the layout of the left-handle bug report).
fn plain_clip_between_two_columns() -> Stack {
    let mut stack = Stack::default();
    for id in ["A2", "A1"] {
        stack.children.push(Track::new(TrackKind::Audio, Some(id.into())));
    }
    stack.children.push(Track::new(TrackKind::Video, Some("V1".into())));
    assert!(stack.associate_tracks("V1", &["A1".into(), "A2".into()]));
    let v1 = track_index_by_id(&stack, "V1");
    let column = |name: &str, duration: f64| -> Vec<Item> {
        (1..=2)
            .map(|i| {
                let mut item = audio_clip(duration, &format!("file:///{name}{i}.wav"), None);
                item.set_id(Some(format!("{name}-a{i}")));
                item
            })
            .collect()
    };
    for (time, id, duration, audio) in [
        (0.0, "C", 2.0, Some(column("C", 2.0))),
        (2.0, "clip2", 1.0, None),
        (3.0, "clip1", 1.5, Some(column("clip1", 1.5))),
    ] {
        assert!(stack
            .insert_item_at_time(
                v1,
                time,
                Item::Clip(clip(duration, Some(id))),
                OverlapPolicy::Override,
                InsertPolicy::SplitAndInsert,
                audio,
                None,
            )
            .is_some());
    }
    stack
}

fn span(stack: &Stack, id: &str) -> (String, f64, f64, f64) {
    let (track_index, item_index, item) = stack.get_item(id).unwrap();
    (
        stack.children[track_index].get_id().unwrap(),
        stack.children[track_index].start_time_of_item(item_index),
        item.duration(),
        source_start(item),
    )
}

#[test]
fn left_handle_growth_of_a_plain_clip_trims_the_previous_column_on_every_track() {
    for policy in [OverlapPolicy::Override, OverlapPolicy::Push] {
        let mut stack = plain_clip_between_two_columns();
        assert!(stack.resize_item("clip2", 1.5, 1.5, policy, false));
        assert_eq!(span(&stack, "clip2"), ("V1".into(), 1.5, 1.5, 0.0), "{policy:?}");
        // C is cut to 1.5s on V1 and on both partner tracks.
        for id in ["C", "C-a1", "C-a2"] {
            let (_, start, duration, _) = span(&stack, id);
            assert_eq!((start, duration), (0.0, 1.5), "{id} {policy:?}");
        }
        // clip1 and its partners did not move.
        for id in ["clip1", "clip1-a1", "clip1-a2"] {
            let (_, start, duration, _) = span(&stack, id);
            assert_eq!((start, duration), (3.0, 1.5), "{id} {policy:?}");
        }
        assert_sync_clips_track_aligned(&stack, "left-handle growth");
    }
}

#[test]
fn left_handle_shrink_of_a_plain_clip_changes_nothing_else() {
    let mut stack = plain_clip_between_two_columns();
    assert!(stack.resize_item("clip2", 2.5, 0.5, OverlapPolicy::Override, false));
    assert_eq!(span(&stack, "clip2"), ("V1".into(), 2.5, 0.5, 0.0));
    for id in ["C", "C-a1", "C-a2"] {
        let (_, start, duration, _) = span(&stack, id);
        assert_eq!((start, duration), (0.0, 2.0), "{id}");
    }
    for id in ["clip1", "clip1-a1", "clip1-a2"] {
        let (_, start, duration, _) = span(&stack, id);
        assert_eq!((start, duration), (3.0, 1.5), "{id}");
    }
    let v1 = &stack.children[track_index_by_id(&stack, "V1")];
    assert!(matches!(v1.items[v1.get_item_at_time(2.2).unwrap()], Item::Gap(_)));
    assert_sync_clips_track_aligned(&stack, "left-handle shrink");
}

#[test]
fn right_handle_growth_with_push_moves_the_next_column_on_every_track() {
    let mut stack = plain_clip_between_two_columns();
    assert!(stack.resize_item("clip2", 2.0, 2.0, OverlapPolicy::Push, false));
    assert_eq!(span(&stack, "clip2"), ("V1".into(), 2.0, 2.0, 0.0));
    for id in ["clip1", "clip1-a1", "clip1-a2"] {
        let (_, start, duration, _) = span(&stack, id);
        assert_eq!((start, duration), (4.0, 1.5), "{id}");
    }
    for id in ["C", "C-a1", "C-a2"] {
        let (_, start, duration, _) = span(&stack, id);
        assert_eq!((start, duration), (0.0, 2.0), "{id}");
    }
    assert_sync_clips_track_aligned(&stack, "right-handle push");
}
