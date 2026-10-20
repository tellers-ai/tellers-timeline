// Editing inside a composite clip: `Stack::with_composite_mut` lends the
// composite's tracks out as a working stack, so the usual item ops apply to
// nested items. A composite without source_range follows its content's length
// and the outer track ripples both ways (a resize of its tail with
// `OverlapPolicy::Push`).

mod common;

use common::*;
use tellers_timeline_core::{CompositeStack, TellersEffect};

const FIXTURE: &str = include_str!("fixtures/composite_clip.json");

fn item_ids(track: &Track) -> Vec<String> {
    track.items.iter().filter_map(|item| item.get_id()).collect()
}

fn start_of(stack: &Stack, id: &str) -> f64 {
    let (ti, ii, _) = stack.get_item(id).expect("top-level item");
    stack.children[ti].start_time_of_item(ii)
}

fn duration_of(stack: &Stack, id: &str) -> f64 {
    stack.get_nested_item(id).expect("item").duration()
}

fn track_with(id: &str, items: Vec<Item>) -> Track {
    let mut track = Track::new(TrackKind::Video, Some(id.to_string()));
    track.items = items;
    track
}

/// V1: [a 2s][comp (no source_range): inner [x 3s]][b 2s]
fn stack_with_auto_composite() -> Stack {
    let inner = track_with("inner", vec![Item::Clip(clip(3.0, Some("x")))]);
    let comp = CompositeStack::new(vec![inner], Some("Comp".into()), Some("comp".into()));
    let v1 = track_with(
        "v1",
        vec![
            Item::Clip(clip(2.0, Some("a"))),
            Item::Composite(comp),
            Item::Clip(clip(2.0, Some("b"))),
        ],
    );
    Stack { children: vec![v1], ..Stack::default() }
}

#[test]
fn nested_lookups_find_items_inside_composites() {
    let timeline: Timeline = serde_json::from_str(FIXTURE).unwrap();
    let stack = &timeline.tracks;
    assert!(stack.get_item("lt-bar-clip").is_none(), "get_item stays top-level");
    assert!(matches!(stack.get_nested_item("lt-bar-clip"), Some(Item::Clip(_))));
    assert_eq!(stack.composite_path("lt-bar-clip"), Some(vec!["lower-third".to_string()]));
    assert_eq!(stack.composite_path("lower-third"), Some(vec![]));
    assert_eq!(stack.composite_path("nope"), None);
    assert_eq!(stack.get_composite("lower-third").unwrap().children.len(), 2);
    assert!(stack.get_composite("lt-bar-clip").is_none());
    assert_eq!(stack.get_nested_item_track("lt-bar-clip").unwrap().get_id().as_deref(), Some("lt-bar"));
}

#[test]
fn unknown_or_non_composite_id_does_not_call_edit() {
    let mut stack = stack_with_auto_composite();
    let before = stack.clone();
    let mut called = false;
    assert!(stack.with_composite_mut("a", |_| called = true).is_none());
    assert!(stack.with_composite_mut("nope", |_| called = true).is_none());
    assert!(!called);
    assert!(!stack.set_composite_tracks("x", vec![]));
    assert_eq!(stack, before);
}

#[test]
fn split_inside_a_composite_keeps_the_outer_track() {
    let mut stack = stack_with_auto_composite();
    let ok = stack
        .with_composite_mut("comp", |inner| {
            assert_eq!(inner.name.as_deref(), Some("Comp"));
            inner.split_item_at_time("x", 1.0)
        })
        .unwrap();
    assert!(ok);
    let comp = stack.get_composite("comp").unwrap();
    assert_eq!(comp.children[0].items.len(), 2);
    assert!(comp.source_range.is_none());
    assert_eq!(duration_of(&stack, "comp"), 3.0);
    assert_eq!(start_of(&stack, "b"), 5.0);
    assert_eq!(comp.get_id().as_deref(), Some("comp"));
}

#[test]
fn growing_an_auto_length_composite_pushes_what_follows() {
    let mut stack = stack_with_auto_composite();
    stack
        .with_composite_mut("comp", |inner| {
            inner.insert_item_at_time(
                0,
                3.0,
                Item::Clip(clip(2.0, Some("y"))),
                OverlapPolicy::Override,
                InsertPolicy::SplitAndInsert,
                None,
                None,
            )
        })
        .unwrap()
        .expect("inserted");
    assert_eq!(duration_of(&stack, "comp"), 5.0);
    assert!(stack.get_composite("comp").unwrap().source_range.is_none());
    assert_eq!(start_of(&stack, "b"), 7.0);
    assert_eq!(item_ids(&stack.children[0]), ["a", "comp", "b"]);
}

#[test]
fn shrinking_an_auto_length_composite_pulls_what_follows() {
    let mut stack = stack_with_auto_composite();
    stack.with_composite_mut("comp", |inner| inner.split_item_at_time("x", 1.0));
    let tail = stack.get_composite("comp").unwrap().children[0].items[1].get_id().unwrap();
    stack.with_composite_mut("comp", |inner| inner.delete_item(&tail, false, true));
    assert_eq!(duration_of(&stack, "comp"), 1.0);
    assert!(stack.get_composite("comp").unwrap().source_range.is_none());
    assert_eq!(start_of(&stack, "b"), 3.0);
    assert_eq!(item_ids(&stack.children[0]), ["a", "comp", "b"]);
}

#[test]
fn composite_with_source_range_keeps_its_length() {
    let mut timeline: Timeline = serde_json::from_str(FIXTURE).unwrap();
    let stack = &mut timeline.tracks;
    let before = stack.children[0].items.iter().map(Item::duration).collect::<Vec<_>>();
    stack.with_composite_mut("lower-third", |inner| {
        inner.insert_item_at_time(
            0,
            5.0,
            Item::Clip(clip(4.0, Some("extra"))),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            None,
            None,
        )
    });
    let after = stack.children[0].items.iter().map(Item::duration).collect::<Vec<_>>();
    assert_eq!(before, after);
    let lower = stack.get_composite("lower-third").unwrap();
    assert_eq!(lower.content_duration(), 9.0);
    assert_eq!(lower.duration(), 5.0);
    assert_eq!(lower.effects.len(), 1, "the composite keeps its effects");
}

#[test]
fn growing_a_composite_pushes_its_sync_partners() {
    // V1: [comp 3s][b 2s], b synced with "b-a" on A1: [gap 3s][b-a 2s]
    let mut stack = stack_with_auto_composite();
    stack.children[0].items.remove(0); // drop "a": comp at 0, b at 3
    let mut a1 = Track::new(TrackKind::Audio, Some("a1".to_string()));
    a1.items = vec![Item::Gap(Gap::new(3.0, None)), synced_clip_item(2.0, "b-a", 8)];
    stack.children.push(a1);
    if let Some(Item::Clip(b)) = stack.get_nested_item_mut("b") {
        b.metadata["Resolve_OTIO"] = serde_json::json!({"Link Group ID": 8});
    }

    stack.with_composite_mut("comp", |inner| {
        inner.insert_item_at_time(
            0,
            3.0,
            Item::Clip(clip(1.0, Some("y"))),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            None,
            None,
        )
    });
    assert_eq!(duration_of(&stack, "comp"), 4.0);
    assert_eq!(start_of(&stack, "b"), 4.0);
    assert_eq!(start_of(&stack, "b-a"), 4.0, "b's audio follows it");

    // Shrinking back pulls b and its audio again.
    stack.with_composite_mut("comp", |inner| inner.delete_item("y", false, true));
    assert_eq!(duration_of(&stack, "comp"), 3.0);
    assert_eq!(start_of(&stack, "b"), 3.0);
    assert_eq!(start_of(&stack, "b-a"), 3.0);
}

#[test]
fn nested_composites_ripple_at_every_level() {
    // top V1: [outer (auto): t [inner (auto): [x 3s]] ][b 2s]
    let inner = CompositeStack::new(
        vec![track_with("t2", vec![Item::Clip(clip(3.0, Some("x")))])],
        None,
        Some("inner".into()),
    );
    let outer = CompositeStack::new(
        vec![track_with("t1", vec![Item::Composite(inner), Item::Clip(clip(1.0, Some("z")))])],
        None,
        Some("outer".into()),
    );
    let v1 = track_with("v1", vec![Item::Composite(outer), Item::Clip(clip(2.0, Some("b")))]);
    let mut stack = Stack { children: vec![v1], ..Stack::default() };
    assert_eq!(stack.composite_path("x"), Some(vec!["outer".into(), "inner".into()]));

    let inserted = stack.with_composite_mut("inner", |s| {
        s.insert_item_at_time(
            0,
            3.0,
            Item::Clip(clip(2.0, Some("y"))),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            None,
            None,
        )
    });
    assert!(inserted.flatten().is_some());
    assert_eq!(duration_of(&stack, "inner"), 5.0);
    assert_eq!(duration_of(&stack, "outer"), 6.0, "z pushed inside outer");
    assert_eq!(start_of(&stack, "b"), 6.0, "b pushed on the top level");
}

#[test]
fn set_composite_tracks_replaces_and_ripples() {
    let mut stack = stack_with_auto_composite();
    let tracks = vec![
        track_with("n1", vec![Item::Clip(clip(4.0, Some("p")))]),
        track_with("n2", vec![Item::Clip(clip(1.0, Some("q")))]),
    ];
    assert!(stack.set_composite_tracks("comp", tracks));
    let comp = stack.get_composite("comp").unwrap();
    assert_eq!(comp.children.len(), 2);
    assert_eq!(duration_of(&stack, "comp"), 4.0);
    assert_eq!(start_of(&stack, "b"), 6.0);
}

#[test]
fn item_tellers_effects_reach_nested_items() {
    let mut timeline: Timeline = serde_json::from_str(FIXTURE).unwrap();
    let before = timeline.get_item_tellers_effects("lt-accent-clip").expect("nested clip found").len();
    assert!(timeline.push_item_tellers_effect("lt-accent-clip", TellersEffect::new("Blur")));
    assert_eq!(timeline.get_item_tellers_effects("lt-accent-clip").unwrap().len(), before + 1);
    assert_eq!(timeline.item_track_kind("lt-accent-clip"), Some(TrackKind::Video));
}

#[test]
fn emptying_an_auto_length_composite_removes_it() {
    // Its length drops to zero, and a resize to zero removes the item.
    let mut stack = stack_with_auto_composite();
    stack.with_composite_mut("comp", |inner| inner.delete_item("x", false, true));
    assert!(stack.get_composite("comp").is_none());
    assert_eq!(item_ids(&stack.children[0]), ["a", "b"]);
    assert_eq!(start_of(&stack, "b"), 2.0);
}
