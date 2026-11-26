// Images timelines (video-player-js #835): `metadata["tellers.ai"].edit_mode`,
// each page's `frame_delay`, and the page list the player's getCompounds()
// returns. `images_pages.otio` is the player's own example.

use tellers_timeline_core::{
    CompositeStack, EditMode, Gap, IdMetadataExt, Item, Timeline, Track, TrackKind,
    DEFAULT_FRAME_DELAY,
};

fn images_pages() -> Timeline {
    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/images_pages.otio"
    ))
    .unwrap();
    serde_json::from_str(&json).unwrap()
}

fn page(name: &str, id: &str, duration: f64) -> Item {
    let mut stack = CompositeStack::new(vec![], Some(name.into()), Some(id.into()));
    stack.set_duration(duration);
    Item::Composite(stack)
}

#[test]
fn edit_mode_defaults_to_video() {
    assert_eq!(Timeline::default().edit_mode(), EditMode::Video);
}

#[test]
fn reads_the_players_images_example() {
    let timeline = images_pages();
    assert_eq!(timeline.edit_mode(), EditMode::Images);
    let pages: Vec<_> = timeline
        .compounds()
        .into_iter()
        .map(|page| (page.name, page.start, page.duration, page.frame_delay))
        .collect();
    assert_eq!(
        pages,
        vec![
            ("Cover".to_string(), 0.0, 2.0, 1.5),
            ("Offer".to_string(), 2.0, 2.0, 1.5),
            ("Call to action".to_string(), 4.0, 2.0, 2.5),
        ]
    );
}

#[test]
fn set_edit_mode_round_trips_and_video_removes_the_key() {
    let mut timeline = Timeline::default();
    timeline.set_edit_mode(EditMode::Images);
    assert_eq!(timeline.metadata["tellers.ai"]["edit_mode"], "images");
    let json = timeline.to_json().unwrap();
    let back: Timeline = serde_json::from_str(&json).unwrap();
    assert_eq!(back.edit_mode(), EditMode::Images);

    timeline.set_edit_mode(EditMode::Video);
    assert!(timeline.metadata["tellers.ai"].get("edit_mode").is_none());
    assert_eq!(timeline.edit_mode(), EditMode::Video);
}

#[test]
fn unknown_edit_mode_reads_as_video() {
    let mut timeline = Timeline::default();
    timeline.metadata["tellers.ai"]["edit_mode"] = "static".into();
    assert_eq!(timeline.edit_mode(), EditMode::Video);
}

#[test]
fn gaps_between_pages_take_no_time_only_in_images_mode() {
    let mut timeline = Timeline::default();
    let mut track = Track::new(TrackKind::Video, Some("v1".into()));
    track.items = vec![
        page("A", "a", 2.0),
        Item::Gap(Gap::new(3.0, None)),
        page("B", "b", 1.0),
    ];
    timeline.add_track(track);

    let starts = |timeline: &Timeline| -> Vec<f64> {
        timeline.compounds().iter().map(|page| page.start).collect()
    };
    assert_eq!(starts(&timeline), vec![0.0, 5.0]);
    timeline.set_edit_mode(EditMode::Images);
    assert_eq!(starts(&timeline), vec![0.0, 2.0]);
}

#[test]
fn compounds_skip_disabled_and_audio_tracks() {
    let mut timeline = Timeline::default();
    let mut video = Track::new(TrackKind::Video, Some("v1".into()));
    video.items = vec![page("A", "a", 1.0), page("B", "b", 1.0)];
    if let Item::Composite(stack) = &mut video.items[1] {
        stack.enabled = Some(false);
    }
    let mut audio = Track::new(TrackKind::Audio, Some("a1".into()));
    audio.items = vec![page("C", "c", 1.0)];
    timeline.add_track(video);
    timeline.add_track(audio);

    let ids: Vec<_> = timeline
        .compounds()
        .into_iter()
        .map(|page| page.id.unwrap())
        .collect();
    assert_eq!(ids, vec!["a"]);
}

#[test]
fn frame_delay_set_read_and_remove() {
    let mut timeline = images_pages();
    let cover = timeline.compounds()[0].id.clone().unwrap();
    assert_eq!(timeline.frame_delay(&cover), Some(1.5));

    assert!(timeline.set_frame_delay(&cover, Some(0.25)));
    assert_eq!(timeline.frame_delay(&cover), Some(0.25));
    assert_eq!(timeline.compounds()[0].frame_delay, 0.25);

    assert!(!timeline.set_frame_delay(&cover, Some(0.0)));
    assert!(!timeline.set_frame_delay(&cover, Some(f64::NAN)));
    assert_eq!(timeline.frame_delay(&cover), Some(0.25));

    assert!(timeline.set_frame_delay(&cover, None));
    assert_eq!(timeline.frame_delay(&cover), None);
    assert_eq!(timeline.compounds()[0].frame_delay, DEFAULT_FRAME_DELAY);
}

#[test]
fn frame_delay_only_on_compounds() {
    let mut timeline = Timeline::default();
    let mut track = Track::new(TrackKind::Video, Some("v1".into()));
    let gap = Gap::new(1.0, Some("g".into()));
    assert_eq!(gap.get_id().as_deref(), Some("g"));
    track.items = vec![Item::Gap(gap)];
    timeline.add_track(track);
    assert!(!timeline.set_frame_delay("g", Some(1.0)));
    assert!(!timeline.set_frame_delay("missing", Some(1.0)));
    assert_eq!(timeline.frame_delay("g"), None);
}
