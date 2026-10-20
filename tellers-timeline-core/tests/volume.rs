use tellers_timeline_core::{
    upgrade_legacy_resolve_volume_effects, Animatable, Clip, Keyframe, TellersEffect, Timeline, Track,
};

fn clip_json() -> &'static str {
    r#"{
        "OTIO_SCHEMA": "Clip.2",
        "source_range": {
            "OTIO_SCHEMA": "TimeRange.1",
            "start_time": { "OTIO_SCHEMA": "RationalTime.1", "rate": 1, "value": 0 },
            "duration": { "OTIO_SCHEMA": "RationalTime.1", "rate": 1, "value": 2 }
        }
    }"#
}

#[test]
fn track_set_volume_applies_to_every_clip() {
    let track_json = format!(
        r#"{{
            "OTIO_SCHEMA": "Track.1",
            "kind": "Audio",
            "children": [{c}, {c}]
        }}"#,
        c = clip_json(),
    );
    let mut track: Track = serde_json::from_str(&track_json).unwrap();

    // Clips start at the default volume.
    for item in &track.items {
        assert_eq!(item.get_volume(), 1.0);
    }

    track.set_volume(0.25);

    // Every item now reflects the track volume.
    for item in &track.items {
        assert_eq!(item.get_volume(), 0.25);
    }
}

fn resolve_volume(db: f64) -> serde_json::Value {
    serde_json::json!({
        "OTIO_SCHEMA": "Effect.1",
        "name": "",
        "effect_name": "Resolve Effect",
        "metadata": { "Resolve_OTIO": {
            "Effect Name": "Fairlight Clip Volume and Fades",
            "Enabled": true,
            "Name": "Volume",
            "Parameters": [{
                "Default Parameter Value": 0.0,
                "Parameter ID": "volume",
                "Parameter Value": db,
                "Variant Type": "Double",
                "maxValue": 30.0,
                "minValue": -100.0
            }],
            "Type": 62
        } }
    })
}

fn clip_with(effects: serde_json::Value) -> Clip {
    let mut clip: serde_json::Value = serde_json::from_str(clip_json()).unwrap();
    clip["effects"] = effects;
    serde_json::from_value(clip).unwrap()
}

fn effect_names(clip: &Clip) -> Vec<String> {
    clip.effects.iter().map(|e| e.effect_name.clone()).collect()
}

#[test]
fn set_volume_writes_tellers_volume_and_drops_resolve() {
    let mut clip = clip_with(serde_json::json!([resolve_volume(-6.0)]));
    assert_eq!(clip.get_volume(), -6.0);

    clip.set_volume(-3.0);

    assert_eq!(effect_names(&clip), vec!["tellers.ai.Volume"]);
    let (_, volume) = clip.find_tellers_effect("Volume").unwrap();
    assert_eq!(volume.value_at("volume", 0.0), Some(-3.0));
    assert_eq!(clip.get_volume(), -3.0);
}

#[test]
fn set_volume_keeps_fades() {
    let mut clip = clip_with(serde_json::json!([]));
    clip.push_tellers_effect(TellersEffect::new("Volume").with_parameter("fadeIn", 0.5));
    clip.set_volume(-2.0);
    let (_, volume) = clip.find_tellers_effect("Volume").unwrap();
    assert_eq!(volume.value_at("fadeIn", 0.0), Some(0.5));
    assert_eq!(volume.value_at("volume", 0.0), Some(-2.0));
}

#[test]
fn legacy_resolve_volume_is_still_read() {
    let clip = clip_with(serde_json::json!([resolve_volume(-12.0)]));
    assert_eq!(clip.get_volume(), -12.0);
}

#[test]
fn a_tellers_volume_overrides_the_resolve_one() {
    let mut clip = clip_with(serde_json::json!([resolve_volume(-6.0)]));
    let ramp = Animatable::from(vec![Keyframe::new(0.0, -2.0), Keyframe::new(1.0, -10.0)]);
    clip.push_tellers_effect(TellersEffect::new("Volume").with_parameter("volume", ramp.to_value()));
    assert_eq!(clip.get_volume(), -2.0);

    assert_eq!(upgrade_legacy_resolve_volume_effects(&mut clip.effects), 1);

    assert_eq!(effect_names(&clip), vec!["tellers.ai.Volume"]);
    let (_, volume) = clip.find_tellers_effect("Volume").unwrap();
    assert_eq!(volume.animatable("volume"), Some(ramp));
    // Already migrated: nothing left to do.
    assert_eq!(upgrade_legacy_resolve_volume_effects(&mut clip.effects), 0);
}

#[test]
fn upgrade_moves_resolve_db_into_a_fades_only_tellers_volume() {
    let mut clip = clip_with(serde_json::json!([resolve_volume(-6.0)]));
    clip.push_tellers_effect(TellersEffect::new("Volume").with_parameter("fadeIn", 0.5));
    assert_eq!(clip.get_volume(), -6.0);

    assert_eq!(upgrade_legacy_resolve_volume_effects(&mut clip.effects), 1);

    let (_, volume) = clip.find_tellers_effect("Volume").unwrap();
    assert_eq!(volume.value_at("volume", 0.0), Some(-6.0));
    assert_eq!(volume.value_at("fadeIn", 0.0), Some(0.5));
    assert_eq!(clip.get_volume(), -6.0);
}

#[test]
fn upgrade_drops_disabled_and_unity_resolve_volume_without_adding_one() {
    let mut disabled = resolve_volume(-6.0);
    disabled["metadata"]["Resolve_OTIO"]["Enabled"] = false.into();
    let mut clip = clip_with(serde_json::json!([disabled, resolve_volume(0.0)]));

    assert_eq!(upgrade_legacy_resolve_volume_effects(&mut clip.effects), 2);
    assert!(clip.effects.is_empty());
}

#[test]
fn timeline_upgrade_reaches_composite_clips_and_sanitize_runs_it() {
    let inner = format!(
        r#"{{ "OTIO_SCHEMA": "Track.1", "kind": "Audio", "children": [{}] }}"#,
        serde_json::to_string(&clip_with(serde_json::json!([resolve_volume(-4.0)]))).unwrap(),
    );
    let json = format!(
        r#"{{
            "OTIO_SCHEMA": "Timeline.1",
            "tracks": {{ "OTIO_SCHEMA": "Stack.1", "children": [{{
                "OTIO_SCHEMA": "Track.1", "kind": "Audio", "children": [
                    {clip},
                    {{ "OTIO_SCHEMA": "Stack.1", "children": [{inner}] }}
                ]
            }}] }}
        }}"#,
        clip = serde_json::to_string(&clip_with(serde_json::json!([resolve_volume(-1.0)]))).unwrap(),
    );
    let original: Timeline = serde_json::from_str(&json).unwrap();

    let mut timeline = original.clone();
    assert_eq!(timeline.upgrade_legacy_resolve_volumes(), 2);
    assert!(!serde_json::to_string(&timeline).unwrap().contains("Fairlight"));
    assert_eq!(timeline.upgrade_legacy_resolve_volumes(), 0);

    let mut sanitized = original;
    sanitized.sanitize();
    assert!(!serde_json::to_string(&sanitized).unwrap().contains("Fairlight"));
}
