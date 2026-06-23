use crate::{IdMetadataExt, InsertPolicy, Item, OverlapPolicy, Stack};

impl Stack {
    pub fn replace_item(
        &mut self,
        item_id: &str,
        item: Item,
        synced_audio_clips: Option<Vec<Item>>,
    ) -> bool {
        let Some((track_index, item_index, existing)) = self.get_item(item_id) else {
            return false;
        };
        let start_time = self.children[track_index].start_time_of_item(item_index);

        if let Some(items) = self.synced_move_items(item_id) {
            return self.replace_synced_item_via_insert(
                items,
                track_index,
                start_time,
                item,
                synced_audio_clips,
            );
        }

        let synced_inputs = Self::normalize_synced_inputs(synced_audio_clips.clone(), None);
        let should_link = !synced_inputs.audio.is_empty();
        if should_link && !matches!(item, Item::Clip(_)) {
            return false;
        }

        let mut replacement = item;
        replacement.clamp_to_active_available_range();
        if let Some(id) = existing.get_id() {
            replacement.set_id(Some(id));
        }
        let replacement_duration = replacement.duration().max(0.0);
        if !Self::synced_inputs_match_duration(replacement_duration, &synced_inputs) {
            return false;
        }

        let backup = self.clone();
        if self.delete_one_item(item_id, false).is_none() {
            return false;
        }

        if self
            .insert_item_at_time(
                track_index,
                start_time,
                replacement,
                OverlapPolicy::Push,
                InsertPolicy::SplitAndInsert,
                synced_audio_clips,
                None,
            )
            .is_some()
        {
            self.sanitize_preserving_all_gap_tracks();
            true
        } else {
            *self = backup;
            false
        }
    }
}
