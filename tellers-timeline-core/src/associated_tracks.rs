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

    /// The association implied by the sync clips on `track_index`: for every
    /// sync group this track is the primary of, the other member tracks,
    /// nearest first (ties to the lower index). Used when the track has no
    /// stored list, e.g. right after a Resolve import.
    pub fn derived_associated_track_indices(&self, track_index: usize) -> Vec<usize> {
        let mut partners: Vec<usize> = Vec::new();
        for sync_clips_id in self.ordered_sync_clips_ids(track_index) {
            let members = self.tracks_holding_sync_clips(sync_clips_id);
            if self.sync_group_primary_track(&members) != Some(track_index) {
                continue;
            }
            let mut group_partners: Vec<usize> = members
                .into_iter()
                .filter(|&index| index != track_index)
                .collect();
            group_partners.sort_by_key(|&index| (index.abs_diff(track_index), index));
            for partner in group_partners {
                if !partners.contains(&partner) {
                    partners.push(partner);
                }
            }
        }
        partners
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
        self.children[track_index].set_associated_track_ids(known);
        true
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
            track.set_associated_track_ids(ids);
        }
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
