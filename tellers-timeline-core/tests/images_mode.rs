// Images timelines (video-player-js #835, #854): `metadata["tellers.ai"].edit_mode`
// and the compounds and pages the player's getCompounds() / getPages() return.
// A page lasts its compound's duration. `images_pages.otio` is the player's own
// example.

use tellers_timeline_core::{
    CompositeStack, EditMode, Gap, Item, PageInfo, Timeline, Track, TrackKind,
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
    let compounds: Vec<_> = timeline
        .compounds()
        .into_iter()
        .map(|page| (page.name, page.start, page.duration))
        .collect();
    assert_eq!(
        compounds,
        vec![
            ("Cover".to_string(), 0.0, 1.5),
            ("Offer".to_string(), 1.5, 1.5),
            ("Call to action".to_string(), 3.0, 2.5),
        ]
    );
    assert_eq!(
        timeline.pages(),
        vec![
            PageInfo {
                time: 0.0,
                duration: 1.5
            },
            PageInfo {
                time: 1.5,
                duration: 1.5
            },
            PageInfo {
                time: 3.0,
                duration: 2.5
            },
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
fn a_page_lasts_until_the_next_starts_and_the_last_until_its_compounds_end() {
    let mut timeline = Timeline::default();
    timeline.set_edit_mode(EditMode::Images);
    let mut v1 = Track::new(TrackKind::Video, Some("v1".into()));
    v1.items = vec![page("A", "a", 2.0), page("B", "b", 1.0)];
    // A second layer: a page starting with A but shorter, and one that
    // outlasts B.
    let mut v2 = Track::new(TrackKind::Video, Some("v2".into()));
    v2.items = vec![
        page("A2", "a2", 1.0),
        Item::Gap(Gap::new(5.0, None)),
        page("C", "c", 3.0),
    ];
    timeline.add_track(v1);
    timeline.add_track(v2);
    // v2's gap takes no time: C starts at 1, inside A.
    assert_eq!(
        timeline.pages(),
        vec![
            PageInfo {
                time: 0.0,
                duration: 1.0
            },
            PageInfo {
                time: 1.0,
                duration: 1.0
            },
            PageInfo {
                time: 2.0,
                duration: 2.0
            },
        ]
    );
}

#[test]
fn no_compounds_no_pages() {
    assert!(Timeline::default().pages().is_empty());
}

#[test]
fn page_duration_is_the_compounds_duration() {
    let mut timeline = images_pages();
    let id = timeline.compounds()[0].id.clone().unwrap();
    let (t, i, _) = timeline.tracks.get_item(&id).unwrap();
    timeline.tracks.children[t].items[i].set_duration(4.0);
    assert_eq!(
        timeline.pages()[0],
        PageInfo {
            time: 0.0,
            duration: 4.0
        }
    );
    assert_eq!(timeline.pages()[1].time, 4.0);
}
