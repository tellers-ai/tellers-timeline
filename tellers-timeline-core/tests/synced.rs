use tellers_timeline_core::{
    Clip, Gap, IdMetadataExt, InsertItemAtTimeResult, InsertPolicy, Item, SyncedInsertResult,
    MediaReference, OverlapPolicy, RationalTime, Stack, SyncTrackInfo, TimeRange, Track,
    TrackKind,
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

fn clip_with_references(duration: f64, active_key: Option<&str>, id: Option<&str>) -> Clip {
    let mut refs = std::collections::HashMap::new();
    refs.insert(
        "ALT".to_string(),
        media_ref("file:///replacement-alt.mov", Some("replacement-alt")),
    );
    refs.insert(
        "DEFAULT_MEDIA".to_string(),
        media_ref("file:///replacement-default.mov", Some("replacement-default")),
    );
    Clip::new(
        range(duration),
        refs,
        active_key.map(str::to_string),
        None,
        id.map(str::to_string),
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

fn audio_clip_with_available_duration(duration: f64, url: &str, available_duration: f64) -> Item {
    Item::Clip(Clip::new_single_media_reference(
        range(duration),
        MediaReference::ExternalReference {
            target_url: url.to_string(),
            available_range: Some(range(available_duration)),
            name: None,
            available_image_bounds: Some(serde_json::Value::Null),
            metadata: serde_json::json!({}),
        },
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

fn sync_clips_id(item: &Item) -> Option<i64> {
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

fn range_is_gap_backed_for_test(track: &Track, start: f64, end: f64) -> bool {
    let mut pos = 0.0;
    for item in &track.items {
        let item_start = pos;
        let item_end = pos + item.duration().max(0.0);
        if item_end > start + 1e-9 && item_start < end - 1e-9 && !matches!(item, Item::Gap(_)) {
            return false;
        }
        pos = item_end;
    }
    true
}

fn synced_clip_item(duration: f64, id: &str, sync_clips_id: i64) -> Item {
    let mut clip = clip(duration, Some(id));
    clip.metadata["Resolve_OTIO"] = serde_json::json!({
        "Link Group ID": sync_clips_id
    });
    Item::Clip(clip)
}

fn synced_clip_item_with_source_start(
    duration: f64,
    source_start: f64,
    id: &str,
    sync_clips_id: i64,
) -> Item {
    let mut item = synced_clip_item(duration, id, sync_clips_id);
    if let Item::Clip(clip) = &mut item {
        clip.source_range.start_time.value = source_start;
    }
    item
}

fn insert_with_audio(
    stack: &mut Stack,
    dest_track_index: usize,
    dest_time: f64,
    clip: Clip,
    synced_audio_clips: Vec<Item>,
) -> Option<SyncedInsertResult> {
    match stack.insert_item_at_time(
        dest_track_index,
        dest_time,
        Item::Clip(clip),
        OverlapPolicy::Override,
        InsertPolicy::InsertBefore,
        Some(synced_audio_clips),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => Some(result),
        _ => None,
    }
}

fn stack_with_synced_audio_below_video() -> Stack {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(2.0)));
    video.items.push(Item::Clip(clip(2.0, Some("linked-video"))));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(2.0)));
    audio
        .items
        .push(audio_clip(2.0, "file:///linked-audio.wav", None));
    audio.items[1].set_id(Some("linked-audio".to_string()));

    let mut stack = Stack::default();
    stack.children.push(audio);
    stack.children.push(video);
    stack
        .sync_item(&["linked-video".to_string(), "linked-audio".to_string()])
        .unwrap();
    stack
}

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
fn synced_insert_reuses_existing_audio_track_below_video() {
    // Standard "video on top" layout: the audio track sits below the video, i.e. at a
    // LOWER index in the data model (audio idx0, video idx1). Inserting a linked clip
    // must reuse that empty audio track below the video rather than creating a new one.
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

    // No new track is created; the existing audio track below the video is reused.
    assert!(result.created_track_indices.is_empty());
    assert_eq!(stack.children.len(), 2);
    assert_eq!(stack.children[0].get_id().as_deref(), Some("audio-track"));
    assert_eq!(stack.children[1].get_id().as_deref(), Some("video-track"));
    assert_eq!(result.audio_clips.len(), 1);
    assert_eq!(result.audio_clips[0].1, 0);

    // Primary and sync track stay aligned and share a link group.
    let (primary_track_index, primary_item_index, _) = stack.get_item("primary").unwrap();
    let primary_start = stack.children[primary_track_index].start_time_of_item(primary_item_index);
    let (audio_id, _) = &result.audio_clips[0];
    let (audio_track_index, audio_item_index, audio_item) = stack.get_item(audio_id).unwrap();
    assert_eq!(audio_track_index, 0);
    assert_eq!(
        stack.children[audio_track_index].start_time_of_item(audio_item_index),
        primary_start
    );
    assert_eq!(sync_clips_id(audio_item), result.sync_clips_id);
}

#[test]
fn synced_insert_reuses_free_audio_tracks_below_video() {
    // far-audio (idx0) and empty-audio (idx1) both sit below the destination video
    // (idx2) in the data model (lower index renders below). They are free (gap only),
    // so the corrected insert reuses them — scanning downward from the video, nearest
    // first — instead of creating new tracks. No video or content-bearing track lies
    // between them and the destination, so nothing blocks the reuse.
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

    // Nearest-first: the first audio clip reuses empty-audio (idx1, just below the
    // video), the second reuses far-audio (idx0). No new tracks are created and the
    // video stays at index 2 (top of the group).
    assert_eq!(
        result
            .audio_clips
            .iter()
            .map(|(_, track_index)| *track_index)
            .collect::<Vec<_>>(),
        vec![1, 0]
    );
    assert!(result.created_track_indices.is_empty());
    assert_eq!(stack.children.len(), 3);
    assert_eq!(stack.children[0].get_id().as_deref(), Some("far-audio"));
    assert_eq!(stack.children[1].get_id().as_deref(), Some("empty-audio"));
    assert_eq!(stack.get_item("primary").unwrap().0, 2);
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
fn sync_track_info_reports_primary_and_bound_tracks() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("linked-v".to_string())));
    insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("linked-video")),
        vec![audio_clip(4.0, "file:///linked-audio.wav", None)],
    )
    .expect("linked insert should succeed");
    // The synced insert creates the audio track ("A1") below the video, so the layout is
    // [A1 (idx0), linked-v (idx1)]. Add a stray unlinked clip onto the synced audio track
    // to make it a mixed track.
    stack.children[0]
        .items
        .push(Item::Clip(clip(2.0, Some("mixed-unlinked-video"))));

    let mut unsynced_audio = Track::new(TrackKind::Audio, Some("unlinked-a".to_string()));
    unsynced_audio
        .items
        .push(audio_clip(4.0, "file:///unlinked-audio.wav", None));
    stack.children.push(unsynced_audio);
    let mut unsynced_video = Track::new(TrackKind::Video, Some("unlinked-v".to_string()));
    unsynced_video
        .items
        .push(Item::Clip(clip(4.0, Some("unlinked-video"))));
    stack.children.push(unsynced_video);

    let groups = stack.sync_track_info();

    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].start_index, 0);
    assert_eq!(groups[0].end_index, 2);
    assert_eq!(groups[0].track_indices, vec![0, 1]);
    // Audio ("A1") sits below the video at index 0; the video ("linked-v") is on top at
    // index 1.
    assert_eq!(
        groups[0].track_ids,
        vec![Some("A1".to_string()), Some("linked-v".to_string())]
    );
    assert_eq!(groups[0].primary_track_index, 1);
    assert_eq!(
        groups[0].primary_track_id.as_deref(),
        Some("linked-v")
    );
    assert_eq!(groups[0].bound_track_indices, vec![0]);
    assert_eq!(groups[0].bound_track_ids, vec![Some("A1".to_string())]);

    assert_eq!(groups[1].track_indices, vec![2]);
    assert_eq!(
        groups[1].primary_track_id.as_deref(),
        Some("unlinked-a")
    );
    assert!(groups[1].bound_track_indices.is_empty());

    assert_eq!(groups[2].track_indices, vec![3]);
    assert_eq!(
        groups[2].primary_track_id.as_deref(),
        Some("unlinked-v")
    );
    assert!(groups[2].bound_track_indices.is_empty());
}

#[test]
fn sync_track_info_merges_mixed_video_with_synced_audio_tracks() {
    let mut stack = Stack::default();

    let mut a2 = Track::new(TrackKind::Audio, Some("A2".to_string()));
    a2.items.push(Item::Gap(Gap::make_gap(6.0)));
    a2.items.push(synced_clip_item(219.0, "a2-g3", 3));
    stack.children.push(a2);

    let mut a1 = Track::new(TrackKind::Audio, Some("A1".to_string()));
    a1.items.push(synced_clip_item(2.0, "a1-g2", 2));
    a1.items.push(Item::Gap(Gap::make_gap(4.0)));
    a1.items.push(synced_clip_item(219.0, "a1-g3", 3));
    a1.items.push(Item::Gap(Gap::make_gap(10.0)));
    a1.items.push(synced_clip_item(50.0, "a1-g1", 1));
    stack.children.push(a1);

    for track_id in ["A3", "A4", "A5"] {
        let mut audio = Track::new(TrackKind::Audio, Some(track_id.to_string()));
        audio.items.push(Item::Gap(Gap::make_gap(6.0)));
        audio.items.push(synced_clip_item(219.0, &format!("{track_id}-g3"), 3));
        stack.children.push(audio);
    }

    let mut video = Track::new(TrackKind::Video, Some("Video 1".to_string()));
    video.items.push(synced_clip_item(2.0, "v-g2", 2));
    video
        .items
        .push(Item::Clip(clip(4.0, Some("unlinked-video-1"))));
    video.items.push(synced_clip_item(219.0, "v-g3", 3));
    video.items.push(Item::Gap(Gap::make_gap(5.0)));
    video
        .items
        .push(Item::Clip(clip(4.0, Some("unlinked-video-2"))));
    video.items.push(Item::Gap(Gap::make_gap(1.0)));
    video.items.push(synced_clip_item(50.0, "v-g1", 1));
    stack.children.push(video);

    let groups = stack.sync_track_info();

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].track_indices, vec![0, 1, 2, 3, 4, 5]);
    assert_eq!(groups[0].primary_track_index, 5);
    assert_eq!(groups[0].primary_track_id.as_deref(), Some("Video 1"));
    assert_eq!(groups[0].bound_track_indices, vec![0, 1, 2, 3, 4]);
}

#[test]
fn sync_track_info_splits_cluster_when_sync_clip_timing_differs() {
    let mut stack = Stack::default();

    let mut aligned_audio = Track::new(TrackKind::Audio, Some("aligned-a".to_string()));
    aligned_audio.items.push(synced_clip_item(4.0, "a-sync", 1));
    stack.children.push(aligned_audio);

    let mut video = Track::new(TrackKind::Video, Some("video".to_string()));
    video.items.push(synced_clip_item(4.0, "v-sync", 1));
    stack.children.push(video);

    let mut misaligned_audio = Track::new(TrackKind::Audio, Some("misaligned-a".to_string()));
    misaligned_audio.items.push(Item::Gap(Gap::make_gap(1.0)));
    misaligned_audio
        .items
        .push(synced_clip_item(4.0, "late-a-sync", 1));
    stack.children.push(misaligned_audio);

    let groups = stack.sync_track_info();

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].track_indices, vec![0, 1]);
    assert_eq!(groups[0].primary_track_index, 1);
    assert_eq!(groups[1].track_indices, vec![2]);
    assert_eq!(groups[1].primary_track_index, 2);
}

#[test]
fn sync_track_info_includes_empty_tracks_in_principal_cluster() {
    let mut stack = Stack::default();

    for id in ["A1", "A2", "A3"] {
        let mut audio = Track::new(TrackKind::Audio, Some(id.to_string()));
        audio.items.push(synced_clip_item(5.28, &format!("{id}-sync"), 1));
        stack.children.push(audio);
    }

    for id in ["A4", "A5", "A6", "A7", "A8", "A9"] {
        stack
            .children
            .push(Track::new(TrackKind::Audio, Some(id.to_string())));
    }

    let mut video = Track::new(TrackKind::Video, Some("video".to_string()));
    video.items.push(synced_clip_item(5.28, "v-sync", 1));
    stack.children.push(video);

    let groups = stack.sync_track_info();

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].track_indices, (0..10).collect::<Vec<_>>());
    assert_eq!(groups[0].primary_track_index, 9);
    assert_eq!(groups[0].primary_track_id.as_deref(), Some("video"));
    assert_eq!(groups[0].bound_track_indices, (0..9).collect::<Vec<_>>());
}

#[test]
fn sync_track_info_splits_unrelated_empty_tracks_into_separate_clusters() {
    let mut stack = Stack::default();

    for id in ["empty-a", "empty-b", "empty-c"] {
        let mut track = Track::new(TrackKind::Audio, Some(id.to_string()));
        track.items.push(Item::Gap(Gap::make_gap(4.0)));
        stack.children.push(track);
    }

    let groups = stack.sync_track_info();

    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].track_indices, vec![0]);
    assert_eq!(groups[0].primary_track_index, 0);
    assert_eq!(groups[1].track_indices, vec![1]);
    assert_eq!(groups[1].primary_track_index, 1);
    assert_eq!(groups[2].track_indices, vec![2]);
    assert_eq!(groups[2].primary_track_index, 2);
}

#[test]
fn sync_track_info_clusters_tracks_within_one_frame() {
    let mut stack = Stack::default();

    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(67.92)));
    audio.items.push(synced_clip_item_with_rate(
        62.92,
        67.92,
        "a-sync",
        2,
        25.0,
    ));
    stack.children.push(audio);

    let mut video = Track::new(TrackKind::Video, Some("video".to_string()));
    video.items.push(synced_clip_item_with_rate(62.88, 0.0, "v-sync-3", 3, 25.0));
    video
        .items
        .push(Item::Gap(Gap::make_gap(5.0)));
    video
        .items
        .push(synced_clip_item_with_rate(62.88, 67.88, "v-sync-2", 2, 25.0));
    stack.children.push(video);

    let groups = stack.sync_track_info();

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].track_indices, vec![0, 1]);
    assert_eq!(groups[0].primary_track_index, 1);
}

#[test]
fn insert_on_video_in_cluster_preserves_cluster_and_pads_bound_tracks() {
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
    video.items.push(Item::Gap(Gap::make_gap(5.0)));
    video
        .items
        .push(synced_clip_item_with_rate(62.88, 67.88, "v-g2", 2, 25.0));
    stack.children.push(video);

    let video_track_index = stack.children.len() - 1;
    let audio_track_indices: Vec<_> = (0..video_track_index).collect();

    let groups_before = stack.sync_track_info();
    assert_eq!(groups_before.len(), 1);
    assert_eq!(groups_before[0].track_indices, vec![0, 1, 2, 3]);
    assert_eq!(groups_before[0].primary_track_index, video_track_index);

    let result = stack.insert_item_at_time(
        video_track_index,
        2.0,
        Item::Clip(clip(1.5, Some("inserted-at-2"))),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        None,
    None,
    );
    assert!(matches!(result, Some(InsertItemAtTimeResult::ItemId(_))));

    let groups_after = stack.sync_track_info();
    assert_eq!(groups_after.len(), 1);
    assert_eq!(groups_after[0].track_indices, vec![0, 1, 2, 3]);
    assert_eq!(groups_after[0].primary_track_index, video_track_index);

    let (insert_track_index, insert_item_index, insert_item) =
        stack.get_item("inserted-at-2").expect("inserted clip should exist");
    assert_eq!(insert_track_index, video_track_index);
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(insert_item_index),
        2.0
    );
    assert_eq!(insert_item.duration(), 1.5);

    let video_track = &stack.children[video_track_index];
    let head_index = video_track.get_item_at_time(0.0).unwrap();
    assert_eq!(video_track.items[head_index].duration(), 2.0);
    let tail_index = video_track.get_item_at_time(3.5).unwrap();
    assert_eq!(video_track.items[tail_index].duration(), 62.88 - 3.5);

    for &audio_track_index in &audio_track_indices {
        let audio_track = &stack.children[audio_track_index];
        let spacer_index = audio_track.get_item_at_time(2.0).unwrap();
        assert!(matches!(audio_track.items[spacer_index], Item::Gap(_)));
        assert_eq!(audio_track.items[spacer_index].duration(), 1.5);
        let head_index = audio_track.get_item_at_time(0.0).unwrap();
        assert_eq!(audio_track.items[head_index].duration(), 2.0);
    }
}

fn resolve_metadata_clip_item(
    duration: f64,
    id: &str,
    link_group_id: i64,
    use_resolve_key: bool,
) -> Item {
    let mut clip = clip(duration, Some(id));
    let key = if use_resolve_key {
        "resolve"
    } else {
        "Resolve_OTIO"
    };
    clip.metadata[key] = serde_json::json!({
        "Link Group ID": link_group_id
    });
    Item::Clip(clip)
}

fn stack_with_offset_linked_clips(use_resolve_key: bool) -> Stack {
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("video-track".to_string()));
    video.items.push(resolve_metadata_clip_item(
        5.0,
        "video-1",
        1,
        use_resolve_key,
    ));
    stack.children.push(video);

    let mut audio = Track::new(TrackKind::Audio, Some("audio-track".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(2.0)));
    audio.items.push(resolve_metadata_clip_item(
        5.0,
        "audio-1",
        1,
        use_resolve_key,
    ));
    stack.children.push(audio);
    stack
}

#[test]
fn move_offset_linked_clips_preserves_relative_offsets() {
    for use_resolve_key in [false, true] {
        let mut stack = stack_with_offset_linked_clips(use_resolve_key);
        assert!(stack.move_item_at_time(
            "video-1",
            "video-track",
            10.0,
            true,
            InsertPolicy::SplitAndInsert,
            OverlapPolicy::Override,
        ));

        let (video_track, video_index, _) = stack.get_item("video-1").unwrap();
        let (audio_track, audio_index, _) = stack.get_item("audio-1").unwrap();
        assert_eq!(
            stack.children[video_track].start_time_of_item(video_index),
            10.0
        );
        assert_eq!(
            stack.children[audio_track].start_time_of_item(audio_index),
            12.0
        );
    }
}

#[test]
fn move_synced_set_on_same_track_in_cluster_preserves_track_and_cluster_count() {
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
    video.items.push(Item::Gap(Gap::make_gap(5.0)));
    video
        .items
        .push(synced_clip_item_with_rate(62.88, 67.88, "v-g2", 2, 25.0));
    stack.children.push(video);

    let sync_group = sync_clips_id(stack.get_item("v-g3").unwrap().2);
    let track_count = stack.children.len();
    let clusters_before = stack.sync_track_info();

    assert!(stack.move_item_at_time(
        "v-g3",
        "video",
        4.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    assert_eq!(stack.children.len(), track_count);
    assert_sync_track_info_unchanged(&clusters_before, &stack.sync_track_info());

    let (video_track, video_index, video_item) = stack.get_item("v-g3").unwrap();
    assert_eq!(
        stack.children[video_track].start_time_of_item(video_index),
        4.0
    );
    assert_eq!(sync_clips_id(video_item), sync_group);

    for id in ["A1-g3", "A2-g3", "A3-g3"] {
        let (audio_track, audio_index, audio_item) = stack.get_item(id).unwrap();
        assert_eq!(
            stack.children[audio_track].start_time_of_item(audio_index),
            4.0
        );
        assert_eq!(sync_clips_id(audio_item), sync_group);
    }
}

#[test]
fn move_synced_audio_creates_video_track_below_audio_group_in_resolve_layout() {
    let mut stack = Stack::default();
    const DUR: f64 = 5.0;

    let mut a15 = Track::new(TrackKind::Audio, Some("A15".to_string()));
    a15.items.push(Item::Gap(Gap::make_gap(100.0)));
    let mut aud = audio_clip(DUR, "file:///moving-aud.wav", None);
    aud.set_id(Some("moving-aud".to_string()));
    a15.items.push(aud);

    let mut v1 = Track::new(TrackKind::Video, Some("V1".to_string()));
    v1.items.push(Item::Gap(Gap::make_gap(100.0)));
    v1.items
        .push(Item::Clip(clip(DUR, Some("moving-vid"))));

    stack.children.push(a15);
    stack.children.push(v1);
    stack
        .sync_item(&["moving-vid".to_string(), "moving-aud".to_string()])
        .unwrap();

    let mut separator = Track::new(TrackKind::Video, Some("separator-v".to_string()));
    separator
        .items
        .push(Item::Clip(clip(100.0, Some("separator-vid"))));
    stack.children.push(separator);

    let mut dest_a = Track::new(TrackKind::Audio, Some("dest-a".to_string()));
    dest_a.items.push(Item::Gap(Gap::make_gap(100.0)));
    stack.children.push(dest_a);

    let dest_a_index = stack.get_track_by_id("dest-a").unwrap().0;
    let v1_index = stack.get_track_by_id("V1").unwrap().0;
    let track_count_before = stack.children.len();

    assert!(stack.move_item_at_time(
        "moving-aud",
        "dest-a",
        50.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    assert_eq!(stack.children.len(), track_count_before + 1);
    let (video_track, video_index, _) = stack.get_item("moving-vid").unwrap();
    assert_eq!(stack.children[video_track].kind, TrackKind::Video);
    assert_eq!(
        video_track, dest_a_index + 1,
        "new video track must sit directly below dest-a, not at stack top or next to V1"
    );
    assert_eq!(stack.get_track_by_id("V1").unwrap().0, v1_index);
    assert_eq!(
        stack.children[video_track].start_time_of_item(video_index),
        50.0
    );
}

#[test]
fn move_unsynced_clip_between_video_tracks_in_cluster_preserves_track_and_cluster_count() {
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

    let mut primary_video = Track::new(TrackKind::Video, Some("video".to_string()));
    primary_video.items.push(synced_clip_item_with_rate(62.88, 0.0, "v-g3", 3, 25.0));
    stack.children.push(primary_video);

    let mut secondary_video = Track::new(TrackKind::Video, Some("video-2".to_string()));
    secondary_video.items.push(Item::Gap(Gap::make_gap(10.0)));
    stack.children.push(secondary_video);

    let primary_video_index = stack
        .get_track_by_id("video")
        .map(|(index, _)| index)
        .unwrap();
    let secondary_video_index = stack
        .get_track_by_id("video-2")
        .map(|(index, _)| index)
        .unwrap();

    stack
        .insert_item_at_time(
            primary_video_index,
            2.0,
            Item::Clip(clip(1.5, Some("inserted-at-2"))),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            None,
            None,
        )
        .expect("insert should succeed");

    let track_count = stack.children.len();
    let clusters_before = stack.sync_track_info();

    assert!(stack.move_item_at_time(
        "inserted-at-2",
        "video-2",
        4.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    assert_eq!(stack.children.len(), track_count);
    assert_sync_track_info_unchanged(&clusters_before, &stack.sync_track_info());

    let (moved_track, moved_index, moved_item) = stack.get_item("inserted-at-2").unwrap();
    assert_eq!(moved_track, secondary_video_index);
    assert_eq!(
        stack.children[moved_track].start_time_of_item(moved_index),
        4.0,
    );
    assert_eq!(moved_item.duration(), 1.5);

    for id in ["A1", "A2", "A3"] {
        let audio_track_index = stack
            .get_track_by_id(id)
            .map(|(index, _)| index)
            .unwrap();
        let audio_track = &stack.children[audio_track_index];
        let spacer_index = audio_track.get_item_at_time(2.0).unwrap();
        assert!(matches!(audio_track.items[spacer_index], Item::Gap(_)));
        assert_eq!(audio_track.items[spacer_index].duration(), 1.5);
    }
}

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

fn assert_sync_track_info_unchanged(before: &[SyncTrackInfo], after: &[SyncTrackInfo]) {
    assert_eq!(
        after, before,
        "sync_track_info cluster grouping should not change",
    );
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
    assert_eq!(
        groups_after_insert[0].primary_track_index,
        video_track_index,
    );

    assert!(stack.modify_item("inserted-at-2", 0.0, 2.0, false, false, false));
    assert_sync_track_info_unchanged(&groups_after_insert, &stack.sync_track_info());

    let audio_track = &stack.children[0];
    let gap_index = audio_track.get_item_at_time(2.0).unwrap();
    let gap_id = audio_track.items[gap_index].get_id().unwrap();

    assert!(stack.modify_item(&gap_id, 0.0, 1.0, false, false, false));
    assert_sync_track_info_unchanged(&groups_after_insert, &stack.sync_track_info());
}

fn synced_clip_item_with_rate(
    duration: f64,
    source_start: f64,
    id: &str,
    sync_clips_id: i64,
    rate: f64,
) -> Item {
    let mut item = synced_clip_item_with_source_start(duration, source_start, id, sync_clips_id);
    if let Item::Clip(clip) = &mut item {
        clip.source_range.duration.rate = rate;
        clip.source_range.start_time.rate = rate;
        clip.source_range.duration.value = duration * rate;
        clip.source_range.start_time.value = source_start * rate;
    }
    item
}

#[test]
fn add_track_at_allows_insertion_inside_boundary_groups() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("linked-v".to_string())));
    insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("linked-video")),
        vec![audio_clip(4.0, "file:///linked-audio.wav", None)],
    )
    .expect("linked insert should succeed");

    let mut unsynced_audio = Track::new(TrackKind::Audio, Some("unlinked-a".to_string()));
    unsynced_audio
        .items
        .push(audio_clip(4.0, "file:///unlinked-audio.wav", None));
    stack.children.push(unsynced_audio);
    let mut unsynced_video = Track::new(TrackKind::Video, Some("unlinked-v".to_string()));
    unsynced_video
        .items
        .push(Item::Clip(clip(4.0, Some("unlinked-video"))));
    stack.children.push(unsynced_video);

    // New layout after synced insert: audio ("A1") below the video at index 0, video
    // ("linked-v") on top at index 1. Adding inside the group at index 1 lands between
    // them.
    assert!(stack.add_track_at(
        Track::new(TrackKind::Audio, Some("inside-linked-group".to_string())),
        1,
    ));
    assert_eq!(stack.children.len(), 5);
    assert_eq!(stack.children[0].get_id().as_deref(), Some("A1"));
    assert_eq!(
        stack.children[1].get_id().as_deref(),
        Some("inside-linked-group")
    );
    assert_eq!(stack.children[2].get_id().as_deref(), Some("linked-v"));

    assert!(stack.add_track_at(
        Track::new(TrackKind::Audio, Some("between-groups".to_string())),
        3,
    ));
    assert_eq!(stack.children.len(), 6);
    assert_eq!(
        stack.children[3].get_id().as_deref(),
        Some("between-groups")
    );
}

#[test]
fn reorder_track_moves_primary_group_only_to_boundary_edges() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("linked-v".to_string())));
    insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("linked-video")),
        vec![audio_clip(4.0, "file:///linked-audio.wav", None)],
    )
    .expect("linked insert should succeed");

    let mut unsynced_audio = Track::new(TrackKind::Audio, Some("unlinked-a".to_string()));
    unsynced_audio
        .items
        .push(audio_clip(4.0, "file:///unlinked-audio.wav", None));
    stack.children.push(unsynced_audio);
    let mut unsynced_video = Track::new(TrackKind::Video, Some("unlinked-v".to_string()));
    unsynced_video
        .items
        .push(Item::Clip(clip(4.0, Some("unlinked-video"))));
    stack.children.push(unsynced_video);

    // New layout: synced audio ("A1") below the video at index 0, video ("linked-v") on
    // top at index 1.
    assert!(!stack.reorder_track("linked-v", 1));
    assert_eq!(stack.children[0].get_id().as_deref(), Some("A1"));
    assert_eq!(stack.children[1].get_id().as_deref(), Some("linked-v"));

    // Moving the primary to the end carries its whole group: the unrelated tracks shift
    // down and the group (A1 then linked-v) lands at the top.
    assert!(stack.reorder_track("linked-v", 4));
    assert_eq!(stack.children[0].get_id().as_deref(), Some("unlinked-a"));
    assert_eq!(stack.children[1].get_id().as_deref(), Some("unlinked-v"));
    assert_eq!(stack.children[2].get_id().as_deref(), Some("A1"));
    assert_eq!(stack.children[3].get_id().as_deref(), Some("linked-v"));
}

#[test]
fn reorder_track_keeps_secondary_tracks_inside_current_boundary() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("linked-v".to_string())));
    insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("linked-video")),
        vec![audio_clip(4.0, "file:///linked-audio.wav", None)],
    )
    .expect("linked insert should succeed");

    let mut unsynced_audio = Track::new(TrackKind::Audio, Some("unlinked-a".to_string()));
    unsynced_audio
        .items
        .push(audio_clip(4.0, "file:///unlinked-audio.wav", None));
    stack.children.push(unsynced_audio);
    let mut unsynced_video = Track::new(TrackKind::Video, Some("unlinked-v".to_string()));
    unsynced_video
        .items
        .push(Item::Clip(clip(4.0, Some("unlinked-video"))));
    stack.children.push(unsynced_video);

    // New layout: synced audio ("A1") below the video at index 0, video ("linked-v") on
    // top at index 1. Reordering the secondary track outside its group boundary is
    // rejected.
    assert!(!stack.reorder_track("A1", 3));
    assert_eq!(stack.children[0].get_id().as_deref(), Some("A1"));
    assert_eq!(stack.children[1].get_id().as_deref(), Some("linked-v"));

    // Moving A1 inside its own boundary (above the video) is allowed.
    assert!(stack.reorder_track("A1", 2));
    assert_eq!(stack.children[0].get_id().as_deref(), Some("linked-v"));
    assert_eq!(stack.children[1].get_id().as_deref(), Some("A1"));
    assert_eq!(stack.children[2].get_id().as_deref(), Some("unlinked-a"));
    assert_eq!(stack.children[3].get_id().as_deref(), Some("unlinked-v"));
}

#[test]
fn insert_audio_primary_with_fewer_audio_links_fills_remaining_audio_track_with_gap() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("primary-audio".to_string())));

    let mut first_primary = audio_clip(2.0, "file:///first-primary.wav", None);
    first_primary.set_id(Some("first-primary".to_string()));
    let first = match stack.insert_item_at_time(
        0,
        0.0,
        first_primary,
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
        _ => panic!("first audio-primary linked insert should succeed"),
    };
    let primary_track_index = stack.get_item("first-primary").unwrap().0;

    let mut second_primary = audio_clip(3.0, "file:///second-primary.wav", None);
    second_primary.set_id(Some("second-primary".to_string()));
    let second = match stack.insert_item_at_time(
        primary_track_index,
        2.0,
        second_primary,
        OverlapPolicy::Push,
        InsertPolicy::InsertBeforeOrAfter,
        Some(vec![
            audio_clip(3.0, "file:///second-a1.wav", None),
            audio_clip(3.0, "file:///second-a2.wav", None),
        ]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("second audio-primary linked insert should succeed"),
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
    assert_eq!(stack.get_item("second-primary").unwrap().0, primary_track_index);
    assert_eq!(first.audio_clips[0].1, second.audio_clips[0].1);
    assert_eq!(first.audio_clips[1].1, second.audio_clips[1].1);
    // The second group has fewer audio links than the cluster has audio tracks, so the
    // extra (third) sync track is padded with a gap spacer. The spacer lands at the end
    // of that track and is trimmed during sanitize, so the track keeps just its original
    // first-group clip and is not given a second-group clip.
    let extra_track_index = first.audio_clips[2].1;
    assert!(!second
        .audio_clips
        .iter()
        .any(|(_, track_index)| *track_index == extra_track_index));
    let extra_track = &stack.children[extra_track_index];
    assert_eq!(extra_track.items.len(), 1);
    assert!(matches!(extra_track.items[0], Item::Clip(_)));
    assert_eq!(extra_track.total_duration(), 2.0);
    // The synced audio tracks sit below the audio primary, i.e. at lower indices.
    assert!(
        first
            .audio_clips
            .iter()
            .all(|(_, track_index)| *track_index < primary_track_index)
    );
}

#[test]
fn insert_audio_primary_with_more_audio_links_creates_additional_audio_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("primary-audio".to_string())));

    let mut first_primary = audio_clip(2.0, "file:///first-primary.wav", None);
    first_primary.set_id(Some("first-primary".to_string()));
    let first = match stack.insert_item_at_time(
        0,
        0.0,
        first_primary,
        OverlapPolicy::Push,
        InsertPolicy::InsertBeforeOrAfter,
        Some(vec![
            audio_clip(2.0, "file:///first-a1.wav", None),
            audio_clip(2.0, "file:///first-a2.wav", None),
        ]),
    None,
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("first audio-primary linked insert should succeed"),
    };
    let primary_track_index = stack.get_item("first-primary").unwrap().0;

    let mut second_primary = audio_clip(3.0, "file:///second-primary.wav", None);
    second_primary.set_id(Some("second-primary".to_string()));
    let second = match stack.insert_item_at_time(
        primary_track_index,
        2.0,
        second_primary,
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
        _ => panic!("second audio-primary linked insert should succeed"),
    };

    assert_eq!(
        stack
            .children
            .iter()
            .filter(|track| track.kind == TrackKind::Audio)
            .count(),
        4
    );
    assert_eq!(second.audio_clips.len(), 3);
    // Both primaries share the same track. The new audio track was created below the
    // primary, pushing it up by one, so its index is now primary_track_index + 1.
    assert_eq!(
        stack.get_item("first-primary").unwrap().0,
        stack.get_item("second-primary").unwrap().0
    );
    assert_eq!(stack.get_item("second-primary").unwrap().0, primary_track_index + 1);
    // The two existing cluster tracks are reused and exactly one new track is created
    // (nearest the primary, so it carries the first assigned clip).
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
fn insert_audio_primary_with_sync_video_clip_creates_video_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("primary-audio-track".to_string())));

    let mut primary = audio_clip(3.0, "file:///primary.wav", None);
    primary.set_id(Some("primary-audio".to_string()));
    let mut synced_video = Item::Clip(clip(3.0, Some("synced-video")));
    synced_video.set_id(Some("synced-video".to_string()));

    let result = match stack.insert_item_at_time(
        0,
        1.0,
        primary,
        OverlapPolicy::Push,
        InsertPolicy::InsertBeforeOrAfter,
        Some(vec![audio_clip(3.0, "file:///linked-a1.wav", None)]),
        Some(synced_video),
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("audio-primary insert with sync video clip should succeed"),
    };

    assert_eq!(result.primary_clip_id, "primary-audio");
    assert_eq!(result.synced_video_clip_id.as_deref(), Some("synced-video"));
    assert_eq!(result.audio_clips.len(), 1);

    let primary_track_index = stack.get_item("primary-audio").unwrap().0;
    let (video_track_index, video_item_index, video_item) =
        stack.get_item("synced-video").unwrap();
    assert_eq!(stack.children[video_track_index].kind, TrackKind::Video);
    assert!(
        video_track_index > primary_track_index,
        "synced video must sit above the audio-primary group"
    );

    let primary_start = stack.children[primary_track_index]
        .start_time_of_item(stack.get_item("primary-audio").unwrap().1);
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        primary_start
    );
    assert_eq!(sync_clips_id(video_item), result.sync_clips_id);
    assert_eq!(
        sync_clips_id(stack.get_item("primary-audio").unwrap().2),
        result.sync_clips_id
    );
    assert_eq!(
        sync_clips_id(stack.get_item(&result.audio_clips[0].0).unwrap().2),
        result.sync_clips_id
    );
}

#[test]
fn insert_audio_primary_with_sync_video_clip_only_links_primary_and_video() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Audio, Some("primary-audio-track".to_string())));

    let mut primary = audio_clip(2.0, "file:///primary.wav", None);
    primary.set_id(Some("primary-audio".to_string()));

    let result = match stack.insert_item_at_time(
        0,
        0.0,
        primary,
        OverlapPolicy::Push,
        InsertPolicy::InsertBeforeOrAfter,
        None,
        Some(Item::Clip(clip(2.0, Some("synced-video")))),
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("audio-primary insert with only sync video clip should succeed"),
    };

    assert_eq!(result.primary_clip_id, "primary-audio");
    assert_eq!(result.audio_clips, Vec::new());
    assert_eq!(result.synced_video_clip_id.as_deref(), Some("synced-video"));
    assert_eq!(
        sync_clips_id(stack.get_item("primary-audio").unwrap().2),
        result.sync_clips_id
    );
    assert_eq!(
        sync_clips_id(stack.get_item("synced-video").unwrap().2),
        result.sync_clips_id
    );
}

#[test]
fn insert_sync_video_clip_requires_audio_destination_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("video".to_string())));

    let result = stack.insert_item_at_time(
        0,
        0.0,
        Item::Clip(clip(2.0, Some("primary"))),
        OverlapPolicy::Push,
        InsertPolicy::InsertBeforeOrAfter,
        None,
        Some(Item::Clip(clip(2.0, Some("synced-video")))),
    );

    assert!(result.is_none());
}

#[test]
fn insert_item_at_index_with_sync_video_clip_creates_synced_video() {
    let mut stack = Stack::default();
    stack.children.push(Track::new(
        TrackKind::Audio,
        Some("primary-audio-track".to_string()),
    ));
    stack.children[0]
        .items
        .push(Item::Gap(Gap::make_gap(5.0)));

    let result = match stack.insert_item_at_index(
        "primary-audio-track",
        0,
        audio_clip(2.0, "file:///primary.wav", None),
        OverlapPolicy::Override,
        None,
        Some(Item::Clip(clip(2.0, Some("synced-video")))),
    ) {
        Some(InsertItemAtTimeResult::Synced(result)) => result,
        _ => panic!("index insert with sync video clip should succeed"),
    };

    assert_eq!(result.synced_video_clip_id.as_deref(), Some("synced-video"));
    let (video_track_index, _, video_item) = stack.get_item("synced-video").unwrap();
    assert_eq!(stack.children[video_track_index].kind, TrackKind::Video);
    assert_eq!(sync_clips_id(video_item), result.sync_clips_id);
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(
            stack.get_item("synced-video").unwrap().1
        ),
        0.0
    );
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
fn insert_on_separate_empty_audio_track_preserves_synced_cluster_and_clips() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let synced = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("clip-a-v")),
        vec![audio_clip(4.0, "file:///clip-a-a.wav", None)],
    )
    .unwrap();
    // Synced layout is [A (idx 0), V (idx 1)]. Add empty A2 just below the synced audio.
    let audio_track_index = synced.audio_clips[0].1;
    let audio_clip_id = synced.audio_clips[0].0.clone();
    let video_track_index = stack.get_item("clip-a-v").unwrap().0;
    let video_clip_duration_before = stack.get_item("clip-a-v").unwrap().2.duration();
    let audio_clip_duration_before = stack.get_item(&audio_clip_id).unwrap().2.duration();
    let sync_group_id = synced.sync_clips_id.unwrap();

    let mut a2 = Track::new(TrackKind::Audio, Some("a2".to_string()));
    a2.items.push(Item::Gap(Gap::make_gap(4.0)));
    stack.children.insert(audio_track_index, a2);
    let a2_track_index = audio_track_index;
    let audio_track_index = audio_track_index + 1;
    let video_track_index = video_track_index + 1;

    let groups_before = stack.sync_track_info();
    assert_eq!(groups_before.len(), 2);
    assert_eq!(
        groups_before
            .iter()
            .find(|group| group.track_indices.contains(&video_track_index))
            .map(|group| group.track_indices.as_slice()),
        Some([audio_track_index, video_track_index].as_slice())
    );
    assert!(
        groups_before
            .iter()
            .any(|group| group.track_indices == vec![a2_track_index])
    );

    let result = stack.insert_item_at_time(
        a2_track_index,
        0.0,
        Item::Clip(clip(2.0, Some("a2-clip"))),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        None,
    None,
    );
    assert!(matches!(result, Some(InsertItemAtTimeResult::ItemId(_))));

    let groups_after = stack.sync_track_info();
    assert_eq!(groups_after.len(), 2);
    assert_eq!(
        groups_after
            .iter()
            .find(|group| group.track_indices.contains(&video_track_index))
            .map(|group| group.track_indices.as_slice()),
        Some([audio_track_index, video_track_index].as_slice())
    );
    assert!(
        groups_after
            .iter()
            .any(|group| group.track_indices == vec![a2_track_index])
    );

    let (_, _, video_clip_after) = stack.get_item("clip-a-v").unwrap();
    let (_, _, audio_clip_after) = stack.get_item(&audio_clip_id).unwrap();
    assert_eq!(video_clip_after.duration(), video_clip_duration_before);
    assert_eq!(audio_clip_after.duration(), audio_clip_duration_before);
    assert_eq!(sync_clips_id(video_clip_after), Some(sync_group_id));
    assert_eq!(sync_clips_id(audio_clip_after), Some(sync_group_id));
    assert_eq!(stack.get_item("a2-clip").unwrap().0, a2_track_index);
}

#[test]
fn insert_without_sync_clips_pads_cluster_audio_track() {
    // A plain insert (no synced audio) still pads the destination cluster's audio tracks
    // with a gap spacer at the insertion point so the synced group stays aligned. The
    // cluster is the destination's sync group (tracks synced with the destination), so we
    // first build a real synced video + audio pair, then do a plain insert on the video.
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let first = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("synced-video")),
        vec![audio_clip(4.0, "file:///synced-audio.wav", None)],
    )
    .unwrap();
    let audio_track_index = first.audio_clips[0].1;
    let primary_track_index = stack.get_item("synced-video").unwrap().0;

    let result = stack.insert_item_at_time(
        primary_track_index,
        1.0,
        Item::Clip(clip(1.0, Some("inserted"))),
        OverlapPolicy::Override,
        InsertPolicy::SplitAndInsert,
        None,
    None,
    );

    assert!(matches!(result, Some(InsertItemAtTimeResult::ItemId(_))));
    assert!(stack.get_item("inserted").is_some());
    // The synced audio clip is split by the 1.0 gap spacer the cluster padding inserted
    // at 1.0, keeping it aligned with the video.
    let audio_track = &stack.children[audio_track_index];
    let spacer_index = audio_track.get_item_at_time(1.0).unwrap();
    assert!(matches!(audio_track.items[spacer_index], Item::Gap(_)));
    assert_eq!(audio_track.items[spacer_index].duration(), 1.0);
    assert_eq!(audio_track.total_duration(), 4.0);
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
fn synced_insert_rejects_synced_audio_with_different_duration() {
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
fn synced_delete_can_remove_entire_sync_clips() {
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
fn synced_delete_keeps_touched_tracks_without_remaining_clips() {
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
fn synced_delete_without_gap_collapses_sync_clips() {
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
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();

    let removed = stack.delete_item("primary", false);
    assert_eq!(removed.len(), 2);
    assert!(stack.children.iter().all(|track| track.items.is_empty()));
}

fn stack_v1_clip_a_clip_b_a1() -> Stack {
    let mut video = Track::new(TrackKind::Video, Some("v1".to_string()));
    video.items.push(Item::Clip(clip(10.0, Some("clip-a"))));
    video.items.push(synced_clip_item(10.0, "clip-b-video", 1));
    let mut audio = Track::new(TrackKind::Audio, Some("a1".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(10.0)));
    audio.items.push(synced_clip_item(10.0, "clip-b-audio", 1));
    let mut stack = Stack::default();
    stack.children.push(audio);
    stack.children.push(video);
    stack
}

fn track_index_by_id(stack: &Stack, id: &str) -> usize {
    stack
        .children
        .iter()
        .position(|track| track.get_id().as_deref() == Some(id))
        .unwrap_or_else(|| panic!("track {id:?} not found"))
}

fn assert_item_span(track: &Track, item_index: usize, expected_start: f64, expected_duration: f64) {
    let start = track.start_time_of_item(item_index);
    let duration = track.items[item_index].duration();
    assert!(
        (start - expected_start).abs() < 1e-9,
        "item {item_index} start: got {start}, expected {expected_start}"
    );
    assert!(
        (duration - expected_duration).abs() < 1e-9,
        "item {item_index} duration: got {duration}, expected {expected_duration}"
    );
}

#[test]
fn delete_clip_a_replace_with_gap_preserves_synced_partner_spans() {
    let mut stack = stack_v1_clip_a_clip_b_a1();
    let v1 = track_index_by_id(&stack, "v1");
    let a1 = track_index_by_id(&stack, "a1");

    let removed = stack.delete_item("clip-a", true);
    assert_eq!(removed.len(), 1);
    assert!(stack.get_item("clip-a").is_none());
    assert!(stack.get_item("clip-b-video").is_some());
    assert!(stack.get_item("clip-b-audio").is_some());

    let video = &stack.children[v1];
    assert_eq!(video.items.len(), 2);
    assert!(matches!(video.items[0], Item::Gap(_)));
    assert_item_span(video, 0, 0.0, 10.0);
    assert_item_span(video, 1, 10.0, 10.0);

    let audio = &stack.children[a1];
    assert_eq!(audio.items.len(), 2);
    assert!(matches!(audio.items[0], Item::Gap(_)));
    assert_item_span(audio, 0, 0.0, 10.0);
    assert_item_span(audio, 1, 10.0, 10.0);
    assert_eq!(
        sync_clips_id(&video.items[1]),
        sync_clips_id(&audio.items[1])
    );
}

#[test]
fn delete_clip_a_collapse_pulls_synced_partner_left() {
    let mut stack = stack_v1_clip_a_clip_b_a1();
    let v1 = track_index_by_id(&stack, "v1");
    let a1 = track_index_by_id(&stack, "a1");

    let removed = stack.delete_item("clip-a", false);
    assert_eq!(removed.len(), 2);
    assert!(stack.get_item("clip-a").is_none());
    assert!(stack.get_item("clip-b-video").is_some());
    assert!(stack.get_item("clip-b-audio").is_some());

    let video = &stack.children[v1];
    assert_eq!(video.items.len(), 1);
    assert_item_span(video, 0, 0.0, 10.0);

    let audio = &stack.children[a1];
    assert_eq!(audio.items.len(), 1);
    assert_item_span(audio, 0, 0.0, 10.0);
    assert_eq!(
        sync_clips_id(&video.items[0]),
        sync_clips_id(&audio.items[0])
    );
}

#[test]
fn delete_unsynced_item_only_removes_selected_item() {
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
fn delete_unsynced_item_without_gap_pulls_later_synced_assets() {
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

    assert_eq!(removed.len(), 2);
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
fn delete_track_removes_synced_assets_left_behind() {
    // "video on top" layout: audio track "a" below the video "v" (lower index), so the
    // synced insert reuses "a" rather than spawning a new track.
    let mut stack = Stack::default();
    let audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    let video = Track::new(TrackKind::Video, Some("v".to_string()));
    stack.children.push(audio);
    stack.children.push(video);

    let result = insert_with_audio(
        &mut stack,
        1,
        0.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    let removed = stack.delete_track("v").unwrap();

    assert_eq!(removed.get_id().as_deref(), Some("v"));
    // The sync track reused the existing audio track "a" instead of spawning a new
    // one, so deleting the video leaves only that single audio track behind.
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
fn move_item_at_time_moves_synced_clips() {
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
    assert_eq!(sync_clips_id(video_item), result.sync_clips_id);
    assert_eq!(sync_clips_id(audio_item), result.sync_clips_id);
}

#[test]
fn move_synced_video_to_new_boundary_creates_audio_track_without_retiming() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("source-v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip_with_media_range(3.0, 2.0, 0.0, 10.0),
        vec![audio_clip(3.0, "file:///source-audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();
    let (source_audio_track_index, source_audio_item_index, _) = stack.get_item(&audio_id).unwrap();
    let source_audio_track_id = stack.children[source_audio_track_index].get_id().unwrap();
    if let Item::Clip(clip) =
        &mut stack.children[source_audio_track_index].items[source_audio_item_index]
    {
        clip.source_range.start_time.value = 1.5;
    }

    let mut dest_video = Track::new(TrackKind::Video, Some("dest-v".to_string()));
    dest_video.items.push(Item::Gap(Gap::make_gap(10.0)));
    stack.children.push(dest_video);

    assert!(stack.move_item_at_time(
        &result.primary_clip_id,
        "dest-v",
        4.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    let (video_track_index, video_item_index, video_item) =
        stack.get_item(&result.primary_clip_id).unwrap();
    let (audio_track_index, audio_item_index, audio_item) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[video_track_index].get_id().as_deref(),
        Some("dest-v")
    );
    // The synced audio was created below the source video (source-v idx1, A1 idx0), so
    // the freed source track A1 is NOT adjacent to the destination video. The move
    // therefore creates a fresh audio track directly below dest-v rather than reusing A1.
    // Timing and the media source offset are preserved without retiming.
    assert_ne!(
        stack.children[audio_track_index].get_id().as_deref(),
        Some(source_audio_track_id.as_str())
    );
    // The relocated audio sits directly below the destination video.
    assert_eq!(audio_track_index + 1, video_track_index);
    // The original source audio track is left behind, now empty.
    let (source_track_index, _) = stack.get_track_by_id(&source_audio_track_id).unwrap();
    assert!(stack.children[source_track_index]
        .items
        .iter()
        .all(|item| matches!(item, Item::Gap(_))));
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        4.0
    );
    assert_eq!(
        stack.children[audio_track_index].start_time_of_item(audio_item_index),
        4.0
    );
    assert_eq!(video_item.duration(), 3.0);
    assert_eq!(audio_item.duration(), 3.0);
    assert_eq!(source_start(video_item), 2.0);
    assert_eq!(source_start(audio_item), 1.5);
    assert_eq!(active_target_url(audio_item), Some("file:///source-audio.wav"));
    assert_eq!(sync_clips_id(video_item), result.sync_clips_id);
    assert_eq!(sync_clips_id(audio_item), result.sync_clips_id);
}

#[test]
fn move_synced_clip_between_boundaries_override_splits_destination_link_groups() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("source-v".to_string())));
    let moving_result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(1.0, Some("moving-video")),
        vec![audio_clip(1.0, "file:///moving-audio.wav", None)],
    )
    .unwrap();
    let moving_audio_id = moving_result.audio_clips[0].0.clone();

    let mut dest_audio = Track::new(TrackKind::Audio, Some("dest-a".to_string()));
    dest_audio.items.push(Item::Gap(Gap::make_gap(10.0)));
    let mut dest_video = Track::new(TrackKind::Video, Some("dest-v".to_string()));
    dest_video.items.push(Item::Gap(Gap::make_gap(10.0)));
    stack.children.push(dest_audio);
    stack.children.push(dest_video);
    let dest_video_index = stack.get_track_by_id("dest-v").unwrap().0;

    let dest_result = insert_with_audio(
        &mut stack,
        dest_video_index,
        0.0,
        clip(4.0, Some("existing-video")),
        vec![audio_clip(4.0, "file:///existing-audio.wav", None)],
    )
    .unwrap();
    let dest_group = dest_result.sync_clips_id.unwrap();
    let dest_audio_index = dest_result.audio_clips[0].1;

    assert!(stack.move_item_at_time(
        "moving-video",
        "dest-v",
        1.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    let (moving_v_t, moving_v_i, moving_v) = stack.get_item("moving-video").unwrap();
    assert_eq!(stack.children[moving_v_t].get_id().as_deref(), Some("dest-v"));
    assert_eq!(
        stack.children[moving_v_t].start_time_of_item(moving_v_i),
        1.0
    );
    assert_eq!(moving_v.duration(), 1.0);
    let moving_group = sync_clips_id(moving_v);
    assert_ne!(moving_group, Some(dest_group));

    let (moving_a_t, moving_a_i, moving_a) = stack.get_item(&moving_audio_id).unwrap();
    assert_eq!(
        stack.children[moving_a_t].start_time_of_item(moving_a_i),
        1.0
    );
    assert_eq!(sync_clips_id(moving_a), moving_group);

    // The destination boundary clip was split by the override insert: left keeps the
    // original link group, right gets a new one.
    let video_track = &stack.children[dest_video_index];
    let left_index = video_track.get_item_at_time(0.5).unwrap();
    let right_index = video_track.get_item_at_time(2.5).unwrap();
    assert_eq!(video_track.items[left_index].duration(), 1.0);
    assert_eq!(video_track.items[right_index].duration(), 2.0);
    let left_group = sync_clips_id(&video_track.items[left_index]);
    let right_group = sync_clips_id(&video_track.items[right_index]);
    assert_eq!(left_group, Some(dest_group));
    assert_ne!(left_group, right_group);
    assert_eq!(right_group, Some(dest_group + 1));

    let audio_track = &stack.children[dest_audio_index];
    let audio_left_index = audio_track.get_item_at_time(0.5).unwrap();
    let audio_right_index = audio_track.get_item_at_time(2.5).unwrap();
    assert_eq!(sync_clips_id(&audio_track.items[audio_left_index]), left_group);
    assert_eq!(
        sync_clips_id(&audio_track.items[audio_right_index]),
        right_group
    );
    assert_sync_clips_track_aligned(&stack, "move-between-boundaries");
}

#[test]
fn move_synced_video_creates_only_missing_destination_audio_tracks() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("source-v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("primary")),
        vec![
            audio_clip(3.0, "file:///a1.wav", None),
            audio_clip(3.0, "file:///a2.wav", None),
        ],
    )
    .unwrap();
    let first_audio_id = result.audio_clips[0].0.clone();
    let second_audio_id = result.audio_clips[1].0.clone();
    let source_audio_track_ids: Vec<_> = [&first_audio_id, &second_audio_id]
        .iter()
        .map(|id| {
            let (track_index, _, _) = stack.get_item(id).unwrap();
            stack.children[track_index].get_id().unwrap()
        })
        .collect();

    let mut dest_audio = Track::new(TrackKind::Audio, Some("dest-a".to_string()));
    dest_audio.items.push(Item::Gap(Gap::make_gap(10.0)));
    let mut dest_video = Track::new(TrackKind::Video, Some("dest-v".to_string()));
    dest_video.items.push(Item::Gap(Gap::make_gap(10.0)));
    stack.children.push(dest_audio);
    stack.children.push(dest_video);
    let track_count = stack.children.len();

    assert!(stack.move_item_at_time(
        "primary",
        "dest-v",
        4.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    // The synced audio tracks are created below the source video, so the freed source
    // tracks sit above source-v — far from dest-v, not adjacent to it. Only the empty
    // "dest-a" track directly below dest-v can be reused; the move must create exactly one
    // new audio track for the remaining synced audio, so the track count grows by one.
    assert_eq!(stack.children.len(), track_count + 1);
    assert_eq!(
        stack.children[stack.get_item(&first_audio_id).unwrap().0]
            .get_id()
            .as_deref(),
        Some("dest-a")
    );
    let (second_audio_track, _, second_audio_item) = stack.get_item(&second_audio_id).unwrap();
    assert_eq!(second_audio_item.duration(), 3.0);
    // The second synced audio lands on a freshly created track (not a freed source track,
    // and not dest-a).
    assert!(!source_audio_track_ids.contains(&stack.children[second_audio_track].get_id().unwrap()));
    assert_ne!(
        stack.children[second_audio_track].get_id().as_deref(),
        Some("dest-a")
    );
    assert_eq!(
        stack.children[stack.get_item("primary").unwrap().0]
            .get_id()
            .as_deref(),
        Some("dest-v")
    );
    // The whole synced group stays aligned at the destination time.
    for item_id in ["primary", &first_audio_id, &second_audio_id] {
        let (track_index, item_index, item) = stack.get_item(item_id).unwrap();
        assert_eq!(stack.children[track_index].start_time_of_item(item_index), 4.0);
        assert_eq!(sync_clips_id(item), result.sync_clips_id);
    }
}

#[test]
fn move_synced_video_at_index_uses_destination_boundary_without_retiming() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("source-v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip_with_media_range(3.0, 2.0, 0.0, 10.0),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    let mut dest_audio = Track::new(TrackKind::Audio, Some("dest-a".to_string()));
    dest_audio.items.push(Item::Gap(Gap::make_gap(10.0)));
    let mut dest_video = Track::new(TrackKind::Video, Some("dest-v".to_string()));
    dest_video.items.push(Item::Gap(Gap::make_gap(2.0)));
    dest_video.items.push(Item::Clip(clip(2.0, Some("dest-v-later"))));
    stack.children.push(dest_audio);
    stack.children.push(dest_video);

    assert!(stack.move_item_at_index(
        &result.primary_clip_id,
        "dest-v",
        1,
        true,
        OverlapPolicy::Push,
    ));

    let (video_track_index, video_item_index, video_item) =
        stack.get_item(&result.primary_clip_id).unwrap();
    let (audio_track_index, audio_item_index, audio_item) = stack.get_item(&audio_id).unwrap();
    assert_eq!(
        stack.children[video_track_index].get_id().as_deref(),
        Some("dest-v")
    );
    assert_eq!(
        stack.children[audio_track_index].get_id().as_deref(),
        Some("dest-a")
    );
    assert_eq!(stack.children[video_track_index].start_time_of_item(video_item_index), 2.0);
    assert_eq!(stack.children[audio_track_index].start_time_of_item(audio_item_index), 2.0);
    assert_eq!(video_item.duration(), 3.0);
    assert_eq!(audio_item.duration(), 3.0);
    assert_eq!(source_start(video_item), 2.0);
    assert_eq!(sync_clips_id(video_item), result.sync_clips_id);
    assert_eq!(sync_clips_id(audio_item), result.sync_clips_id);
}

#[test]
fn move_item_at_index_moves_synced_clips() {
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
    assert_eq!(sync_clips_id(video_item), result.sync_clips_id);
    assert_eq!(sync_clips_id(audio_item), result.sync_clips_id);
}

#[test]
fn move_synced_item_rejects_duration_mismatch_unchanged() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(3.0, Some("primary")),
        vec![audio_clip(3.0, "file:///a1.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();
    let (audio_track, audio_index, _) = stack.get_item(&audio_id).unwrap();
    if let Item::Clip(clip) = &mut stack.children[audio_track].items[audio_index] {
        clip.source_range.duration.value = 4.0;
    }
    let original = stack.clone();

    assert!(!stack.move_item_at_time(
        "primary",
        "v",
        1.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));
    assert_eq!(stack.children, original.children);
}

#[test]
fn move_unsynced_item_only_moves_selected_item() {
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
fn move_unsynced_item_without_gap_pulls_later_synced_assets() {
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
    // The synced group is pulled back so the video and audio members stay aligned.
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
    // The audio track holds only the synced audio clip; the "unlinked" clip is video
    // only, and there is no trailing gap on the audio track (it would be trimmed).
    assert_eq!(stack.children[audio_track_index].items.len(), 1);
    assert!(matches!(
        stack.children[audio_track_index].items[audio_item_index],
        Item::Clip(_)
    ));
}

#[test]
fn move_unsynced_item_with_gap_and_split_target_updates_synced_assets() {
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
    assert_eq!(sync_clips_id(&video_items[1]), result.sync_clips_id);
    assert_eq!(video_items[1].duration(), 1.0);
    assert_eq!(video_items[2].get_id().as_deref(), Some("unlinked"));
    assert_eq!(video_items[2].duration(), 1.0);
    assert_eq!(sync_clips_id(&video_items[3]), result.sync_clips_id);
    assert_eq!(video_items[3].duration(), 1.0);

    assert_eq!(audio_items.len(), 4);
    assert!(matches!(audio_items[0], Item::Gap(_)));
    assert_eq!(audio_items[0].duration(), 1.0);
    assert_eq!(sync_clips_id(&audio_items[1]), result.sync_clips_id);
    assert_eq!(audio_items[1].duration(), 1.0);
    assert!(matches!(audio_items[2], Item::Gap(_)));
    assert_eq!(audio_items[2].duration(), 1.0);
    assert_eq!(sync_clips_id(&audio_items[3]), result.sync_clips_id);
    assert_eq!(audio_items[3].duration(), 1.0);
}

#[test]
fn move_unsynced_item_pushes_full_synced_boundary_with_gap_sync_tracks() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(1.0, Some("unlinked"))));

    let mut stack = Stack::default();
    stack.children.push(video);
    let result = insert_with_audio(
        &mut stack,
        0,
        1.0,
        clip(2.0, Some("linked-video")),
        vec![
            audio_clip(2.0, "file:///linked-a1.wav", None),
            audio_clip(2.0, "file:///linked-a2.wav", None),
        ],
    )
    .unwrap();
    let audio_ids: Vec<_> = result
        .audio_clips
        .iter()
        .map(|(id, _)| id.clone())
        .collect();

    assert!(stack.move_item_at_time(
        "unlinked",
        "v",
        0.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Push,
    ));

    let (video_track_index, video_item_index, moved_item) = stack.get_item("unlinked").unwrap();
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        0.0
    );
    assert_eq!(moved_item.duration(), 1.0);

    let (synced_video_track_index, synced_video_item_index, synced_video) =
        stack.get_item("linked-video").unwrap();
    assert_eq!(
        stack.children[synced_video_track_index].start_time_of_item(synced_video_item_index),
        2.0
    );
    assert_eq!(sync_clips_id(synced_video), result.sync_clips_id);

    for audio_id in &audio_ids {
        let (audio_track_index, audio_item_index, audio_item) = stack.get_item(audio_id).unwrap();
        assert_eq!(
            stack.children[audio_track_index].start_time_of_item(audio_item_index),
            2.0
        );
        assert_eq!(sync_clips_id(audio_item), result.sync_clips_id);
        // The synced audio is aligned to start 2.0 behind a single leading gap; the
        // dedicated per-boundary gap track is gone, so the leading gap simply spans
        // 0..2 to line the audio up with the video's synced clip.
        let spacer_index = stack.children[audio_track_index].get_item_at_time(0.5).unwrap();
        assert!(matches!(
            stack.children[audio_track_index].items[spacer_index],
            Item::Gap(_)
        ));
        assert_eq!(
            stack.children[audio_track_index].items[spacer_index].duration(),
            2.0
        );
    }
}

#[test]
fn move_unsynced_item_pushes_full_synced_boundary_for_insert_policies() {
    let cases = [
        (InsertPolicy::SplitAndInsert, 0.0, 0.0, 2.0),
        (InsertPolicy::InsertBefore, 1.5, 1.0, 2.0),
        (InsertPolicy::InsertAfter, 0.5, 1.0, 2.0),
        (InsertPolicy::InsertBeforeOrAfter, 1.2, 1.0, 2.0),
    ];

    for (insert_policy, dest_time, moved_start, synced_start) in cases {
        let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
        video.items.push(Item::Clip(clip(1.0, Some("unlinked"))));

        let mut stack = Stack::default();
        stack.children.push(video);
        let result = insert_with_audio(
            &mut stack,
            0,
            1.0,
            clip(2.0, Some("linked-video")),
            vec![
                audio_clip(2.0, "file:///linked-a1.wav", None),
                audio_clip(2.0, "file:///linked-a2.wav", None),
            ],
        )
        .unwrap();
        let audio_ids: Vec<_> = result
            .audio_clips
            .iter()
            .map(|(id, _)| id.clone())
            .collect();

        assert!(stack.move_item_at_time(
            "unlinked",
            "v",
            dest_time,
            true,
            insert_policy,
            OverlapPolicy::Push,
        ));

        let (moved_track_index, moved_item_index, moved_item) =
            stack.get_item("unlinked").unwrap();
        assert_eq!(
            stack.children[moved_track_index].start_time_of_item(moved_item_index),
            moved_start,
            "moved start for {insert_policy:?}"
        );
        assert_eq!(moved_item.duration(), 1.0);

        let (synced_video_track_index, synced_video_item_index, synced_video) =
            stack.get_item("linked-video").unwrap();
        assert_eq!(
            stack.children[synced_video_track_index].start_time_of_item(synced_video_item_index),
            synced_start,
            "linked video start for {insert_policy:?}"
        );
        assert_eq!(sync_clips_id(synced_video), result.sync_clips_id);

        for audio_id in &audio_ids {
            let (audio_track_index, audio_item_index, audio_item) =
                stack.get_item(audio_id).unwrap();
            let audio_track = &stack.children[audio_track_index];
            assert_eq!(
                audio_track.start_time_of_item(audio_item_index),
                synced_start,
                "linked audio start for {insert_policy:?}"
            );
            assert_eq!(sync_clips_id(audio_item), result.sync_clips_id);
            assert!(
                range_is_gap_backed_for_test(audio_track, moved_start, moved_start + 1.0),
                "missing gap sync track for {insert_policy:?}"
            );
        }
    }
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

#[test]
fn replace_item_updates_synced_clips_duration_and_preserves_identity() {
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
    ));

    let primary = stack.get_item("primary").unwrap().2;
    let audio = stack.get_item(&audio_id).unwrap().2;
    assert_eq!(primary.get_id().as_deref(), Some("primary"));
    assert_eq!(primary.duration(), 5.0);
    assert_eq!(audio.duration(), 5.0);
    assert_eq!(sync_clips_id(primary), result.sync_clips_id);
    assert_eq!(sync_clips_id(audio), result.sync_clips_id);
    assert!(stack.get_item("replacement").is_none());
}

#[test]
fn replace_unsynced_item_only_replaces_selected_item() {
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
    ));

    assert_eq!(stack.get_item("primary").unwrap().2.duration(), 2.0);
    assert!(stack.get_item("replacement").is_none());
    assert!(stack.get_item("unlinked-audio").is_some());
}

#[test]
fn replace_unsynced_item_in_synced_boundary_adds_gap_sync_tracks() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(1.0, Some("unlinked"))));

    let mut stack = Stack::default();
    stack.children.push(video);
    let result = insert_with_audio(
        &mut stack,
        0,
        1.0,
        clip(2.0, Some("linked-video")),
        vec![
            audio_clip(2.0, "file:///linked-a1.wav", None),
            audio_clip(2.0, "file:///linked-a2.wav", None),
        ],
    )
    .unwrap();
    let audio_ids: Vec<_> = result
        .audio_clips
        .iter()
        .map(|(id, _)| id.clone())
        .collect();

    assert!(stack.replace_item(
        "unlinked",
        Item::Clip(clip(1.5, Some("replacement"))),
        None,
    ));

    let (video_track_index, video_item_index, replaced_item) =
        stack.get_item("unlinked").unwrap();
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        0.0
    );
    assert_eq!(replaced_item.duration(), 1.5);
    assert_eq!(sync_clips_id(replaced_item), None);
    assert!(stack.get_item("replacement").is_none());

    let (synced_video_track_index, synced_video_item_index, synced_video) =
        stack.get_item("linked-video").unwrap();
    assert_eq!(
        stack.children[synced_video_track_index].start_time_of_item(synced_video_item_index),
        1.5
    );
    assert_eq!(sync_clips_id(synced_video), result.sync_clips_id);

    for audio_id in &audio_ids {
        let (audio_track_index, audio_item_index, audio_item) = stack.get_item(audio_id).unwrap();
        let audio_track = &stack.children[audio_track_index];
        assert_eq!(audio_track.start_time_of_item(audio_item_index), 1.5);
        assert_eq!(sync_clips_id(audio_item), result.sync_clips_id);
        assert!(range_is_gap_backed_for_test(audio_track, 0.0, 1.5));
    }
}

#[test]
fn replace_unsynced_item_in_synced_boundary_removes_gap_sync_tracks() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(1.5, Some("unlinked"))));

    let mut stack = Stack::default();
    stack.children.push(video);
    let result = insert_with_audio(
        &mut stack,
        0,
        1.5,
        clip(2.0, Some("linked-video")),
        vec![
            audio_clip(2.0, "file:///linked-a1.wav", None),
            audio_clip(2.0, "file:///linked-a2.wav", None),
        ],
    )
    .unwrap();
    let audio_ids: Vec<_> = result
        .audio_clips
        .iter()
        .map(|(id, _)| id.clone())
        .collect();

    assert!(stack.replace_item(
        "unlinked",
        Item::Clip(clip(1.0, Some("replacement"))),
        None,
    ));

    let (video_track_index, video_item_index, replaced_item) =
        stack.get_item("unlinked").unwrap();
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        0.0
    );
    assert_eq!(replaced_item.duration(), 1.0);
    assert_eq!(sync_clips_id(replaced_item), None);

    let (synced_video_track_index, synced_video_item_index, synced_video) =
        stack.get_item("linked-video").unwrap();
    assert_eq!(
        stack.children[synced_video_track_index].start_time_of_item(synced_video_item_index),
        1.0
    );
    assert_eq!(sync_clips_id(synced_video), result.sync_clips_id);

    for audio_id in &audio_ids {
        let (audio_track_index, audio_item_index, audio_item) = stack.get_item(audio_id).unwrap();
        let audio_track = &stack.children[audio_track_index];
        assert_eq!(audio_track.start_time_of_item(audio_item_index), 1.0);
        assert_eq!(sync_clips_id(audio_item), result.sync_clips_id);
        assert!(range_is_gap_backed_for_test(audio_track, 0.0, 1.0));
    }
}

#[test]
fn replace_item_missing_active_reference_does_not_bind_default_media() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    stack.children[0]
        .items
        .push(Item::Clip(clip(3.0, Some("primary"))));

    assert!(stack.replace_item(
        "primary",
        Item::Clip(clip_with_references(2.0, None, Some("replacement"))),
        None,
    ));

    let replaced = stack.get_item("primary").unwrap().2;
    let Item::Clip(clip) = replaced else {
        panic!("expected replacement clip");
    };
    assert_eq!(clip.active_media_reference_key.as_deref(), None);
}

#[test]
fn replace_item_preserves_valid_non_default_active_reference() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    stack.children[0]
        .items
        .push(Item::Clip(clip(3.0, Some("primary"))));

    assert!(stack.replace_item(
        "primary",
        Item::Clip(clip_with_references(2.0, Some("ALT"), Some("replacement"))),
        None,
    ));

    let replaced = stack.get_item("primary").unwrap().2;
    let Item::Clip(clip) = replaced else {
        panic!("expected replacement clip");
    };
    assert_eq!(clip.active_media_reference_key.as_deref(), Some("ALT"));
}

#[test]
fn replace_item_clamps_replacement_to_available_range() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    stack.children[0]
        .items
        .push(Item::Clip(clip(3.0, Some("primary"))));

    assert!(stack.replace_item(
        "primary",
        Item::Clip(clip_with_references(
            150.0,
            Some("DEFAULT_MEDIA"),
            Some("replacement")
        )),
        None,
    ));

    let replaced = stack.get_item("primary").unwrap().2;
    let Item::Clip(clip) = replaced else {
        panic!("expected replacement clip");
    };
    assert_eq!(clip.source_range.duration.value, 100.0);
    assert_eq!(
        clip.active_media_reference_key.as_deref(),
        Some("DEFAULT_MEDIA")
    );
}

#[test]
fn replace_item_can_add_synced_audio_clip() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));

    assert!(
        stack.replace_item(
            "primary",
            Item::Clip(clip(3.0, Some("replacement"))),
            Some(vec![audio_clip(3.0, "file:///a1.wav", None)]),
        ) == false
    );

    stack.children[0]
        .items
        .push(Item::Clip(clip(3.0, Some("primary"))));

    assert!(stack.replace_item(
        "primary",
        Item::Clip(clip(3.0, Some("replacement"))),
        Some(vec![audio_clip(3.0, "file:///a1.wav", None)]),
    ));

    let primary = stack.get_item("primary").unwrap().2;
    let group = sync_clips_id(primary);
    assert!(group.is_some());
    assert_eq!(stack.children.len(), 2);
    assert_eq!(stack.children[0].kind, TrackKind::Audio);
    let audio = stack.children[0]
        .items
        .iter()
        .find(|item| matches!(item, Item::Clip(_)))
        .unwrap();
    assert_eq!(audio.duration(), 3.0);
    assert_eq!(sync_clips_id(audio), group);
}

#[test]
fn replace_item_replaces_existing_synced_audio_input() {
    // "video on top" layout: audio track "a" below the video "v" (lower index), so the
    // synced insert reuses "a" instead of creating a new audio track.
    let mut stack = Stack::default();
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(10.0)));
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(10.0)));
    stack.children.push(audio);
    stack.children.push(video);

    let result = insert_with_audio(
        &mut stack,
        1,
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
    ));

    let replacement_audio = stack.get_item(&first_audio_id).unwrap().2;
    assert_eq!(active_target_url(replacement_audio), Some("file:///a2.wav"));
    // The sync track reused the existing audio track below the video, so no extra
    // audio track was created during the linked insert.
    assert_eq!(stack.children.len(), 2);
    assert_eq!(
        stack
            .children
            .iter()
            .filter(|track| track.kind == TrackKind::Audio)
            .flat_map(|track| track.items.iter())
            .filter(|item| matches!(item, Item::Clip(_)))
            .count(),
        1
    );
    assert_eq!(
        sync_clips_id(replacement_audio),
        sync_clips_id(stack.get_item("primary").unwrap().2)
    );
}

#[test]
fn replace_item_with_fewer_audio_links_gaps_remaining_boundary_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("primary")),
        vec![
            audio_clip(4.0, "file:///original-a1.wav", None),
            audio_clip(4.0, "file:///original-a2.wav", None),
            audio_clip(4.0, "file:///original-a3.wav", None),
        ],
    )
    .unwrap();

    assert!(stack.replace_item(
        "primary",
        Item::Clip(clip(3.0, Some("replacement"))),
        Some(vec![
            audio_clip(3.0, "file:///replacement-a1.wav", None),
            audio_clip(3.0, "file:///replacement-a2.wav", None),
        ]),
    ));

    let primary = stack.get_item("primary").unwrap().2;
    assert_eq!(primary.duration(), 3.0);
    assert_eq!(sync_clips_id(primary), result.sync_clips_id);
    assert_eq!(
        stack
            .children
            .iter()
            .filter(|track| track.kind == TrackKind::Audio)
            .count(),
        3
    );
    assert_eq!(
        stack
            .children
            .iter()
            .filter(|track| track.kind == TrackKind::Audio)
            .flat_map(|track| track.items.iter())
            .filter(|item| matches!(item, Item::Clip(_)))
            .count(),
        2
    );
    assert!(stack
        .children
        .iter()
        .filter(|track| track.kind == TrackKind::Audio)
        .flat_map(|track| track.items.iter())
        .any(|item| matches!(item, Item::Gap(_)) && (item.duration() - 3.0).abs() <= 1e-9));
}

#[test]
fn replace_item_with_more_audio_links_creates_additional_audio_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("v".to_string())));
    let result = insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("primary")),
        vec![
            audio_clip(4.0, "file:///original-a1.wav", None),
            audio_clip(4.0, "file:///original-a2.wav", None),
        ],
    )
    .unwrap();

    assert!(stack.replace_item(
        "primary",
        Item::Clip(clip(3.0, Some("replacement"))),
        Some(vec![
            audio_clip(3.0, "file:///replacement-a1.wav", None),
            audio_clip(3.0, "file:///replacement-a2.wav", None),
            audio_clip(3.0, "file:///replacement-a3.wav", None),
        ]),
    ));

    let primary = stack.get_item("primary").unwrap().2;
    assert_eq!(primary.duration(), 3.0);
    assert_eq!(sync_clips_id(primary), result.sync_clips_id);
    assert_eq!(
        stack
            .children
            .iter()
            .filter(|track| track.kind == TrackKind::Audio)
            .count(),
        3
    );
    assert_eq!(
        stack
            .children
            .iter()
            .filter(|track| track.kind == TrackKind::Audio)
            .flat_map(|track| track.items.iter())
            .filter(|item| matches!(item, Item::Clip(_)))
            .count(),
        3
    );
    assert!(stack
        .children
        .iter()
        .filter(|track| track.kind == TrackKind::Audio)
        .flat_map(|track| track.items.iter())
        .all(|item| item.duration() == 3.0));
}

#[test]
fn replace_item_on_synced_audio_replaces_audio_asset_and_keeps_video_link() {
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
    ));

    let video = stack.get_item("video").unwrap().2;
    let audio = stack.get_item(&audio_id).unwrap().2;
    assert_eq!(video.duration(), 4.0);
    assert_eq!(audio.duration(), 4.0);
    assert_eq!(sync_clips_id(video), result.sync_clips_id);
    assert_eq!(sync_clips_id(audio), result.sync_clips_id);
    assert_eq!(active_target_url(video), Some("file:///video.mov"));
    assert_eq!(active_target_url(audio), Some("file:///replacement-audio.wav"));
}

#[test]
fn split_item_at_time_splits_synced_clips() {
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
        sync_clips_id(&stack.children[0].items[0]),
        result.sync_clips_id
    );
    assert_eq!(
        sync_clips_id(&stack.children[0].items[1]),
        Some(result.sync_clips_id.unwrap() + 1)
    );
    assert_eq!(
        sync_clips_id(&stack.children[result.audio_clips[0].1].items[0]),
        result.sync_clips_id
    );
    assert_eq!(
        sync_clips_id(&stack.children[result.audio_clips[0].1].items[1]),
        Some(result.sync_clips_id.unwrap() + 1)
    );
    assert!(stack.get_item(&audio_id).is_some());
    assert_ne!(
        stack.children[0].items[0].get_id(),
        stack.children[0].items[1].get_id()
    );
    assert_ne!(
        stack.children[result.audio_clips[0].1].items[0].get_id(),
        stack.children[result.audio_clips[0].1].items[1].get_id()
    );
}

#[test]
fn split_unsynced_item_only_splits_selected_track() {
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
    for item_id in [&right_video_id, &right_audio_id] {
        let (_, _, item) = stack.get_item(item_id).unwrap();
        assert_eq!(item.duration(), 1.0);
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
    assert!(stack.get_item("second-video").is_none());
    assert!(stack.get_item(&second_audio_id).is_none());
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
fn replace_item_rejects_synced_audio_with_different_duration() {
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
    ));
    assert_eq!(stack.children, original.children);
}

#[test]
fn unsync_item_accepts_multiple_ids_and_cleans_singletons() {
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
        stack.unsync_item(&["primary".to_string(), "primary-2".to_string()]),
        4
    );

    assert_eq!(sync_clips_id(stack.get_item("primary").unwrap().2), None);
    assert_eq!(
        sync_clips_id(stack.get_item(&first.audio_clips[0].0).unwrap().2),
        None
    );
    assert_eq!(sync_clips_id(stack.get_item("primary-2").unwrap().2), None);
    assert_eq!(
        sync_clips_id(stack.get_item(&second.audio_clips[0].0).unwrap().2),
        None
    );
}

#[test]
fn sync_item_links_arbitrary_existing_clips_with_new_group() {
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(3.0, Some("primary"))));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(Item::Clip(clip(3.0, Some("audio"))));
    stack.children.push(video);
    stack.children.push(audio);

    let group = stack
        .sync_item(&["primary".to_string(), "audio".to_string()])
        .unwrap();

    assert_eq!(
        sync_clips_id(stack.get_item("primary").unwrap().2),
        Some(group)
    );
    assert_eq!(
        sync_clips_id(stack.get_item("audio").unwrap().2),
        Some(group)
    );
}

#[test]
fn sync_item_rejects_items_with_different_boundaries() {
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(3.0, Some("primary"))));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(1.0)));
    audio.items.push(Item::Clip(clip(3.0, Some("audio"))));
    stack.children.push(video);
    stack.children.push(audio);

    assert_eq!(
        stack.sync_item(&["primary".to_string(), "audio".to_string()]),
        None
    );
    assert_eq!(sync_clips_id(stack.get_item("primary").unwrap().2), None);
    assert_eq!(sync_clips_id(stack.get_item("audio").unwrap().2), None);
}

// ---------------------------------------------------------------------------
// Adding an UNSYNCED clip into a timeline that already has two synced clips.
//
// Setup: a video track with two clips (vA, vB), each synced to an audio clip
// (aA, aB) on the audio track. We then insert a clip that has no sync partner and
// check, across every position and policy, that (1) the inserted clip stays
// unsynced and (2) every surviving sync clips keeps one shared duration footprint
// across its tracks (the sync invariant: equal duration, members may differ in
// absolute start).
// ---------------------------------------------------------------------------

fn two_synced_clips() -> Stack {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(4.0, Some("vA")))); // 0..4
    video.items.push(Item::Clip(clip(4.0, Some("vB")))); // 4..8
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(audio_clip(4.0, "file:///aA.wav", None));
    audio.items[0].set_id(Some("aA".to_string()));
    audio.items.push(audio_clip(4.0, "file:///aB.wav", None));
    audio.items[1].set_id(Some("aB".to_string()));
    let mut stack = Stack::default();
    stack.children.push(audio);
    stack.children.push(video);
    stack.sync_item(&["vA".to_string(), "aA".to_string()]).unwrap();
    stack.sync_item(&["vB".to_string(), "aB".to_string()]).unwrap();
    stack
}

// For every sync clips, the sorted list of member durations must be identical on
// each track the group occupies (a group split into segments must split the same
// way on every track).
fn assert_sync_clips_track_aligned(stack: &Stack, label: &str) {
    use std::collections::HashMap;
    let mut map: HashMap<i64, HashMap<usize, Vec<f64>>> = HashMap::new();
    for (track_index, track) in stack.children.iter().enumerate() {
        for item in &track.items {
            if let Some(group) = sync_clips_id(item) {
                map.entry(group)
                    .or_default()
                    .entry(track_index)
                    .or_default()
                    .push(item.duration().max(0.0));
            }
        }
    }
    for (group, per_track) in &map {
        let mut sorted: Vec<Vec<f64>> = per_track
            .values()
            .map(|v| {
                let mut v = v.clone();
                v.sort_by(|a, b| a.partial_cmp(b).unwrap());
                v
            })
            .collect();
        let reference = sorted.pop().unwrap();
        for durs in &sorted {
            assert_eq!(
                durs.len(),
                reference.len(),
                "{label}: sync clips {group} segment count differs across tracks: {durs:?} vs {reference:?}"
            );
            for (x, y) in durs.iter().zip(reference.iter()) {
                assert!(
                    (x - y).abs() < 1e-9,
                    "{label}: sync clips {group} duration footprint differs across tracks: {durs:?} vs {reference:?}"
                );
            }
        }
    }
}

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
fn move_unsynced_image_into_synced_clip_override_assigns_new_link_group_to_right() {
    const SYNC_ID: i64 = 2;
    const SYNC_DUR: f64 = 42.0;
    const IMAGE_DUR: f64 = 5.0;
    const LEAD: f64 = 5.0;

    let mut audio = Track::new(TrackKind::Audio, Some("A3".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(LEAD)));
    audio.items.push(synced_clip_item(SYNC_DUR, "sync-audio", SYNC_ID));

    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(LEAD)));
    video.items.push(synced_clip_item(SYNC_DUR, "sync-video", SYNC_ID));
    video
        .items
        .push(Item::Clip(clip(IMAGE_DUR, Some("screenshot"))));

    let mut stack = Stack::default();
    stack.children.push(audio);
    stack.children.push(video);

    assert!(stack.move_item_at_time(
        "screenshot",
        "v",
        20.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    assert_eq!(
        sync_clips_id(stack.get_item("sync-video").unwrap().2),
        Some(SYNC_ID)
    );

    let (_, video_track) = stack.get_track_by_id("v").unwrap();
    let sync_group_ids: Vec<_> = video_track
        .items
        .iter()
        .filter_map(sync_clips_id)
        .collect();
    assert_eq!(sync_group_ids, vec![SYNC_ID, SYNC_ID + 1]);
    assert_eq!(
        sync_clips_id(stack.get_item("screenshot").unwrap().2),
        None
    );

    let (_, audio_track) = stack.get_track_by_id("A3").unwrap();
    let audio_sync_group_ids: Vec<_> = audio_track
        .items
        .iter()
        .filter_map(sync_clips_id)
        .collect();
    assert_eq!(audio_sync_group_ids, vec![SYNC_ID, SYNC_ID + 1]);
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

// ---------------------------------------------------------------------------
// Cross-track synced-move matrix.
//
// Move a sync set's primary onto a DIFFERENT track across every combination of:
//   primary kind {video, audio} x audio count {1, 2}
//   x destination {empty track, another sync track} x overlap {Override, Push}
//   x placement {by time, by index} x replace_with_gap {true, false}
// and assert in every case that the whole set relocates onto the destination,
// stays one sync set with a single shared duration, and the timeline stays
// coherent (every sync set duration-aligned across its tracks).
// ---------------------------------------------------------------------------

// Build one sync set (1 video + `audio_count` audios), all at time 0, duration `dur`.
// Tracks are named "{prefix}-v" / "{prefix}-a{i}", clips "{prefix}-vid" / "{prefix}-aud{i}".
fn push_sync_set(stack: &mut Stack, prefix: &str, dur: f64, audio_count: usize) {
    let mut video = Track::new(TrackKind::Video, Some(format!("{prefix}-v")));
    video
        .items
        .push(Item::Clip(clip(dur, Some(&format!("{prefix}-vid")))));
    stack.children.push(video);
    let mut ids = vec![format!("{prefix}-vid")];
    for i in 0..audio_count {
        let mut audio = Track::new(TrackKind::Audio, Some(format!("{prefix}-a{i}")));
        let mut a = audio_clip(dur, &format!("file:///{prefix}-a{i}.wav"), None);
        a.set_id(Some(format!("{prefix}-aud{i}")));
        audio.items.push(a);
        stack.children.push(audio);
        ids.push(format!("{prefix}-aud{i}"));
    }
    stack.sync_item(&ids.iter().map(String::as_str).map(str::to_string).collect::<Vec<_>>())
        .unwrap();
}

// Build an empty destination: 1 video track + `audio_count` audio tracks, all gaps.
fn push_empty_dest(stack: &mut Stack, prefix: &str, audio_count: usize, len: f64) {
    let mut video = Track::new(TrackKind::Video, Some(format!("{prefix}-v")));
    video.items.push(Item::Gap(Gap::make_gap(len)));
    stack.children.push(video);
    for i in 0..audio_count {
        let mut audio = Track::new(TrackKind::Audio, Some(format!("{prefix}-a{i}")));
        audio.items.push(Item::Gap(Gap::make_gap(len)));
        stack.children.push(audio);
    }
}

fn ids_for_set(prefix: &str, audio_count: usize) -> Vec<String> {
    let mut ids = vec![format!("{prefix}-vid")];
    for i in 0..audio_count {
        ids.push(format!("{prefix}-aud{i}"));
    }
    ids
}

#[test]
fn move_sync_set_to_different_track_matrix() {
    for &primary_is_video in &[true, false] {
        for &audio_count in &[1usize, 2] {
            for &dest_has_sync in &[false, true] {
                for &op in &[OverlapPolicy::Override, OverlapPolicy::Push] {
                    for &by_index in &[false, true] {
                        for &replace_with_gap in &[true, false] {
                            let label = format!(
                                "video={primary_is_video} n_audio={audio_count} dest_sync={dest_has_sync} op={op:?} by_index={by_index} gap={replace_with_gap}"
                            );
                            let mut stack = Stack::default();
                            push_sync_set(&mut stack, "s", 3.0, audio_count);
                            if dest_has_sync {
                                push_sync_set(&mut stack, "d", 2.0, audio_count);
                            } else {
                                push_empty_dest(&mut stack, "d", audio_count, 20.0);
                            }

                            let primary_id = if primary_is_video {
                                "s-vid".to_string()
                            } else {
                                "s-aud0".to_string()
                            };
                            let dest_track_id = if primary_is_video {
                                "d-v".to_string()
                            } else {
                                "d-a0".to_string()
                            };

                            let moved = if by_index {
                                stack.move_item_at_index(
                                    &primary_id,
                                    &dest_track_id,
                                    0,
                                    replace_with_gap,
                                    op,
                                )
                            } else {
                                stack.move_item_at_time(
                                    &primary_id,
                                    &dest_track_id,
                                    0.0,
                                    replace_with_gap,
                                    InsertPolicy::InsertBefore,
                                    op,
                                )
                            };
                            assert!(moved, "{label}: move returned false");

                            // The whole source set survived as one sync set, one shared
                            // duration (3.0), and the primary landed on the destination track.
                            let group = sync_clips_id(stack.get_item(&primary_id).unwrap().2);
                            assert!(group.is_some(), "{label}: moved primary lost its sync id");
                            for id in ids_for_set("s", audio_count) {
                                let (track_index, item_index, item) = stack
                                    .get_item(&id)
                                    .unwrap_or_else(|| panic!("{label}: {id} missing after move"));
                                assert_eq!(
                                    sync_clips_id(item),
                                    group,
                                    "{label}: {id} left the sync set"
                                );
                                assert_eq!(
                                    item.duration(),
                                    3.0,
                                    "{label}: {id} duration changed"
                                );
                                assert_eq!(
                                    stack.children[track_index].start_time_of_item(item_index),
                                    0.0,
                                    "{label}: {id} not at moved time"
                                );
                            }
                            let (primary_track, _, _) = stack.get_item(&primary_id).unwrap();
                            assert_eq!(
                                stack.children[primary_track].get_id().as_deref(),
                                Some(dest_track_id.as_str()),
                                "{label}: primary not on destination track"
                            );

                            // Whole timeline stays coherent (every sync set aligned).
                            assert_sync_clips_track_aligned(&stack, &label);
                        }
                    }
                }
            }
        }
    }
}

// Regression for the "OTOT" shape: a video track whose first clip is a sync set
// spanning many audio tracks. A Push insert at t=0 must push the video clip AND every
// synced audio clip in lockstep — previously the audio was left behind, desyncing the
// group (video pushed right, audio stuck at 0).
#[test]
fn push_insert_at_start_of_synced_group_pushes_all_synced_audio_tracks() {
    let group_dur = 5.0;
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(group_dur, Some("v-g")))); // 0..5 synced
    video.items.push(Item::Gap(Gap::make_gap(3.0)));
    video.items.push(Item::Clip(clip(2.0, Some("v-tail"))));

    let mut stack = Stack::default();
    let mut group_ids = vec!["v-g".to_string()];
    for i in 0..4 {
        let mut audio = Track::new(TrackKind::Audio, Some(format!("a{i}")));
        let mut a = audio_clip(group_dur, &format!("file:///a{i}.wav"), None);
        a.set_id(Some(format!("a-g{i}")));
        audio.items.push(a); // 0..5 synced
        stack.children.push(audio);
        group_ids.push(format!("a-g{i}"));
    }
    stack.children.push(video);
    stack.sync_item(&group_ids).unwrap();
    let group = sync_clips_id(stack.get_item("v-g").unwrap().2);
    let video_track_index = track_index_by_id(&stack, "v");

    // Push-insert an unsynced clip at the very start of the video track.
    let insert_dur = 4.0;
    let result = stack.insert_item_at_time(
        video_track_index,
        0.0,
        Item::Clip(clip(insert_dur, Some("inserted"))),
        OverlapPolicy::Push,
        InsertPolicy::SplitAndInsert,
        None,
    None,
    );
    assert!(result.is_some(), "insert returned None");
    assert_eq!(sync_clips_id(stack.get_item("inserted").unwrap().2), None);

    let (vt, vi, _) = stack.get_item("v-g").unwrap();
    let video_start = stack.children[vt].start_time_of_item(vi);
    assert!(
        (video_start - insert_dur).abs() < 1e-9,
        "video clip not pushed: {video_start}"
    );
    // Every synced audio clip must be pushed to the same start as the video clip.
    for i in 0..4 {
        let id = format!("a-g{i}");
        let (at, ai, item) = stack.get_item(&id).unwrap();
        let audio_start = stack.children[at].start_time_of_item(ai);
        assert!(
            (audio_start - video_start).abs() < 1e-9,
            "synced audio {id} left behind: audio_start={audio_start} video_start={video_start}"
        );
        assert_eq!(sync_clips_id(item), group, "{id} left the sync group");
    }
    assert_sync_clips_track_aligned(&stack, "push-insert-at-start-of-synced-group");
}
