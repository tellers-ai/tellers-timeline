// Clip groups as video-player-js reads them (`otio-groups.ts`): members carry
// `metadata["tellers.ai"].group_id`, the timeline's
// `metadata["tellers.ai"].groups[id].parameters` is their shared transform.

mod common;

use common::*;
use serde_json::json;
use tellers_timeline_core::*;

fn timeline() -> Timeline {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Gap(Gap::new(1.0, None)));
    video.items.push(Item::Clip(clip(2.0, Some("a"))));
    let mut titles = Track::new(TrackKind::Video, Some("t".to_string()));
    titles.items.push(Item::Clip(clip(3.0, Some("b"))));
    titles.items.push(Item::Composite(CompositeStack::new(Vec::new(), None, Some("comp".to_string()))));
    let mut timeline = Timeline::default();
    timeline.tracks.children.push(video);
    timeline.tracks.children.push(titles);
    timeline
}

#[test]
fn reads_the_player_groups_and_members() {
    let mut timeline = timeline();
    timeline.metadata = json!({"tellers.ai": {"groups": {
        "g1": {"parameters": {
            "x": {"keyframes": [{"t": 0.0, "v": 0.0}, {"t": 2.0, "v": 0.5}]},
            "scale": 2.0, "scaleY": 0.5, "opacity": 3.0
        }, "label": "kept"},
        "g0": {}
    }}});
    assert!(timeline.set_item_clip_group_id("a", Some("g1")));
    assert!(timeline.set_item_clip_group_id("b", Some("g1")));

    let groups = timeline.get_clip_groups();
    assert_eq!(groups.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["g0", "g1"]);
    let group = timeline.get_clip_group("g1").unwrap();
    let at = group.transform_at(1.0);
    assert_eq!(at.x, 0.25);
    assert_eq!((at.scale_x, at.scale_y), (2.0, 1.0));
    assert_eq!(at.opacity, 1.0);
    assert_eq!(at.pivot_x, 0.0);

    assert_eq!(timeline.clip_group_member_ids("g1"), ["a", "b"]);
    // Keyframe time 0 is the earliest member's start ("b" at 0, not "a" at 1).
    assert_eq!(timeline.clip_group_start("g1"), Some(0.0));
    assert_eq!(timeline.get_item_clip_group_id("a").as_deref(), Some("g1"));

    // Numeric ids read as strings, like the player.
    if let Item::Clip(clip) = &mut timeline.tracks.children[0].items[1] {
        clip.metadata["tellers.ai"]["group_id"] = json!(7);
    }
    assert_eq!(timeline.get_item_clip_group_id("a").as_deref(), Some("7"));
}

#[test]
fn set_clip_group_keeps_other_keys_and_validates() {
    let mut timeline = timeline();
    timeline.metadata = json!({"tellers.ai": {"groups": {"g": {"parameters": {}, "label": "kept"}}}});
    let mut group = ClipGroup::new("g").with_parameter("rotation", 15.0);
    group.set_animatable("pivotX", Animatable::from(vec![Keyframe::new(0.0, 0.1)]));
    timeline.set_clip_group(&group).unwrap();
    let written = &timeline.metadata["tellers.ai"]["groups"]["g"];
    assert_eq!(written["label"], json!("kept"));
    assert_eq!(written["parameters"]["rotation"], json!(15.0));
    assert_eq!(written["parameters"]["pivotX"], json!({"keyframes": [{"t": 0.0, "v": 0.1}]}));

    assert!(timeline.set_clip_group(&ClipGroup::new("g").with_parameter("blur", 1.0)).is_err());
    assert!(timeline.set_clip_group(&ClipGroup::new("g").with_parameter("x", "far")).is_err());
    assert!(timeline.set_clip_group(&ClipGroup::new("")).is_err());
}

#[test]
fn create_and_remove_a_clip_group() {
    let mut timeline = timeline();
    let mut parameters = serde_json::Map::new();
    parameters.insert("opacity".to_string(), json!(0.5));
    let ids = vec!["a".to_string(), "comp".to_string()];
    let id = timeline.create_clip_group(&ids, parameters.clone()).unwrap();
    assert_eq!(timeline.clip_group_member_ids(&id), ["a", "comp"]);
    assert_eq!(timeline.get_clip_group(&id).unwrap().parameters, parameters);
    assert_eq!(timeline.clip_group_start(&id), Some(1.0));

    // A gap cannot be a member; nothing changes.
    let gap_id = timeline.tracks.children[0].items[0].get_id().unwrap();
    let before = timeline.clone();
    assert!(timeline.create_clip_group(&[gap_id, "b".to_string()], parameters).is_err());
    assert_eq!(timeline, before);

    assert!(timeline.remove_clip_group(&id));
    assert!(!timeline.remove_clip_group(&id));
    assert!(timeline.clip_group_member_ids(&id).is_empty());
    assert_eq!(timeline.get_item_clip_group_id("a"), None);
    assert!(timeline.get_clip_groups().is_empty());
}

#[test]
fn group_id_survives_a_split_and_a_round_trip() {
    let mut timeline = timeline();
    let id = timeline.create_clip_group(&["b".to_string()], Default::default()).unwrap();
    assert!(timeline.tracks.split_item_at_time("b", 1.0));
    assert_eq!(timeline.clip_group_member_ids(&id).len(), 2);

    let text = serde_json::to_string(&timeline).unwrap();
    let back: Timeline = serde_json::from_str(&text).unwrap();
    assert_eq!(back.clip_group_member_ids(&id).len(), 2);
    assert!(back.get_clip_group(&id).is_some());
}
