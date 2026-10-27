//! Slip: choose which part of its media a clip plays, without moving or
//! resizing it on the timeline. Only `source_range.start_time` changes.
//!
//! A clip plays [`Clip::source_duration`] seconds of source from its start —
//! its timeline duration scaled by its speed — so a 2x clip needs twice as
//! much media to slip within as a 1x one, and a freeze needs none.

use crate::{Clip, Item, Seconds, Stack};

use super::{resolve_sync_clips_id, EPS};

impl Clip {
    /// The earliest and latest `source_range.start_time` this clip can take
    /// while the source it plays stays inside its active media's
    /// `available_range`. `None` when the media length is unknown. A clip
    /// that plays more source than the media has can only start at the
    /// media start.
    pub fn source_start_bounds(&self) -> Option<(Seconds, Seconds)> {
        let key = self.active_media_reference_key.as_deref()?;
        let available = self.media_references.get(key)?.available_range().as_ref()?;
        let media_start = available.start_time.to_seconds().max(0.0);
        let media_end = media_start + available.duration.to_seconds().max(0.0);
        let latest = (media_end - self.source_duration()).max(media_start);
        Some((media_start, latest))
    }
}

impl Stack {
    /// The source starts the clip `item_id` can slip to: its own
    /// [`Clip::source_start_bounds`], narrowed by its sync partners' with
    /// `with_sync_clips` (they slip by the same amount). The upper bound is
    /// infinite when no clip knows its media length. `None` when no clip has
    /// this id.
    pub fn item_slip_bounds(
        &self,
        item_id: &str,
        with_sync_clips: bool,
    ) -> Option<(Seconds, Seconds)> {
        let (low, high) = self.slip_delta_bounds(&self.slip_targets(item_id, with_sync_clips))?;
        let start = self.clip_source_start(item_id)?;
        // Partners that disagree on their media leave no room to slip.
        if high < low {
            return Some((start, start));
        }
        Some((start + low, start + high))
    }

    /// Move the clip `item_id`'s source window to start at `source_start`
    /// (clamped to [`Stack::item_slip_bounds`]), keeping its timeline start
    /// and duration. With `with_sync_clips`, its sync partners slip by the
    /// same amount, so picture and sound stay in sync. Returns how many
    /// clips changed: 0 when no clip has this id or it is already there.
    pub fn slip_item(
        &mut self,
        item_id: &str,
        source_start: Seconds,
        with_sync_clips: bool,
    ) -> usize {
        let targets = self.slip_targets(item_id, with_sync_clips);
        let (Some((low, high)), Some(start)) = (
            self.slip_delta_bounds(&targets),
            self.clip_source_start(item_id),
        ) else {
            return 0;
        };
        if high < low {
            return 0;
        }
        let delta = (source_start - start).clamp(low, high);
        if delta.abs() <= EPS {
            return 0;
        }
        for &(track_index, item_index) in &targets {
            if let Item::Clip(clip) = &mut self.children[track_index].items[item_index] {
                let shifted = clip.source_range.start_time.to_seconds() + delta;
                clip.source_range.start_time.set_from_seconds(shifted);
            }
        }
        targets.len()
    }

    fn slip_targets(&self, item_id: &str, with_sync_clips: bool) -> Vec<(usize, usize)> {
        let Some(target) = self.clip_target(item_id) else {
            return Vec::new();
        };
        if with_sync_clips {
            if let Item::Clip(clip) = &self.children[target.0].items[target.1] {
                if let Some(sync_id) = resolve_sync_clips_id(&clip.metadata) {
                    return self.synced_clips_targets(sync_id);
                }
            }
        }
        vec![target]
    }

    fn clip_source_start(&self, item_id: &str) -> Option<Seconds> {
        match self.get_item(item_id)?.2 {
            Item::Clip(clip) => Some(clip.source_range.start_time.to_seconds()),
            _ => None,
        }
    }

    /// How far every target can slip: the intersection of each clip's
    /// bounds relative to its own start, never before source 0.
    fn slip_delta_bounds(&self, targets: &[(usize, usize)]) -> Option<(Seconds, Seconds)> {
        if targets.is_empty() {
            return None;
        }
        let (mut low, mut high) = (f64::NEG_INFINITY, f64::INFINITY);
        for &(track_index, item_index) in targets {
            let Item::Clip(clip) = &self.children[track_index].items[item_index] else {
                continue;
            };
            let start = clip.source_range.start_time.to_seconds();
            let (earliest, latest) = clip.source_start_bounds().unwrap_or((0.0, f64::INFINITY));
            low = low.max(earliest.max(0.0) - start);
            high = high.min(latest - start);
        }
        Some((low, high))
    }
}
