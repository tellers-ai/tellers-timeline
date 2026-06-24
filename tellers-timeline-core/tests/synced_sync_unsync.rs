mod common;
use common::*;

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
fn sync_item_links_clips_with_different_start_times() {
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(3.0, Some("primary"))));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(1.0)));
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
    assert_eq!(sync_clips_id(stack.get_item("audio").unwrap().2), Some(group));
    let (primary_track, primary_index, _) = stack.get_item("primary").unwrap();
    let (audio_track, audio_index, _) = stack.get_item("audio").unwrap();
    assert_eq!(
        stack.children[primary_track].start_time_of_item(primary_index),
        0.0
    );
    assert_eq!(
        stack.children[audio_track].start_time_of_item(audio_index),
        1.0
    );
}

#[test]
fn sync_item_links_clips_with_different_durations() {
    let mut stack = Stack::default();
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(5.0, Some("primary"))));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio.items.push(Item::Clip(clip(3.0, Some("audio"))));
    stack.children.push(video);
    stack.children.push(audio);

    let group = stack
        .sync_item(&["primary".to_string(), "audio".to_string()])
        .unwrap();

    let (_, _, primary_item) = stack.get_item("primary").unwrap();
    let (_, _, audio_item) = stack.get_item("audio").unwrap();
    assert_eq!(sync_clips_id(primary_item), Some(group));
    assert_eq!(sync_clips_id(audio_item), Some(group));
    assert_eq!(primary_item.duration(), 5.0);
    assert_eq!(audio_item.duration(), 3.0);
}
