use thiserror::Error;

use crate::Timeline;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("negative duration for an item")]
    NegativeDuration,
    /// An enabled effect naming a track no other video track matches; see
    /// [`crate::track_refs`].
    #[error("{effect} on {owner} names track {track:?}, which no other video track matches")]
    DanglingTrackReference { owner: String, effect: String, track: String },
}

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

    errors
}
