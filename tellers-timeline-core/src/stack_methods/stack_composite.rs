//! Editing the inside of a composite clip ([`CompositeStack`], an OTIO
//! `Stack.1` inside a track).
//!
//! Item ops (`get_item`, `split_item_at_time`, `delete_item`, `insert_item_*`,
//! `move_item_*`, ...) see a stack's top-level items only. (The timeline's
//! `*_item_tellers_effect*` accessors, which never change a length, find
//! nested items directly.) To edit inside a composite, [`Stack::with_composite_mut`] lends
//! its tracks out as a working [`Stack`], so every op applies within the
//! composite's own track list: track indices are indices into the
//! composite's tracks, and sync clips / link groups pair clips of that level
//! only.
//!
//! A composite without `source_range` lasts as long as its longest track, so
//! an edit inside can change its length. The outer track then ripples: the
//! change is applied as a resize of the composite's tail with
//! [`OverlapPolicy::Push`] (see [`Stack::resize_item`]), so a longer composite
//! pushes what follows it and a shorter one pulls it back, sync partners on
//! other tracks following as they do for any resize. An edit that empties
//! such a composite brings it to zero length, which removes it. A composite
//! with a `source_range` keeps its length.

use crate::track_methods::track_item_insert::OverlapPolicy;
use crate::{CompositeStack, IdMetadataExt, Item, Stack, Track};

const EPS: crate::Seconds = 1e-9;

fn find_composite<'a>(tracks: &'a [Track], id: &str) -> Option<&'a CompositeStack> {
    tracks.iter().flat_map(|track| &track.items).find_map(|item| match item {
        Item::Composite(stack) if item.get_id().as_deref() == Some(id) => Some(stack),
        Item::Composite(stack) => find_composite(&stack.children, id),
        _ => None,
    })
}

fn find_item<'a>(tracks: &'a [Track], id: &str) -> Option<&'a Item> {
    tracks.iter().flat_map(|track| &track.items).find_map(|item| {
        if item.get_id().as_deref() == Some(id) {
            return Some(item);
        }
        match item {
            Item::Composite(stack) => find_item(&stack.children, id),
            _ => None,
        }
    })
}

fn find_item_mut<'a>(tracks: &'a mut [Track], id: &str) -> Option<&'a mut Item> {
    for item in tracks.iter_mut().flat_map(|track| &mut track.items) {
        if item.get_id().as_deref() == Some(id) {
            return Some(item);
        }
        if let Item::Composite(stack) = item {
            if let Some(found) = find_item_mut(&mut stack.children, id) {
                return Some(found);
            }
        }
    }
    None
}

fn find_item_track<'a>(tracks: &'a [Track], id: &str) -> Option<&'a Track> {
    tracks.iter().find_map(|track| {
        if track.get_item_by_id(id).is_some() {
            return Some(track);
        }
        track.items.iter().find_map(|item| match item {
            Item::Composite(stack) => find_item_track(&stack.children, id),
            _ => None,
        })
    })
}

/// The ids of the composites enclosing `id`, outermost first.
fn composite_path(tracks: &[Track], id: &str) -> Option<Vec<String>> {
    for item in tracks.iter().flat_map(|track| &track.items) {
        if item.get_id().as_deref() == Some(id) {
            return Some(Vec::new());
        }
        if let Item::Composite(stack) = item {
            if let Some(mut path) = composite_path(&stack.children, id) {
                path.insert(0, item.get_id().unwrap_or_default());
                return Some(path);
            }
        }
    }
    None
}

impl Stack {
    /// The composite clip `composite_id`, at any depth.
    pub fn get_composite(&self, composite_id: &str) -> Option<&CompositeStack> {
        find_composite(&self.children, composite_id)
    }

    /// The item `item_id` at any depth, nested composite content included.
    /// [`Self::get_item`] sees top-level items only.
    pub fn get_nested_item(&self, item_id: &str) -> Option<&Item> {
        find_item(&self.children, item_id)
    }

    /// Mutable [`Self::get_nested_item`]. Changing an item's length through it
    /// does not ripple anything; use [`Self::with_composite_mut`] for edits.
    pub fn get_nested_item_mut(&mut self, item_id: &str) -> Option<&mut Item> {
        find_item_mut(&mut self.children, item_id)
    }

    /// The track directly holding the item `item_id`, at any depth.
    pub fn get_nested_item_track(&self, item_id: &str) -> Option<&Track> {
        find_item_track(&self.children, item_id)
    }

    /// The ids of the composite clips enclosing `item_id`, outermost first:
    /// empty for a top-level item, `None` when there is no such item. Its last
    /// entry is the composite to open with [`Self::with_composite_mut`] to
    /// edit the item.
    pub fn composite_path(&self, item_id: &str) -> Option<Vec<String>> {
        composite_path(&self.children, item_id)
    }

    /// Edit the tracks of the composite clip `composite_id` (at any depth) as
    /// a working [`Stack`]: `edit` gets a stack holding the composite's tracks
    /// (and its name), and whatever tracks it leaves become the composite's.
    /// The composite keeps its own id, metadata, effects, markers and source
    /// range; the working stack's own metadata, effects and markers are
    /// dropped. Returns `None`, without calling `edit`, when there is no such
    /// composite.
    ///
    /// When the composite has no `source_range` and the edit changed its
    /// length, the enclosing track ripples as described in the module docs
    /// (for a nested composite, at every level up to this stack). Should that
    /// resize be refused, the composite is given a `source_range` keeping its
    /// old length instead, so the enclosing track is left as it was.
    pub fn with_composite_mut<R>(
        &mut self,
        composite_id: &str,
        edit: impl FnOnce(&mut Stack) -> R,
    ) -> Option<R> {
        let mut edit = Some(edit);
        let mut result = None;
        self.edit_composite_dyn(composite_id, &mut |inner| {
            if let Some(edit) = edit.take() {
                result = Some(edit(inner));
            }
        });
        result
    }

    /// [`Self::with_composite_mut`] without generics, so it can recurse.
    fn edit_composite_dyn(&mut self, composite_id: &str, edit: &mut dyn FnMut(&mut Stack)) -> bool {
        let Some(path) = self.composite_path(composite_id) else {
            return false;
        };
        if !matches!(self.get_nested_item(composite_id), Some(Item::Composite(_))) {
            return false;
        }
        match path.first() {
            // Nested: open the outermost composite and recurse, so the length
            // change ripples at every level on the way back up.
            Some(outer_id) => {
                let outer_id = outer_id.clone();
                let mut found = false;
                self.edit_top_level_composite(&outer_id, &mut |inner| {
                    found = inner.edit_composite_dyn(composite_id, edit);
                });
                found
            }
            None => {
                self.edit_top_level_composite(composite_id, edit);
                true
            }
        }
    }

    /// Replace the tracks of the composite clip `composite_id` (at any depth),
    /// rippling as [`Self::with_composite_mut`] does. False when there is no
    /// such composite.
    pub fn set_composite_tracks(&mut self, composite_id: &str, tracks: Vec<Track>) -> bool {
        self.with_composite_mut(composite_id, |inner| inner.children = tracks)
            .is_some()
    }

    fn edit_top_level_composite(&mut self, composite_id: &str, edit: &mut dyn FnMut(&mut Stack)) {
        let (track_index, item_index, _) = self
            .get_item(composite_id)
            .expect("composite_path found it at the top level");
        let start = self.children[track_index].start_time_of_item(item_index);
        let Item::Composite(composite) = &mut self.children[track_index].items[item_index] else {
            unreachable!("checked by the caller");
        };
        let old_duration = composite.duration();
        let auto_length = composite.source_range.is_none();

        let mut inner = Stack {
            name: composite.name.clone(),
            children: std::mem::take(&mut composite.children),
            ..Stack::default()
        };
        edit(&mut inner);
        composite.children = inner.children;

        let new_duration = composite.content_duration();
        if auto_length && (new_duration - old_duration).abs() > EPS {
            // Back to the old footprint, then resize to the new one so the
            // track (and the composite's sync partners) follows the usual
            // resize rules.
            composite.set_duration(old_duration);
            let resized =
                self.resize_item(composite_id, start, new_duration, OverlapPolicy::Push, false);
            if let Some((ti, ii, _)) = self.get_item(composite_id) {
                if let Item::Composite(composite) = &mut self.children[ti].items[ii] {
                    if resized {
                        composite.source_range = None;
                    }
                }
            }
        }
    }
}
