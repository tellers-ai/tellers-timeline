// An images timeline has no sound: switching to images drops its audio
// tracks, and every sanitize (after any edit, in the app or the bindings)
// drops audio an edit brought in, inside compounds too. A video timeline
// keeps its audio.

mod common;

use common::{audio_clip, synced_clip_item};
use tellers_timeline_core::{
    CompositeStack, EditMode, IdMetadataExt, Item, Timeline, Track, TrackKind,
};

/// A video clip and its sound, linked, on a video and an audio track.
fn video_with_sound() -> Timeline {
    let mut timeline = Timeline::default();
    let mut video = Track::new(TrackKind::Video, Some("v1".into()));
    video.items = vec![synced_clip_item(2.0, "picture", 7)];
    let mut audio = Track::new(TrackKind::Audio, Some("a1".into()));
    audio.items = vec![synced_clip_item(2.0, "sound", 7)];
    timeline.add_track(video);
    timeline.add_track(audio);
    timeline
}

fn kinds(tracks: &[Track]) -> Vec<TrackKind> {
    tracks.iter().map(|track| track.kind.clone()).collect()
}

#[test]
fn a_video_timeline_accepts_and_keeps_audio() {
    let mut timeline = video_with_sound();
    assert!(timeline.accepts_audio());
    timeline.sanitize();
    assert_eq!(
        kinds(&timeline.tracks.children),
        vec![TrackKind::Video, TrackKind::Audio]
    );
}

#[test]
fn switching_to_images_drops_the_audio() {
    let mut timeline = video_with_sound();
    timeline.set_edit_mode(EditMode::Images);
    assert!(!timeline.accepts_audio());
    assert_eq!(kinds(&timeline.tracks.children), vec![TrackKind::Video]);
    assert_eq!(
        timeline.tracks.children[0].items[0].get_id().as_deref(),
        Some("picture")
    );
}

#[test]
fn sanitize_drops_audio_added_to_an_images_timeline_inside_compounds_too() {
    let mut timeline = Timeline::default();
    timeline.set_edit_mode(EditMode::Images);

    let mut inner_video = Track::new(TrackKind::Video, Some("pv".into()));
    inner_video.items = vec![Item::Clip(common::clip(1.0, Some("still")))];
    let mut inner_audio = Track::new(TrackKind::Audio, Some("pa".into()));
    inner_audio.items = vec![audio_clip(1.0, "file:///music.mp3", None)];
    let page = CompositeStack::new(
        vec![inner_video, inner_audio],
        Some("Page".into()),
        Some("page".into()),
    );
    let mut video = Track::new(TrackKind::Video, Some("v1".into()));
    video.items = vec![Item::Composite(page)];
    let mut audio = Track::new(TrackKind::Audio, Some("a1".into()));
    audio.items = vec![audio_clip(1.0, "file:///voice.mp3", None)];
    timeline.add_track(video);
    timeline.add_track(audio);

    timeline.sanitize();

    assert_eq!(kinds(&timeline.tracks.children), vec![TrackKind::Video]);
    let Item::Composite(page) = &timeline.tracks.children[0].items[0] else {
        panic!("the page stays");
    };
    assert_eq!(kinds(&page.children), vec![TrackKind::Video]);
}

#[test]
fn remove_audio_says_whether_there_was_any() {
    let mut timeline = video_with_sound();
    assert!(timeline.remove_audio());
    assert!(!timeline.remove_audio());
}
