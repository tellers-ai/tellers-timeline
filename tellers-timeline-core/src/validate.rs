use thiserror::Error;

use crate::{
    is_time_warp, unplayable_speed, Clip, CompositeStack, Effect, IdMetadataExt, Item, Seconds,
    TellersEffect, Timeline, TrackKind,
};

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("negative duration for an item")]
    NegativeDuration,
    /// An enabled effect naming a track no other video track matches; see
    /// [`crate::track_refs`].
    #[error("{effect} on {owner} names track {track:?}, which no other video track matches")]
    DanglingTrackReference { owner: String, effect: String, track: String },
    /// An enabled time effect on a track, the timeline or a composite clip:
    /// the player reads speed from clips only.
    #[error("speed on {owner} is ignored: only clips play at a speed")]
    SpeedNotOnClip { owner: String },
    /// An enabled time effect on a Text, Shape or Rich Text clip, which the
    /// player shows at normal speed.
    #[error("speed on {owner} is ignored: Text, Shape and Rich Text clips play at normal speed")]
    SpeedOnGenerator { owner: String },
    /// An enabled time effect the player does not play as written; see
    /// [`crate::unplayable_speed`].
    #[error("{owner}: {reason}")]
    UnplayableSpeed { owner: String, reason: String },
    /// More than one enabled effect the player reads once per owner
    /// (`Blend`, `TrackMatte`, `Volume`, `Pan`, `MotionBlur`, speed): only
    /// the first applies.
    #[error("{owner} has {count} enabled {effect} effects; only the first applies")]
    DuplicateEffect { owner: String, effect: String, count: usize },
    /// Content of a composite clip the player skips: a nested composite, a
    /// track that is not a video track, a transition, a Rich Text or other
    /// generator clip, a clip's speed or `ColorLut`.
    #[error("composite {owner}: {content} is not drawn inside a composite")]
    CompositeContentSkipped { owner: String, content: String },
    /// Video or image clips of a composite clip overlapping in time: the
    /// player shows one at a time and skips the later ones.
    #[error("composite {owner}: {count} video or image clip(s) overlap another one and are skipped")]
    CompositeOverlappingMedia { owner: String, count: usize },
}

/// Effects the player reads once per owner: the first enabled one applies.
const ONCE_PER_OWNER: &[&str] = &["Blend", "TrackMatte", "Volume", "Pan", "MotionBlur"];

pub fn validate_timeline(tl: &Timeline) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for track in &tl.tracks.children {
        for item in &track.items {
            if item.duration() < 0.0 {
                errors.push(ValidationError::NegativeDuration);
            }
        }
    }

    for reference in tl.tracks.dangling_track_references() {
        let owner = match (&reference.owner_id, reference.item_index) {
            (Some(id), _) => id.clone(),
            (None, Some(item)) => format!("item {item} of track {}", reference.track_index),
            (None, None) => format!("track {}", reference.track_index),
        };
        errors.push(ValidationError::DanglingTrackReference {
            owner,
            effect: reference.effect_name,
            track: reference.track,
        });
    }

    check_player_support(tl, &mut errors);

    errors
}

/// Report what the player (`video-player-js`) cannot draw as written: speed
/// anywhere but a media clip, speed out of range, effects it reads once
/// repeated, and composite clip content it skips.
fn check_player_support(tl: &Timeline, errors: &mut Vec<ValidationError>) {
    check_not_on_clip(&tl.tracks.effects, "the timeline", errors);
    check_duplicates(&tl.tracks.effects, "the timeline", errors);
    for (track_index, track) in tl.tracks.children.iter().enumerate() {
        let track_owner = track.get_id().unwrap_or_else(|| format!("track {track_index}"));
        check_not_on_clip(&track.effects, &track_owner, errors);
        check_duplicates(&track.effects, &track_owner, errors);
        for (item_index, item) in track.items.iter().enumerate() {
            let owner = item.get_id().unwrap_or_else(|| format!("item {item_index} of track {track_index}"));
            match item {
                Item::Clip(clip) => check_clip(clip, &owner, errors),
                Item::Composite(stack) => check_composite(stack, &owner, errors),
                Item::Gap(_) | Item::Transition(_) => {}
            }
        }
    }
}

fn enabled(effects: &[Effect]) -> impl Iterator<Item = &Effect> {
    effects.iter().filter(|effect| effect.enabled != Some(false))
}

fn has_speed(effects: &[Effect]) -> bool {
    enabled(effects).any(is_time_warp)
}

fn check_not_on_clip(effects: &[Effect], owner: &str, errors: &mut Vec<ValidationError>) {
    if has_speed(effects) {
        errors.push(ValidationError::SpeedNotOnClip { owner: owner.to_string() });
    }
}

fn check_clip(clip: &Clip, owner: &str, errors: &mut Vec<ValidationError>) {
    check_duplicates(&clip.effects, owner, errors);
    if clip.is_generator_clip() {
        if has_speed(&clip.effects) {
            errors.push(ValidationError::SpeedOnGenerator { owner: owner.to_string() });
        }
        return;
    }
    // The player plays the first enabled time effect; later ones are duplicates.
    if let Some(reason) = enabled(&clip.effects).find(|e| is_time_warp(e)).and_then(unplayable_speed) {
        errors.push(ValidationError::UnplayableSpeed { owner: owner.to_string(), reason });
    }
}

fn check_duplicates(effects: &[Effect], owner: &str, errors: &mut Vec<ValidationError>) {
    let speeds = enabled(effects).filter(|e| is_time_warp(e)).count();
    if speeds > 1 {
        errors.push(ValidationError::DuplicateEffect {
            owner: owner.to_string(),
            effect: "speed".to_string(),
            count: speeds,
        });
    }
    let tellers: Vec<TellersEffect> = effects
        .iter()
        .filter_map(TellersEffect::from_effect)
        .filter(|effect| effect.enabled)
        .collect();
    for name in ONCE_PER_OWNER {
        let count = tellers.iter().filter(|effect| effect.effect_name == *name).count();
        if count > 1 {
            errors.push(ValidationError::DuplicateEffect {
                owner: owner.to_string(),
                effect: name.to_string(),
                count,
            });
        }
    }
}

/// A composite clip's own effects, and its content as the player draws it
/// (`convertOtioCompositeStack`): one level of video tracks holding Text,
/// Shape, video and image clips, one video or image at a time.
fn check_composite(stack: &CompositeStack, owner: &str, errors: &mut Vec<ValidationError>) {
    check_not_on_clip(&stack.effects, owner, errors);
    check_duplicates(&stack.effects, owner, errors);
    let mut skip = |content: String| {
        errors.push(ValidationError::CompositeContentSkipped { owner: owner.to_string(), content });
    };
    let mut media: Vec<(usize, Seconds, Seconds)> = Vec::new();
    for (track_index, track) in stack.children.iter().enumerate().filter(|(_, t)| t.enabled) {
        if track.kind != TrackKind::Video {
            skip(format!("{:?} track {track_index}", track.kind));
            continue;
        }
        let mut time = 0.0;
        for (item_index, item) in track.items.iter().enumerate() {
            let child = || item.get_id().unwrap_or_else(|| format!("item {item_index} of track {track_index}"));
            let start = time;
            if !item.is_transition() {
                time += item.duration();
            }
            match item {
                Item::Transition(_) => skip(format!("transition {}", child())),
                Item::Composite(_) => skip(format!("nested composite {}", child())),
                Item::Gap(_) => {}
                Item::Clip(clip) if !clip.enabled => {}
                Item::Clip(clip) => {
                    if clip.is_generator_clip() && !clip.is_text_clip() && !clip.is_shape_clip() {
                        skip(format!("generator clip {}", child()));
                        continue;
                    }
                    if has_speed(&clip.effects) {
                        skip(format!("the speed of {}", child()));
                    }
                    if enabled_tellers(&clip.effects).any(|e| e.effect_name == "ColorLut") {
                        skip(format!("the ColorLut of {}", child()));
                    }
                    if !clip.is_generator_clip() {
                        media.push((track_index, start, time));
                    }
                }
            }
        }
    }
    let offset = stack.source_range.as_ref().map_or(0.0, |r| r.start_time.to_seconds());
    let overlapping = overlapping_media(&media, offset, offset + stack.duration());
    if overlapping > 0 {
        errors.push(ValidationError::CompositeOverlappingMedia { owner: owner.to_string(), count: overlapping });
    }
}

fn enabled_tellers(effects: &[Effect]) -> impl Iterator<Item = TellersEffect> + '_ {
    effects.iter().filter_map(TellersEffect::from_effect).filter(|e| e.enabled)
}

/// How many of the media clips (`(track, start, end)`, in the composite's
/// own time) showing in its visible span [`from`, `to`) the player drops
/// for overlapping one that starts earlier (`compositeSegments`; bottom
/// track first on a tie).
fn overlapping_media(media: &[(usize, Seconds, Seconds)], from: Seconds, to: Seconds) -> usize {
    const EPS: Seconds = 1e-9;
    let mut visible: Vec<_> = media.iter().filter(|(_, start, end)| *start < to && *end > from).collect();
    visible.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    let mut time = from;
    let mut dropped = 0;
    for (_, start, end) in visible {
        if start.max(from) < time - EPS {
            dropped += 1;
        } else {
            time = end.min(to);
        }
    }
    dropped
}

