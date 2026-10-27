//! Media references backed by an asset.
//!
//! A clip's media reference names the asset it plays in its metadata, and
//! the player (`video-player-js`) loads it from its `target_url`:
//!
//! ```json
//! { "OTIO_SCHEMA": "ExternalReference.1", "target_url": "https://cdn.example.test/…",
//!   "metadata": { "tellers.ai": { "media_id": "a1b2" } } }
//! ```
//!
//! `target_url` is resolved from the media id like an effect's or a font's
//! from its `asset_id`: list the ids with [`Timeline::media_asset_ids`], set
//! the URLs with [`Timeline::set_media_target_urls`] before handing the
//! timeline to the player, and strip them with
//! [`Timeline::clear_media_asset_target_urls`] (or
//! [`Timeline::clear_target_urls`]) so ephemeral (e.g. presigned) URLs are
//! not persisted. The media id is what the timeline persists.
//!
//! These functions read each clip's active media reference, and cover every
//! clip of the timeline: those of its tracks and those inside composite
//! clips, nested ones included.

use std::collections::HashMap;

use crate::{Clip, Item, MediaReference, Timeline, Track};

/// Key, under a media reference's `metadata["tellers.ai"]`, naming the asset
/// it plays.
pub const MEDIA_ID_KEY: &str = "media_id";

const DEFAULT_MEDIA_KEY: &str = "DEFAULT_MEDIA";

impl MediaReference {
    /// The asset this reference plays: its `metadata["tellers.ai"]["media_id"]`,
    /// trimmed; `None` when it has none or it is empty. A legacy root-level
    /// `media_id` is copied there on load.
    pub fn media_id(&self) -> Option<&str> {
        self.metadata()
            .get("tellers.ai")?
            .get(MEDIA_ID_KEY)?
            .as_str()
            .map(str::trim)
            .filter(|id| !id.is_empty())
    }
}

impl Clip {
    /// The key of the media reference this clip plays: its
    /// `active_media_reference_key`, or `DEFAULT_MEDIA` when unset.
    pub fn active_media_key(&self) -> &str {
        self.active_media_reference_key
            .as_deref()
            .unwrap_or(DEFAULT_MEDIA_KEY)
    }

    /// The media reference this clip plays (see [`Clip::active_media_key`]).
    pub fn active_media_reference(&self) -> Option<&MediaReference> {
        self.media_references.get(self.active_media_key())
    }

    pub fn active_media_reference_mut(&mut self) -> Option<&mut MediaReference> {
        let key = self.active_media_key().to_string();
        self.media_references.get_mut(&key)
    }

    /// The asset id of the media this clip plays (see
    /// [`MediaReference::media_id`]).
    pub fn media_id(&self) -> Option<&str> {
        self.active_media_reference()?.media_id()
    }
}

/// Visit every clip of `tracks`, descending into composite clips (nested
/// ones included).
pub fn for_each_clip(tracks: &[Track], f: &mut dyn FnMut(&Clip)) {
    for track in tracks {
        for item in &track.items {
            match item {
                Item::Clip(clip) => f(clip),
                Item::Composite(stack) => for_each_clip(&stack.children, f),
                Item::Gap(_) | Item::Transition(_) => {}
            }
        }
    }
}

/// [`for_each_clip`], mutably.
pub fn for_each_clip_mut(tracks: &mut [Track], f: &mut dyn FnMut(&mut Clip)) {
    for track in tracks {
        for item in &mut track.items {
            match item {
                Item::Clip(clip) => f(clip),
                Item::Composite(stack) => for_each_clip_mut(&mut stack.children, f),
                Item::Gap(_) | Item::Transition(_) => {}
            }
        }
    }
}

impl Timeline {
    /// Visit every clip of the timeline, those inside composite clips
    /// included, in timeline order.
    pub fn for_each_clip(&self, f: &mut dyn FnMut(&Clip)) {
        for_each_clip(&self.tracks.children, f);
    }

    /// [`Timeline::for_each_clip`], mutably.
    pub fn for_each_clip_mut(&mut self, f: &mut dyn FnMut(&mut Clip)) {
        for_each_clip_mut(&mut self.tracks.children, f);
    }

    /// Every distinct asset id the clips of the timeline play (each clip's
    /// active media reference), in timeline order: the ids to resolve for
    /// [`Timeline::set_media_target_urls`].
    pub fn media_asset_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        self.for_each_clip(&mut |clip| {
            if let Some(id) = clip.media_id() {
                if !ids.iter().any(|known| known == id) {
                    ids.push(id.to_string());
                }
            }
        });
        ids
    }

    /// Set the `target_url` of every clip's active external reference whose
    /// asset id is in `urls` (asset id → URL). Empty URLs and unknown ids
    /// are ignored. Returns how many references were updated.
    pub fn set_media_target_urls(&mut self, urls: &HashMap<String, String>) -> usize {
        let mut updated = 0;
        self.for_each_clip_mut(&mut |clip| {
            let Some(url) = clip
                .media_id()
                .and_then(|id| urls.get(id))
                .map(|url| url.trim().to_string())
                .filter(|url| !url.is_empty())
            else {
                return;
            };
            if let Some(MediaReference::ExternalReference { target_url, .. }) =
                clip.active_media_reference_mut()
            {
                *target_url = url;
                updated += 1;
            }
        });
        updated
    }

    /// [`Timeline::set_media_target_urls`] for the single asset `asset_id`.
    pub fn set_media_target_url(&mut self, asset_id: &str, url: &str) -> usize {
        let urls = HashMap::from([(asset_id.trim().to_string(), url.to_string())]);
        self.set_media_target_urls(&urls)
    }

    /// Empty the `target_url` of every asset-backed media reference (one with
    /// a media id), active or not, keeping the media ids. A reference with no
    /// media id keeps its URL: it is the only record of the media. Returns
    /// how many had one. [`Timeline::clear_target_urls`] empties them all.
    pub fn clear_media_asset_target_urls(&mut self) -> usize {
        let mut cleared = 0;
        self.for_each_clip_mut(&mut |clip| {
            for reference in clip.media_references.values_mut() {
                if reference.media_id().is_none() {
                    continue;
                }
                if let MediaReference::ExternalReference { target_url, .. } = reference {
                    if !target_url.is_empty() {
                        target_url.clear();
                        cleared += 1;
                    }
                }
            }
        });
        cleared
    }
}
