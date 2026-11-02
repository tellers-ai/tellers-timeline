use crate::{Item, Stack};

use super::resolve_sync_clips_id;

impl Stack {
    /// Turn the clip or compound clip `item_id` on or off (`enabled: false`
    /// in the OTIO: the player skips it and keeps its slot). With
    /// `with_sync_clips`, its sync partners (the audio of a video clip, and
    /// the reverse) follow, so muting a video's picture can also mute its
    /// sound. Nothing moves. Returns how many items were set: 0 when there is
    /// no clip or compound clip with this id.
    pub fn set_item_enabled(
        &mut self,
        item_id: &str,
        enabled: bool,
        with_sync_clips: bool,
    ) -> usize {
        let Some((track_index, item_index, item)) = self.get_item(item_id) else {
            return 0;
        };
        let mut targets = match item {
            Item::Clip(_) | Item::Composite(_) => vec![(track_index, item_index)],
            Item::Gap(_) | Item::Transition(_) => return 0,
        };
        if with_sync_clips {
            if let Item::Clip(clip) = item {
                if let Some(sync_id) = resolve_sync_clips_id(&clip.metadata) {
                    targets = self.synced_clips_targets(sync_id);
                }
            }
        }
        for &(track_index, item_index) in &targets {
            self.children[track_index].items[item_index].set_enabled(enabled);
        }
        targets.len()
    }
}
