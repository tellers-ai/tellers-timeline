use crate::{Item, SpeedError, Stack};

use super::resolve_sync_clips_id;

impl Stack {
    /// Play the clip `item_id` at a constant `speed` (0 freezes its first
    /// frame); see [`crate::set_clip_speed`]. With `with_sync_clips`, its
    /// sync partners (the audio of a video clip, and the reverse) get the
    /// same speed, so picture and sound stay in sync. Returns how many clips
    /// were changed: 0 when no clip has this id.
    pub fn set_item_speed(
        &mut self,
        item_id: &str,
        speed: f64,
        preserve_pitch: Option<bool>,
        with_sync_clips: bool,
    ) -> Result<usize, SpeedError> {
        let Some(target) = self.clip_target(item_id) else {
            return Ok(0);
        };
        let mut targets = vec![target];
        if with_sync_clips {
            if let Item::Clip(clip) = &self.children[target.0].items[target.1] {
                if let Some(sync_id) = resolve_sync_clips_id(&clip.metadata) {
                    targets = self.synced_clips_targets(sync_id);
                }
            }
        }
        for (track_index, item_index) in &targets {
            if let Item::Clip(clip) = &mut self.children[*track_index].items[*item_index] {
                clip.set_speed(speed, preserve_pitch)?;
            }
        }
        Ok(targets.len())
    }
}
