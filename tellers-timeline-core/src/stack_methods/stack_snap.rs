use std::collections::HashSet;

use crate::metadata::{item_link_group_id, item_tellers_group_id};
use crate::{IdMetadataExt, Item, Seconds, Stack};

/// Which edge of a time range a snap refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapEdge {
    Start,
    End,
}

/// What the moving range snapped to.
#[derive(Debug, Clone, PartialEq)]
pub enum SnapTarget {
    /// The start or end of a clip already on the timeline.
    ItemEdge {
        item_id: Option<String>,
        track_id: Option<String>,
        track_index: usize,
        edge: SnapEdge,
    },
    /// One of the caller's extra snap points (playhead, markers, ...), by its
    /// index in `extra_points`.
    Point { index: usize },
}

/// The closest snap found by [`Stack::find_snap`].
#[derive(Debug, Clone, PartialEq)]
pub struct SnapMatch {
    pub target: SnapTarget,
    /// The time of the target edge or point: where a guide line belongs.
    pub target_time: Seconds,
    /// Which edge of the moving range lands on `target_time`.
    pub moving_edge: SnapEdge,
    /// The shift to add to the moving start so it snaps (`|delta| <= tolerance`).
    pub delta: Seconds,
}

/// Target times this close are the same snap line.
const SNAP_TIME_EPS: Seconds = 1e-6;

impl Stack {
    /// Find the clip edge (or extra point) closest to either edge of the moving
    /// range `[moving_start, moving_start + moving_duration]`, within
    /// `tolerance` seconds.
    ///
    /// Snapping is a query only: the caller decides when it is enabled and
    /// converts its on-screen distance into `tolerance`, then moves to
    /// `moving_start + delta`. Pass `moving_duration = 0` to snap a single edge
    /// (a resize handle); the match then reports `SnapEdge::Start`.
    ///
    /// Clips in `exclude_item_ids` never act as targets, and neither do their
    /// sync partners or Tellers group members, which travel with them. Gaps
    /// are never targets. Targets at any of `ignored_times` are skipped too,
    /// so an editor can let a line it has already been pulled off pass by
    /// while still snapping to the next one. A snap that would move the range
    /// before 0 is skipped.
    pub fn find_snap(
        &self,
        moving_start: Seconds,
        moving_duration: Seconds,
        tolerance: Seconds,
        exclude_item_ids: &[String],
        extra_points: &[Seconds],
        ignored_times: &[Seconds],
    ) -> Option<SnapMatch> {
        if tolerance.is_nan()
            || tolerance <= 0.0
            || !moving_start.is_finite()
            || !moving_duration.is_finite()
        {
            return None;
        }
        let moving_duration = moving_duration.max(0.0);
        let moving_end = moving_start + moving_duration;
        let excluded = self.snap_excluded_items(exclude_item_ids);

        let mut best: Option<SnapMatch> = None;
        let mut consider = |target: SnapTarget, target_time: Seconds| {
            if ignored_times
                .iter()
                .any(|ignored| (ignored - target_time).abs() <= SNAP_TIME_EPS)
            {
                return;
            }
            let mut edges = vec![(SnapEdge::Start, moving_start)];
            if moving_duration > 0.0 {
                edges.push((SnapEdge::End, moving_end));
            }
            for (moving_edge, moving_time) in edges {
                let delta = target_time - moving_time;
                if delta.abs() > tolerance || moving_start + delta < -super::EPS {
                    continue;
                }
                if best.as_ref().is_some_and(|b| b.delta.abs() <= delta.abs()) {
                    continue;
                }
                best = Some(SnapMatch {
                    target: target.clone(),
                    target_time,
                    moving_edge,
                    delta,
                });
            }
        };

        for (track_index, track) in self.children.iter().enumerate() {
            let track_id = track.get_id();
            let mut cursor = 0.0;
            for (item_index, item) in track.items.iter().enumerate() {
                let start = cursor;
                cursor += item.duration();
                if matches!(item, Item::Gap(_)) || excluded.contains(&(track_index, item_index)) {
                    continue;
                }
                for (edge, time) in [(SnapEdge::Start, start), (SnapEdge::End, cursor)] {
                    consider(
                        SnapTarget::ItemEdge {
                            item_id: item.get_id(),
                            track_id: track_id.clone(),
                            track_index,
                            edge,
                        },
                        time,
                    );
                }
            }
        }
        for (index, &time) in extra_points.iter().enumerate() {
            if time.is_finite() {
                consider(SnapTarget::Point { index }, time);
            }
        }
        best
    }

    /// The excluded clips plus everything that moves with them: their sync
    /// (link group) partners and Tellers group members.
    /// Returned as `(track_index, item_index)` pairs.
    fn snap_excluded_items(&self, exclude_item_ids: &[String]) -> HashSet<(usize, usize)> {
        let ids: HashSet<&str> = exclude_item_ids.iter().map(String::as_str).collect();
        let mut link_groups = HashSet::new();
        let mut tellers_groups = HashSet::new();
        for id in &ids {
            if let Some((_, _, item)) = self.get_item(id) {
                link_groups.extend(item_link_group_id(item));
                tellers_groups.extend(item_tellers_group_id(item));
            }
        }
        let mut excluded = HashSet::new();
        for (track_index, track) in self.children.iter().enumerate() {
            for (item_index, item) in track.items.iter().enumerate() {
                if item.get_id().is_some_and(|id| ids.contains(id.as_str()))
                    || item_link_group_id(item).is_some_and(|g| link_groups.contains(&g))
                    || item_tellers_group_id(item).is_some_and(|g| tellers_groups.contains(&g))
                {
                    excluded.insert((track_index, item_index));
                }
            }
        }
        excluded
    }
}
