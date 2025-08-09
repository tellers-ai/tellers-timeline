use thiserror::Error;

use crate::Timeline;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("negative duration for an item")]
    NegativeDuration,
    #[error("overlap in track")]
    Overlap,
    #[error("items not sorted by start in a track")]
    NotSorted,
}

pub fn validate_timeline(tl: &Timeline) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for track in &tl.tracks {
        let mut last_start = f64::NEG_INFINITY;
        let mut last_end = f64::NEG_INFINITY;
        for item in &track.items {
            if item.duration() < 0.0 {
                errors.push(ValidationError::NegativeDuration);
            }
            let start = item.start();
            if start < last_start {
                errors.push(ValidationError::NotSorted);
            }
            if start < last_end {
                errors.push(ValidationError::Overlap);
            }
            last_start = start;
            last_end = start + item.duration().max(0.0);
        }
    }

    errors
}
