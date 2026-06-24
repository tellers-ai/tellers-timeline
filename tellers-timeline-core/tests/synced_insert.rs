mod common;
use common::*;

#[test]
fn synced_insert_adds_primary_and_audio_tracks_without_touching_clips() {
    let mut video = Track::new(TrackKind::Video, Some("video-track".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(10.0)));

    let mut audio = Track::new(TrackKind::Audio, Some("audio-track".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(10.0)));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(audio);

    let result = insert_with_audio(
        &mut stack,
        0,
        2.0,
        clip(4.0, Some("primary-id")),
        vec![
            audio_clip(4.0, "file:///a1.wav", Some("same-media")),
            audio_clip(4.0, "file:///a2.wav", Some("same-media")),
            audio_clip(4.0, "file:///a3.wav", Some("same-media")),
        ],
    )
    .expect("linked insert should succeed");

    assert_eq!(result.primary_clip_id, "primary-id");
    assert_eq!(result.audio_clips.len(), 3);
    // Fresh insert: the video is not yet in a sync group, so three new audio tracks are
    // created directly below it (at indices [0, 1, 2]), pushing the video up to the top of
    // its group (index 3). The pre-existing empty audio track is unrelated and untouched,
    // ending up at the bottom (index 4). Nearest-first: the first audio clip sits directly
    // below the video (index 2), the last at index 0.
    assert_eq!(
        result
            .audio_clips
            .iter()
            .map(|(_, track_index)| *track_index)
            .collect::<Vec<_>>(),
        vec![2, 1, 0]
    );
    assert_eq!(result.created_track_indices, vec![0, 1, 2]);
    assert_eq!(stack.children.len(), 5);
    assert_eq!(stack.children[0].kind, TrackKind::Audio);
    assert_eq!(stack.children[1].kind, TrackKind::Audio);
    assert_eq!(stack.children[2].kind, TrackKind::Audio);
    assert_eq!(stack.children[3].kind, TrackKind::Video);
    assert_eq!(stack.children[4].kind, TrackKind::Audio);
    assert_eq!(stack.children[3].get_id().as_deref(), Some("video-track"));
    assert_eq!(stack.get_item("primary-id").unwrap().0, 3);
    assert_eq!(stack.children[4].get_id().as_deref(), Some("audio-track"));
    assert_eq!(stack.children[0].get_id().as_deref(), Some("A1"));
    assert_eq!(stack.children[0].name.as_deref(), Some("A1"));
    assert_eq!(stack.children[1].get_id().as_deref(), Some("A2"));
    assert_eq!(stack.children[1].name.as_deref(), Some("A2"));
    assert_eq!(stack.children[2].get_id().as_deref(), Some("A3"));
    assert_eq!(stack.children[2].name.as_deref(), Some("A3"));

    let primary = stack.get_item("primary-id").unwrap().2;
    assert_eq!(primary.duration(), 4.0);
    assert_eq!(sync_clips_id(primary), result.sync_clips_id);
    let (primary_track_index, primary_item_index, _) = stack.get_item("primary-id").unwrap();
    let primary_start = stack.children[primary_track_index].start_time_of_item(primary_item_index);

    for (audio_id, track_index) in result.audio_clips {
        let (actual_track_index, item_index, item) = stack.get_item(&audio_id).unwrap();
        assert_eq!(actual_track_index, track_index);
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            primary_start
        );
        assert_eq!(item.duration(), 4.0);
        assert_eq!(sync_clips_id(item), result.sync_clips_id);
    }
}

#[test]
fn synced_insert_master_clip_with_multiple_audio_clips_at_time() {
    let mut video = Track::new(TrackKind::Video, Some("video-track".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(10.0)));

    let mut stack = Stack::default();
    stack.children.push(video);

    let result = match stack.insert_item_at_time(
        0,
        2.0,
        Item::Clip(clip(4.0, Some("master-video"))),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        Some(vec![
            audio_clip(4.0, "file:///master-audio-1.wav", Some("master-media")),
            audio_clip(4.0, "file:///master-audio-2.wav", Some("master-media")),
            audio_clip(4.0, "file:///master-audio-3.wav", Some("master-media")),
        ]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("master clip linked insert should succeed"),
    };

    assert_eq!(result.primary_clip_id, "master-video");
    assert_eq!(result.audio_clips.len(), 3);
    // Fresh insert with only a video track: three new audio tracks are created below it
    // at indices [0, 1, 2], pushing the video up to the top of its group (index 3).
    assert_eq!(result.created_track_indices, vec![0, 1, 2]);
    let (primary_track_index, primary_item_index, primary_item) =
        stack.get_item("master-video").unwrap();
    assert_eq!(
        stack.children[primary_track_index].start_time_of_item(primary_item_index),
        2.0
    );
    assert_eq!(primary_item.duration(), 4.0);
    assert_eq!(sync_clips_id(primary_item), result.sync_clips_id);

    for (audio_id, track_index) in result.audio_clips {
        let (actual_track_index, item_index, audio_item) = stack.get_item(&audio_id).unwrap();
        assert_eq!(actual_track_index, track_index);
        assert_eq!(stack.children[track_index].kind, TrackKind::Audio);
        assert_eq!(stack.children[track_index].start_time_of_item(item_index), 2.0);
        assert_eq!(audio_item.duration(), 4.0);
        assert_eq!(sync_clips_id(audio_item), result.sync_clips_id);
    }
}

#[test]
fn synced_insert_places_audio_below_video_when_audio_track_exists_above() {
    let mut unrelated_audio = Track::new(TrackKind::Audio, Some("audio-above".to_string()));
    unrelated_audio
        .items
        .push(audio_clip(4.0, "file:///existing.wav", None));

    let mut video = Track::new(TrackKind::Video, Some("video-track".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(10.0)));

    let mut stack = Stack::default();
    stack.children.push(unrelated_audio);
    stack.children.push(video);

    let result = match stack.insert_item_at_time(
        1,
        2.0,
        Item::Clip(clip(3.0, Some("primary"))),
        OverlapPolicy::Push,
        InsertPolicy::SplitAndInsert,
        Some(vec![audio_clip(3.0, "file:///linked.wav", None)]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("linked insert should create audio below the target video track"),
    };

    // The audio-above track (idx0) sits below the destination video in the data model
    // but carries unrelated content, so it cannot be reused. A new audio track is
    // created directly below the video (at the video's index 1), pushing the video up
    // to index 2 where it stays on top of its group.
    assert_eq!(result.audio_clips[0].1, 1);
    assert_eq!(result.created_track_indices, vec![1]);
    assert_eq!(stack.children[0].get_id().as_deref(), Some("audio-above"));
    assert_eq!(stack.children[2].get_id().as_deref(), Some("video-track"));
    assert_eq!(stack.children[1].kind, TrackKind::Audio);
    let audio_track = &stack.children[1];
    assert!(matches!(audio_track.items[0], Item::Gap(_)));
    assert_eq!(audio_track.items[0].duration(), 2.0);
    assert_eq!(
        audio_track.items[1].get_id(),
        Some(result.audio_clips[0].0.clone())
    );
    assert_eq!(audio_track.items[1].duration(), 3.0);
}

#[test]
fn synced_insert_creates_audio_track_below_video_when_cluster_has_no_audio() {
    // Standard "video on top" layout: audio sits below the video at a lower index, but
    // unless it is already in the destination sync cluster the insert creates a fresh
    // audio track directly below the video instead of scanning for a free boundary track.
    let mut audio = Track::new(TrackKind::Audio, Some("audio-track".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(10.0)));
    let mut video = Track::new(TrackKind::Video, Some("video-track".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(10.0)));

    let mut stack = Stack::default();
    stack.children.push(audio);
    stack.children.push(video);

    let result = insert_with_audio(
        &mut stack,
        1,
        2.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .expect("linked insert should succeed");

    assert_eq!(result.created_track_indices, vec![1]);
    assert_eq!(stack.children.len(), 3);
    assert_eq!(stack.children[0].get_id().as_deref(), Some("audio-track"));
    assert_eq!(stack.children[1].get_id().as_deref(), Some("A1"));
    assert_eq!(stack.children[2].get_id().as_deref(), Some("video-track"));
    assert_eq!(result.audio_clips.len(), 1);
    assert_eq!(result.audio_clips[0].1, 1);

    // Primary and sync track stay aligned and share a link group.
    let (primary_track_index, primary_item_index, _) = stack.get_item("primary").unwrap();
    let primary_start = stack.children[primary_track_index].start_time_of_item(primary_item_index);
    let (audio_id, _) = &result.audio_clips[0];
    let (audio_track_index, audio_item_index, audio_item) = stack.get_item(audio_id).unwrap();
    assert_eq!(audio_track_index, 1);
    assert_eq!(
        stack.children[audio_track_index].start_time_of_item(audio_item_index),
        primary_start
    );
    assert_eq!(sync_clips_id(audio_item), result.sync_clips_id);
}

#[test]
fn synced_insert_creates_audio_tracks_for_each_synced_audio_clip() {
    // Two synced audio clips each get a freshly created track directly below the video
    // when they are not already present in the destination sync cluster.
    let mut far_audio = Track::new(TrackKind::Audio, Some("far-audio".to_string()));
    far_audio.items.push(Item::Gap(Gap::make_gap(10.0)));
    let mut empty_audio = Track::new(TrackKind::Audio, Some("empty-audio".to_string()));
    empty_audio.items.push(Item::Gap(Gap::make_gap(10.0)));
    let mut video = Track::new(TrackKind::Video, Some("video-track".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(10.0)));

    let mut stack = Stack::default();
    stack.children.push(far_audio);
    stack.children.push(empty_audio);
    stack.children.push(video);

    let result = insert_with_audio(
        &mut stack,
        2,
        1.0,
        clip(2.0, Some("primary")),
        vec![
            audio_clip(2.0, "file:///a1.wav", None),
            audio_clip(2.0, "file:///a2.wav", None),
        ],
    )
    .unwrap();

    assert_eq!(
        result
            .audio_clips
            .iter()
            .map(|(_, track_index)| *track_index)
            .collect::<Vec<_>>(),
        vec![3, 2]
    );
    assert_eq!(result.created_track_indices, vec![2, 3]);
    assert_eq!(stack.children.len(), 5);
    assert_eq!(stack.children[0].get_id().as_deref(), Some("far-audio"));
    assert_eq!(stack.children[1].get_id().as_deref(), Some("empty-audio"));
    assert_eq!(stack.get_item("primary").unwrap().0, 4);
}

#[test]
fn synced_insert_regenerates_colliding_timeline_ids_and_preserves_media_id() {
    let mut video = Track::new(TrackKind::Video, Some("video-track".to_string()));
    let existing = clip(1.0, Some("duplicate-id"));
    video.items.push(Item::Clip(existing));
    video.items.push(Item::Gap(Gap::make_gap(10.0)));

    let mut stack = Stack::default();
    stack.children.push(video);

    let result = insert_with_audio(
        &mut stack,
        0,
        1.0,
        clip(2.0, Some("duplicate-id")),
        vec![audio_clip(2.0, "file:///a1.wav", Some("shared-media"))],
    )
    .expect("linked insert should create an audio track");

    assert_ne!(result.primary_clip_id, "duplicate-id");
    // The sync audio track is created below the video (at the video's index 0), pushing
    // the video up to index 1 where it stays on top of its group.
    assert_eq!(stack.get_item("duplicate-id").unwrap().0, 1);

    let audio_item = stack.children[result.audio_clips[0].1]
        .items
        .iter()
        .find(|item| matches!(item, Item::Clip(_)))
        .expect("expected audio clip");
    let Item::Clip(audio_clip) = audio_item else {
        panic!("expected audio clip");
    };
    let media = audio_clip.media_references.get("DEFAULT_MEDIA").unwrap();
    assert_eq!(
        media
            .metadata()
            .get("tellers.ai")
            .and_then(|v| v.get("media_id"))
            .and_then(|v| v.as_str()),
        Some("shared-media")
    );
    assert_eq!(
        media.metadata().get("media_id").and_then(|v| v.as_str()),
        Some("shared-media")
    );
}

#[test]
fn synced_insert_uses_normal_primary_insert_on_video_conflict() {
    let mut video = Track::new(TrackKind::Video, Some("video-track".to_string()));
    video.items.push(Item::Clip(clip(5.0, Some("existing"))));

    let mut stack = Stack::default();
    stack.children.push(video);

    // No synced audio clips, so the insert returns a plain ItemId (not Synced) and just
    // splits the existing clip on the destination track.
    let result = stack.insert_item_at_time(
        0,
        1.0,
        Item::Clip(clip(2.0, None)),
        OverlapPolicy::Override,
        InsertPolicy::InsertBefore,
        Some(vec![]),
    None,
    );

    assert!(matches!(result, Some(InsertItemAtTimeResult::ItemId(_))));
    assert_eq!(stack.children[0].items.len(), 2);
    assert_eq!(stack.children[0].items[0].duration(), 2.0);
    assert_eq!(stack.children[0].items[1].duration(), 3.0);
    assert!(stack.children[0].items[1].get_id().is_some());
}

#[test]
fn synced_insert_clamps_primary_clip_to_active_available_range() {
    let mut video = Track::new(TrackKind::Video, Some("video-track".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(10.0)));

    let mut stack = Stack::default();
    stack.children.push(video);

    // No synced audio clips, so the insert returns a plain ItemId. The primary clip is
    // still clamped to its active available range.
    let primary_id = match stack.insert_item_at_time(
        0,
        0.0,
        Item::Clip(clip_with_media_range(10.0, 2.0, 0.0, 5.0)),
        OverlapPolicy::Override,
        InsertPolicy::InsertBefore,
        Some(vec![]),
    None,
    ) {
        Some(InsertItemAtTimeResult::ItemId(id)) => id,
        other => panic!("insert should clamp and succeed: {other:?}"),
    };

    let item = stack.children[0]
        .items
        .iter()
        .find(|item| matches!(item, Item::Clip(_)))
        .unwrap();
    let Item::Clip(clip) = item else {
        panic!("expected clip");
    };
    assert_eq!(primary_id, clip.get_id().unwrap());
    assert_eq!(clip.source_range.start_time.value, 2.0);
    assert_eq!(clip.source_range.duration.value, 3.0);
    assert_eq!(sync_clips_id(item), None);
}

#[test]
fn synced_insert_clamps_synced_audio_before_duration_check() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));

    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("primary")),
        vec![audio_clip_with_available_duration(
            10.0,
            "file:///short-audio.wav",
            3.0,
        )],
    )
    .expect("linked audio should clamp before duration validation");

    let (audio_id, audio_track_index) = &result.audio_clips[0];
    let (actual_audio_track, _, audio_item) = stack.get_item(audio_id).unwrap();
    assert_eq!(actual_audio_track, *audio_track_index);
    assert_eq!(audio_item.duration(), 3.0);
    assert_eq!(sync_clips_id(audio_item), result.sync_clips_id);
}

#[test]
fn synced_insert_allows_synced_audio_with_different_duration() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));

    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(8.0, Some("primary")),
        vec![Item::Clip(Clip::new_single_media_reference(
            range(3.0),
            MediaReference::ExternalReference {
                target_url: "file:///short-audio.wav".to_string(),
                available_range: Some(range(3.0)),
                name: None,
                available_image_bounds: Some(serde_json::Value::Null),
                metadata: serde_json::json!({}),
            },
            None,
            None,
        ))],
    )
    .expect("linked audio may differ in duration from the primary");

    let (audio_id, _) = &result.audio_clips[0];
    let (_, _, primary_item) = stack.get_item("primary").unwrap();
    let (_, _, audio_item) = stack.get_item(audio_id).unwrap();
    assert_eq!(primary_item.duration(), 8.0);
    assert_eq!(audio_item.duration(), 3.0);
    assert_eq!(sync_clips_id(primary_item), sync_clips_id(audio_item));
}

#[test]
fn synced_insert_allows_sync_video_with_different_duration_from_primary() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("primary-audio-track".to_string())));

    let mut primary = audio_clip(5.0, "file:///primary.wav", None);
    primary.set_id(Some("primary-audio".to_string()));
    let synced_video = Item::Clip(clip(2.0, Some("synced-video")));

    let result = match stack.insert_item_at_time(
        0,
        0.0,
        primary,
        OverlapPolicy::Push,
        InsertPolicy::InsertBeforeOrAfter,
        None,
        Some(synced_video),
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("audio-primary insert with shorter sync video should succeed"),
    };

    let (_, _, primary_item) = stack.get_item("primary-audio").unwrap();
    let (_, _, video_item) = stack.get_item(result.synced_video_clip_id.as_deref().unwrap()).unwrap();
    assert_eq!(primary_item.duration(), 5.0);
    assert_eq!(video_item.duration(), 2.0);
    assert_eq!(sync_clips_id(primary_item), sync_clips_id(video_item));
}

#[test]
fn synced_insert_at_index_adds_audio_sync_track() {
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(5.0)));
    stack.children.push(video);

    let result = match stack.insert_item_at_index(
        "v",
        0,
        Item::Clip(clip(3.0, Some("primary"))),
        OverlapPolicy::Override,
        Some(vec![audio_clip(3.0, "file:///audio.wav", None)]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("linked index insert should succeed"),
    };

    assert_eq!(result.primary_clip_id, "primary");
    assert_eq!(result.audio_clips.len(), 1);
    // The sync audio track is created below the video (at the video's index 0), pushing
    // the video up to index 1 where it stays on top of its group.
    assert_eq!(result.created_track_indices, vec![0]);
    assert_eq!(
        sync_clips_id(stack.get_item("primary").unwrap().2),
        result.sync_clips_id
    );
    assert_eq!(
        sync_clips_id(stack.get_item(&result.audio_clips[0].0).unwrap().2),
        result.sync_clips_id
    );
}

#[test]
fn synced_insert_fails_when_available_range_leaves_zero_duration() {
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(10.0)));
    stack.children.push(video.clone());

    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip_with_media_range(2.0, 10.0, 0.0, 5.0),
        vec![],
    );

    assert!(result.is_none());
    assert_eq!(stack.children, vec![video]);
}

#[test]
fn track_timeline_ids_returns_child_item_ids_in_order() {
    let mut track = Track::new(TrackKind::Video, Some("track".to_string()));
    track.items.push(Item::Clip(clip(1.0, Some("clip-1"))));
    track
        .items
        .push(Item::Gap(Gap::new(1.0, Some("gap-1".to_string()))));
    track.items.push(Item::Clip(clip(1.0, Some("clip-2"))));

    assert_eq!(track.timeline_ids(), vec!["clip-1", "gap-1", "clip-2"]);
}

#[test]
fn created_sync_tracks_use_numbered_names_without_colliding() {
    let mut existing = Track::new(TrackKind::Audio, Some("A1".to_string()));
    existing.name = Some("A2".to_string());
    existing
        .items
        .push(audio_clip(3.0, "file:///existing.wav", None));
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(10.0)));

    let mut stack = Stack::default();
    stack.children.push(existing);
    stack.children.push(video);

    let result = insert_with_audio(
        &mut stack,
        1,
        0.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();

    let track = &stack.children[result.audio_clips[0].1];
    assert_eq!(track.get_id().as_deref(), Some("A3"));
    assert_eq!(track.name.as_deref(), Some("A3"));
}
