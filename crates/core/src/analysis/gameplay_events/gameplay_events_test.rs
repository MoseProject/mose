use super::*;
use crate::timeline::Timeline;

fn empty_timeline() -> Timeline {
    Timeline {
        tracks: vec![],
        playhead_secs: 0.0,
        markers: vec![],
        multicam_groups: vec![],
    }
}

fn sample_sidecar() -> EventSidecar {
    EventSidecar {
        schema_version: CURRENT_SCHEMA_VERSION,
        source_media_filename: "2026-08-29_ranked.mp4".to_string(),
        game_id: None,
        events: vec![
            GameplayEvent {
                kind: GameplayEventKind::Kill,
                source_timestamp_secs: 12.5,
                confidence: 0.9,
                pre_roll_secs: None,
                post_roll_secs: None,
            },
            GameplayEvent {
                kind: GameplayEventKind::Death,
                source_timestamp_secs: 200.0,
                confidence: 1.0,
                pre_roll_secs: Some(3.0),
                post_roll_secs: Some(1.0),
            },
        ],
    }
}

#[test]
fn round_trips_through_json() {
    let sidecar = sample_sidecar();
    let json = serde_json::to_string(&sidecar).unwrap();
    let parsed = EventSidecar::parse_and_validate(&json).unwrap();
    assert_eq!(parsed, sidecar);
}

#[test]
fn rejects_an_unsupported_schema_version() {
    let mut sidecar = sample_sidecar();
    sidecar.schema_version = 999;
    assert_eq!(
        sidecar.validate(),
        Err(SidecarValidationError::UnsupportedSchemaVersion { found: 999 })
    );
}

#[test]
fn rejects_an_empty_source_media_filename() {
    let mut sidecar = sample_sidecar();
    sidecar.source_media_filename = String::new();
    assert!(matches!(
        sidecar.validate(),
        Err(SidecarValidationError::InvalidSourceMediaFilename(_))
    ));
}

#[test]
fn rejects_a_source_media_filename_containing_a_path_separator() {
    for bad in [
        "C:\\recordings\\clip.mp4",
        "/home/user/clip.mp4",
        "sub/clip.mp4",
    ] {
        let mut sidecar = sample_sidecar();
        sidecar.source_media_filename = bad.to_string();
        assert!(
            matches!(
                sidecar.validate(),
                Err(SidecarValidationError::InvalidSourceMediaFilename(_))
            ),
            "expected {bad:?} to be rejected as non-portable"
        );
    }
}

#[test]
fn rejects_a_negative_or_non_finite_timestamp() {
    for bad in [-1.0, f64::NAN, f64::INFINITY] {
        let mut sidecar = sample_sidecar();
        sidecar.events[0].source_timestamp_secs = bad;
        assert!(matches!(
            sidecar.validate(),
            Err(SidecarValidationError::InvalidTimestamp { index: 0, .. })
        ));
    }
}

#[test]
fn rejects_confidence_outside_zero_to_one() {
    for bad in [-0.1, 1.1] {
        let mut sidecar = sample_sidecar();
        sidecar.events[0].confidence = bad;
        assert!(matches!(
            sidecar.validate(),
            Err(SidecarValidationError::InvalidConfidence { index: 0, .. })
        ));
    }
}

#[test]
fn rejects_a_negative_pre_or_post_roll() {
    let mut sidecar = sample_sidecar();
    sidecar.events[0].pre_roll_secs = Some(-1.0);
    assert!(matches!(
        sidecar.validate(),
        Err(SidecarValidationError::InvalidPreRoll { index: 0, .. })
    ));

    let mut sidecar = sample_sidecar();
    sidecar.events[0].post_roll_secs = Some(-1.0);
    assert!(matches!(
        sidecar.validate(),
        Err(SidecarValidationError::InvalidPostRoll { index: 0, .. })
    ));
}

#[test]
fn rejects_an_unrecognized_event_kind_with_an_actionable_message() {
    let json = r#"{
        "schema_version": 1,
        "source_media_filename": "clip.mp4",
        "events": [
            {"kind": "headshot", "source_timestamp_secs": 1.0, "confidence": 1.0}
        ]
    }"#;
    let err = EventSidecar::parse_and_validate(json).unwrap_err();
    assert!(matches!(err, SidecarParseOrValidationError::Parse(_)));
    // serde_json's own message names the offending value -- confirm it's not a blank/opaque
    // failure a user couldn't act on.
    assert!(format!("{err}").contains("headshot"));
}

#[test]
fn validation_error_messages_name_the_field_and_value() {
    let err = SidecarValidationError::UnsupportedSchemaVersion { found: 7 };
    assert!(format!("{err}").contains("schema_version"));
    assert!(format!("{err}").contains('7'));

    let err = SidecarValidationError::InvalidConfidence {
        index: 2,
        value: 5.0,
    };
    let msg = format!("{err}");
    assert!(msg.contains("event 2"));
    assert!(msg.contains("confidence"));
}

#[test]
fn imports_events_as_highlight_markers() {
    let sidecar = sample_sidecar();
    let mut timeline = empty_timeline();
    let added = import_events_as_markers(&mut timeline, &sidecar.events, |t| t);
    assert_eq!(added, 2);
    assert_eq!(timeline.markers.len(), 2);
    assert!(timeline
        .markers
        .iter()
        .all(|m| m.kind == MarkerKind::Highlight));
    let mut positions: Vec<f64> = timeline.markers.iter().map(|m| m.position_secs).collect();
    positions.sort_by(|a, b| a.total_cmp(b));
    assert_eq!(positions, vec![12.5, 200.0]);
}

#[test]
fn import_maps_source_timestamps_through_the_callers_offset() {
    let sidecar = sample_sidecar();
    let mut timeline = empty_timeline();
    // Recording placed starting at timeline t=50s.
    import_events_as_markers(&mut timeline, &sidecar.events, |source_secs| {
        50.0 + source_secs
    });
    let mut positions: Vec<f64> = timeline.markers.iter().map(|m| m.position_secs).collect();
    positions.sort_by(|a, b| a.total_cmp(b));
    assert_eq!(positions, vec![62.5, 250.0]);
}

#[test]
fn re_importing_the_same_events_is_idempotent() {
    let sidecar = sample_sidecar();
    let mut timeline = empty_timeline();
    let first = import_events_as_markers(&mut timeline, &sidecar.events, |t| t);
    let second = import_events_as_markers(&mut timeline, &sidecar.events, |t| t);
    assert_eq!(first, 2);
    assert_eq!(
        second, 0,
        "re-importing the same sidecar must not duplicate markers"
    );
    assert_eq!(timeline.markers.len(), 2);
}

#[test]
fn importing_a_different_kind_at_the_same_position_is_not_deduplicated() {
    let mut timeline = empty_timeline();
    let kill = GameplayEvent {
        kind: GameplayEventKind::Kill,
        source_timestamp_secs: 10.0,
        confidence: 1.0,
        pre_roll_secs: None,
        post_roll_secs: None,
    };
    let death = GameplayEvent {
        kind: GameplayEventKind::Death,
        source_timestamp_secs: 10.0,
        confidence: 1.0,
        pre_roll_secs: None,
        post_roll_secs: None,
    };
    import_events_as_markers(&mut timeline, &[kill], |t| t);
    import_events_as_markers(&mut timeline, &[death], |t| t);
    assert_eq!(timeline.markers.len(), 2);
}

#[test]
fn imported_markers_carry_a_human_readable_label() {
    let mut timeline = empty_timeline();
    let event = GameplayEvent {
        kind: GameplayEventKind::Objective,
        source_timestamp_secs: 5.0,
        confidence: 1.0,
        pre_roll_secs: None,
        post_roll_secs: None,
    };
    import_events_as_markers(&mut timeline, &[event], |t| t);
    assert_eq!(timeline.markers[0].label, "Objective");
}

#[test]
fn every_event_kind_has_a_distinct_label() {
    let labels: std::collections::HashSet<&str> = GameplayEventKind::ALL
        .iter()
        .map(|kind| marker_label_for(*kind))
        .collect();
    assert_eq!(labels.len(), GameplayEventKind::ALL.len());
}

fn sample_event(kind: GameplayEventKind) -> GameplayEvent {
    GameplayEvent {
        kind,
        source_timestamp_secs: 10.0,
        confidence: 1.0,
        pre_roll_secs: None,
        post_roll_secs: None,
    }
}

#[test]
fn apply_game_event_allowlist_returns_events_unchanged_when_no_allowlist_matches() {
    let events = vec![
        sample_event(GameplayEventKind::Kill),
        sample_event(GameplayEventKind::Death),
    ];
    let result = apply_game_event_allowlist(&events, None);
    assert_eq!(result, events);
}

#[test]
fn apply_game_event_allowlist_filters_to_only_the_allowed_kinds() {
    let events = vec![
        sample_event(GameplayEventKind::Kill),
        sample_event(GameplayEventKind::Death),
        sample_event(GameplayEventKind::Bookmark),
    ];
    let allowlist = GameEventAllowlist {
        game_id: "valorant".to_string(),
        allowed_kinds: vec![GameplayEventKind::Kill, GameplayEventKind::Bookmark],
        default_pre_roll_secs: None,
        default_post_roll_secs: None,
    };
    let result = apply_game_event_allowlist(&events, Some(&allowlist));
    let kinds: Vec<GameplayEventKind> = result.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![GameplayEventKind::Kill, GameplayEventKind::Bookmark]
    );
}

#[test]
fn apply_game_event_allowlist_with_empty_allowed_kinds_filters_out_everything() {
    let events = vec![sample_event(GameplayEventKind::Kill)];
    let allowlist = GameEventAllowlist {
        game_id: "valorant".to_string(),
        allowed_kinds: vec![],
        default_pre_roll_secs: None,
        default_post_roll_secs: None,
    };
    let result = apply_game_event_allowlist(&events, Some(&allowlist));
    assert!(result.is_empty());
}

#[test]
fn apply_game_event_allowlist_fills_roll_defaults_only_when_the_event_has_none() {
    let mut with_override = sample_event(GameplayEventKind::Death);
    with_override.pre_roll_secs = Some(9.0);
    let events = vec![sample_event(GameplayEventKind::Kill), with_override];
    let allowlist = GameEventAllowlist {
        game_id: "valorant".to_string(),
        allowed_kinds: vec![GameplayEventKind::Kill, GameplayEventKind::Death],
        default_pre_roll_secs: Some(2.0),
        default_post_roll_secs: Some(1.5),
    };
    let result = apply_game_event_allowlist(&events, Some(&allowlist));
    // The Kill event had no override -- gets the allowlist defaults.
    assert_eq!(result[0].pre_roll_secs, Some(2.0));
    assert_eq!(result[0].post_roll_secs, Some(1.5));
    // The Death event's own pre_roll_secs override must survive; only its missing post_roll
    // gets the default.
    assert_eq!(result[1].pre_roll_secs, Some(9.0));
    assert_eq!(result[1].post_roll_secs, Some(1.5));
}

#[test]
fn event_sidecar_without_a_game_id_field_parses_as_none() {
    // Back-compat: a sidecar produced before game_id existed must still parse.
    let json = r#"{"schema_version":1,"source_media_filename":"a.mp4","events":[]}"#;
    let sidecar = EventSidecar::parse_and_validate(json).unwrap();
    assert_eq!(sidecar.game_id, None);
}

#[test]
fn event_sidecar_game_id_round_trips_through_json() {
    let mut sidecar = sample_sidecar();
    sidecar.game_id = Some("valorant".to_string());
    let json = serde_json::to_string(&sidecar).unwrap();
    let parsed = EventSidecar::parse_and_validate(&json).unwrap();
    assert_eq!(parsed.game_id, Some("valorant".to_string()));
}
