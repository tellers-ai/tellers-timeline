mod common;
use common::*;

#[test]
fn move_unsynced_clip_between_video_tracks_in_cluster_preserves_track_and_cluster_count() {
    let mut stack = Stack::default();

    for id in ["A1", "A2", "A3"] {
        let mut audio = Track::new(TrackKind::Audio, Some(id.to_string()));
        audio.items.push(synced_clip_item_with_rate(
            62.88,
            0.0,
            &format!("{id}-g3"),
            3,
            25.0,
        ));
        stack.children.push(audio);
    }

    let mut primary_video = Track::new(TrackKind::Video, Some("video".to_string()));
    primary_video.items.push(synced_clip_item_with_rate(62.88, 0.0, "v-g3", 3, 25.0));
    stack.children.push(primary_video);

    let mut secondary_video = Track::new(TrackKind::Video, Some("video-2".to_string()));
    secondary_video.items.push(Item::Gap(Gap::make_gap(10.0)));
    stack.children.push(secondary_video);

    let primary_video_index = stack
        .get_track_by_id("video")
        .map(|(index, _)| index)
        .unwrap();
    let secondary_video_index = stack
        .get_track_by_id("video-2")
        .map(|(index, _)| index)
        .unwrap();

    stack
        .insert_item_at_time(
            primary_video_index,
            2.0,
            Item::Clip(clip(1.5, Some("inserted-at-2"))),
            OverlapPolicy::Override,
            InsertPolicy::SplitAndInsert,
            None,
            None,
        )
        .expect("insert should succeed");

    let track_count = stack.children.len();
    let clusters_before = stack.sync_track_info();

    assert!(stack.move_item_at_time(
        "inserted-at-2",
        "video-2",
        4.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    assert_eq!(stack.children.len(), track_count);
    assert_sync_track_info_unchanged(&clusters_before, &stack.sync_track_info());

    let (moved_track, moved_index, moved_item) = stack.get_item("inserted-at-2").unwrap();
    assert_eq!(moved_track, secondary_video_index);
    assert_eq!(
        stack.children[moved_track].start_time_of_item(moved_index),
        4.0,
    );
    assert_eq!(moved_item.duration(), 1.5);

    for id in ["A1", "A2", "A3"] {
        let audio_track_index = stack
            .get_track_by_id(id)
            .map(|(index, _)| index)
            .unwrap();
        let audio_track = &stack.children[audio_track_index];
        let spacer_index = audio_track.get_item_at_time(2.0).unwrap();
        assert!(matches!(audio_track.items[spacer_index], Item::Gap(_)));
        assert_eq!(audio_track.items[spacer_index].duration(), 1.5);
    }
}

#[test]
fn reorder_track_moves_only_the_selected_track() {
    let mut stack = Stack::default();
    stack
        .children
        .push(Track::new(TrackKind::Video, Some("linked-v".to_string())));
    insert_with_audio(
        &mut stack,
        0,
        0.0,
        clip(4.0, Some("linked-video")),
        vec![audio_clip(4.0, "file:///linked-audio.wav", None)],
    )
    .expect("linked insert should succeed");

    let mut unsynced_audio = Track::new(TrackKind::Audio, Some("unlinked-a".to_string()));
    unsynced_audio
        .items
        .push(audio_clip(4.0, "file:///unlinked-audio.wav", None));
    stack.children.push(unsynced_audio);
    let mut unsynced_video = Track::new(TrackKind::Video, Some("unlinked-v".to_string()));
    unsynced_video
        .items
        .push(Item::Clip(clip(4.0, Some("unlinked-video"))));
    stack.children.push(unsynced_video);

    // Layout: [A1 (0), linked-v (1), unlinked-a (2), unlinked-v (3)]
    assert!(stack.reorder_track("linked-v", 4));
    assert_eq!(stack.children[0].get_id().as_deref(), Some("A1"));
    assert_eq!(stack.children[1].get_id().as_deref(), Some("unlinked-a"));
    assert_eq!(stack.children[2].get_id().as_deref(), Some("unlinked-v"));
    assert_eq!(stack.children[3].get_id().as_deref(), Some("linked-v"));

    assert!(stack.reorder_track("A1", 2));
    assert_eq!(stack.children[0].get_id().as_deref(), Some("unlinked-a"));
    assert_eq!(stack.children[1].get_id().as_deref(), Some("A1"));
    assert_eq!(stack.children[2].get_id().as_deref(), Some("unlinked-v"));
    assert_eq!(stack.children[3].get_id().as_deref(), Some("linked-v"));
}

#[test]
fn move_unsynced_item_only_moves_selected_item() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(3.0, Some("primary"))));
    let mut audio = Track::new(TrackKind::Audio, Some("a".to_string()));
    audio
        .items
        .push(Item::Clip(clip(3.0, Some("unlinked-audio"))));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(audio);

    assert!(stack.move_item_at_time(
        "primary",
        "a",
        3.0,
        true,
        InsertPolicy::InsertAfter,
        OverlapPolicy::Override,
    ));

    assert_eq!(stack.get_item("primary").unwrap().0, 1);
    assert!(stack.get_item("unlinked-audio").is_some());
}

#[test]
fn move_unsynced_item_without_gap_pulls_later_synced_assets() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(1.0, Some("unlinked"))));
    let audio = Track::new(TrackKind::Audio, Some("a".to_string()));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(audio);
    let result = insert_with_audio(
        &mut stack,
        0,
        1.0,
        clip(2.0, Some("linked-video")),
        vec![audio_clip(2.0, "file:///linked-audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();
    let video_track_id = stack
        .children
        .iter()
        .find(|track| track.get_id().as_deref() == Some("v"))
        .unwrap()
        .get_id()
        .unwrap();

    assert!(stack.move_item_at_time(
        "unlinked",
        &video_track_id,
        2.0,
        false,
        InsertPolicy::InsertBeforeOrAfter,
        OverlapPolicy::Push,
    ));

    let (video_track_index, video_item_index, _) = stack.get_item("linked-video").unwrap();
    let (audio_track_index, audio_item_index, _) = stack.get_item(&audio_id).unwrap();
    // The synced group is pulled back so the video and audio members stay aligned.
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        stack.children[audio_track_index].start_time_of_item(audio_item_index)
    );
    assert_eq!(stack.children[video_track_index].items.len(), 2);
    assert!(stack.children[video_track_index]
        .items
        .iter()
        .all(|item| matches!(item, Item::Clip(_))));
    assert_eq!(
        stack.children[video_track_index].timeline_ids(),
        vec!["linked-video", "unlinked"]
    );
    // The audio track holds only the synced audio clip; the "unlinked" clip is video
    // only, and there is no trailing gap on the audio track (it would be trimmed).
    assert_eq!(stack.children[audio_track_index].items.len(), 1);
    assert!(matches!(
        stack.children[audio_track_index].items[audio_item_index],
        Item::Clip(_)
    ));
}

#[test]
fn move_unsynced_item_with_gap_and_split_target_updates_synced_assets() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(1.0, Some("unlinked"))));
    let audio = Track::new(TrackKind::Audio, Some("a".to_string()));

    let mut stack = Stack::default();
    stack.children.push(video);
    stack.children.push(audio);
    let result = insert_with_audio(
        &mut stack,
        0,
        1.0,
        clip(2.0, Some("linked-video")),
        vec![audio_clip(2.0, "file:///linked-audio.wav", None)],
    )
    .unwrap();
    let audio_id = result.audio_clips[0].0.clone();

    assert!(stack.move_item_at_time(
        "unlinked",
        "v",
        2.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Push,
    ));

    let video_track_index = stack.get_track_by_id("v").unwrap().0;
    let audio_track_index = stack.get_item(&audio_id).unwrap().0;
    let video_items = &stack.children[video_track_index].items;
    let audio_items = &stack.children[audio_track_index].items;

    assert_eq!(video_items.len(), 4);
    assert!(matches!(video_items[0], Item::Gap(_)));
    assert_eq!(video_items[0].duration(), 1.0);
    assert_eq!(sync_clips_id(&video_items[1]), result.sync_clips_id);
    assert_eq!(video_items[1].duration(), 1.0);
    assert_eq!(video_items[2].get_id().as_deref(), Some("unlinked"));
    assert_eq!(video_items[2].duration(), 1.0);
    assert_eq!(sync_clips_id(&video_items[3]), result.sync_clips_id);
    assert_eq!(video_items[3].duration(), 1.0);

    assert_eq!(audio_items.len(), 4);
    assert!(matches!(audio_items[0], Item::Gap(_)));
    assert_eq!(audio_items[0].duration(), 1.0);
    assert_eq!(sync_clips_id(&audio_items[1]), result.sync_clips_id);
    assert_eq!(audio_items[1].duration(), 1.0);
    assert!(matches!(audio_items[2], Item::Gap(_)));
    assert_eq!(audio_items[2].duration(), 1.0);
    assert_eq!(sync_clips_id(&audio_items[3]), result.sync_clips_id);
    assert_eq!(audio_items[3].duration(), 1.0);
}

#[test]
fn move_unsynced_item_pushes_full_synced_boundary_with_gap_sync_tracks() {
    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Clip(clip(1.0, Some("unlinked"))));

    let mut stack = Stack::default();
    stack.children.push(video);
    let result = insert_with_audio(
        &mut stack,
        0,
        1.0,
        clip(2.0, Some("linked-video")),
        vec![
            audio_clip(2.0, "file:///linked-a1.wav", None),
            audio_clip(2.0, "file:///linked-a2.wav", None),
        ],
    )
    .unwrap();
    let audio_ids: Vec<_> = result
        .audio_clips
        .iter()
        .map(|(id, _)| id.clone())
        .collect();

    assert!(stack.move_item_at_time(
        "unlinked",
        "v",
        0.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Push,
    ));

    let (video_track_index, video_item_index, moved_item) = stack.get_item("unlinked").unwrap();
    assert_eq!(
        stack.children[video_track_index].start_time_of_item(video_item_index),
        0.0
    );
    assert_eq!(moved_item.duration(), 1.0);

    let (synced_video_track_index, synced_video_item_index, synced_video) =
        stack.get_item("linked-video").unwrap();
    assert_eq!(
        stack.children[synced_video_track_index].start_time_of_item(synced_video_item_index),
        2.0
    );
    assert_eq!(sync_clips_id(synced_video), result.sync_clips_id);

    for audio_id in &audio_ids {
        let (audio_track_index, audio_item_index, audio_item) = stack.get_item(audio_id).unwrap();
        assert_eq!(
            stack.children[audio_track_index].start_time_of_item(audio_item_index),
            2.0
        );
        assert_eq!(sync_clips_id(audio_item), result.sync_clips_id);
        // The synced audio is aligned to start 2.0 behind a single leading gap; the
        // dedicated per-boundary gap track is gone, so the leading gap simply spans
        // 0..2 to line the audio up with the video's synced clip.
        let spacer_index = stack.children[audio_track_index].get_item_at_time(0.5).unwrap();
        assert!(matches!(
            stack.children[audio_track_index].items[spacer_index],
            Item::Gap(_)
        ));
        assert_eq!(
            stack.children[audio_track_index].items[spacer_index].duration(),
            2.0
        );
    }
}

#[test]
fn move_unsynced_item_pushes_full_synced_boundary_for_insert_policies() {
    let cases = [
        (InsertPolicy::SplitAndInsert, 0.0, 0.0, 2.0),
        (InsertPolicy::InsertBefore, 1.5, 1.0, 2.0),
        (InsertPolicy::InsertAfter, 0.5, 1.0, 2.0),
        (InsertPolicy::InsertBeforeOrAfter, 1.2, 1.0, 2.0),
    ];

    for (insert_policy, dest_time, moved_start, synced_start) in cases {
        let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
        video.items.push(Item::Clip(clip(1.0, Some("unlinked"))));

        let mut stack = Stack::default();
        stack.children.push(video);
        let result = insert_with_audio(
            &mut stack,
            0,
            1.0,
            clip(2.0, Some("linked-video")),
            vec![
                audio_clip(2.0, "file:///linked-a1.wav", None),
                audio_clip(2.0, "file:///linked-a2.wav", None),
            ],
        )
        .unwrap();
        let audio_ids: Vec<_> = result
            .audio_clips
            .iter()
            .map(|(id, _)| id.clone())
            .collect();

        assert!(stack.move_item_at_time(
            "unlinked",
            "v",
            dest_time,
            true,
            insert_policy,
            OverlapPolicy::Push,
        ));

        let (moved_track_index, moved_item_index, moved_item) =
            stack.get_item("unlinked").unwrap();
        assert_eq!(
            stack.children[moved_track_index].start_time_of_item(moved_item_index),
            moved_start,
            "moved start for {insert_policy:?}"
        );
        assert_eq!(moved_item.duration(), 1.0);

        let (synced_video_track_index, synced_video_item_index, synced_video) =
            stack.get_item("linked-video").unwrap();
        assert_eq!(
            stack.children[synced_video_track_index].start_time_of_item(synced_video_item_index),
            synced_start,
            "linked video start for {insert_policy:?}"
        );
        assert_eq!(sync_clips_id(synced_video), result.sync_clips_id);

        for audio_id in &audio_ids {
            let (audio_track_index, audio_item_index, audio_item) =
                stack.get_item(audio_id).unwrap();
            let audio_track = &stack.children[audio_track_index];
            assert_eq!(
                audio_track.start_time_of_item(audio_item_index),
                synced_start,
                "linked audio start for {insert_policy:?}"
            );
            assert_eq!(sync_clips_id(audio_item), result.sync_clips_id);
            assert!(
                range_is_gap_backed_for_test(audio_track, moved_start, moved_start + 1.0),
                "missing gap sync track for {insert_policy:?}"
            );
        }
    }
}

#[test]
fn move_unsynced_image_into_synced_clip_override_assigns_new_link_group_to_right() {
    const SYNC_ID: i64 = 2;
    const SYNC_DUR: f64 = 42.0;
    const IMAGE_DUR: f64 = 5.0;
    const LEAD: f64 = 5.0;

    let mut audio = Track::new(TrackKind::Audio, Some("A3".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(LEAD)));
    audio.items.push(synced_clip_item(SYNC_DUR, "sync-audio", SYNC_ID));

    let mut video = Track::new(TrackKind::Video, Some("v".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(LEAD)));
    video.items.push(synced_clip_item(SYNC_DUR, "sync-video", SYNC_ID));
    video
        .items
        .push(Item::Clip(clip(IMAGE_DUR, Some("screenshot"))));

    let mut stack = Stack::default();
    stack.children.push(audio);
    stack.children.push(video);

    assert!(stack.move_item_at_time(
        "screenshot",
        "v",
        20.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    assert_eq!(
        sync_clips_id(stack.get_item("sync-video").unwrap().2),
        Some(SYNC_ID)
    );

    let (_, video_track) = stack.get_track_by_id("v").unwrap();
    let sync_group_ids: Vec<_> = video_track
        .items
        .iter()
        .filter_map(sync_clips_id)
        .collect();
    assert_eq!(sync_group_ids, vec![SYNC_ID, SYNC_ID + 1]);
    assert_eq!(
        sync_clips_id(stack.get_item("screenshot").unwrap().2),
        None
    );

    let (_, audio_track) = stack.get_track_by_id("A3").unwrap();
    let audio_sync_group_ids: Vec<_> = audio_track
        .items
        .iter()
        .filter_map(sync_clips_id)
        .collect();
    assert_eq!(audio_sync_group_ids, vec![SYNC_ID, SYNC_ID + 1]);
}

#[test]
fn move_unsynced_photo_onto_main_video_does_not_split_unrelated_a9_a11_sync_cluster() {
    // Minimal repro of New Project (4)->(5): unsynced photo 987c30ab0258 moved onto
    // Main Video at ~7s must split HH10 (group 3) on the destination cluster only,
    // not the unrelated Video 2 / A9-A11 cluster (group 5).
    const GROUP3: i64 = 3;
    const GROUP5: i64 = 5;
    const LEAD: f64 = 5.0;
    const HH10_DUR: f64 = 5.84;
    const PHOTO_DUR: f64 = 5.0;
    const FAR_DUR: f64 = 12.08;
    const GAP_BEFORE_PHOTO: f64 = 3.4;

    let mut stack = Stack::default();

    let mut a1 = Track::new(TrackKind::Audio, Some("A1".to_string()));
    a1.items.push(Item::Gap(Gap::make_gap(LEAD)));
    a1.items.push(synced_clip_item(HH10_DUR, "hh10-a1", GROUP3));
    stack.children.push(a1);

    let mut main_video = Track::new(TrackKind::Video, Some("Main Video".to_string()));
    main_video.items.push(Item::Gap(Gap::make_gap(LEAD)));
    main_video.items.push(synced_clip_item(HH10_DUR, "hh10-v", GROUP3));
    main_video
        .items
        .push(Item::Gap(Gap::make_gap(GAP_BEFORE_PHOTO)));
    main_video
        .items
        .push(Item::Clip(clip(PHOTO_DUR, Some("987c30ab0258"))));
    stack.children.push(main_video);

    for id in ["A9", "A10", "A11"] {
        let mut audio = Track::new(TrackKind::Audio, Some(id.to_string()));
        audio
            .items
            .push(synced_clip_item(FAR_DUR, &format!("{id}-g5"), GROUP5));
        stack.children.push(audio);
    }
    let mut video2 = Track::new(TrackKind::Video, Some("Video 2".to_string()));
    video2.items.push(synced_clip_item(FAR_DUR, "v2-g5", GROUP5));
    stack.children.push(video2);

    assert!(stack.move_item_at_time(
        "987c30ab0258",
        "Main Video",
        7.0,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    let (_, _, hh10_v) = stack.get_item("hh10-v").unwrap();
    assert!(hh10_v.duration() < HH10_DUR);

    let (photo_track, photo_index, _) = stack.get_item("987c30ab0258").unwrap();
    let photo_start = stack.children[photo_track].start_time_of_item(photo_index);
    assert!((photo_start - 7.0).abs() < 0.05);

    for track_id in ["A9", "A10", "A11", "Video 2"] {
        let (_, track) = stack.get_track_by_id(track_id).unwrap();
        assert_eq!(
            track.items.len(),
            1,
            "{track_id} must stay unsplit when the insert target is Main Video"
        );
        assert_eq!(sync_clips_id(&track.items[0]), Some(GROUP5));
        assert!((track.items[0].duration() - FAR_DUR).abs() < 1e-6);
    }
}

#[test]
fn space_talking_cat_move_paris_onto_hh10_does_not_split_a1_a7() {
    let otio = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/space_talking_cat.otio"),
    )
    .expect("fixture exists");
    let mut timeline: Timeline = serde_json::from_str(&otio).expect("parse otio");
    timeline.tracks.sanitize();

    // paris clip on Main (link group 4); HH10 on A1-A7 is unsynced (no link group).
    let hh10_a1_dur_before = timeline
        .tracks
        .get_item("1e659f868531")
        .unwrap()
        .2
        .duration();
    let a1_items_before = timeline
        .tracks
        .get_track_by_id("A1")
        .unwrap()
        .1
        .items
        .len();

    // Move paris onto HH10 start on Main (~16.84s).
    assert!(timeline.tracks.move_item_at_time(
        "1acd33f0570a",
        "23d9c4f632ed",
        16.84,
        true,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Override,
    ));

    let (_, a1_track) = timeline.tracks.get_track_by_id("A1").unwrap();
    assert_eq!(
        a1_track.items.len(),
        a1_items_before,
        "A1 HH10 must not be split when moving paris on Main"
    );
    let (_, _, hh10_a1_after) = timeline.tracks.get_item("1e659f868531").unwrap();
    assert!(
        (hh10_a1_after.duration() - hh10_a1_dur_before).abs() < 1e-6,
        "A1 HH10 duration must be unchanged"
    );

    for track_id in ["A2", "A3", "A4", "A5", "A6", "A7"] {
        let (_, track) = timeline.tracks.get_track_by_id(track_id).unwrap();
        assert_eq!(
            track.items.len(),
            2,
            "{track_id} must stay gap+clip when moving paris on Main"
        );
    }
}

#[test]
fn move_unsynced_earlier_into_leading_gap_does_not_add_spurious_sync_padding() {
    const LEAD: f64 = 5.0;
    const SYNC_DUR: f64 = 2.0;
    const GROUP: i64 = 1;

    let mut stack = Stack::default();

    let mut audio = Track::new(TrackKind::Audio, Some("A1".to_string()));
    audio.items.push(Item::Gap(Gap::make_gap(LEAD)));
    audio
        .items
        .push(synced_clip_item(SYNC_DUR, "sync-a", GROUP));
    stack.children.push(audio);

    let mut video = Track::new(TrackKind::Video, Some("Main".to_string()));
    video.items.push(Item::Gap(Gap::make_gap(LEAD)));
    video
        .items
        .push(synced_clip_item(SYNC_DUR, "sync-v", GROUP));
    video
        .items
        .push(Item::Clip(clip(1.0, Some("unsynced"))));
    stack.children.push(video);

    // Photo starts at LEAD + SYNC_DUR = 7. Move earlier into the leading gap at 2.
    assert!(stack.move_item_at_time(
        "unsynced",
        "Main",
        2.0,
        false,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Push,
    ));

    let (sync_v_track, sync_v_index, sync_v) = stack.get_item("sync-v").unwrap();
    let sync_v_start = stack.children[sync_v_track].start_time_of_item(sync_v_index);
    assert!(
        (sync_v_start - LEAD).abs() < 1e-6,
        "synced video should stay at {LEAD}, got {sync_v_start}"
    );
    assert_eq!(sync_v.duration(), SYNC_DUR);

    let (sync_a_track, sync_a_index, sync_a) = stack.get_item("sync-a").unwrap();
    let audio_track = &stack.children[sync_a_track];
    assert_eq!(
        audio_track.start_time_of_item(sync_a_index),
        sync_v_start,
        "synced audio should stay aligned with video"
    );
    assert_eq!(sync_a.duration(), SYNC_DUR);
    assert_eq!(
        audio_track.items.len(),
        2,
        "audio must not gain a spurious gap when synced clip was not pushed"
    );
    assert!(
        matches!(audio_track.items[0], Item::Gap(_)),
        "leading cluster gap should remain"
    );
    assert!(
        matches!(audio_track.items[1], Item::Clip(_)),
        "synced audio clip should remain directly after the leading gap"
    );
}

#[test]
fn move_unsynced_backward_onto_synced_clip_does_not_clobber_partner_unsynced_clip() {
    // Repro for the reported bug:
    //   v1: C1[0-2] (sync) - gap[2-4] - c2[4-6] (unsynced)
    //   a1: CA1[0-2] (sync) - gap[2-2.5] - c3[2.5-4.5] (unsynced)
    // C1 and CA1 are synced. Moving c2 backward onto C1 (dest=1) flips Push->Override
    // and splits C1. The cluster propagation reserves an Override gap of c2's full
    // duration on the audio partner at the insert point -- which must NOT delete or
    // shrink the unrelated unsynced c3 sitting on the audio track.
    const GROUP: i64 = 1;
    let mut stack = Stack::default();

    let mut video = Track::new(TrackKind::Video, Some("v1".to_string()));
    video.items.push(synced_clip_item(2.0, "C1", GROUP));
    video.items.push(Item::Gap(Gap::make_gap(2.0)));
    video.items.push(Item::Clip(clip(2.0, Some("c2"))));
    stack.children.push(video);

    let mut audio = Track::new(TrackKind::Audio, Some("a1".to_string()));
    audio.items.push(synced_clip_item(2.0, "CA1", GROUP));
    audio.items.push(Item::Gap(Gap::make_gap(0.5)));
    audio.items.push(Item::Clip(clip(2.0, Some("c3"))));
    stack.children.push(audio);

    assert!(stack.move_item_at_time(
        "c2",
        "v1",
        1.0,
        false,
        InsertPolicy::SplitAndInsert,
        OverlapPolicy::Push,
    ));

    // The unrelated unsynced c3 must be completely untouched: same start, same duration.
    let (c3_track, c3_index, c3) = stack
        .get_item("c3")
        .expect("c3 must still exist after moving the unrelated unsynced c2");
    assert_eq!(c3.duration(), 2.0, "c3 duration must not shrink");
    assert_eq!(
        stack.children[c3_track].start_time_of_item(c3_index),
        2.5,
        "c3 start must not move"
    );

    // The synced audio partner CA1 is trimmed to stay aligned with C1 on video: C1 was
    // overwritten down to [0-1], so CA1 keeps only its [0-1] footprint.
    let (ca1_track, ca1_index, ca1) = stack.get_item("CA1").expect("CA1 must still exist");
    assert_eq!(
        stack.children[ca1_track].start_time_of_item(ca1_index),
        0.0,
        "CA1 should still start at 0"
    );
    assert_eq!(ca1.duration(), 1.0, "CA1 should be trimmed to align with C1");

    let (c1_track, c1_index, c1) = stack.get_item("C1").expect("C1 must still exist");
    assert_eq!(
        stack.children[c1_track].start_time_of_item(c1_index),
        0.0,
        "C1 should still start at 0"
    );
    assert_eq!(c1.duration(), 1.0, "C1 was overwritten down to [0-1]");
}
