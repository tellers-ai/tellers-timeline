use super::{resolve_sync_clips_id, EPS};
use crate::{Gap, IdMetadataExt, InsertPolicy, Item, OverlapPolicy, Stack, TrackKind};

fn shift_track_index_after_insert(track_index: &mut usize, inserted_track_index: usize) {
    if inserted_track_index <= *track_index {
        *track_index += 1;
    }
}

fn shift_replacement_tracks_after_insert(
    replacements: &mut [(usize, usize, Item)],
    inserted_track_index: usize,
) {
    for (track_index, _, _) in replacements {
        shift_track_index_after_insert(track_index, inserted_track_index);
    }
}

fn shift_insert_tracks_after_insert(insertions: &mut [(usize, Item)], inserted_track_index: usize) {
    for (track_index, _) in insertions {
        shift_track_index_after_insert(track_index, inserted_track_index);
    }
}

impl Stack {
    pub fn replace_item(
        &mut self,
        item_id: &str,
        item: Item,
        synced_audio_clips: Option<Vec<Item>>,
    ) -> bool {
        let Some((selected_track_index, selected_item_index, selected_item)) =
            self.get_item(item_id)
        else {
            return false;
        };
        let selected_start =
            self.children[selected_track_index].start_time_of_item(selected_item_index);
        let selected_duration = selected_item.duration().max(0.0);
        let selected_sync_clips = match selected_item {
            Item::Clip(clip) => resolve_sync_clips_id(&clip.metadata),
            Item::Gap(_) => None,
        };
        let synced_audio_input_provided = synced_audio_clips.is_some();
        let synced_inputs = Self::normalize_synced_inputs(synced_audio_clips);
        let should_link = selected_sync_clips.is_some() || !synced_inputs.audio.is_empty();
        let sync_clips =
            selected_sync_clips.or_else(|| should_link.then(|| self.next_sync_clips_id()));
        let targets = selected_sync_clips
            .map(|sync_clips_id| self.synced_clips_targets(sync_clips_id))
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
            let duration = clip.source_range.duration.to_seconds().max(0.0);
            if duration <= EPS {
                return false;
            }
            duration
        } else {
            replacement_item.duration().max(0.0)
        };
        if !Self::synced_inputs_match_duration(replacement_duration, &synced_inputs) {
            return false;
        }

        let mut synced_audio_inputs = synced_inputs.audio.into_iter();
        let mut replacements = Vec::new();
        for (track_index, item_index) in targets {
            let Some(track) = self.children.get(track_index) else {
                *self = backup;
                return false;
            };
            let track_kind = track.kind.clone();
            let Some(existing) = track.items.get(item_index) else {
                *self = backup;
                return false;
            };
            let existing_id = existing.get_id();

            let mut next = if track_index == selected_track_index
                && item_index == selected_item_index
            {
                replacement_item.clone()
            } else if synced_audio_input_provided && track_kind == TrackKind::Audio {
                synced_audio_inputs
                    .next()
                    .unwrap_or_else(|| Item::Gap(Gap::make_gap(replacement_duration)))
            } else {
                existing.clone()
            };
            next.set_id(existing_id);
            next.set_duration(replacement_duration);
            Self::set_item_sync_clips(&mut next, sync_clips);
            replacements.push((track_index, item_index, next));
        }

        let Some(sync_clips_id) = sync_clips else {
            let mut adjacent_sync_clips =
                self.synced_clips_adjacent_to_time(selected_track_index, selected_start);
            for sync_clips in self.synced_clips_adjacent_to_time(
                selected_track_index,
                selected_start + selected_duration,
            ) {
                if !adjacent_sync_clips.contains(&sync_clips) {
                    adjacent_sync_clips.push(sync_clips);
                }
            }
            let boundary_track_indices = self.boundary_track_indices_for_anchors(
                &adjacent_sync_clips,
                &[selected_track_index],
                &[],
            );
            for (track_index, item_index, item) in replacements {
                let Some(track) = self.children.get_mut(track_index) else {
                    *self = backup;
                    return false;
                };
                if !track.replace_item_by_index(item_index, item) {
                    *self = backup;
                    return false;
                }
            }
            if (replacement_duration - selected_duration).abs() > EPS {
                let mut used_ids = self.collect_timeline_ids();
                for track_index in boundary_track_indices {
                    if track_index == selected_track_index {
                        continue;
                    }
                    if replacement_duration > selected_duration + EPS {
                        let mut gap =
                            Item::Gap(Gap::make_gap(replacement_duration - selected_duration));
                        Self::ensure_unique_item_id(&mut gap, &mut used_ids);
                        let Some(track) = self.children.get_mut(track_index) else {
                            *self = backup;
                            return false;
                        };
                        track.insert_at_time(
                            selected_start + selected_duration,
                            gap,
                            OverlapPolicy::Push,
                            InsertPolicy::SplitAndInsert,
                        );
                    } else {
                        let shrink = selected_duration - replacement_duration;
                        let end = selected_start + selected_duration;
                        let Some(track) = self.children.get_mut(track_index) else {
                            *self = backup;
                            return false;
                        };
                        if !super::remove_gap_range(track, (end - shrink).max(selected_start), end) {
                            *self = backup;
                            return false;
                        }
                    }
                }
            }
            self.sanitize_preserving_all_gap_tracks();
            return true;
        };

        let mut used_ids = self.collect_timeline_ids();
        let mut primary_track_index = selected_track_index;
        let mut created_track_indices = Vec::new();
        let mut insertions = Vec::new();

        let mut inserted_audio_tracks = Vec::new();
        let mut inserted_audio_boundary_tracks = Vec::new();
        for audio_item in synced_audio_inputs {
            let track_count_before_audio = self.children.len();
            let Some(audio_track_index) = self.find_or_create_audio_track(
                primary_track_index,
                selected_start,
                replacement_duration,
                &mut created_track_indices,
                &inserted_audio_tracks,
                &inserted_audio_boundary_tracks,
                Some(sync_clips_id),
                false,
                crate::OverlapPolicy::Override,
            ) else {
                *self = backup;
                return false;
            };
            let reused_empty_boundary_track = self.children.len() == track_count_before_audio
                && super::track_is_empty_boundary(&self.children[audio_track_index]);
            if self.children.len() > track_count_before_audio {
                shift_track_index_after_insert(&mut primary_track_index, audio_track_index);
                shift_replacement_tracks_after_insert(&mut replacements, audio_track_index);
                shift_insert_tracks_after_insert(&mut insertions, audio_track_index);
            }
            let Some((audio_item, _audio_id)) = Self::prepare_synced_item(
                audio_item,
                replacement_duration,
                Some(sync_clips_id),
                &mut used_ids,
            ) else {
                *self = backup;
                return false;
            };
            insertions.push((audio_track_index, audio_item));
            inserted_audio_tracks.push(audio_track_index);
            if reused_empty_boundary_track {
                inserted_audio_boundary_tracks.push(audio_track_index);
            }
        }

        let boundary_groups = selected_sync_clips.into_iter().collect::<Vec<_>>();
        let mut boundary_track_indices =
            self.boundary_track_indices_for_anchors(&boundary_groups, &[primary_track_index], &[]);
        for (track_index, _, _) in &replacements {
            if !boundary_track_indices.contains(track_index) {
                boundary_track_indices.push(*track_index);
            }
        }
        for (track_index, _) in &insertions {
            if !boundary_track_indices.contains(track_index) {
                boundary_track_indices.push(*track_index);
            }
        }
        boundary_track_indices.sort_unstable();
        boundary_track_indices.dedup();

        for track_index in boundary_track_indices {
            if replacements
                .iter()
                .any(|(replacement_track_index, _, _)| *replacement_track_index == track_index)
                || insertions
                    .iter()
                    .any(|(insertion_track_index, _)| *insertion_track_index == track_index)
            {
                continue;
            }
            let mut gap = Item::Gap(Gap::make_gap(replacement_duration));
            Self::ensure_unique_item_id(&mut gap, &mut used_ids);
            insertions.push((track_index, gap));
        }

        replacements.sort_by_key(|(track_index, _, _)| *track_index);
        insertions.sort_by_key(|(track_index, _)| *track_index);

        for (track_index, item_index, item) in replacements {
            let Some(track) = self.children.get_mut(track_index) else {
                *self = backup;
                return false;
            };
            if !track.replace_item_by_index(item_index, item) {
                *self = backup;
                return false;
            }
        }
        for (track_index, item) in insertions {
            if !self.insert_gap_only(track_index, selected_start, item) {
                *self = backup;
                return false;
            }
        }

        self.sanitize_preserving_all_gap_tracks();
        true
    }
}
