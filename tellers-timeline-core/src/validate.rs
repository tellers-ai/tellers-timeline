use thiserror::Error;

use crate::Timeline;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("negative duration for an item")]
    NegativeDuration,
}

pub fn validate_timeline(tl: &Timeline) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for track in &tl.tracks {
        for item in &track.items {
            if item.duration() < 0.0 {
                errors.push(ValidationError::NegativeDuration);
            }
        }
    }

    errors
}
