mod common;
use common::*;
use std::collections::HashMap;

/// The richer timeline-ui sample (`tools/timeline-ui/server.py` `load_sample`):
/// partner tracks holding columns of different sizes.
///
/// V1:      A[0+3] _ B[4+2]~1 _ C[7+2.5]~2
/// A1:      _ "B audio"[4]~1 _ "C audio 1"[7]~2
/// A2:      _ "C audio 2"[7]~2
/// V2:      _ D[2+2]~3 _ E[6+1.5]          (A3-A5 hold D's three channels)
/// Cutaway: _ F[5+2]~4                     (VO holds "F voice", linked after)
/// Music:   Music[0+6]                     (no owner)
/// Text:    _ Title[1+2]
fn sample() -> Stack {
    let mut stack = Stack::default();
    fn add(stack: &mut Stack, kind: TrackKind, id: &str, index: Option<isize>) -> usize {
        let index = index.unwrap_or(stack.children.len() as isize);
        assert!(stack.add_track_at(Track::new(kind, Some(id.to_string())), index));
        track_index_by_id(stack, id)
    }
    fn insert(stack: &mut Stack, track: &str, time: f64, item: Item, audio: &[(&str, f64)]) {
        let track = track_index_by_id(stack, track);
        let audio = (!audio.is_empty()).then(|| {
            audio
                .iter()
                .map(|&(id, duration)| {
                    let mut item = audio_clip(duration, "file:///audio.wav", None);
                    item.set_id(Some(id.to_string()));
                    item
                })
                .collect()
        });
        stack
            .insert_item_at_time(
                track,
                time,
                item,
                OverlapPolicy::Override,
                InsertPolicy::SplitAndInsert,
                audio,
                None,
            )
            .unwrap();
    }
    let video = |id: &str, duration: f64| Item::Clip(clip(duration, Some(id)));

    add(&mut stack, TrackKind::Video, "V1", Some(0));
    insert(&mut stack, "V1", 0.0, video("A", 3.0), &[]);
    insert(&mut stack, "V1", 4.0, video("B", 2.0), &[("B audio", 2.0)]);
    insert(
        &mut stack,
        "V1",
        7.0,
        video("C", 2.5),
        &[("C audio 1", 2.5), ("C audio 2", 2.5)],
    );
    add(&mut stack, TrackKind::Video, "V2", None);
    insert(
        &mut stack,
        "V2",
        2.0,
        video("D", 2.0),
        &[("D audio 1", 2.0), ("D audio 2", 2.0), ("D audio 3", 2.0)],
    );
    insert(&mut stack, "V2", 6.0, video("E", 1.5), &[]);
    add(&mut stack, TrackKind::Video, "Cutaway", None);
    insert(&mut stack, "Cutaway", 5.0, video("F", 2.0), &[]);
    add(&mut stack, TrackKind::Audio, "VO", Some(0));
    let mut voice = audio_clip(2.0, "file:///voice.wav", None);
    voice.set_id(Some("F voice".to_string()));
    insert(&mut stack, "VO", 5.0, voice, &[]);
    assert!(stack
        .sync_item(&["F".to_string(), "F voice".to_string()])
        .is_some());
    add(&mut stack, TrackKind::Audio, "Music", Some(0));
    let mut music = audio_clip(6.0, "file:///music.wav", None);
    music.set_id(Some("Music".to_string()));
    insert(&mut stack, "Music", 0.0, music, &[]);
    add(&mut stack, TrackKind::Video, "Text", None);
    insert(&mut stack, "Text", 1.0, video("Title", 2.0), &[]);
    stack.normalize_track_order();
    stack
}

/// Every clip by id: (track index, start, duration, sync id).
fn clips(stack: &Stack) -> HashMap<String, (usize, f64, f64, Option<i64>)> {
    let mut out = HashMap::new();
    for (track_index, track) in stack.children.iter().enumerate() {
        let mut start = 0.0;
        for item in &track.items {
            if let Item::Clip(clip) = item {
                out.insert(
                    item.get_id().unwrap(),
                    (track_index, start, item.duration(), clip.sync_clips_id()),
                );
            }
            start += item.duration();
        }
    }
    out
}

fn start(stack: &Stack, id: &str) -> f64 {
    clips(stack)[id].1
}

/// Every clip survives at its duration, every sync column starts at one time,
/// and no track holds two clips of one column.
fn check(base: &Stack, stack: &Stack, what: &str) -> Vec<String> {
    let before = clips(base);
    let after = clips(stack);
    let mut problems = Vec::new();
    for (id, &(_, _, duration, _)) in &before {
        match after.get(id) {
            None => problems.push(format!("{what}: lost {id}")),
            Some(&(_, _, d, _)) if (d - duration).abs() > 1e-6 => {
                problems.push(format!("{what}: {id} {duration} -> {d}"))
            }
            _ => {}
        }
    }
    let mut columns: HashMap<i64, Vec<(&String, usize, f64)>> = HashMap::new();
    for (id, &(track, start, _, sync)) in &after {
        if let Some(sync) = sync {
            columns.entry(sync).or_default().push((id, track, start));
        }
    }
    for column in columns.values() {
        if column.iter().any(|c| (c.2 - column[0].2).abs() > 1e-6) {
            problems.push(format!("{what}: misaligned {column:?}"));
        }
        let mut tracks: Vec<_> = column.iter().map(|c| c.1).collect();
        tracks.sort_unstable();
        tracks.dedup();
        if tracks.len() != column.len() {
            problems.push(format!("{what}: one track holds two of {column:?}"));
        }
    }
    problems
}

fn assert_intact(base: &Stack, stack: &Stack, what: &str) {
    let problems = check(base, stack, what);
    assert!(problems.is_empty(), "{problems:#?}");
}

fn index_move(stack: &mut Stack, id: &str, track: &str, index: usize) -> bool {
    stack.move_item_at_index(id, track, index, false, OverlapPolicy::Push)
}

#[test]
fn moving_a_into_v1_keeps_b_audio_and_c_columns() {
    // The lift of A pulled V1 by 3s but A1 and A2 by less, deleting B audio
    // and leaving C's channels at 4s while C was at 7s.
    let base = sample();
    let mut stack = base.clone();
    assert!(index_move(&mut stack, "A", "V1", 2));
    assert_intact(&base, &stack, "A -> V1 @ 2");
    assert_eq!(start(&stack, "A"), 1.0);
    assert_eq!(start(&stack, "B"), 4.0);
    assert_eq!(start(&stack, "B audio"), 4.0);
    assert_eq!(start(&stack, "C"), 7.0);
    assert_eq!(start(&stack, "C audio 1"), 7.0);
    assert_eq!(start(&stack, "C audio 2"), 7.0);
}

#[test]
fn moving_a_to_the_cutaway_keeps_b_audio_and_c_columns() {
    let base = sample();
    let mut stack = base.clone();
    assert!(index_move(&mut stack, "A", "Cutaway", 0));
    assert_intact(&base, &stack, "A -> Cutaway @ 0");
    assert_eq!(start(&stack, "A"), 0.0);
    assert_eq!(start(&stack, "B"), 1.0);
    assert_eq!(start(&stack, "B audio"), 1.0);
    assert_eq!(start(&stack, "C"), 4.0);
    assert_eq!(start(&stack, "C audio 1"), 4.0);
    assert_eq!(start(&stack, "C audio 2"), 4.0);
}

#[test]
fn moving_b_to_the_front_of_v1_keeps_c_audio_2_under_c() {
    let base = sample();
    let mut stack = base.clone();
    assert!(index_move(&mut stack, "B", "V1", 0));
    assert_intact(&base, &stack, "B -> V1 @ 0");
    assert_eq!(start(&stack, "B"), 0.0);
    assert_eq!(start(&stack, "B audio"), 0.0);
    assert_eq!(start(&stack, "A"), 2.0);
    assert_eq!(start(&stack, "C"), 7.0);
    assert_eq!(start(&stack, "C audio 2"), 7.0);
}

#[test]
fn a_video_column_moved_past_the_end_of_another_track_does_not_split_it() {
    // B's own partner track has C audio 1 under the drop; snapping on it put
    // B at 7s on V2, splitting E.
    let base = sample();
    let mut stack = base.clone();
    assert!(stack.move_item_at_time(
        "B",
        "V2",
        8.0,
        false,
        InsertPolicy::InsertBeforeOrAfter,
        OverlapPolicy::Push,
    ));
    assert_intact(&base, &stack, "B -> V2 @ 8");
    assert_eq!(start(&stack, "E"), 6.0);
    assert_eq!(start(&stack, "B"), 8.0);
    assert_eq!(start(&stack, "B audio"), 8.0);
}

fn tracks(stack: &Stack) -> Vec<String> {
    stack.children.iter().filter_map(|t| t.get_id()).collect()
}

#[test]
fn ripple_index_moves_keep_every_column_intact() {
    let base = sample();
    let mut problems = Vec::new();
    for id in clips(&base).keys() {
        for track in tracks(&base) {
            let len = base.get_track_by_id(&track).unwrap().1.items.len();
            for index in 0..=len {
                let mut stack = base.clone();
                if index_move(&mut stack, id, &track, index) {
                    problems.extend(check(&base, &stack, &format!("{id} -> {track} @ {index}")));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn ripple_time_moves_keep_every_column_intact() {
    let base = sample();
    let mut problems = Vec::new();
    for id in clips(&base).keys() {
        for track in tracks(&base) {
            for step in 0..=26 {
                let time = step as f64 * 0.5;
                let mut stack = base.clone();
                if stack.move_item_at_time(
                    id,
                    &track,
                    time,
                    false,
                    InsertPolicy::InsertBeforeOrAfter,
                    OverlapPolicy::Push,
                ) {
                    problems.extend(check(&base, &stack, &format!("{id} -> {track} @ {time}")));
                } else {
                    assert_eq!(
                        serde_json::to_string(&stack).unwrap(),
                        serde_json::to_string(&base).unwrap(),
                        "a refused move leaves the stack untouched ({id} -> {track} @ {time})"
                    );
                }
            }
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}
