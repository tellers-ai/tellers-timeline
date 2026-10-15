//! A push or a ripple that shifts a sync column shifts every clip of the
//! column, on every track holding one, whether or not that track is
//! associated with the column's owner, and never cuts a clip.
mod common;
use std::collections::{BTreeMap, HashMap};

use common::*;

fn add_clip(stack: &mut Stack, track: usize, time: f64, duration: f64, id: &str) {
    stack
        .insert_item_at_time(
            track,
            time,
            Item::Clip(clip(duration, Some(id))),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            None,
            None,
        )
        .unwrap();
}

fn audio(duration: f64, id: &str) -> Item {
    let mut item = audio_clip(duration, &format!("file:///{id}.wav"), None);
    item.set_id(Some(id.to_string()));
    item
}

/// The timeline-ui sample with a 2s "Clip 1" inserted on V1 at 9.5 with two
/// linked audio clips (Push, InsertBeforeOrAfter):
///
/// ```text
/// A2:   _                                    Clip1 audio2[9.5+2]~2
/// A1:   _                     C audio[7]~1  Clip1 audio1[9.5+2]~2
/// V1:   A[0+3] _ B[4+2] _     C[7+2.5]~1    Clip 1[9.5+2]~2
/// Text: _ Title[1+2]
/// ```
fn sample_with_clip_1() -> Stack {
    let mut stack = Stack::default();
    stack.add_track(Track::new(TrackKind::Video, Some("V1".to_string())));
    add_clip(&mut stack, 0, 0.0, 3.0, "A");
    add_clip(&mut stack, 0, 4.0, 2.0, "B");
    stack
        .insert_item_at_time(
            0,
            7.0,
            Item::Clip(clip(2.5, Some("C"))),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            Some(vec![audio(2.5, "C audio")]),
            None,
        )
        .unwrap();
    let len = stack.children.len() as isize;
    assert!(stack.add_track_at(Track::new(TrackKind::Video, Some("Text".to_string())), len));
    let text = track_index_by_id(&stack, "Text");
    add_clip(&mut stack, text, 1.0, 2.0, "Title");
    let v1 = track_index_by_id(&stack, "V1");
    stack
        .insert_item_at_time(
            v1,
            9.5,
            Item::Clip(clip(2.0, Some("Clip 1"))),
            OverlapPolicy::Push,
            InsertPolicy::InsertBeforeOrAfter,
            Some(vec![audio(2.0, "Clip1 audio1"), audio(2.0, "Clip1 audio2")]),
            None,
        )
        .unwrap();
    stack
}

/// Same columns built by hand, with the track holding "Clip1 audio2" owned
/// by another video track (V2 has two columns there, V1 only one), so it is
/// not associated with V1:
///
/// ```text
/// A2:   _ Clip1 audio2[9.5+2]~2 _ V2a audio[14+1]~10 _ V2b audio[16+1]~11
/// A1:   _ C audio[7+2.5]~1 Clip1 audio1[9.5+2]~2
/// V1:   A[0+3] _ B[4+2] _ C[7+2.5]~1 Clip 1[9.5+2]~2
/// V2:   _ V2a[14+1]~10 _ V2b[16+1]~11
/// Text: _ Title[1+2]
/// ```
fn unassociated_layout() -> Stack {
    fn track(kind: TrackKind, id: &str, items: Vec<Item>) -> Track {
        let mut track = Track::new(kind, Some(id.to_string()));
        track.items = items;
        track
    }
    let gap = |duration: f64| Item::Gap(Gap::make_gap(duration));
    let mut stack = Stack::default();
    stack.children.extend([
        track(
            TrackKind::Audio,
            "A2",
            vec![
                gap(9.5),
                synced_clip_item(2.0, "Clip1 audio2", 2),
                gap(2.5),
                synced_clip_item(1.0, "V2a audio", 10),
                gap(1.0),
                synced_clip_item(1.0, "V2b audio", 11),
            ],
        ),
        track(
            TrackKind::Audio,
            "A1",
            vec![
                gap(7.0),
                synced_clip_item(2.5, "C audio", 1),
                synced_clip_item(2.0, "Clip1 audio1", 2),
            ],
        ),
        track(
            TrackKind::Video,
            "V1",
            vec![
                Item::Clip(clip(3.0, Some("A"))),
                gap(1.0),
                Item::Clip(clip(2.0, Some("B"))),
                gap(1.0),
                synced_clip_item(2.5, "C", 1),
                synced_clip_item(2.0, "Clip 1", 2),
            ],
        ),
        track(
            TrackKind::Video,
            "V2",
            vec![
                gap(14.0),
                synced_clip_item(1.0, "V2a", 10),
                gap(1.0),
                synced_clip_item(1.0, "V2b", 11),
            ],
        ),
        track(
            TrackKind::Video,
            "Text",
            vec![gap(1.0), Item::Clip(clip(2.0, Some("Title")))],
        ),
    ]);
    stack
}

/// Every clip by id: (track id, start, duration, link group).
fn clips(stack: &Stack) -> BTreeMap<String, (String, f64, f64, Option<i64>)> {
    let mut out = BTreeMap::new();
    for track in &stack.children {
        let mut pos = 0.0;
        for item in &track.items {
            if let (Item::Clip(_), Some(id)) = (item, item.get_id()) {
                out.insert(
                    id,
                    (
                        track.get_id().unwrap(),
                        pos,
                        item.duration(),
                        sync_clips_id(item),
                    ),
                );
            }
            pos += item.duration();
        }
    }
    out
}

fn start(stack: &Stack, id: &str) -> f64 {
    clips(stack)[id].1
}

fn track_of(stack: &Stack, id: &str) -> String {
    clips(stack)[id].0.clone()
}

/// Problems with `after`, a stack derived from `before` by one move: a link
/// group out of alignment, a track holding two clips of one group, a clip
/// lost, added or changed in duration.
fn violations(before: &Stack, after: &Stack) -> Vec<String> {
    let before = clips(before);
    let after_clips = clips(after);
    let mut problems = Vec::new();
    for (id, (_, _, duration, _)) in &before {
        match after_clips.get(id) {
            None => problems.push(format!("{id} lost")),
            Some((_, _, d, _)) if (d - duration).abs() > 1e-6 => {
                problems.push(format!("{id} duration {duration} -> {d}"))
            }
            _ => {}
        }
    }
    for id in after_clips.keys() {
        if !before.contains_key(id) {
            problems.push(format!("{id} appeared"));
        }
    }
    let mut groups: HashMap<i64, Vec<(&String, &String, f64)>> = HashMap::new();
    for (id, (track, start, _, link)) in &after_clips {
        if let Some(link) = link {
            groups.entry(*link).or_default().push((id, track, *start));
        }
    }
    for (link, members) in groups {
        let first = members[0].2;
        if members.iter().any(|m| (m.2 - first).abs() > 1e-6) {
            problems.push(format!("group {link} misaligned: {members:?}"));
        }
        let mut tracks: Vec<_> = members.iter().map(|m| m.1).collect();
        tracks.sort();
        let count = tracks.len();
        tracks.dedup();
        if tracks.len() != count {
            problems.push(format!(
                "group {link} has two clips on one track: {members:?}"
            ));
        }
    }
    problems
}

#[test]
fn inserted_column_is_associated_with_its_video_track() {
    let stack = sample_with_clip_1();
    let v1 = track_index_by_id(&stack, "V1");
    let audio2 = track_index_by_id(&stack, &track_of(&stack, "Clip1 audio2"));
    assert!(stack.association_map()[v1].contains(&audio2));
}

#[test]
fn unassociated_layout_has_column_audio_on_another_owners_track() {
    let stack = unassociated_layout();
    let map = stack.association_map();
    let a2 = track_index_by_id(&stack, "A2");
    assert!(!map[track_index_by_id(&stack, "V1")].contains(&a2));
    assert!(map[track_index_by_id(&stack, "V2")].contains(&a2));
}

#[test]
fn index_move_onto_v1_pushes_every_audio_of_clip_1() {
    for mut stack in [sample_with_clip_1(), unassociated_layout()] {
        let before = stack.clone();
        assert!(stack.move_item_at_index("Title", "V1", 0, false, OverlapPolicy::Push));
        assert_eq!(start(&stack, "Clip 1"), 11.5);
        assert_eq!(start(&stack, "Clip1 audio1"), 11.5);
        assert_eq!(start(&stack, "Clip1 audio2"), 11.5);
        assert_eq!(violations(&before, &stack), Vec::<String>::new());
    }
}

#[test]
fn push_insert_before_clip_1_pushes_the_unassociated_track() {
    let mut stack = unassociated_layout();
    let before = stack.clone();
    let v1 = track_index_by_id(&stack, "V1");
    stack
        .insert_item_at_time(
            v1,
            0.0,
            Item::Clip(clip(1.0, Some("New"))),
            OverlapPolicy::Push,
            InsertPolicy::InsertBeforeOrAfter,
            None,
            None,
        )
        .unwrap();
    for id in ["C", "C audio", "Clip 1", "Clip1 audio1", "Clip1 audio2"] {
        assert_eq!(start(&stack, id) - clips(&before)[id].1, 1.0, "{id}");
    }
    // A2 carries V2's columns along, and V2 follows them.
    for id in ["V2a", "V2a audio", "V2b", "V2b audio"] {
        assert_eq!(start(&stack, id) - clips(&before)[id].1, 1.0, "{id}");
    }
    let problems: Vec<_> = violations(&before, &stack)
        .into_iter()
        .filter(|p| p != "New appeared")
        .collect();
    assert_eq!(problems, Vec::<String>::new());
}

#[test]
fn ripple_delete_pulls_the_unassociated_track() {
    let mut stack = unassociated_layout();
    let before = clips(&stack);
    assert!(stack.move_item_at_index("A", "V1", 1, false, OverlapPolicy::Push));
    // A moved after the gap that followed it: everything from B on is where
    // it was, and every audio of the columns too.
    for id in [
        "C",
        "C audio",
        "Clip 1",
        "Clip1 audio1",
        "Clip1 audio2",
        "V2a audio",
    ] {
        assert_eq!(start(&stack, id), before[id].1, "{id}");
    }
}

#[test]
fn moving_an_audio_partner_to_its_sibling_track_keeps_the_video_on_v1() {
    let mut stack = sample_with_clip_1();
    let audio2_track = track_of(&stack, "Clip1 audio2");
    assert!(stack.move_item_at_time(
        "Clip1 audio1",
        &audio2_track,
        9.5,
        true,
        InsertPolicy::InsertBeforeOrAfter,
        OverlapPolicy::Push,
    ));
    assert_eq!(track_of(&stack, "Clip 1"), "V1");
    assert_eq!(track_of(&stack, "Clip1 audio1"), audio2_track);
}

/// Every clip moved to every track of its kind, by time (0 to 14s in half
/// seconds) and by index, leaving a gap or not, with Push: every link group
/// stays aligned, no track holds two clips of one group, no clip is lost or
/// changes duration, and a moved audio partner leaves its video on its track.
#[test]
fn every_push_move_keeps_columns_aligned_and_clips_whole() {
    for (name, layout) in [
        ("associated", sample_with_clip_1()),
        ("unassociated", unassociated_layout()),
    ] {
        let original = clips(&layout);
        let mut failures = Vec::new();
        let mut moves = 0;
        for (id, (source_track, _, _, link)) in &original {
            let kind = layout.children[track_index_by_id(&layout, source_track)]
                .kind
                .clone();
            // The video of the moved clip's column, when the moved clip is
            // one of its audio partners.
            let column_video = link.and_then(|link| {
                original.iter().find_map(|(other, (track, _, _, l))| {
                    let index = track_index_by_id(&layout, track);
                    (other != id
                        && *l == Some(link)
                        && kind == TrackKind::Audio
                        && layout.children[index].kind == TrackKind::Video)
                        .then(|| (other.clone(), track.clone()))
                })
            });
            for dest in layout.children.iter().filter(|t| t.kind == kind) {
                let dest_id = dest.get_id().unwrap();
                for replace_with_gap in [true, false] {
                    let mut attempts: Vec<(String, Stack)> = Vec::new();
                    for half_seconds in 0..=28 {
                        let time = half_seconds as f64 / 2.0;
                        let mut stack = layout.clone();
                        stack.move_item_at_time(
                            id,
                            &dest_id,
                            time,
                            replace_with_gap,
                            InsertPolicy::InsertBeforeOrAfter,
                            OverlapPolicy::Push,
                        );
                        attempts.push((format!("time {time}"), stack));
                    }
                    for index in 0..=dest.items.len() {
                        let mut stack = layout.clone();
                        stack.move_item_at_index(
                            id,
                            &dest_id,
                            index,
                            replace_with_gap,
                            OverlapPolicy::Push,
                        );
                        attempts.push((format!("index {index}"), stack));
                    }
                    for (at, stack) in attempts {
                        moves += 1;
                        let mut problems = violations(&layout, &stack);
                        if let Some((video, video_track)) = &column_video {
                            let now = track_of(&stack, video);
                            if &now != video_track {
                                problems.push(format!("{video} left {video_track} for {now}"));
                            }
                        }
                        if !problems.is_empty() {
                            failures.push(format!(
                                "{name}: {id} -> {dest_id} {at} gap={replace_with_gap}: {problems:?}"
                            ));
                        }
                    }
                }
            }
        }
        assert!(moves > 1000, "{name}: only {moves} moves");
        assert!(
            failures.is_empty(),
            "{} of {moves} moves failed:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}
