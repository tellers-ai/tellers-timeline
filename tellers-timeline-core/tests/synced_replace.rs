mod common;
use common::*;

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
    assert_eq!(stack.children.len(), 3);
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
