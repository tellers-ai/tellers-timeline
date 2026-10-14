//! End-to-end scenarios replayed from editing sessions in the test UI: every
//! step is a public call with the UI's defaults (Override, SplitAndInsert,
//! leave a gap on move), and every assertion is what the user saw, or should
//! have seen, on screen.

mod common;
use common::*;

fn empty_track(kind: TrackKind, id: &str, len: f64) -> Track {
    let mut track = Track::new(kind, Some(id.to_string()));
    track.items.push(Item::Gap(Gap::make_gap(len)));
    track
}

fn named_audio(id: &str, duration: f64) -> Item {
    let mut item = audio_clip(duration, &format!("file:///{id}.wav"), None);
    item.set_id(Some(id.to_string()));
    item
}

/// (track id, start, duration, media start) of a clip.
fn span(stack: &Stack, id: &str) -> (String, f64, f64, f64) {
    let (track_index, item_index, item) = stack.get_item(id).unwrap();
    (
        stack.children[track_index].get_id().unwrap(),
        stack.children[track_index].start_time_of_item(item_index),
        item.duration(),
        source_start(item),
    )
}

fn insert(
    stack: &mut Stack,
    track_id: &str,
    time: f64,
    item: Item,
    audio: Option<Vec<Item>>,
) -> bool {
    stack
        .insert_item_at_time_by_id(
            track_id,
            time,
            item,
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            audio,
            None,
        )
        .is_some()
}

fn mv(stack: &mut Stack, id: &str, track_id: &str, time: f64) -> bool {
    stack.move_item_at_time(
        id,
        track_id,
        time,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    )
}

fn resize(stack: &mut Stack, id: &str, start: f64, duration: f64) -> bool {
    stack.resize_item(id, start, duration, OverlapPolicy::Override, false)
}

fn clip_count(stack: &Stack, track_id: &str) -> usize {
    stack.children[track_index_by_id(stack, track_id)]
        .items
        .iter()
        .filter(|item| matches!(item, Item::Clip(_)))
        .count()
}

/// The sample layout of the test UI: A1 below V1 (its partner through the C
/// column at 7), a Text video track on top. Bottom to top: A1, V1, Text.
fn sample() -> Stack {
    let mut stack = Stack::default();
    stack
        .children
        .push(empty_track(TrackKind::Audio, "A1", 10.0));
    stack
        .children
        .push(empty_track(TrackKind::Video, "V1", 10.0));
    stack
        .children
        .push(empty_track(TrackKind::Video, "Text", 10.0));
    assert!(insert(
        &mut stack,
        "V1",
        0.0,
        Item::Clip(clip(3.0, Some("A"))),
        None
    ));
    assert!(insert(
        &mut stack,
        "V1",
        4.0,
        Item::Clip(clip(2.0, Some("B"))),
        None
    ));
    assert!(insert(
        &mut stack,
        "V1",
        7.0,
        Item::Clip(clip(2.5, Some("C"))),
        Some(vec![named_audio("C audio", 2.5)])
    ));
    assert!(insert(
        &mut stack,
        "Text",
        1.0,
        Item::Clip(clip(2.0, Some("Title"))),
        None
    ));
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A1".to_string()]
    );
    stack
}

#[test]
fn plain_clips_move_and_resize_around_a_column_without_touching_it() {
    let mut stack = sample();
    // Title comes down from Text onto V1 (over A's middle), gets shortened,
    // moved next to C, then widened over C's head. At every step the C
    // column stays one column: same start, length and media offset on V1
    // and A1, and both clips keep their ids.
    let column_in_sync = |stack: &Stack, step: &str| {
        let (_, start, duration, media) = span(stack, "C");
        assert_eq!(
            span(stack, "C audio"),
            ("A1".into(), start, duration, media),
            "{step}"
        );
        assert_sync_clips_track_aligned(stack, step);
    };
    assert!(mv(&mut stack, "Title", "V1", 0.5));
    column_in_sync(&stack, "Title onto V1");
    assert_eq!(span(&stack, "C"), ("V1".into(), 7.0, 2.5, 0.0));

    // A shorter Title with Override ripples what follows on V1 (the library's
    // contract for a shrink that keeps its start), partners included.
    assert!(resize(&mut stack, "Title", 0.5, 1.0));
    column_in_sync(&stack, "Title shortened");
    assert_eq!(span(&stack, "C"), ("V1".into(), 6.0, 2.5, 0.0));

    // Title (1s) dropped at 5.5 overrides the first half second of C.
    assert!(mv(&mut stack, "Title", "V1", 5.5));
    column_in_sync(&stack, "Title next to C");
    assert_eq!(span(&stack, "C"), ("V1".into(), 6.5, 2.0, 0.5));

    // Title widened to 5.5..8 eats more of C: 8..8.5 remains, media from 2s.
    assert!(resize(&mut stack, "Title", 5.5, 2.5));
    column_in_sync(&stack, "Title widened");
    assert_eq!(span(&stack, "Title"), ("V1".into(), 5.5, 2.5, 0.0));
    assert_eq!(span(&stack, "C"), ("V1".into(), 8.0, 0.5, 2.0));
}

#[test]
fn refusals_seen_in_the_session_leave_the_timeline_untouched() {
    let mut stack = sample();
    let before = stack.clone();
    // A clip from a video track onto V1's partner A1, which holds C's audio.
    assert!(!mv(&mut stack, "Title", "A1", 6.0));
    // The C video clip onto an audio track.
    assert!(!mv(&mut stack, "C", "A1", 3.5));
    assert_eq!(stack, before);

    // A stem column onto a video track.
    stack
        .children
        .insert(0, empty_track(TrackKind::Audio, "M1", 10.0));
    assert!(insert(
        &mut stack,
        "M1",
        8.5,
        named_audio("Clip 10", 2.0),
        Some(vec![
            named_audio("Clip 10 audio 1", 2.0),
            named_audio("Clip 10 audio 2", 2.0)
        ])
    ));
    let before = stack.clone();
    assert!(!mv(&mut stack, "Clip 10", "V1", 8.0));
    assert_eq!(stack, before);
}

#[test]
fn stems_on_a_new_audio_track_make_their_own_group() {
    let mut stack = sample();
    stack
        .children
        .push(empty_track(TrackKind::Audio, "M1", 10.0));
    assert!(insert(
        &mut stack,
        "M1",
        8.5,
        named_audio("Clip 10", 2.0),
        Some(vec![
            named_audio("Clip 10 audio 1", 2.0),
            named_audio("Clip 10 audio 2", 2.0)
        ])
    ));
    let partners = stack.associated_track_ids("M1").unwrap();
    assert_eq!(partners.len(), 2);
    assert_eq!(span(&stack, "Clip 10 audio 1").0, partners[0]);
    assert_eq!(span(&stack, "Clip 10 audio 2").0, partners[1]);
    // V1's group is untouched and the new tracks belong to M1 only.
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A1".to_string()]
    );
    let groups = stack.track_groups();
    let m1 = groups
        .iter()
        .find(|g| g.owner_track_id.as_deref() == Some("M1"))
        .unwrap();
    assert_eq!(m1.kind, TrackKind::Audio);
    assert!(m1.stored);

    // The stem column moves in time as one.
    assert!(mv(&mut stack, "Clip 10", "M1", 7.5));
    for id in ["Clip 10", "Clip 10 audio 1", "Clip 10 audio 2"] {
        assert_eq!(span(&stack, id).1, 7.5, "{id}");
    }
    assert_sync_clips_track_aligned(&stack, "stem column moved in time");
}

#[test]
fn a_column_follows_its_video_clip_to_another_video_group() {
    let mut stack = sample();
    // Text gets its own partners through a column of its own.
    assert!(insert(
        &mut stack,
        "Text",
        8.5,
        Item::Clip(clip(2.0, Some("Clip 11"))),
        Some(vec![
            named_audio("Clip 11 audio 1", 2.0),
            named_audio("Clip 11 audio 2", 2.0)
        ])
    ));
    let text_partners = stack.associated_track_ids("Text").unwrap();
    assert_eq!(text_partners.len(), 2);
    assert!(!text_partners.contains(&"A1".to_string()), "A1 is V1's");

    // C moves to Text at 3: its audio lands on Text's first slot.
    assert!(mv(&mut stack, "C", "Text", 3.0));
    assert_eq!(span(&stack, "C"), ("Text".into(), 3.0, 2.5, 0.0));
    assert_eq!(
        span(&stack, "C audio"),
        (text_partners[0].clone(), 3.0, 2.5, 0.0)
    );
    assert_eq!(clip_count(&stack, "A1"), 0, "A1 keeps nothing of C");
    assert_eq!(
        stack.associated_track_ids("V1").unwrap(),
        vec!["A1".to_string()]
    );
    assert_sync_clips_track_aligned(&stack, "column moved to another group");

    // Its audio cannot be dragged to Text's second slot from the video clip.
    let before = stack.clone();
    assert!(!mv(&mut stack, "C", &text_partners[1], 3.5));
    assert_eq!(stack, before);
}

#[test]
fn a_plain_clip_on_the_stem_owner_trims_then_removes_the_stems_as_it_grows() {
    let mut stack = sample();
    stack
        .children
        .push(empty_track(TrackKind::Audio, "M1", 12.0));
    assert!(insert(
        &mut stack,
        "M1",
        7.5,
        named_audio("Clip 10", 2.0),
        Some(vec![
            named_audio("Clip 10 audio 1", 2.0),
            named_audio("Clip 10 audio 2", 2.0)
        ])
    ));
    let partners = stack.associated_track_ids("M1").unwrap();

    // A plain audio clip is welcome on an audio group's owner.
    assert!(insert(
        &mut stack,
        "M1",
        5.0,
        named_audio("Clip 12", 2.0),
        None
    ));
    assert_eq!(
        stack.associated_track_ids("M1").unwrap(),
        partners,
        "the group is unchanged"
    );

    // Grown to 5..8 it eats the head of the stems: all three keep their ids,
    // start at 8, last 1.5s and skip their first half second.
    assert!(resize(&mut stack, "Clip 12", 5.0, 3.0));
    for id in ["Clip 10", "Clip 10 audio 1", "Clip 10 audio 2"] {
        let (_, start, duration, media_start) = span(&stack, id);
        assert_eq!((start, duration, media_start), (8.0, 1.5, 0.5), "{id}");
    }
    assert_sync_clips_track_aligned(&stack, "stems trimmed by a plain clip");

    // Grown to 5..10 it covers them entirely: the stems and both partners go.
    assert!(resize(&mut stack, "Clip 12", 5.0, 5.0));
    for id in ["Clip 10", "Clip 10 audio 1", "Clip 10 audio 2"] {
        assert!(stack.get_item(id).is_none(), "{id} should be gone");
    }
    for partner in &partners {
        assert_eq!(clip_count(&stack, partner), 0, "{partner}");
    }
    assert_eq!(span(&stack, "Clip 12"), ("M1".into(), 5.0, 5.0, 0.0));
    // The empty partners stay until the host prunes them.
    assert_eq!(stack.associated_track_ids("M1").unwrap(), partners);
    assert_eq!(stack.prune_empty_partner_tracks(), partners);
    assert!(stack.associated_track_ids("M1").unwrap().is_empty());
}

#[test]
fn a_stem_partner_swaps_with_the_owner_channel_inside_its_group() {
    let mut stack = sample();
    stack
        .children
        .push(empty_track(TrackKind::Audio, "M1", 12.0));
    assert!(insert(
        &mut stack,
        "M1",
        7.5,
        named_audio("Clip 10", 2.0),
        Some(vec![
            named_audio("Clip 10 audio 1", 2.0),
            named_audio("Clip 10 audio 2", 2.0)
        ])
    ));
    let partners = stack.associated_track_ids("M1").unwrap();
    // Partner 1 onto the owner's track at the same time: the two channels swap.
    assert!(mv(&mut stack, "Clip 10 audio 1", "M1", 7.5));
    assert_eq!(span(&stack, "Clip 10 audio 1").0, "M1");
    assert_eq!(span(&stack, "Clip 10").0, partners[0]);
    assert_eq!(span(&stack, "Clip 10 audio 2").0, partners[1]);
    for id in ["Clip 10", "Clip 10 audio 1", "Clip 10 audio 2"] {
        assert_eq!(span(&stack, id).1, 7.5, "{id}");
    }
    assert_sync_clips_track_aligned(&stack, "stem swap");
    // A plain clip on a stem partner track is accepted too (audio group).
    assert!(insert(
        &mut stack,
        &partners[1],
        0.0,
        named_audio("fx", 1.0),
        None
    ));
    assert_eq!(stack.associated_track_ids("M1").unwrap(), partners);
}

#[test]
fn normalize_and_prune_after_the_session_leave_a_tidy_stack() {
    let mut stack = sample();
    stack
        .children
        .insert(0, empty_track(TrackKind::Audio, "M1", 12.0));
    assert!(insert(
        &mut stack,
        "M1",
        8.5,
        named_audio("Clip 10", 2.0),
        Some(vec![
            named_audio("Clip 10 audio 1", 2.0),
            named_audio("Clip 10 audio 2", 2.0)
        ])
    ));
    assert!(insert(
        &mut stack,
        "Text",
        8.5,
        Item::Clip(clip(2.0, Some("Clip 11"))),
        Some(vec![
            named_audio("Clip 11 audio 1", 2.0),
            named_audio("Clip 11 audio 2", 2.0)
        ])
    ));
    // Scramble by hand, then normalize: every group contiguous, owner on top,
    // groups in their owners' order; nothing lost.
    let clips_before: usize = stack
        .children
        .iter()
        .map(|t| {
            t.items
                .iter()
                .filter(|i| matches!(i, Item::Clip(_)))
                .count()
        })
        .sum();
    assert!(stack.reorder_track("M1", stack.children.len() as isize));
    stack.normalize_track_order();
    let groups = stack.track_groups();
    for group in &groups {
        let mut expected: Vec<usize> = group.partner_track_indices.clone();
        expected.reverse();
        expected.push(group.owner_track_index);
        let first = *expected.first().unwrap();
        let contiguous: Vec<usize> = (first..first + expected.len()).collect();
        assert_eq!(
            expected, contiguous,
            "group of {:?} is contiguous",
            group.owner_track_id
        );
    }
    let clips_after: usize = stack
        .children
        .iter()
        .map(|t| {
            t.items
                .iter()
                .filter(|i| matches!(i, Item::Clip(_)))
                .count()
        })
        .sum();
    assert_eq!(clips_before, clips_after);
    // Nothing is empty yet, so nothing is pruned.
    assert!(stack.prune_empty_partner_tracks().is_empty());
    assert!(!stack.normalize_track_order());
}
