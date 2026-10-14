mod common;
use common::*;

/// The timeline-ui sample (`tools/timeline-ui/server.py` `load_sample`):
/// V1: A[0-3] - gap[3-4] - B[4-6] - gap[6-7] - C[7-9.5] (sync with "C audio"
/// on its audio track), Text: gap[0-1] - Title[1-3].
fn sample() -> Stack {
    let mut stack = Stack::default();
    stack.add_track(Track::new(TrackKind::Video, Some("V1".to_string())));
    for (time, duration, id) in [(0.0, 3.0, "A"), (4.0, 2.0, "B")] {
        stack
            .insert_item_at_time(
                0,
                time,
                Item::Clip(clip(duration, Some(id))),
                OverlapPolicy::Override,
                InsertPolicy::SplitAndInsert,
                None,
                None,
            )
            .unwrap();
    }
    let mut audio = audio_clip(2.5, "file:///c.wav", None);
    audio.set_id(Some("C audio".to_string()));
    stack
        .insert_item_at_time(
            0,
            7.0,
            Item::Clip(clip(2.5, Some("C"))),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            Some(vec![audio]),
            None,
        )
        .unwrap();
    let len = stack.children.len() as isize;
    assert!(stack.add_track_at(Track::new(TrackKind::Video, Some("Text".to_string())), len));
    let text = track_index_by_id(&stack, "Text");
    stack
        .insert_item_at_time(
            text,
            1.0,
            Item::Clip(clip(2.0, Some("Title"))),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            None,
            None,
        )
        .unwrap();
    stack
}

const CLIPS: [&str; 5] = ["A", "B", "C", "C audio", "Title"];

fn span(stack: &Stack, id: &str) -> Option<(String, f64, f64)> {
    let (ti, ii, item) = stack.get_item(id)?;
    Some((
        stack.children[ti].get_id().unwrap(),
        stack.children[ti].start_time_of_item(ii),
        item.duration(),
    ))
}

fn ripple_move(stack: &mut Stack, id: &str, track: &str, time: f64) -> bool {
    stack.move_item_at_time(
        id,
        track,
        time,
        false,
        InsertPolicy::InsertBeforeOrAfter,
        OverlapPolicy::Push,
    )
}

#[test]
fn ripple_move_back_into_a_gap_pushes_without_trimming() {
    // B -> 3.5 used to close B's slot (pulling C to 5), then land B over
    // [3.5-5.5] with Override, cutting C and its audio from 2.5s to 2s.
    let mut stack = sample();
    assert!(ripple_move(&mut stack, "B", "V1", 3.5));
    assert_eq!(span(&stack, "A"), Some(("V1".to_string(), 0.0, 3.0)));
    assert_eq!(span(&stack, "B"), Some(("V1".to_string(), 3.5, 2.0)));
    let (_, c_start, c_duration) = span(&stack, "C").unwrap();
    let (_, audio_start, audio_duration) = span(&stack, "C audio").unwrap();
    assert_eq!(c_duration, 2.5, "C keeps its duration");
    assert_eq!(audio_duration, 2.5, "C audio keeps its duration");
    assert_eq!(c_start, audio_start, "C audio stays under C");
    assert!(c_start >= 5.5, "C is pushed past B, not overlapped");
}

#[test]
fn ripple_moves_of_the_sample_never_trim_or_lose_another_clip() {
    let base = sample();
    let tracks: Vec<String> = base.children.iter().filter_map(|t| t.get_id()).collect();
    let mut failures = Vec::new();
    for id in CLIPS {
        for track in &tracks {
            for step in 0..=24 {
                let time = step as f64 * 0.5;
                let mut stack = base.clone();
                if !ripple_move(&mut stack, id, track, time) {
                    assert_eq!(
                        serde_json::to_string(&stack).unwrap(),
                        serde_json::to_string(&base).unwrap(),
                        "a refused move leaves the stack untouched ({id} -> {track} @ {time})"
                    );
                    continue;
                }
                for other in CLIPS {
                    let before = span(&base, other).unwrap().2;
                    match span(&stack, other) {
                        None => failures.push(format!("{id} -> {track} @ {time}: {other} lost")),
                        Some((_, _, after)) if (after - before).abs() > 1e-9 => failures.push(
                            format!("{id} -> {track} @ {time}: {other} {before} -> {after}"),
                        ),
                        _ => {}
                    }
                }
                let (c, audio) = (span(&stack, "C").unwrap(), span(&stack, "C audio").unwrap());
                if (c.1 - audio.1).abs() > 1e-9 {
                    failures.push(format!(
                        "{id} -> {track} @ {time}: C at {} but C audio at {}",
                        c.1, audio.1
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn ripple_move_closes_the_source_slot_and_pushes_at_the_drop() {
    // A -> 4: lifting A pulls B to 1 and C to 4, and A then lands before C,
    // pushing C (and C audio) back to 7. No gap is absorbed.
    let mut stack = sample();
    assert!(ripple_move(&mut stack, "A", "V1", 4.0));
    assert_eq!(span(&stack, "B"), Some(("V1".to_string(), 1.0, 2.0)));
    assert_eq!(span(&stack, "A"), Some(("V1".to_string(), 4.0, 3.0)));
    assert_eq!(span(&stack, "C").map(|s| (s.1, s.2)), Some((7.0, 2.5)));
    assert_eq!(
        span(&stack, "C audio").map(|s| (s.1, s.2)),
        Some((7.0, 2.5))
    );
}
