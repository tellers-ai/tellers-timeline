//! Associated tracks: where a track's sync partners go.
//!
//! A clip inserted with sync partners (a video with its audio channels, or an
//! audio clip with other audio clips synced to it) lands as a column across
//! several tracks. The track receiving the primary clip is the *primary track*;
//! the tracks receiving its partners are its *associated tracks*.
//!
//! Nothing is stored for this: the association is computed from the sync
//! clips (the Resolve "Link Group ID" on clips) every time it is needed, so
//! the saved document is plain OTIO and never drifts from the clips.
//!
//! Every sync column has an owner track: its video track, or, for an
//! audio-only column, the member track holding the most clips (highest index
//! on a tie). The owner's associated tracks are the tracks holding the other
//! clips of its columns, nearest to the owner first. A track belongs to one
//! owner: when two owners share a track it goes to the one it shares the
//! most columns with. A track holding no sync clip belongs to nobody: it is
//! *free*, and any column may adopt it.
//!
//! Slot order follows: the first partner of a column goes to the nearest
//! associated track that is free over the column, the second to the next,
//! and so on; when there is no room a new track is created past the existing
//! partners, so after [`Stack::normalize_track_order`] the slot order is also
//! the stack order below the owner. Associated tracks are reserved for their
//! owner's columns; [`Stack::free_audio_track_for`] picks a track for a plain
//! audio clip accordingly.
//!
//! The tracks the library creates (numbered `A3`, `V2`, …) are marked at
//! `metadata["tellers.ai"]["created_by"] = "auto"`, so
//! [`Stack::prune_empty_partner_tracks`] can remove the audio ones once empty without
//! guessing from their name. Renaming a track with [`Track::set_name`] clears
//! the mark: a track the user named is theirs to keep.

use std::collections::BTreeMap;

use crate::{IdMetadataExt, Item, Seconds, Stack, Track, TrackKind};

/// The key of a track's creator at `metadata["tellers.ai"]`.
pub const TRACK_CREATED_BY_KEY: &str = "created_by";
/// The creator value of the tracks the library creates itself.
pub const TRACK_CREATED_BY_AUTO: &str = "auto";

impl Track {
    /// Whether the library created this track (see [`Self::set_auto_created`]).
    pub fn is_auto_created(&self) -> bool {
        self.metadata
            .get("tellers.ai")
            .and_then(|ai| ai.get(TRACK_CREATED_BY_KEY))
            .and_then(|value| value.as_str())
            == Some(TRACK_CREATED_BY_AUTO)
    }

    /// Mark (or unmark) this track as created by the library, which makes it
    /// a candidate for [`Stack::prune_empty_partner_tracks`].
    pub fn set_auto_created(&mut self, auto_created: bool) {
        if self.metadata.as_object().is_none() {
            self.metadata = serde_json::Value::Object(serde_json::Map::new());
        }
        let map = self.metadata.as_object_mut().unwrap();
        if auto_created {
            let ai = map
                .entry("tellers.ai".to_string())
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            if ai.as_object().is_none() {
                *ai = serde_json::Value::Object(serde_json::Map::new());
            }
            ai.as_object_mut().unwrap().insert(
                TRACK_CREATED_BY_KEY.to_string(),
                serde_json::Value::String(TRACK_CREATED_BY_AUTO.to_string()),
            );
        } else if let Some(ai) = map.get_mut("tellers.ai").and_then(|ai| ai.as_object_mut()) {
            ai.remove(TRACK_CREATED_BY_KEY);
        }
    }

    /// Rename the track. A rename makes the track the user's: it is no longer
    /// marked as created by the library, so pruning leaves it alone.
    pub fn set_name(&mut self, name: Option<String>) {
        self.name = name;
        self.set_auto_created(false);
    }
}

/// One track group: an owner with its partners in slot order. A track in no
/// group is reported as a group of one. Hosts use this to draw or collapse a
/// group as one unit.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackGroup {
    /// Index of the owner track (the video track of a video group, the main
    /// track of an audio group).
    pub owner_track_index: usize,
    /// Id of the owner track, when it has one.
    pub owner_track_id: Option<String>,
    /// Kind of the owner: `Video` for a video group, `Audio` for an audio group.
    pub kind: TrackKind,
    /// Partner track indices in slot order (first partner right below the
    /// owner once normalized).
    pub partner_track_indices: Vec<usize>,
    /// Partner track ids in slot order.
    pub partner_track_ids: Vec<Option<String>>,
}

impl Stack {
    /// Sync id -> member track indices, groups in order of first appearance.
    fn sync_group_members(&self) -> BTreeMap<i64, Vec<usize>> {
        let mut members: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
        for (track_index, track) in self.children.iter().enumerate() {
            for item in &track.items {
                let Item::Clip(clip) = item else {
                    continue;
                };
                let Some(sync_clips_id) = clip.sync_clips_id() else {
                    continue;
                };
                let tracks = members.entry(sync_clips_id).or_default();
                if !tracks.contains(&track_index) {
                    tracks.push(track_index);
                }
            }
        }
        members
    }

    /// Number of clips (not gaps) on a track.
    fn track_clip_count(&self, track_index: usize) -> usize {
        self.children
            .get(track_index)
            .map(|track| {
                track
                    .items
                    .iter()
                    .filter(|item| matches!(item, Item::Clip(_)))
                    .count()
            })
            .unwrap_or(0)
    }

    /// The main track of an audio group: the member holding the most clips,
    /// the highest index on a tie. It owns the group and sits on top of it
    /// once normalized.
    pub fn audio_group_main_track(&self, member_track_indices: &[usize]) -> Option<usize> {
        member_track_indices
            .iter()
            .copied()
            .max_by_key(|&index| (self.track_clip_count(index), index))
    }

    /// The owner of a sync column spread over `member_track_indices`: its
    /// highest video track, else the audio group's main track.
    fn sync_group_owner(&self, member_track_indices: &[usize]) -> Option<usize> {
        member_track_indices
            .iter()
            .copied()
            .filter(|&index| self.children[index].kind == TrackKind::Video)
            .max()
            .or_else(|| self.audio_group_main_track(member_track_indices))
    }

    /// The association for every track at once: `map[owner]` lists the
    /// owner's partner tracks nearest first. Computed from the sync clips, see
    /// the module docs for the owner and exclusivity rules.
    pub fn association_map(&self) -> Vec<Vec<usize>> {
        let len = self.children.len();
        // (partner, owner) -> how many columns they share.
        let mut shared: Vec<Vec<(usize, usize)>> = vec![Vec::new(); len];
        for members in self.sync_group_members().values() {
            if members.len() < 2 {
                continue;
            }
            let Some(owner) = self.sync_group_owner(members) else {
                continue;
            };
            for &partner in members.iter().filter(|&&index| index != owner) {
                match shared[owner]
                    .iter_mut()
                    .find(|(index, _)| *index == partner)
                {
                    Some((_, count)) => *count += 1,
                    None => shared[owner].push((partner, 1)),
                }
            }
        }
        // Exclusive membership: the owner sharing the most columns wins,
        // the lower index on a tie.
        let mut best_owner: Vec<Option<(usize, usize)>> = vec![None; len];
        for (owner, list) in shared.iter().enumerate() {
            for &(partner, count) in list {
                if best_owner[partner].is_none_or(|(_, best)| count > best) {
                    best_owner[partner] = Some((owner, count));
                }
            }
        }
        shared
            .into_iter()
            .enumerate()
            .map(|(owner, list)| {
                let mut partners: Vec<usize> = list
                    .into_iter()
                    .filter(|&(partner, _)| best_owner[partner].map(|(o, _)| o) == Some(owner))
                    .map(|(partner, _)| partner)
                    .collect();
                partners.sort_by_key(|&index| (index.abs_diff(owner), index));
                partners
            })
            .collect()
    }

    /// The associated tracks of `track_index`, nearest first.
    pub fn associated_track_indices(&self, track_index: usize) -> Vec<usize> {
        self.association_map()
            .into_iter()
            .nth(track_index)
            .unwrap_or_default()
    }

    /// The associated track ids of the track with `track_id`, nearest first.
    pub fn associated_track_ids(&self, track_id: &str) -> Option<Vec<String>> {
        let (track_index, _) = self.get_track_by_id(track_id)?;
        Some(
            self.associated_track_indices(track_index)
                .into_iter()
                .filter_map(|index| self.children[index].get_id())
                .collect(),
        )
    }

    /// Indices of the tracks whose association contains `track_index`.
    pub fn tracks_associating(&self, track_index: usize) -> Vec<usize> {
        self.association_map()
            .iter()
            .enumerate()
            .filter(|(owner, partners)| *owner != track_index && partners.contains(&track_index))
            .map(|(owner, _)| owner)
            .collect()
    }

    /// Whether some other track has `track_index` as an associated track.
    pub fn is_associated_partner(&self, track_index: usize) -> bool {
        !self.tracks_associating(track_index).is_empty()
    }

    /// The owner of the group `track_index` belongs to: the track it is a
    /// partner of, or itself (an owner, or a track in no group).
    pub fn group_owner_of(&self, track_index: usize) -> usize {
        self.tracks_associating(track_index)
            .into_iter()
            .next()
            .unwrap_or(track_index)
    }

    /// Whether `track_index` is in no group at all: neither a partner nor an
    /// owner. Such a track can be adopted by any column.
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
    /// group's track. A track holding no sync clip is free, so an empty
    /// track never blocks anything.
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

    /// Every track group, owners in stack order (bottom to top). Tracks in no
    /// group come back as groups of one, so every track appears exactly once.
    pub fn track_groups(&self) -> Vec<TrackGroup> {
        let map = self.association_map();
        let len = self.children.len();
        let mut owner_of: Vec<usize> = (0..len).collect();
        for (owner, partners) in map.iter().enumerate() {
            for &partner in partners {
                owner_of[partner] = owner;
            }
        }
        (0..len)
            .filter(|&owner| owner_of[owner] == owner)
            .map(|owner| {
                let partner_track_indices: Vec<usize> = map[owner]
                    .iter()
                    .copied()
                    .filter(|&partner| owner_of[partner] == owner)
                    .collect();
                TrackGroup {
                    owner_track_index: owner,
                    owner_track_id: self.children[owner].get_id(),
                    kind: self.children[owner].kind.clone(),
                    partner_track_ids: partner_track_indices
                        .iter()
                        .map(|&partner| self.children[partner].get_id())
                        .collect(),
                    partner_track_indices,
                }
            })
            .collect()
    }

    /// The track that owns a column spread over `member_track_indices`: the
    /// member with the most of the others as associated tracks; ties go to a
    /// video track, then to the highest index. Partner slots of the column
    /// are resolved from this track whichever member is moved.
    pub(crate) fn column_owner_track_index(&self, member_track_indices: &[usize]) -> Option<usize> {
        let map = self.association_map();
        member_track_indices
            .iter()
            .copied()
            .filter(|&index| index < self.children.len())
            .max_by_key(|&index| {
                let listed = map[index]
                    .iter()
                    .filter(|partner| member_track_indices.contains(partner))
                    .count();
                (listed, self.children[index].kind == TrackKind::Video, index)
            })
    }

    /// Remove the audio tracks the library created that hold no clip any
    /// more: tracks marked as auto-created (see [`Track::is_auto_created`])
    /// with gaps only, or nothing. Tracks the host created or renamed stay, as
    /// do all tracks holding a clip. Returns the removed track ids.
    ///
    /// A separate step, like [`Self::normalize_track_order`]: the host runs it
    /// when it wants the clutter gone, e.g. on save.
    pub fn prune_empty_partner_tracks(&mut self) -> Vec<String> {
        let candidates: Vec<String> = self
            .children
            .iter()
            .filter(|track| {
                track.kind == TrackKind::Audio
                    && track.items.iter().all(|item| matches!(item, Item::Gap(_)))
                    && track.is_auto_created()
            })
            .filter_map(|track| track.get_id())
            .collect();
        let mut removed = Vec::new();
        for id in candidates {
            if let Some((index, _)) = self.get_track_by_id(&id) {
                self.children.remove(index);
                removed.push(id);
            }
        }
        removed
    }

    /// Reorder the tracks so every group is contiguous: the owner on top
    /// (highest index of the group) with its partners right below it in slot
    /// order. Groups keep the relative order of their owners; tracks in no
    /// group keep their place among them. Returns whether the order changed.
    ///
    /// This is a separate step the host runs once per request, after its
    /// edits: every index it holds across library calls stays valid until then.
    pub fn normalize_track_order(&mut self) -> bool {
        let len = self.children.len();
        let mut order: Vec<usize> = Vec::with_capacity(len);
        for group in self.track_groups() {
            order.extend(group.partner_track_indices.iter().rev());
            order.push(group.owner_track_index);
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

    /// Pick an existing audio track for a clip that has no sync partners,
    /// free over `[start, start + duration]`, never a video group's audio
    /// track (a plain clip is refused there). Tracks in no group come first,
    /// then audio-group tracks, lowest index first. `None` when no audio
    /// track is free there; the caller then adds a track.
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
                    && !self.track_in_video_group(*index)
            })
            .map(|(index, _)| index)
            .collect();
        candidates.sort_by_key(|&index| (!self.track_is_free(index), index));
        candidates
            .into_iter()
            .next()
            .and_then(|index| self.children[index].get_id())
    }
}
