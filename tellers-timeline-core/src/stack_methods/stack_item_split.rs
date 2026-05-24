use crate::{Item, Seconds, Stack};

const EPS: Seconds = super::EPS;

impl Stack {
    pub fn split_item_at_time(&mut self, item_id: &str, split_time: Seconds) -> bool {
        let Some((selected_track_index, selected_item_index, selected_item)) =
            self.get_item(item_id)
        else {
            return false;
        };
        let Item::Clip(selected_clip) = selected_item else {
            return false;
        };
        let selected_start =
            self.children[selected_track_index].start_time_of_item(selected_item_index);
        let selected_end = selected_start + selected_clip.source_range.duration.value.max(0.0);
        if split_time < selected_start - EPS || split_time > selected_end + EPS {
            return false;
        }
        if split_time <= selected_start + EPS || split_time >= selected_end - EPS {
            return true;
        }

        let targets = super::resolve_link_group_id(&selected_clip.metadata)
            .map(|link_group_id| self.linked_group_targets(link_group_id))
            .filter(|targets| targets.len() > 1)
            .unwrap_or_else(|| vec![(selected_track_index, selected_item_index)]);
        let backup = self.clone();
        for (track_index, item_index) in &targets {
            let Some(item) = self
                .children
                .get(*track_index)
                .and_then(|track| track.items.get(*item_index))
            else {
                *self = backup;
                return false;
            };
            let Item::Clip(clip) = item else {
                *self = backup;
                return false;
            };
            let item_start = self.children[*track_index].start_time_of_item(*item_index);
            let item_end = item_start + clip.source_range.duration.value.max(0.0);
            if split_time <= item_start + EPS || split_time >= item_end - EPS {
                *self = backup;
                return false;
            }
        }

        let mut target_tracks: Vec<_> = targets
            .into_iter()
            .map(|(track_index, _)| track_index)
            .collect();
        target_tracks.sort_unstable();
        target_tracks.dedup();
        for track_index in target_tracks {
            self.children[track_index].split_at_time(split_time);
        }
        true
    }
}
