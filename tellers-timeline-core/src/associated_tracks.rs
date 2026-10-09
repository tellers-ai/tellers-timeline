//! Associated tracks: where a track's sync partners go by preference.
//!
//! A clip inserted with sync partners (a video with its audio channels, or an
//! audio clip with other audio clips synced to it) lands as a column across
//! several tracks. The track receiving the primary clip is the *primary track*;
//! the tracks receiving its partners are its *associated tracks*.
//!
//! The association is stored on the primary track, as an ordered list of
//! track ids at `metadata["tellers.ai"]["associated_track_ids"]`. The order is
//! the slot order: the first partner of a column goes to the first associated
//! track that is free over the column, the second to the next, and so on. When
//! the associated tracks cannot take every partner a new track is created
//! right below them and appended to the list, so the next column reuses it.
//!
//! The association is kind-agnostic: a video track lists its audio tracks, an
//! audio track that receives audio-only sync groups lists the audio tracks its
//! partners go to. A track with no stored list falls back to the association
//! implied by the sync clips already on it (Resolve imports, timelines edited
//! before this metadata existed), see [`Stack::derived_associated_track_indices`].
//!
//! Associated tracks are reserved: a clip that is not part of a sync column
//! (plain audio, music) prefers a track nobody lists as a partner, so a video's
//! channels keep their slots. [`Stack::free_audio_track_for`] implements that
//! preference for hosts that let the library pick the track.

use std::collections::HashSet;

use crate::{IdMetadataExt, Item, Seconds, Stack, Track, TrackKind};

/// Metadata key, under the `tellers.ai` namespace, holding the ordered list
/// of associated track ids.
pub const ASSOCIATED_TRACK_IDS_KEY: &str = "associated_track_ids";

fn tellers_namespace_mut(
    metadata: &mut serde_json::Value,
) -> &mut serde_json::Map<String, serde_json::Value> {
    if metadata.as_object().is_none() {
        *metadata = serde_json::Value::Object(serde_json::Map::new());
    }
    let map = metadata.as_object_mut().unwrap();
    let entry = map
        .entry("tellers.ai".to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if entry.as_object().is_none() {
        *entry = serde_json::Value::Object(serde_json::Map::new());
    }
    entry.as_object_mut().unwrap()
}

impl Track {
    /// The stored associated track ids, in slot order, or `None` when the
    /// track has no stored list (as opposed to an empty one).
    pub fn stored_associated_track_ids(&self) -> Option<Vec<String>> {
        let raw = self
            .metadata
            .get("tellers.ai")
            .and_then(|v| v.get(ASSOCIATED_TRACK_IDS_KEY))?;
        let list = raw.as_array()?;
        let mut ids = Vec::with_capacity(list.len());
        for value in list {
            if let Some(id) = value.as_str() {
                if !id.is_empty() && !ids.iter().any(|known: &String| known == id) {
                    ids.push(id.to_string());
                }
            }
        }
        Some(ids)
    }

    /// The stored associated track ids, in slot order (empty when none).
    pub fn associated_track_ids(&self) -> Vec<String> {
        self.stored_associated_track_ids().unwrap_or_default()
    }

    /// Replace the associated track list. Duplicates and empty ids are dropped;
    /// the track's own id is never listed.
    pub fn set_associated_track_ids(&mut self, ids: Vec<String>) {
        let own_id = self.get_id();
        let mut unique: Vec<String> = Vec::with_capacity(ids.len());
        for id in ids {
            if id.is_empty() || own_id.as_deref() == Some(id.as_str()) || unique.contains(&id) {
                continue;
            }
            unique.push(id);
        }
        let values = unique.into_iter().map(serde_json::Value::String).collect();
        tellers_namespace_mut(&mut self.metadata).insert(
            ASSOCIATED_TRACK_IDS_KEY.to_string(),
            serde_json::Value::Array(values),
        );
    }

    /// Remove the stored list entirely (the track falls back to the derived
    /// association). Returns whether a list was present.
    pub fn clear_associated_track_ids(&mut self) -> bool {
        let Some(ai) = self
            .metadata
            .get_mut("tellers.ai")
            .and_then(|value| value.as_object_mut())
        else {
            return false;
        };
        ai.remove(ASSOCIATED_TRACK_IDS_KEY).is_some()
    }

    /// Append `id` to the list unless already present. Returns whether it was added.
    pub fn push_associated_track_id(&mut self, id: &str) -> bool {
        let mut ids = self.associated_track_ids();
        if id.is_empty() || ids.iter().any(|known| known == id) {
            return false;
        }
        if self.get_id().as_deref() == Some(id) {
            return false;
        }
        ids.push(id.to_string());
        self.set_associated_track_ids(ids);
        true
    }

    /// Remove `id` from the list. Returns whether it was present.
    pub fn remove_associated_track_id(&mut self, id: &str) -> bool {
        let Some(ids) = self.stored_associated_track_ids() else {
            return false;
        };
        if !ids.iter().any(|known| known == id) {
            return false;
        }
        let remaining = ids.into_iter().filter(|known| known != id).collect();
        self.set_associated_track_ids(remaining);
        true
    }
}

impl Stack {
    /// The sync ids on `track_index`, in order of first appearance.
    fn ordered_sync_clips_ids(&self, track_index: usize) -> Vec<i64> {
        let mut ids = Vec::new();
        let Some(track) = self.children.get(track_index) else {
            return ids;
        };
        for item in &track.items {
            if let Item::Clip(clip) = item {
                if let Some(id) = clip.sync_clips_id() {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
        }
        ids
    }

    fn tracks_holding_sync_clips(&self, sync_clips_id: i64) -> Vec<usize> {
        self.children
            .iter()
            .enumerate()
            .filter(|(_, track)| {
                track.items.iter().any(|item| match item {
                    Item::Clip(clip) => clip.sync_clips_id() == Some(sync_clips_id),
                    Item::Gap(_) => false,
                })
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// The track that owns a sync group: its highest video track, or, for an
    /// audio-only group, its highest track.
    fn sync_group_primary_track(&self, member_track_indices: &[usize]) -> Option<usize> {
        member_track_indices
            .iter()
            .copied()
            .filter(|&index| self.children[index].kind == TrackKind::Video)
            .max()
            .or_else(|| member_track_indices.iter().copied().max())
    }

    /// The association implied by the sync clips, for every track at once:
    /// for each sync group, its primary track (highest video, else highest
    /// track) lists the other member tracks nearest first. Membership is
    /// exclusive: a track that would be listed by two primaries stays with
    /// the one it shares the most sync groups with (ties to the lower index).
    pub fn derived_association_map(&self) -> Vec<Vec<usize>> {
        let len = self.children.len();
        let mut lists: Vec<Vec<(usize, usize)>> = vec![Vec::new(); len];
        for (owner, owner_list) in lists.iter_mut().enumerate() {
            for sync_clips_id in self.ordered_sync_clips_ids(owner) {
                let members = self.tracks_holding_sync_clips(sync_clips_id);
                if self.sync_group_primary_track(&members) != Some(owner) {
                    continue;
                }
                let mut group_partners: Vec<usize> = members
                    .into_iter()
                    .filter(|&index| index != owner)
                    .collect();
                group_partners.sort_by_key(|&index| (index.abs_diff(owner), index));
                for partner in group_partners {
                    match owner_list.iter_mut().find(|(index, _)| *index == partner) {
                        Some((_, count)) => *count += 1,
                        None => owner_list.push((partner, 1)),
                    }
                }
            }
        }
        let mut best_owner: Vec<Option<(usize, usize)>> = vec![None; len];
        for (owner, list) in lists.iter().enumerate() {
            for &(partner, count) in list {
                let better = match best_owner[partner] {
                    None => true,
                    Some((_, best)) => count > best,
                };
                if better {
                    best_owner[partner] = Some((owner, count));
                }
            }
        }
        lists
            .into_iter()
            .enumerate()
            .map(|(owner, list)| {
                list.into_iter()
                    .filter(|&(partner, _)| best_owner[partner].map(|(o, _)| o) == Some(owner))
                    .map(|(partner, _)| partner)
                    .collect()
            })
            .collect()
    }

    /// The association implied by the sync clips on `track_index` (see
    /// [`Self::derived_association_map`]). Used when the track has no stored
    /// list, e.g. right after a Resolve import.
    pub fn derived_associated_track_indices(&self, track_index: usize) -> Vec<usize> {
        self.derived_association_map()
            .into_iter()
            .nth(track_index)
            .unwrap_or_default()
    }

    /// The associated tracks of `track_index` as indices, in slot order: the
    /// stored list resolved by id (ids of tracks no longer in the stack are
    /// skipped), or the derived association when no list is stored.
    pub fn associated_track_indices(&self, track_index: usize) -> Vec<usize> {
        let Some(track) = self.children.get(track_index) else {
            return Vec::new();
        };
        match track.stored_associated_track_ids() {
            Some(ids) => ids
                .iter()
                .filter_map(|id| self.get_track_by_id(id).map(|(index, _)| index))
                .filter(|&index| index != track_index)
                .collect(),
            None => self.derived_associated_track_indices(track_index),
        }
    }

    /// The associated track ids of the track with `track_id`, stored or derived.
    pub fn associated_track_ids(&self, track_id: &str) -> Option<Vec<String>> {
        let (track_index, _) = self.get_track_by_id(track_id)?;
        Some(
            self.associated_track_indices(track_index)
                .into_iter()
                .filter_map(|index| self.children[index].get_id())
                .collect(),
        )
    }

    /// Store `partner_ids` as the associated tracks of `track_id`, in that
    /// order. Ids that are not tracks of this stack are dropped. Returns false
    /// when `track_id` is unknown.
    pub fn associate_tracks(&mut self, track_id: &str, partner_ids: &[String]) -> bool {
        let Some((track_index, _)) = self.get_track_by_id(track_id) else {
            return false;
        };
        let known: Vec<String> = partner_ids
            .iter()
            .filter(|id| self.get_track_by_id(id).is_some())
            .cloned()
            .collect();
        self.children[track_index].set_associated_track_ids(known.clone());
        self.release_partners_from_other_owners(track_index, &known);
        true
    }

    /// A track belongs to one group: drop `partner_ids` from the stored list
    /// of every track other than `owner_index`.
    fn release_partners_from_other_owners(&mut self, owner_index: usize, partner_ids: &[String]) {
        for (index, track) in self.children.iter_mut().enumerate() {
            if index == owner_index || track.stored_associated_track_ids().is_none() {
                continue;
            }
            for id in partner_ids {
                track.remove_associated_track_id(id);
            }
        }
    }

    /// The owner of the group `track_index` belongs to: the track listing it,
    /// or itself when no track does (an owner, or a track in no group).
    pub fn group_owner_of(&self, track_index: usize) -> usize {
        self.tracks_associating(track_index)
            .into_iter()
            .next()
            .unwrap_or(track_index)
    }

    /// Whether `track_index` is in no group at all: nobody lists it and it
    /// lists nothing. Such a track can be adopted by any group.
    pub fn track_is_free(&self, track_index: usize) -> bool {
        !self.is_associated_partner(track_index)
            && self.associated_track_indices(track_index).is_empty()
    }

    /// Whether `track_index` is the owner or a partner of a video group.
    pub fn track_in_video_group(&self, track_index: usize) -> bool {
        self.children
            .get(self.group_owner_of(track_index))
            .is_some_and(|track| track.kind == TrackKind::Video)
    }

    /// Whether a column may land with its primary clip on `dest_track_index`.
    ///
    /// On a video track anything but a column that already has a video clip.
    /// On an audio track: a gap always; the audio partner of a video column on
    /// a track of a video group (its own, for a swap, or another one, moving
    /// the whole column) or on a free track (adopted), never on an audio
    /// group; a plain audio clip or an audio-only column never on a video
    /// group's track.
    pub(crate) fn column_may_land_on(
        &self,
        dest_track_index: usize,
        primary_is_gap: bool,
        has_video_partner: bool,
    ) -> bool {
        let Some(dest) = self.children.get(dest_track_index) else {
            return false;
        };
        match dest.kind {
            TrackKind::Video => !has_video_partner,
            TrackKind::Audio => {
                if primary_is_gap {
                    return true;
                }
                let in_video_group = self.track_in_video_group(dest_track_index);
                if has_video_partner {
                    in_video_group || self.track_is_free(dest_track_index)
                } else {
                    !in_video_group
                }
            }
            TrackKind::Other => !has_video_partner,
        }
    }

    /// Reorder the tracks so every group is contiguous: the owner on top
    /// (highest index of the group) with its partners right below it in list
    /// order. Groups keep the relative order of their owners; tracks in no
    /// group keep their place among them. Returns whether the order changed.
    ///
    /// This is a separate step the host runs once per request, after its
    /// edits: every index it holds across library calls stays valid until then.
    pub fn normalize_track_order(&mut self) -> bool {
        let len = self.children.len();
        let owner_of: Vec<usize> = (0..len).map(|index| self.group_owner_of(index)).collect();
        let mut order: Vec<usize> = Vec::with_capacity(len);
        for owner in 0..len {
            if owner_of[owner] != owner {
                continue;
            }
            let partners: Vec<usize> = self
                .associated_track_indices(owner)
                .into_iter()
                .filter(|&partner| owner_of[partner] == owner && partner != owner)
                .collect();
            order.extend(partners.iter().rev());
            order.push(owner);
        }
        for index in 0..len {
            if !order.contains(&index) {
                order.push(index);
            }
        }
        if order
            .iter()
            .enumerate()
            .all(|(position, &index)| position == index)
        {
            return false;
        }
        let mut tracks: Vec<Option<Track>> = self.children.drain(..).map(Some).collect();
        self.children = order
            .into_iter()
            .map(|index| tracks[index].take().expect("each track placed once"))
            .collect();
        true
    }

    /// After a manual reorder of `track_index`, make its owner's stored list
    /// follow the physical order (partners nearest the owner first), so the
    /// next normalization keeps the user's layout instead of undoing it.
    pub(crate) fn sync_list_with_physical_order(&mut self, track_index: usize) {
        let owner = self.group_owner_of(track_index);
        let Some(ids) = self.children[owner].stored_associated_track_ids() else {
            return;
        };
        let mut indexed: Vec<(usize, String)> = ids
            .into_iter()
            .filter_map(|id| self.get_track_by_id(&id).map(|(index, _)| (index, id)))
            .collect();
        indexed.sort_by_key(|&(index, _)| (index.abs_diff(owner), std::cmp::Reverse(index)));
        self.children[owner]
            .set_associated_track_ids(indexed.into_iter().map(|(_, id)| id).collect());
    }

    /// Make sure `track_index` lists every track in `partner_indices`,
    /// materializing the derived association first when nothing is stored yet.
    pub(crate) fn record_associated_tracks(
        &mut self,
        track_index: usize,
        partner_indices: &[usize],
    ) {
        let mut ids: Vec<String> = self
            .associated_track_indices(track_index)
            .into_iter()
            .filter_map(|index| self.children.get(index).and_then(|track| track.get_id()))
            .collect();
        for &partner in partner_indices {
            if partner == track_index {
                continue;
            }
            let Some(id) = self.children.get(partner).and_then(|track| track.get_id()) else {
                continue;
            };
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        if let Some(track) = self.children.get_mut(track_index) {
            track.set_associated_track_ids(ids.clone());
        }
        self.release_partners_from_other_owners(track_index, &ids);
    }

    /// The track that owns a column spread over `member_track_indices`: the
    /// member listing the most other members as associated tracks; ties go
    /// to a video track, then to the highest index. Partner slots of the
    /// column are resolved from this track's list whichever member is moved.
    pub(crate) fn column_owner_track_index(&self, member_track_indices: &[usize]) -> Option<usize> {
        member_track_indices
            .iter()
            .copied()
            .filter(|&index| index < self.children.len())
            .max_by_key(|&index| {
                let listed = self
                    .associated_track_indices(index)
                    .iter()
                    .filter(|partner| member_track_indices.contains(partner))
                    .count();
                (listed, self.children[index].kind == TrackKind::Video, index)
            })
    }

    /// Indices of the tracks whose stored or derived list contains `track_index`.
    pub fn tracks_associating(&self, track_index: usize) -> Vec<usize> {
        (0..self.children.len())
            .filter(|&primary| {
                primary != track_index
                    && self
                        .associated_track_indices(primary)
                        .contains(&track_index)
            })
            .collect()
    }

    /// Whether some other track lists `track_index` as an associated track.
    pub fn is_associated_partner(&self, track_index: usize) -> bool {
        !self.tracks_associating(track_index).is_empty()
    }

    /// Drop every stored association id that no longer names a track of this stack.
    pub(crate) fn prune_associated_track_ids(&mut self) {
        let existing: HashSet<String> = self
            .children
            .iter()
            .filter_map(|track| track.get_id())
            .collect();
        for track in &mut self.children {
            let Some(ids) = track.stored_associated_track_ids() else {
                continue;
            };
            let kept: Vec<String> = ids
                .iter()
                .filter(|id| existing.contains(*id))
                .cloned()
                .collect();
            if kept.len() != ids.len() {
                track.set_associated_track_ids(kept);
            }
        }
    }

    /// Pick an existing audio track for a clip that has no sync partners,
    /// free over `[start, start + duration]`. Tracks nobody lists as an
    /// associated track come first (so a video's channels keep their slots),
    /// then the lowest index. `None` when no audio track is free there; the
    /// caller then adds a track.
    pub fn free_audio_track_for(&self, start: Seconds, duration: Seconds) -> Option<String> {
        let end = start + duration.max(0.0);
        let mut candidates: Vec<usize> = self
            .children
            .iter()
            .enumerate()
            .filter(|(index, track)| {
                track.kind == TrackKind::Audio
                    && Stack::track_range_is_free(track, start, end)
                    && self.children[*index].get_id().is_some()
            })
            .map(|(index, _)| index)
            .collect();
        candidates.sort_by_key(|&index| (self.is_associated_partner(index), index));
        candidates
            .into_iter()
            .next()
            .and_then(|index| self.children[index].get_id())
    }
}
