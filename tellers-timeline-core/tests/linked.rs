use tellers_timeline_core::{
    Clip, Gap, IdMetadataExt, InsertItemAtTimeResult, InsertPolicy, Item, LinkedInsertResult,
    MediaReference, OverlapPolicy, RationalTime, Stack, TimeRange, Track, TrackKind,
};

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

fn media_ref(url: &str, media_id: Option<&str>) -> MediaReference {
    let metadata = media_id
        .map(|id| {
            serde_json::json!({
                "media_id": id,
                "tellers.ai": {
                    "media_id": id
                }
            })
        })
        .unwrap_or_else(|| serde_json::json!({}));

    MediaReference::ExternalReference {
        target_url: url.to_string(),
        available_range: Some(range(100.0)),
        name: None,
        available_image_bounds: Some(serde_json::Value::Null),
        metadata,
    }
}

fn clip(duration: f64, id: Option<&str>) -> Clip {
    Clip::new_single_media_reference(
        range(duration),
        media_ref("file:///video.mov", Some("shared-media")),
        None,
        id.map(|s| s.to_string()),
    )
}

fn audio_clip(duration: f64, url: &str, media_id: Option<&str>) -> Item {
    Item::Clip(Clip::new_single_media_reference(
        range(duration),
        media_ref(url, media_id),
        None,
        None,
    ))
}

fn clip_with_media_range(
    duration: f64,
    source_start: f64,
    media_start: f64,
    media_duration: f64,
) -> Clip {
    let mut c = clip(duration, None);
    c.source_range.start_time.value = source_start;
    c.media_references.insert(
        "DEFAULT_MEDIA".to_string(),
        MediaReference::ExternalReference {
            target_url: "file:///ranged.mov".to_string(),
            available_range: Some(TimeRange {
                otio_schema: "TimeRange.1".to_string(),
                start_time: RationalTime {
                    otio_schema: "RationalTime.1".to_string(),
                    rate: 1.0,
                    value: media_start,
                },
                duration: RationalTime {
                    otio_schema: "RationalTime.1".to_string(),
                    rate: 1.0,
                    value: media_duration,
                },
            }),
            name: None,
            available_image_bounds: Some(serde_json::Value::Null),
            metadata: serde_json::json!({}),
        },
    );
    c
}

fn link_group_id(item: &Item) -> Option<i64> {
    match item {
        Item::Clip(clip) => clip
            .metadata
            .get("Resolve_OTIO")
            .and_then(|v| v.get("Link Group ID"))
            .and_then(|v| v.as_i64()),
        Item::Gap(_) => None,
    }
}

fn source_start(item: &Item) -> f64 {
    match item {
        Item::Clip(clip) => clip.source_range.start_time.value,
        Item::Gap(gap) => gap.source_range.start_time.value,
    }
}

fn active_target_url(item: &Item) -> Option<&str> {
    let Item::Clip(clip) = item else {
        return None;
    };
    let key = clip
        .active_media_reference_key
        .as_deref()
        .unwrap_or("DEFAULT_MEDIA");
    clip.media_references
        .get(key)
        .and_then(MediaReference::target_url)
        .map(String::as_str)
}

fn insert_with_audio(
    stack: &mut Stack,
    dest_track_index: usize,
    dest_time: f64,
    clip: Clip,
    linked_audio_clips: Vec<Item>,
) -> Option<LinkedInsertResult> {
    match stack.insert_item_at_time(
        dest_track_index,
        dest_time,
        Item::Clip(clip),
        OverlapPolicy::Override,
        InsertPolicy::InsertBefore,
        Some(linked_audio_clips),
        None,
    ) {
        Some(InsertItemAtTimeResult::Linked(result)) => Some(result),
        _ => None,
    }
}

#[test]
fn linked_insert_adds_primary_and_audio_tracks_without_touching_clips() {
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
    assert_eq!(
        result
            .audio_clips
            .iter()
            .map(|(_, track_index)| *track_index)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(result.created_track_indices, vec![2, 3]);
    assert_eq!(stack.children.len(), 4);
    assert_eq!(stack.children[0].kind, TrackKind::Video);
    assert_eq!(stack.children[1].kind, TrackKind::Audio);
    assert_eq!(stack.children[2].kind, TrackKind::Audio);
    assert_eq!(stack.children[3].kind, TrackKind::Audio);
    assert_eq!(stack.children[1].get_id().as_deref(), Some("audio-track"));
    assert_eq!(stack.children[2].get_id().as_deref(), Some("A1"));
    assert_eq!(stack.children[2].name.as_deref(), Some("A1"));
    assert_eq!(stack.children[3].get_id().as_deref(), Some("A2"));
    assert_eq!(stack.children[3].name.as_deref(), Some("A2"));
    assert_eq!(stack.get_item("primary-id").unwrap().0, 0);

    let primary = stack.get_item("primary-id").unwrap().2;
    assert_eq!(primary.duration(), 4.0);
    assert_eq!(link_group_id(primary), result.link_group_id);
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
        assert_eq!(link_group_id(item), result.link_group_id);
    }
}

#[test]
fn linked_insert_master_clip_with_multiple_audio_clips_at_time() {
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
        Some(InsertItemAtTimeResult::Linked(result)) => result,
        _ => panic!("master clip linked insert should succeed"),
    };

    assert_eq!(result.primary_clip_id, "master-video");
    assert_eq!(result.audio_clips.len(), 3);
    assert_eq!(result.created_track_indices, vec![1, 2, 3]);
    let (primary_track_index, primary_item_index, primary_item) =
        stack.get_item("master-video").unwrap();
    assert_eq!(
        stack.children[primary_track_index].start_time_of_item(primary_item_index),
        2.0
    );
    assert_eq!(primary_item.duration(), 4.0);
    assert_eq!(link_group_id(primary_item), result.link_group_id);

    for (audio_id, track_index) in result.audio_clips {
        let (actual_track_index, item_index, audio_item) = stack.get_item(&audio_id).unwrap();
        assert_eq!(actual_track_index, track_index);
        assert_eq!(stack.children[track_index].kind, TrackKind::Audio);
        assert_eq!(stack.children[track_index].start_time_of_item(item_index), 2.0);
        assert_eq!(audio_item.duration(), 4.0);
        assert_eq!(link_group_id(audio_item), result.link_group_id);
    }
}

#[test]
fn linked_insert_creates_audio_track_before_unlinked_boundary_clip() {
    let mut video = Track::new(TrackKind::Video, Some("video-track".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(10.0)));

    let mut blocked_audio = Track::new(TrackKind::Audio, Some("blocked-audio".to_string()));
    blocked_audio
        .items
        .push(audio_clip(4.0, "file:///existing.wav", None));

    let mut later_audio = Track::new(TrackKind::Audio, Some("later-audio".to_string()));
    later_audio.items.push(Item::Gap(Gap::make_gap(10.0)));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(blocked_audio);
    stack.children.push(later_audio);

    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///linked.wav", None)],
    )
    .expect("linked insert should create a track before the unlinked clip");

    assert_eq!(result.created_track_indices, vec![1]);
    assert_eq!(result.audio_clips[0].1, 1);
    assert_eq!(stack.children[0].kind, TrackKind::Video);
    assert_eq!(stack.children[1].kind, TrackKind::Audio);
    assert_eq!(stack.children[2].get_id().as_deref(), Some("blocked-audio"));
    assert_eq!(stack.children[3].get_id().as_deref(), Some("later-audio"));
}

#[test]
fn linked_insert_places_audio_below_video_when_audio_track_exists_above() {
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
        Some(InsertItemAtTimeResult::Linked(result)) => result,
        _ => panic!("linked insert should create audio below the target video track"),
    };

    assert_eq!(result.audio_clips[0].1, 2);
    assert_eq!(result.created_track_indices, vec![2]);
    assert_eq!(stack.children[0].get_id().as_deref(), Some("audio-above"));
    assert_eq!(stack.children[1].get_id().as_deref(), Some("video-track"));
    assert_eq!(stack.children[2].kind, TrackKind::Audio);
    let audio_track = &stack.children[2];
    assert!(matches!(audio_track.items[0], Item::Gap(_)));
    assert_eq!(audio_track.items[0].duration(), 2.0);
    assert_eq!(
        audio_track.items[1].get_id(),
        Some(result.audio_clips[0].0.clone())
    );
    assert_eq!(audio_track.items[1].duration(), 3.0);
}

#[test]
fn linked_insert_moves_empty_audio_track_above_video_into_link_block() {
    let mut empty_audio = Track::new(TrackKind::Audio, Some("empty-audio".to_string()));
    empty_audio.items.push(Item::Gap(Gap::make_gap(10.0)));

    let mut video = Track::new(TrackKind::Video, Some("video-track".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(10.0)));

    let mut stack = Stack::default();
    stack.children.push(empty_audio);
    stack.children.push(video);

    let result = match stack.insert_item_at_time(
        1,
        5.0,
        Item::Clip(clip(2.0, Some("primary"))),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        Some(vec![audio_clip(2.0, "file:///linked.wav", None)]),
        None,
    ) {
        Some(InsertItemAtTimeResult::Linked(result)) => result,
        _ => panic!("linked insert should reuse the empty audio track below video"),
    };

    assert_eq!(stack.children.len(), 2);
    assert_eq!(stack.children[0].get_id().as_deref(), Some("video-track"));
    assert_eq!(stack.children[1].get_id().as_deref(), Some("empty-audio"));
    assert_eq!(result.audio_clips[0].1, 1);
    assert_eq!(result.created_track_indices, Vec::<usize>::new());
    assert_eq!(stack.get_item("primary").unwrap().0, 0);
    assert_eq!(stack.get_item(&result.audio_clips[0].0).unwrap().0, 1);
}

#[test]
fn linked_insert_does_not_cross_empty_audio_track_boundary() {
    let mut video = Track::new(TrackKind::Video, Some("video-track".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(10.0)));
    let mut empty_audio = Track::new(TrackKind::Audio, Some("empty-audio".to_string()));
    empty_audio.items.push(Item::Gap(Gap::make_gap(10.0)));
    let mut far_audio = Track::new(TrackKind::Audio, Some("far-audio".to_string()));
    far_audio.items.push(Item::Gap(Gap::make_gap(10.0)));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(empty_audio);
    stack.children.push(far_audio);

    let result = insert_with_audio(
        &mut stack,
        0,
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
        vec![1, 2]
    );
    assert_eq!(result.created_track_indices, vec![2]);
    assert_eq!(stack.children[0].get_id().as_deref(), Some("video-track"));
    assert_eq!(stack.children[1].get_id().as_deref(), Some("empty-audio"));
    assert_eq!(stack.children[3].get_id().as_deref(), Some("far-audio"));
    assert!(stack.children[3]
        .items
        .iter()
        .all(|item| matches!(item, Item::Gap(_))));
    assert_eq!(stack.get_item("primary").unwrap().0, 0);
}

#[test]
fn linked_insert_regenerates_colliding_timeline_ids_and_preserves_media_id() {
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
    assert_eq!(stack.get_item("duplicate-id").unwrap().0, 0);

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
fn linked_insert_uses_normal_primary_insert_on_video_conflict() {
    let mut video = Track::new(TrackKind::Video, Some("video-track".to_string()));
    video.items.push(Item::Clip(clip(5.0, Some("existing"))));

    let mut stack = Stack::default();
    stack.children.push(video);

    let result = insert_with_audio(&mut stack, 0, 1.0, clip(2.0, None), vec![]);

    assert!(result.is_some());
    assert_eq!(stack.children[0].items.len(), 2);
    assert_eq!(stack.children[0].items[0].duration(), 2.0);
    assert_eq!(stack.children[0].items[1].duration(), 3.0);
    assert!(stack.children[0].items[1].get_id().is_some());
}

#[test]
fn insert_into_linked_clip_adds_spacer_gap_on_same_link_group_track() {
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

    assert!(matches!(result, Some(InsertItemAtTimeResult::Linked(_))));
    assert_eq!(stack.get_item("inserted").unwrap().0, primary_track_index);
    let audio_track = &stack.children[first.audio_clips[0].1];
    let spacer_index = audio_track.get_item_at_time(1.0).unwrap();
    assert!(matches!(audio_track.items[spacer_index], Item::Gap(_)));
    assert_eq!(audio_track.items[spacer_index].duration(), 1.0);
}

#[test]
fn insert_into_linked_clip_updates_every_same_link_group_track() {
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

    assert!(matches!(result, Some(InsertItemAtTimeResult::Linked(_))));
    for (_, track_index) in first.audio_clips {
        let track = &stack.children[track_index];
        let spacer_index = track.get_item_at_time(1.0).unwrap();
        assert!(matches!(track.items[spacer_index], Item::Gap(_)));
        assert_eq!(track.items[spacer_index].duration(), 1.0);
    }
}

#[test]
fn insert_unlinked_clip_with_push_moves_later_linked_assets() {
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

    assert!(matches!(insert_result, Some(InsertItemAtTimeResult::Linked(_))));
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
fn insert_unlinked_clip_at_index_with_push_moves_later_linked_assets() {
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

    assert!(matches!(insert_result, Some(InsertItemAtTimeResult::Linked(_))));
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
fn insert_unlinked_clip_before_policy_pushes_linked_assets_from_boundary() {
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

    assert!(matches!(insert_result, Some(InsertItemAtTimeResult::Linked(_))));
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
fn insert_unlinked_clip_after_policy_does_not_touch_linked_assets() {
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
}

#[test]
fn insert_unlinked_clip_override_after_policy_does_not_gap_linked_asset() {
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
}

#[test]
fn insert_into_linked_clip_stops_at_empty_track_boundary() {
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
    let audio_track_index = first.audio_clips[0].1;
    let mut empty = Track::new(TrackKind::Audio, Some("empty".to_string()));
    empty.items.push(Item::Gap(Gap::make_gap(4.0)));
    stack.children.insert(audio_track_index + 1, empty);
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

    assert!(matches!(result, Some(InsertItemAtTimeResult::Linked(_))));
    assert_eq!(stack.children[audio_track_index].items.len(), 3);
    assert_eq!(stack.children[audio_track_index + 1].items.len(), 1);
    assert!(matches!(
        stack.children[audio_track_index + 1].items[0],
        Item::Gap(_)
    ));
}

#[test]
fn insert_without_link_group_only_changes_destination_track() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(4.0)));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio
        .items
        .push(Item::Clip(clip(4.0, Some("unlinked-audio"))));
    let before_audio = audio.items.clone();

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(audio);

    let result = stack.insert_item_at_time(
        0,
        1.0,
        Item::Clip(clip(1.0, Some("inserted"))),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        None,
        None,
    );

    assert!(matches!(result, Some(InsertItemAtTimeResult::ItemId(_))));
    assert!(stack.get_item("inserted").is_some());
    assert_eq!(stack.children[1].items, before_audio);
}

#[test]
fn insert_into_linked_clip_replaces_same_link_group_spacer_with_new_linked_audio() {
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
        Some(InsertItemAtTimeResult::Linked(result)) => result,
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
        link_group_id(&audio_track.items[audio_index]),
        result.link_group_id
    );
}

#[test]
fn linked_insert_clamps_primary_clip_to_active_available_range() {
    let mut video = Track::new(TrackKind::Video, Some("video-track".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(10.0)));

    let mut stack = Stack::default();
    stack.children.push(video);

    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip_with_media_range(10.0, 2.0, 0.0, 5.0),
        vec![],
    )
    .expect("insert should clamp and succeed");

    let item = stack.children[0]
        .items
        .iter()
        .find(|item| matches!(item, Item::Clip(_)))
        .unwrap();
    let Item::Clip(clip) = item else {
        panic!("expected clip");
    };
    assert_eq!(result.primary_clip_id, clip.get_id().unwrap());
    assert_eq!(clip.source_range.start_time.value, 2.0);
    assert_eq!(clip.source_range.duration.value, 3.0);
    assert_eq!(result.link_group_id, None);
    assert_eq!(link_group_id(item), None);
}

#[test]
fn linked_insert_rejects_linked_audio_with_different_duration() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let original = stack.clone();

    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(8.0, Some("primary")),
        vec![Item::Clip(Clip::new_single_media_reference(
            range(8.0),
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
    .is_none();

    assert!(result);
    assert_eq!(stack.children, original.children);
}

#[test]
fn linked_insert_rejects_linked_video_with_different_duration() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("a".to_string())));
    let original = stack.clone();

    let result = stack.insert_item_at_time(
        0,
        0.0,
        audio_clip(4.0, "file:///audio.wav", None),
        OverlapPolicy::Override,
        InsertPolicy::InsertBefore,
        None,
        Some(Item::Clip(clip(3.0, Some("video")))),
    );

    assert!(result.is_none());
    assert_eq!(stack.children, original.children);
}

#[test]
fn linked_insert_can_add_video_for_audio_primary() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("a".to_string())));

    let result = match stack.insert_item_at_time(
        0,
        0.0,
        audio_clip(4.0, "file:///audio.wav", None),
        OverlapPolicy::Override,
        InsertPolicy::InsertBefore,
        None,
        Some(Item::Clip(clip(4.0, Some("video")))),
    ) {
        Some(InsertItemAtTimeResult::Linked(result)) => result,
        _ => panic!("linked insert should succeed"),
    };

    assert_eq!(result.linked_video_clip_id.as_deref(), Some("video"));
    assert_eq!(result.created_track_indices, vec![0]);
    assert_eq!(stack.children[0].kind, TrackKind::Video);
    assert_eq!(stack.children[1].kind, TrackKind::Audio);
    assert_eq!(
        link_group_id(stack.get_item(&result.primary_clip_id).unwrap().2),
        result.link_group_id
    );
    assert_eq!(
        link_group_id(stack.get_item("video").unwrap().2),
        result.link_group_id
    );
}

#[test]
fn linked_insert_at_index_adds_audio_companion() {
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
        Some(InsertItemAtTimeResult::Linked(result)) => result,
        _ => panic!("linked index insert should succeed"),
    };

    assert_eq!(result.primary_clip_id, "primary");
    assert_eq!(result.audio_clips.len(), 1);
    assert_eq!(result.created_track_indices, vec![1]);
    assert_eq!(
        link_group_id(stack.get_item("primary").unwrap().2),
        result.link_group_id
    );
    assert_eq!(
        link_group_id(stack.get_item(&result.audio_clips[0].0).unwrap().2),
        result.link_group_id
    );
}

#[test]
fn linked_insert_keeps_same_content_audio_and_removes_same_content_video_input() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));

    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", Some("media"))],
    )
    .unwrap();

    // Sanity check helper still creates a companion when the id is distinct.
    assert_eq!(result.audio_clips.len(), 1);

    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("primary")),
        vec![Item::Clip(clip(3.0, Some("different-id")))],
    )
    .unwrap();

    assert_eq!(result.audio_clips.len(), 1);
    assert!(result.link_group_id.is_some());
    assert_eq!(stack.children.len(), 2);
    assert_eq!(
        stack.children[result.audio_clips[0].1].kind,
        TrackKind::Audio
    );

    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("a".to_string())));
    let result = match stack.insert_item_at_time(
        0,
        0.0,
        Item::Clip(clip(3.0, Some("audio-primary"))),
        OverlapPolicy::Override,
        InsertPolicy::InsertBefore,
        None,
        Some(Item::Clip(clip(3.0, Some("different-video-id")))),
    ) {
        Some(InsertItemAtTimeResult::Linked(result)) => result,
        _ => panic!("expected linked result"),
    };

    assert_eq!(result.linked_video_clip_id, None);
    assert_eq!(result.link_group_id, None);
    assert_eq!(stack.children.len(), 1);

    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = match stack.insert_item_at_time(
        0,
        0.0,
        Item::Clip(clip(3.0, Some("primary"))),
        OverlapPolicy::Override,
        InsertPolicy::InsertBefore,
        None,
        Some(Item::Clip(clip(3.0, Some("different-video-id")))),
    ) {
        Some(InsertItemAtTimeResult::Linked(result)) => result,
        _ => panic!("expected linked result"),
    };

    assert_eq!(result.linked_video_clip_id, None);
    assert_eq!(result.link_group_id, None);
    assert_eq!(stack.children.len(), 1);
}

#[test]
fn linked_insert_fails_when_available_range_leaves_zero_duration() {
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
fn linked_delete_can_remove_entire_link_group() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("a".to_string())));

    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();

    let removed = stack.delete_item("primary", true);
    assert_eq!(removed.len(), 2);
    assert!(stack.children[0]
        .items
        .iter()
        .all(|item| matches!(item, Item::Gap(_))));
    assert!(stack.children[result.audio_clips[0].1]
        .items
        .iter()
        .all(|item| matches!(item, Item::Gap(_))));

    let gap_ids: Vec<_> = stack
        .children
        .iter()
        .flat_map(|track| track.items.iter())
        .filter_map(|item| item.get_id())
        .collect();
    let unique: std::collections::HashSet<_> = gap_ids.iter().collect();
    assert_eq!(gap_ids.len(), unique.len());
}

#[test]
fn linked_delete_removes_video_asset_linked_to_audio_primary() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("a".to_string())));

    let result = match stack.insert_item_at_time(
        0,
        0.0,
        audio_clip(4.0, "file:///audio.wav", None),
        OverlapPolicy::Override,
        InsertPolicy::InsertBefore,
        None,
        Some(Item::Clip(clip(4.0, Some("video")))),
    ) {
        Some(InsertItemAtTimeResult::Linked(result)) => result,
        _ => panic!("linked insert should succeed"),
    };

    let removed = stack.delete_item(&result.primary_clip_id, true);

    assert_eq!(removed.len(), 2);
    assert!(stack.get_item(&result.primary_clip_id).is_none());
    assert!(stack.get_item("video").is_none());
    assert!(stack
        .children
        .iter()
        .all(|track| track.items.iter().all(|item| matches!(item, Item::Gap(_)))));
}

#[test]
fn linked_delete_keeps_touched_tracks_without_remaining_clips() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));

    insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();

    let removed = stack.delete_item("primary", true);

    assert_eq!(removed.len(), 2);
    assert_eq!(stack.children.len(), 2);
    assert!(stack
        .children
        .iter()
        .all(|track| track.items.iter().all(|item| matches!(item, Item::Gap(_)))));
}

#[test]
fn delete_unlinked_item_only_removes_selected_item() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(3.0, Some("primary"))));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio
        .items
        .push(Item::Clip(clip(3.0, Some("unlinked-audio"))));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(audio);

    let removed = stack.delete_item("primary", true);

    assert_eq!(removed.len(), 1);
    assert!(stack.get_item("primary").is_none());
    assert!(stack.get_item("unlinked-audio").is_some());
}

#[test]
fn delete_unlinked_item_without_gap_pulls_later_linked_assets() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(1.0, Some("unlinked"))));
    let audio = Track::new(TrackKind::Audio, Some("a".to_string()));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(audio);
    let result = insert_with_audio(
        &mut stack,
        0,
        1.0,
        clip(2.0, Some("linked-video")),
        vec![audio_clip(2.0, "file:///linked-audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    let removed = stack.delete_item("unlinked", false);

    assert_eq!(removed.len(), 1);
    let (video_track_index, video_item_index, _) = stack.get_item("linked-video").unwrap();
    let (audio_track_index, audio_item_index, _) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        0.0
    );
    assert_eq!(
        stack.children[audio_track_index].start_time_of_item(audio_item_index),
        0.0
    );
}

#[test]
fn delete_track_removes_linked_assets_left_behind() {
    let mut stack = Stack::default();
    let video = Track::new(TrackKind::Video, Some("v".to_string()));
    let audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    stack.children.push(video);
    stack.children.push(audio);

    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    let removed = stack.delete_track("v").unwrap();

    assert_eq!(removed.get_id().as_deref(), Some("v"));
    assert_eq!(stack.children.len(), 1);
    assert!(stack
        .children
        .iter()
        .any(|track| track.get_id().as_deref() == Some("a")));
    assert!(stack.get_item(&audio_id).is_none());
    assert!(stack.children.iter().any(|track| {
        track
            .items
            .iter()
            .any(|item| matches!(item, Item::Gap(_)) && (item.duration() - 3.0).abs() <= 1e-9)
    }));
}

#[test]
fn move_item_at_time_moves_linked_group() {
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(8.0)));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(8.0)));
    stack.children.push(video);
    stack.children.push(audio);

    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.move_item_at_time(
        &audio_id,
        "a",
        5.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    let (video_track, video_index, video_item) = stack.get_item("primary").unwrap();
    let (audio_track, audio_index, audio_item) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[video_track].start_time_of_item(video_index),
        stack.children[audio_track].start_time_of_item(audio_index)
    );
    assert_eq!(link_group_id(video_item), result.link_group_id);
    assert_eq!(link_group_id(audio_item), result.link_group_id);
}

#[test]
fn move_item_at_index_moves_linked_group() {
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(8.0)));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(8.0)));
    stack.children.push(video);
    stack.children.push(audio);

    let result = insert_with_audio(
        &mut stack,
        0,
        2.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.move_item_at_index("primary", "v", 0, true, OverlapPolicy::Override,));

    let (video_track, video_index, video_item) = stack.get_item("primary").unwrap();
    let (audio_track, audio_index, audio_item) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[video_track].start_time_of_item(video_index),
        0.0
    );
    assert_eq!(
        stack.children[audio_track].start_time_of_item(audio_index),
        0.0
    );
    assert_eq!(link_group_id(video_item), result.link_group_id);
    assert_eq!(link_group_id(audio_item), result.link_group_id);
}

#[test]
fn move_unlinked_item_only_moves_selected_item() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(3.0, Some("primary"))));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio
        .items
        .push(Item::Clip(clip(3.0, Some("unlinked-audio"))));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(audio);

    assert!(stack.move_item_at_time(
        "primary",
        "a",
        3.0,
        true,
        InsertPolicy::InsertAfter,
        OverlapPolicy::Override,
    ));

    assert_eq!(stack.get_item("primary").unwrap().0, 1);
    assert!(stack.get_item("unlinked-audio").is_some());
}

#[test]
fn move_unlinked_item_without_gap_pulls_later_linked_assets() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(1.0, Some("unlinked"))));
    let audio = Track::new(TrackKind::Audio, Some("a".to_string()));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(audio);
    let result = insert_with_audio(
        &mut stack,
        0,
        1.0,
        clip(2.0, Some("linked-video")),
        vec![audio_clip(2.0, "file:///linked-audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();
    let video_track_id = stack
        .children
        .iter()
        .find(|track| track.get_id().as_deref() == Some("v"))
        .unwrap()
        .get_id()
        .unwrap();

    assert!(stack.move_item_at_time(
        "unlinked",
        &video_track_id,
        2.0,
        false,
        InsertPolicy::InsertBeforeOrAfter,
        OverlapPolicy::Push,
    ));

    let (video_track_index, video_item_index, _) = stack.get_item("linked-video").unwrap();
    let (audio_track_index, audio_item_index, _) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        stack.children[audio_track_index].start_time_of_item(audio_item_index)
    );
    assert_eq!(stack.children[video_track_index].items.len(), 2);
    assert!(stack.children[video_track_index]
        .items
        .iter()
        .all(|item| matches!(item, Item::Clip(_))));
    assert_eq!(
        stack.children[video_track_index].timeline_ids(),
        vec!["linked-video", "unlinked"]
    );
    assert_eq!(stack.children[audio_track_index].items.len(), 1);
    assert!(matches!(
        stack.children[audio_track_index].items[audio_item_index],
        Item::Clip(_)
    ));
}

#[test]
fn move_unlinked_item_with_gap_and_split_target_updates_linked_assets() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(1.0, Some("unlinked"))));
    let audio = Track::new(TrackKind::Audio, Some("a".to_string()));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(audio);
    let result = insert_with_audio(
        &mut stack,
        0,
        1.0,
        clip(2.0, Some("linked-video")),
        vec![audio_clip(2.0, "file:///linked-audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.move_item_at_time(
        "unlinked",
        "v",
        2.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Push,
    ));

    let video_track_index = stack.get_track_by_id("v").unwrap().0;
    let audio_track_index = stack.get_item(&audio_id).unwrap().0;
    let video_items = &stack.children[video_track_index].items;
    let audio_items = &stack.children[audio_track_index].items;

    assert_eq!(video_items.len(), 4);
    assert!(matches!(video_items[0], Item::Gap(_)));
    assert_eq!(video_items[0].duration(), 1.0);
    assert_eq!(link_group_id(&video_items[1]), result.link_group_id);
    assert_eq!(video_items[1].duration(), 1.0);
    assert_eq!(video_items[2].get_id().as_deref(), Some("unlinked"));
    assert_eq!(video_items[2].duration(), 1.0);
    assert_eq!(link_group_id(&video_items[3]), result.link_group_id);
    assert_eq!(video_items[3].duration(), 1.0);

    assert_eq!(audio_items.len(), 4);
    assert!(matches!(audio_items[0], Item::Gap(_)));
    assert_eq!(audio_items[0].duration(), 1.0);
    assert_eq!(link_group_id(&audio_items[1]), result.link_group_id);
    assert_eq!(audio_items[1].duration(), 1.0);
    assert!(matches!(audio_items[2], Item::Gap(_)));
    assert_eq!(audio_items[2].duration(), 1.0);
    assert_eq!(link_group_id(&audio_items[3]), result.link_group_id);
    assert_eq!(audio_items[3].duration(), 1.0);
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
fn created_link_tracks_use_numbered_names_without_colliding() {
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

#[test]
fn replace_item_updates_linked_group_duration_and_preserves_identity() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("a".to_string())));

    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.replace_item(
        "primary",
        Item::Clip(clip(5.0, Some("replacement"))),
        None,
        None,
    ));

    let primary = stack.get_item("primary").unwrap().2;
    let audio = stack.get_item(&audio_id).unwrap().2;
    assert_eq!(primary.get_id().as_deref(), Some("primary"));
    assert_eq!(primary.duration(), 5.0);
    assert_eq!(audio.duration(), 5.0);
    assert_eq!(link_group_id(primary), result.link_group_id);
    assert_eq!(link_group_id(audio), result.link_group_id);
    assert!(stack.get_item("replacement").is_none());
}

#[test]
fn replace_unlinked_item_only_replaces_selected_item() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(3.0, Some("primary"))));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio
        .items
        .push(Item::Clip(clip(3.0, Some("unlinked-audio"))));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(audio);

    assert!(stack.replace_item(
        "primary",
        Item::Clip(clip(2.0, Some("replacement"))),
        None,
        None,
    ));

    assert_eq!(stack.get_item("primary").unwrap().2.duration(), 2.0);
    assert!(stack.get_item("replacement").is_none());
    assert!(stack.get_item("unlinked-audio").is_some());
}

#[test]
fn replace_item_can_add_linked_audio_clip() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));

    assert!(
        stack.replace_item(
            "primary",
            Item::Clip(clip(3.0, Some("replacement"))),
            Some(vec![audio_clip(3.0, "file:///a1.wav", None)]),
            None,
        ) == false
    );

    stack.children[0]
        .items
        .push(Item::Clip(clip(3.0, Some("primary"))));

    assert!(stack.replace_item(
        "primary",
        Item::Clip(clip(3.0, Some("replacement"))),
        Some(vec![audio_clip(3.0, "file:///a1.wav", None)]),
        None,
    ));

    let primary = stack.get_item("primary").unwrap().2;
    let group = link_group_id(primary);
    assert!(group.is_some());
    assert_eq!(stack.children.len(), 2);
    assert_eq!(stack.children[0].kind, TrackKind::Video);
    assert_eq!(stack.children[1].kind, TrackKind::Audio);
    let audio = stack.children[1]
        .items
        .iter()
        .find(|item| matches!(item, Item::Clip(_)))
        .unwrap();
    assert_eq!(audio.duration(), 3.0);
    assert_eq!(link_group_id(audio), group);
}

#[test]
fn replace_item_can_add_audio_past_same_link_clip() {
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(10.0)));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(10.0)));
    stack.children.push(video);
    stack.children.push(audio);

    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();
    let first_audio_id = result.audio_clips[0].0.clone();

    assert!(stack.replace_item(
        "primary",
        Item::Clip(clip(3.0, Some("replacement"))),
        Some(vec![audio_clip(3.0, "file:///a2.wav", None)]),
        None,
    ));

    assert_eq!(stack.get_item(&first_audio_id).unwrap().0, 1);
    assert_eq!(stack.children.len(), 3);
    assert_eq!(stack.children[1].kind, TrackKind::Audio);
    assert_eq!(
        stack.children[1]
            .items
            .iter()
            .filter(|item| matches!(item, Item::Clip(_)))
            .count(),
        1
    );
}

#[test]
fn replace_item_on_linked_audio_replaces_audio_asset_and_keeps_video_link() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("video")),
        vec![audio_clip(3.0, "file:///original-audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.replace_item(
        &audio_id,
        audio_clip(4.0, "file:///replacement-audio.wav", None),
        None,
        None,
    ));

    let video = stack.get_item("video").unwrap().2;
    let audio = stack.get_item(&audio_id).unwrap().2;
    assert_eq!(video.duration(), 4.0);
    assert_eq!(audio.duration(), 4.0);
    assert_eq!(link_group_id(video), result.link_group_id);
    assert_eq!(link_group_id(audio), result.link_group_id);
    assert_eq!(active_target_url(video), Some("file:///video.mov"));
    assert_eq!(active_target_url(audio), Some("file:///replacement-audio.wav"));
}

#[test]
fn replace_item_on_linked_audio_can_add_extra_linked_video_asset() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("video")),
        vec![audio_clip(4.0, "file:///audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.replace_item(
        &audio_id,
        audio_clip(4.0, "file:///replacement-audio.wav", None),
        None,
        Some(Item::Clip(clip(4.0, Some("extra-video")))),
    ));

    let audio = stack.get_item(&audio_id).unwrap().2;
    let original_video = stack.get_item("video").unwrap().2;
    let extra_video = stack.get_item("extra-video").unwrap().2;
    assert_eq!(link_group_id(audio), result.link_group_id);
    assert_eq!(link_group_id(original_video), result.link_group_id);
    assert_eq!(link_group_id(extra_video), result.link_group_id);
    assert_eq!(
        stack
            .children
            .iter()
            .filter(|track| track.kind == TrackKind::Video)
            .flat_map(|track| track.items.iter())
            .filter(|item| matches!(item, Item::Clip(_)))
            .count(),
        2
    );
    assert_eq!(active_target_url(audio), Some("file:///replacement-audio.wav"));
}

#[test]
fn replace_item_keeps_same_content_audio_and_removes_same_content_video_input() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    stack.children[0]
        .items
        .push(Item::Clip(clip(3.0, Some("primary"))));

    assert!(stack.replace_item(
        "primary",
        Item::Clip(clip(3.0, Some("replacement"))),
        Some(vec![
            audio_clip(3.0, "file:///a1.wav", None),
            Item::Clip(clip(3.0, Some("different-id"))),
        ]),
        None,
    ));

    assert_eq!(stack.children.len(), 3);
    assert!(stack.get_item("replacement").is_none());
    let audio_clip_count = stack
        .children
        .iter()
        .filter(|track| track.kind == TrackKind::Audio)
        .flat_map(|track| track.items.iter())
        .filter(|item| matches!(item, Item::Clip(_)))
        .count();
    assert_eq!(audio_clip_count, 2);

    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    stack.children[0]
        .items
        .push(Item::Clip(clip(3.0, Some("primary"))));

    assert!(stack.replace_item(
        "primary",
        Item::Clip(clip(3.0, Some("replacement"))),
        None,
        Some(Item::Clip(clip(3.0, Some("different-video-id")))),
    ));

    assert_eq!(stack.children.len(), 1);
    assert!(stack.get_item("replacement").is_none());
    assert_eq!(stack.get_item("primary").unwrap().2.duration(), 3.0);
}

#[test]
fn split_item_at_time_splits_linked_group() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("a".to_string())));
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

    assert_eq!(stack.children[0].items.len(), 2);
    assert_eq!(stack.children[result.audio_clips[0].1].items.len(), 2);
    assert_eq!(stack.children[0].items[0].duration(), 2.0);
    assert_eq!(stack.children[0].items[1].duration(), 2.0);
    assert_eq!(
        stack.children[result.audio_clips[0].1].items[0].duration(),
        2.0
    );
    assert_eq!(
        stack.children[result.audio_clips[0].1].items[1].duration(),
        2.0
    );
    assert_eq!(
        link_group_id(&stack.children[0].items[0]),
        result.link_group_id
    );
    assert_eq!(
        link_group_id(&stack.children[0].items[1]),
        result.link_group_id
    );
    assert_eq!(
        link_group_id(&stack.children[result.audio_clips[0].1].items[0]),
        result.link_group_id
    );
    assert_eq!(
        link_group_id(&stack.children[result.audio_clips[0].1].items[1]),
        result.link_group_id
    );
    assert!(stack.get_item(&audio_id).is_some());
}

#[test]
fn split_item_at_time_from_audio_splits_linked_video() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("a".to_string())));
    let result = match stack.insert_item_at_time(
        0,
        0.0,
        audio_clip(4.0, "file:///audio.wav", None),
        OverlapPolicy::Override,
        InsertPolicy::InsertBefore,
        None,
        Some(Item::Clip(clip(4.0, Some("video")))),
    ) {
        Some(InsertItemAtTimeResult::Linked(result)) => result,
        _ => panic!("linked insert should succeed"),
    };

    assert!(stack.split_item_at_time(&result.primary_clip_id, 1.5));

    let (audio_track_index, _, _) = stack.get_item(&result.primary_clip_id).unwrap();
    let (video_track_index, _, _) = stack.get_item("video").unwrap();
    assert_eq!(
        stack.children[video_track_index].get_id().as_deref(),
        Some("V1")
    );
    assert_eq!(
        stack.children[video_track_index].name.as_deref(),
        Some("V1")
    );
    assert_eq!(stack.children[audio_track_index].items.len(), 2);
    assert_eq!(stack.children[video_track_index].items.len(), 2);
    assert_eq!(stack.children[audio_track_index].items[0].duration(), 1.5);
    assert_eq!(stack.children[audio_track_index].items[1].duration(), 2.5);
    assert_eq!(stack.children[video_track_index].items[0].duration(), 1.5);
    assert_eq!(stack.children[video_track_index].items[1].duration(), 2.5);
}

#[test]
fn split_unlinked_item_only_splits_selected_track() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(4.0, Some("primary"))));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio
        .items
        .push(Item::Clip(clip(4.0, Some("unlinked-audio"))));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(audio);

    assert!(stack.split_item_at_time("primary", 2.0));

    assert_eq!(stack.children[0].items.len(), 2);
    assert_eq!(stack.children[1].items.len(), 1);
    assert!(stack.get_item("unlinked-audio").is_some());
}

#[test]
fn resize_item_updates_linked_group() {
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
fn resize_item_moves_linked_group_by_selected_delta() {
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

    assert!(stack.resize_item("primary", 1.0, 1.0, OverlapPolicy::Override, false));

    for item_id in [&"primary".to_string(), &audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            1.0
        );
        assert_eq!(item.duration(), 1.0);
    }
    for item_id in [&right_video_id, &right_audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            3.0
        );
        assert_eq!(item.duration(), 1.0);
    }
}

#[test]
fn resize_item_push_updates_linked_assets_of_pushed_clip() {
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
fn resize_linked_item_push_updates_linked_assets_of_pushed_group() {
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
fn resize_item_override_updates_linked_assets_of_trimmed_clip() {
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
fn resize_linked_item_override_updates_linked_assets_of_trimmed_group() {
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
    for item_id in ["second-video", &second_audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(
            stack.children[track_index].start_time_of_item(item_index),
            0.0
        );
        assert_eq!(item.duration(), 1.0);
    }
}

#[test]
fn modify_item_right_shrink_removes_trailing_gap_on_linked_group() {
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
fn modify_item_left_shrink_leaves_gap_on_linked_group() {
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
fn modify_item_left_shrink_with_push_updates_linked_source_starts() {
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
fn modify_item_from_audio_updates_linked_video() {
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
fn modify_item_negative_source_start_clamps_linked_group() {
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
fn modify_item_extend_updates_linked_group_duration() {
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
fn modify_item_negative_duration_deletes_linked_group() {
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
fn replace_item_rejects_linked_audio_with_different_duration() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    stack.children[0]
        .items
        .push(Item::Clip(clip(3.0, Some("primary"))));
    let original = stack.clone();

    assert!(!stack.replace_item(
        "primary",
        Item::Clip(clip(3.0, Some("replacement"))),
        Some(vec![audio_clip(2.0, "file:///a1.wav", None)]),
        None,
    ));
    assert_eq!(stack.children, original.children);
}

#[test]
fn replace_item_rejects_linked_video_with_different_duration() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("a".to_string())));
    stack.children[0]
        .items
        .push(audio_clip(4.0, "file:///audio.wav", None));
    stack.children[0].items[0].set_id(Some("audio".to_string()));
    let original = stack.clone();

    assert!(!stack.replace_item(
        "audio",
        audio_clip(4.0, "file:///replacement.wav", None),
        None,
        Some(Item::Clip(clip(3.0, Some("video")))),
    ));
    assert_eq!(stack.children, original.children);
}

#[test]
fn replace_item_can_add_linked_video_clip_for_audio() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("a".to_string())));
    stack.children[0]
        .items
        .push(audio_clip(4.0, "file:///audio.wav", None));
    stack.children[0].items[0].set_id(Some("audio".to_string()));

    assert!(stack.replace_item(
        "audio",
        audio_clip(4.0, "file:///replacement.wav", None),
        None,
        Some(Item::Clip(clip(4.0, Some("video")))),
    ));

    assert_eq!(stack.children[0].kind, TrackKind::Video);
    assert_eq!(stack.children[1].kind, TrackKind::Audio);
    let audio = stack.get_item("audio").unwrap().2;
    let video = stack.get_item("video").unwrap().2;
    assert_eq!(link_group_id(audio), link_group_id(video));
    assert!(link_group_id(audio).is_some());
}

#[test]
fn unlink_item_accepts_multiple_ids_and_cleans_singletons() {
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(10.0)));
    let mut audio_one = Track::new(TrackKind::Audio, Some("a1".to_string()));
    audio_one.items.push(Item::Gap(Gap::make_gap(10.0)));
    let mut audio_two = Track::new(TrackKind::Audio, Some("a2".to_string()));
    audio_two.items.push(Item::Gap(Gap::make_gap(10.0)));
    stack.children.push(video);
    stack.children.push(audio_one);
    stack.children.push(audio_two);

    let first = insert_with_audio(
        &mut stack,
        0,
        1.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();
    let primary_track_index = stack.get_item("primary").unwrap().0;
    let second = insert_with_audio(
        &mut stack,
        primary_track_index,
        5.0,
        clip(2.0, Some("primary-2")),
        vec![audio_clip(2.0, "file:///a2.wav", None)],
    )
    .unwrap();

    assert_eq!(
        stack.unlink_item(&["primary".to_string(), "primary-2".to_string()]),
        4
    );

    assert_eq!(link_group_id(stack.get_item("primary").unwrap().2), None);
    assert_eq!(
        link_group_id(stack.get_item(&first.audio_clips[0].0).unwrap().2),
        None
    );
    assert_eq!(link_group_id(stack.get_item("primary-2").unwrap().2), None);
    assert_eq!(
        link_group_id(stack.get_item(&second.audio_clips[0].0).unwrap().2),
        None
    );
}

#[test]
fn link_item_links_arbitrary_existing_clips_with_new_group() {
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(3.0, Some("primary"))));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(Item::Clip(clip(3.0, Some("audio"))));
    stack.children.push(video);
    stack.children.push(audio);

    let group = stack
        .link_item(&["primary".to_string(), "audio".to_string()])
        .unwrap();

    assert_eq!(
        link_group_id(stack.get_item("primary").unwrap().2),
        Some(group)
    );
    assert_eq!(
        link_group_id(stack.get_item("audio").unwrap().2),
        Some(group)
    );
}

#[test]
fn link_item_rejects_items_with_different_boundaries() {
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(3.0, Some("primary"))));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(1.0)));
    audio.items.push(Item::Clip(clip(3.0, Some("audio"))));
    stack.children.push(video);
    stack.children.push(audio);

    assert_eq!(
        stack.link_item(&["primary".to_string(), "audio".to_string()]),
        None
    );
    assert_eq!(link_group_id(stack.get_item("primary").unwrap().2), None);
    assert_eq!(link_group_id(stack.get_item("audio").unwrap().2), None);
}
