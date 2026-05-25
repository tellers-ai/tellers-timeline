use super::{range_is_gap_backed, resolve_link_group_id, split_gap_boundary, EPS};
use crate::{Gap, IdMetadataExt, InsertPolicy, Item, OverlapPolicy, Seconds, Stack, Track, TrackKind};

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

fn remove_gap_range(track: &mut Track, start: Seconds, end: Seconds) -> bool {
    if !range_is_gap_backed(track, start, end) {
        return false;
    }

    split_gap_boundary(track, end);
    split_gap_boundary(track, start);

    let mut pos = 0.0;
    let mut index = 0;
    while index < track.items.len() {
        let duration = track.items[index].duration().max(0.0);
        let item_start = pos;
        let item_end = pos + duration;
        if item_start >= start - EPS && item_end <= end + EPS {
            if !matches!(track.items[index], Item::Gap(_)) {
                return false;
            }
            track.items.remove(index);
            pos = item_end;
        } else {
            pos = item_end;
            index += 1;
        }
    }
    true
}

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
        let selected_duration = selected_item.duration().max(0.0);
        let selected_link_group = match selected_item {
            Item::Clip(clip) => resolve_link_group_id(&clip.metadata),
            Item::Gap(_) => None,
        };
        let linked_audio_input_provided = linked_audio_clips.is_some();
        let linked_video_input_provided = linked_video_clip.is_some();
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
            let duration = clip.source_range.duration.to_seconds().max(0.0);
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

        let mut linked_audio_inputs = linked_inputs.audio.into_iter();
        let mut linked_video_input = linked_inputs.video;
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
            } else if linked_audio_input_provided && track_kind == TrackKind::Audio {
                linked_audio_inputs
                    .next()
                    .unwrap_or_else(|| Item::Gap(Gap::make_gap(replacement_duration)))
            } else if linked_video_input_provided && track_kind == TrackKind::Video {
                linked_video_input
                    .take()
                    .unwrap_or_else(|| Item::Gap(Gap::make_gap(replacement_duration)))
            } else {
                existing.clone()
            };
            next.set_id(existing_id);
            next.set_duration(replacement_duration);
            Self::set_item_link_group(&mut next, link_group);
            replacements.push((track_index, item_index, next));
        }

        let Some(link_group_id) = link_group else {
            let mut adjacent_link_groups =
                self.linked_groups_adjacent_to_time(selected_track_index, selected_start);
            for link_group in self.linked_groups_adjacent_to_time(
                selected_track_index,
                selected_start + selected_duration,
            ) {
                if !adjacent_link_groups.contains(&link_group) {
                    adjacent_link_groups.push(link_group);
                }
            }
            let boundary_track_indices = self.boundary_track_indices_for_anchors(
                &adjacent_link_groups,
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
                        if !remove_gap_range(track, (end - shrink).max(selected_start), end) {
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
        if let Some(video_item) = linked_video_input {
            let track_count_before_video = self.children.len();
            let Some(video_track_index) = self.find_or_create_video_track_for_audio(
                primary_track_index,
                selected_start,
                replacement_duration,
                &mut created_track_indices,
                Some(link_group_id),
                false,
                crate::OverlapPolicy::Override,
            ) else {
                *self = backup;
                return false;
            };
            if self.children.len() > track_count_before_video {
                shift_track_index_after_insert(&mut primary_track_index, video_track_index);
                shift_replacement_tracks_after_insert(&mut replacements, video_track_index);
                shift_insert_tracks_after_insert(&mut insertions, video_track_index);
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
            insertions.push((video_track_index, video_item));
        }

        let mut inserted_audio_tracks = Vec::new();
        let mut inserted_audio_boundary_tracks = Vec::new();
        for audio_item in linked_audio_inputs {
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
            let Some((audio_item, _audio_id)) = Self::prepare_linked_item(
                audio_item,
                replacement_duration,
                Some(link_group_id),
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

        let boundary_groups = selected_link_group.into_iter().collect::<Vec<_>>();
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
