// Copy and paste (`Timeline::copy_items` / `Timeline::paste_items`): the copy
// lands on its source tracks at the paste time, never overwrites, and shares
// no identity (ids, sync links, edit groups, clip groups) with the original.

mod common;

use common::*;
use serde_json::{json, Value};
use tellers_timeline_core::*;

/// "v1" at 0..2 on "video-1", "v2" at 4..6 on "video-2" (on top).
fn two_video_tracks() -> Timeline {
    let mut low = Track::new(TrackKind::Video, Some("video-1".to_string()));
    low.items.push(Item::Clip(clip(2.0, Some("v1"))));
    let mut high = Track::new(TrackKind::Video, Some("video-2".to_string()));
    high.items.push(Item::Gap(Gap::make_gap(4.0)));
    high.items.push(Item::Clip(clip(2.0, Some("v2"))));
    let mut timeline = Timeline::default();
    timeline.tracks.children.push(low);
    timeline.tracks.children.push(high);
    timeline
}

/// "v1" on "video" at 0..2, synced with "v1a" on "audio" below it.
fn synced_video_and_audio() -> Timeline {
    let mut timeline = Timeline::default();
    timeline
        .tracks
        .children
        .push(Track::new(TrackKind::Audio, Some("audio".to_string())));
    timeline
        .tracks
        .children
        .push(Track::new(TrackKind::Video, Some("video".to_string())));
    insert_with_audio(
        &mut timeline.tracks,
        1,
        0.0,
        clip(2.0, Some("v1")),
        vec![audio_clip(2.0, "file:///a.wav", Some("a"))],
    )
    .expect("synced insert");
    timeline
}

fn place(timeline: &Timeline, id: &str) -> (String, f64) {
    let (track_index, item_index, _) = timeline.tracks.get_item(id).expect("item present");
    let track = &timeline.tracks.children[track_index];
    (
        track.get_id().unwrap(),
        track.start_time_of_item(item_index),
    )
}

fn track_ids(timeline: &Timeline) -> Vec<String> {
    timeline
        .tracks
        .children
        .iter()
        .map(|t| t.get_id().unwrap())
        .collect()
}

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| v.to_string()).collect()
}

#[test]
fn paste_puts_each_clip_back_on_its_track_at_the_paste_time() {
    let mut timeline = two_video_tracks();
    let clipboard = timeline.copy_items(&ids(&["v1", "v2"])).expect("copied");

    let pasted = timeline.paste_items(&clipboard, 10.0);

    assert_eq!(pasted.len(), 2);
    assert!(!pasted.iter().any(|id| id == "v1" || id == "v2"));
    let mut places: Vec<_> = pasted.iter().map(|id| place(&timeline, id)).collect();
    places.sort_by(|a, b| a.1.total_cmp(&b.1));
    assert_eq!(places, [("video-1".into(), 10.0), ("video-2".into(), 14.0)]);
    assert_eq!(track_ids(&timeline), ["video-1", "video-2"]);
    assert_eq!(place(&timeline, "v1"), ("video-1".into(), 0.0));
    assert_eq!(place(&timeline, "v2"), ("video-2".into(), 4.0));
}

#[test]
fn paste_onto_an_occupied_track_goes_to_a_new_track_and_overwrites_nothing() {
    let mut timeline = two_video_tracks();
    let clipboard = timeline.copy_items(&ids(&["v1"])).unwrap();

    // 1..3 overlaps v1 (0..2) on video-1.
    let pasted = timeline.paste_items(&clipboard, 1.0);

    assert_eq!(pasted.len(), 1);
    let tracks = track_ids(&timeline);
    assert_eq!(tracks.len(), 3);
    // Right above video-1, under video-2.
    assert_eq!(
        (tracks[0].as_str(), tracks[2].as_str()),
        ("video-1", "video-2")
    );
    assert_eq!(place(&timeline, &pasted[0]), (tracks[1].clone(), 1.0));
    assert_eq!(place(&timeline, "v1"), ("video-1".into(), 0.0));
    assert_eq!(timeline.tracks.get_item("v1").unwrap().2.duration(), 2.0);
}

#[test]
fn a_video_clip_is_copied_and_pasted_with_its_own_linked_audio() {
    let mut timeline = synced_video_and_audio();
    let original_link = sync_clips_id(timeline.tracks.get_item("v1").unwrap().2).unwrap();

    // The video alone brings its audio.
    let clipboard = timeline.copy_items(&ids(&["v1"])).unwrap();
    assert_eq!(clipboard.columns.len(), 1);
    assert_eq!(clipboard.columns[0].linked_audio.len(), 1);
    let pasted = timeline.paste_items(&clipboard, 5.0);

    assert_eq!(pasted.len(), 2);
    let mut places: Vec<_> = pasted.iter().map(|id| place(&timeline, id)).collect();
    places.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(places, [("audio".into(), 5.0), ("video".into(), 5.0)]);
    let link = |id: &str| sync_clips_id(timeline.tracks.get_item(id).unwrap().2);
    let pasted_link = link(&pasted[0]).expect("the copy is synced");
    assert_eq!(link(&pasted[1]), Some(pasted_link));
    assert_ne!(pasted_link, original_link);
    assert_eq!(track_ids(&timeline), ["audio", "video"]);
}

#[test]
fn a_pasted_edit_group_is_a_new_group_each_time() {
    let mut timeline = two_video_tracks();
    timeline.tracks.group_item(&ids(&["v1", "v2"])).unwrap();
    let group = |timeline: &Timeline, id: &str| {
        item_tellers_group_id(timeline.tracks.get_item(id).unwrap().2)
    };
    let original = group(&timeline, "v1").expect("grouped");

    let clipboard = timeline.copy_items(&ids(&["v1", "v2"])).unwrap();
    let first = timeline.paste_items(&clipboard, 10.0);
    let second = timeline.paste_items(&clipboard, 20.0);

    let first_group = group(&timeline, &first[0]).unwrap();
    let second_group = group(&timeline, &second[0]).unwrap();
    assert_eq!(group(&timeline, &first[1]), Some(first_group));
    assert_eq!(group(&timeline, &second[1]), Some(second_group));
    assert_ne!(first_group, original);
    assert_ne!(second_group, original);
    assert_ne!(first_group, second_group);
    assert_eq!(group(&timeline, "v1"), Some(original));
}

#[test]
fn a_pasted_clip_group_gets_its_own_copy_of_the_transform() {
    let mut timeline = two_video_tracks();
    let parameters =
        json!({"x": {"keyframes": [{"t": 0.0, "v": 0.0}, {"t": 1.0, "v": 0.5}]}, "opacity": 0.5});
    let original = timeline
        .create_clip_group(&ids(&["v1", "v2"]), parameters.as_object().unwrap().clone())
        .unwrap();

    let clipboard = timeline.copy_items(&ids(&["v1", "v2"])).unwrap();
    let pasted = timeline.paste_items(&clipboard, 10.0);

    let copy = timeline
        .get_item_clip_group_id(&pasted[0])
        .expect("in a group");
    assert_ne!(copy, original);
    assert_eq!(
        timeline.get_item_clip_group_id(&pasted[1]),
        Some(copy.clone())
    );
    assert_eq!(
        Value::Object(timeline.get_clip_group(&copy).unwrap().parameters),
        parameters
    );
    assert_eq!(timeline.clip_group_member_ids(&original), ["v1", "v2"]);
}

#[test]
fn a_clipboard_pastes_into_another_timeline_through_json() {
    let source = synced_video_and_audio();
    let json = source.copy_items(&ids(&["v1"])).unwrap().to_json();
    let clipboard = TimelineClipboard::from_json(&json).unwrap();

    // A timeline without the source tracks gets new ones.
    let mut target = Timeline::default();
    target
        .tracks
        .children
        .push(Track::new(TrackKind::Video, Some("other".to_string())));
    let pasted = target.paste_items(&clipboard, 3.0);

    assert_eq!(pasted.len(), 2);
    for id in &pasted {
        let (track, start) = place(&target, id);
        assert_ne!(track, "other");
        assert_eq!(start, 3.0);
    }
    let kinds: Vec<_> = pasted
        .iter()
        .map(|id| {
            target.tracks.children[target.tracks.get_item(id).unwrap().0]
                .kind
                .clone()
        })
        .collect();
    assert!(kinds.contains(&TrackKind::Video) && kinds.contains(&TrackKind::Audio));
}

#[test]
fn a_pasted_composite_holds_fresh_ids() {
    let mut inner = Track::new(TrackKind::Video, Some("inner-track".to_string()));
    inner.items.push(Item::Clip(clip(2.0, Some("inner-clip"))));
    let composite = CompositeStack::new(vec![inner], None, Some("comp".to_string()));
    let mut track = Track::new(TrackKind::Video, Some("video".to_string()));
    track.items.push(Item::Composite(composite));
    let mut timeline = Timeline::default();
    timeline.tracks.children.push(track);

    let clipboard = timeline.copy_items(&ids(&["comp"])).unwrap();
    let pasted = timeline.paste_items(&clipboard, 5.0);

    assert_eq!(pasted.len(), 1);
    let Item::Composite(copy) = timeline.tracks.get_item(&pasted[0]).unwrap().2 else {
        panic!("a composite was pasted");
    };
    assert_ne!(copy.children[0].get_id().as_deref(), Some("inner-track"));
    assert_ne!(
        copy.children[0].items[0].get_id().as_deref(),
        Some("inner-clip")
    );
}

#[test]
fn nothing_to_copy_or_paste_changes_nothing() {
    let mut timeline = two_video_tracks();
    assert_eq!(timeline.copy_items(&[]), None);
    assert_eq!(timeline.copy_items(&ids(&["missing"])), None);
    let before = timeline.clone();
    assert!(timeline
        .paste_items(&TimelineClipboard::default(), 0.0)
        .is_empty());
    assert_eq!(timeline, before);
    assert!(TimelineClipboard::from_json("not json").is_err());
}
