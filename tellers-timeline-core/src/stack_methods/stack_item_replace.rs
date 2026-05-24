use super::{resolve_link_group_id, track_is_empty_boundary, EPS};
use crate::{IdMetadataExt, Item, Stack, TrackKind};

impl Stack {
    pub fn replace_item(
        &mut self,
        item_id: &str,
        item: Item,
        linked_audio_clips: Option<Vec<Item>>,
        linked_video_clip: Option<Item>,
    ) -> bool {
        let Some((selected_track_index, selected_item_index, selected_item)) =
            self.get_item(item_id)
        else {
            return false;
        };
        let selected_start =
            self.children[selected_track_index].start_time_of_item(selected_item_index);
        let selected_link_group = match selected_item {
            Item::Clip(clip) => resolve_link_group_id(&clip.metadata),
            Item::Gap(_) => None,
        };
        let linked_inputs =
            Self::normalize_linked_inputs(&item, linked_audio_clips, linked_video_clip);
        if linked_inputs.video.is_some()
            && self.children[selected_track_index].kind != TrackKind::Audio
        {
            return false;
        }
        let should_link = selected_link_group.is_some()
            || !linked_inputs.audio.is_empty()
            || linked_inputs.video.is_some();
        let link_group =
            selected_link_group.or_else(|| should_link.then(|| self.next_link_group_id()));
        let targets = selected_link_group
            .map(|link_group_id| self.linked_group_targets(link_group_id))
            .unwrap_or_else(|| vec![(selected_track_index, selected_item_index)]);
        if targets.is_empty() {
            return false;
        }
        if should_link && !matches!(item, Item::Clip(_)) {
            return false;
        }

        let backup = self.clone();
        let mut replacement_item = item;
        replacement_item.clamp_to_active_available_range();
        let replacement_duration = if should_link {
            let Item::Clip(clip) = &mut replacement_item else {
                return false;
            };
            let duration = clip.source_range.duration.value.max(0.0);
            if duration <= EPS {
                return false;
            }
            duration
        } else {
            replacement_item.duration().max(0.0)
        };
        if !Self::linked_inputs_match_duration(replacement_duration, &linked_inputs) {
            return false;
        }

        for (track_index, item_index) in targets {
            let Some(existing) = self
                .children
                .get(track_index)
                .and_then(|track| track.items.get(item_index))
            else {
                *self = backup;
                return false;
            };
            let existing_id = existing.get_id();

            let mut next =
                if track_index == selected_track_index && item_index == selected_item_index {
                    replacement_item.clone()
                } else {
                    existing.clone()
                };
            next.set_id(existing_id);
            next.set_duration(replacement_duration);
            Self::set_item_link_group(&mut next, link_group);

            let Some(track) = self.children.get_mut(track_index) else {
                *self = backup;
                return false;
            };
            if !track.replace_item_by_index(item_index, next) {
                *self = backup;
                return false;
            }
        }

        let Some(link_group_id) = link_group else {
            self.sanitize();
            return true;
        };

        let mut used_ids = self.collect_timeline_ids();
        let mut primary_track_index = selected_track_index;
        let mut created_track_indices = Vec::new();
        if let Some(video_item) = linked_inputs.video {
            let track_count_before_video = self.children.len();
            let Some(video_track_index) = self.find_or_create_video_track_for_audio(
                primary_track_index,
                selected_start,
                replacement_duration,
                &mut created_track_indices,
                Some(link_group_id),
                false,
            ) else {
                *self = backup;
                return false;
            };
            if self.children.len() > track_count_before_video
                && video_track_index <= primary_track_index
            {
                primary_track_index += 1;
            }
            let Some((video_item, _video_id)) = Self::prepare_linked_item(
                video_item,
                replacement_duration,
                Some(link_group_id),
                &mut used_ids,
            ) else {
                *self = backup;
                return false;
            };
            if !self.insert_gap_only(video_track_index, selected_start, video_item) {
                *self = backup;
                return false;
            }
        }

        let mut inserted_audio_tracks = Vec::new();
        let mut inserted_audio_boundary_tracks = Vec::new();
        for audio_item in linked_inputs.audio {
            let track_count_before_audio = self.children.len();
            let Some(audio_track_index) = self.find_or_create_audio_track(
                primary_track_index,
                selected_start,
                replacement_duration,
                &mut created_track_indices,
                &inserted_audio_tracks,
                &inserted_audio_boundary_tracks,
                Some(link_group_id),
                false,
            ) else {
                *self = backup;
                return false;
            };
            let reused_empty_boundary_track = self.children.len() == track_count_before_audio
                && track_is_empty_boundary(&self.children[audio_track_index]);
            if self.children.len() > track_count_before_audio
                && audio_track_index <= primary_track_index
            {
                primary_track_index += 1;
            }
            let Some((audio_item, _audio_id)) = Self::prepare_linked_item(
                audio_item,
                replacement_duration,
                Some(link_group_id),
                &mut used_ids,
            ) else {
                *self = backup;
                return false;
            };
            if !self.insert_gap_only(audio_track_index, selected_start, audio_item) {
                *self = backup;
                return false;
            }
            inserted_audio_tracks.push(audio_track_index);
            if reused_empty_boundary_track {
                inserted_audio_boundary_tracks.push(audio_track_index);
            }
        }

        self.sanitize();
        true
    }
}
