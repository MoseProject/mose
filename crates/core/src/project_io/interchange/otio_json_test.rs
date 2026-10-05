// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use super::*;
use crate::interchange::{
    InterchangeClip, InterchangeMarker, InterchangeTimeline, InterchangeTrack,
    InterchangeTrackItem, InterchangeTrackKind, InterchangeTransitionKind, MediaReference,
    TimeRange,
};
use crate::timeline::MarkerKind;

fn sample_timeline() -> InterchangeTimeline {
    InterchangeTimeline {
        name: "Sequence 1".to_string(),
        tracks: vec![InterchangeTrack {
            name: "V1".to_string(),
            kind: InterchangeTrackKind::Video,
            items: vec![
                InterchangeTrackItem::Gap(TimeRange::from_seconds(0.0, 1.0, 1_000_000.0)),
                InterchangeTrackItem::Clip(InterchangeClip {
                    source_id: 42,
                    source_range: TimeRange::from_seconds(2.0, 5.0, 1_000_000.0),
                    media_reference: MediaReference::External {
                        target_url: "file:///tmp/clip.mp4".to_string(),
                    },
                    speed_factor: 1.5,
                    transition_in: Some(InterchangeTransitionKind::Fade),
                }),
                InterchangeTrackItem::Clip(InterchangeClip {
                    source_id: 43,
                    source_range: TimeRange::from_seconds(0.0, 3.0, 1_000_000.0),
                    media_reference: MediaReference::Missing,
                    speed_factor: 1.0,
                    transition_in: None,
                }),
            ],
        }],
        markers: vec![InterchangeMarker {
            name: "Highlight".to_string(),
            marked_range: TimeRange::from_seconds(1.5, 0.0, 1_000_000.0),
            kind: MarkerKind::Highlight,
        }],
    }
}

fn schema_of(value: &Value) -> &str {
    value["OTIO_SCHEMA"].as_str().expect("OTIO_SCHEMA present")
}

#[test]
fn top_level_schema_tags_match_the_real_otio_wire_format() {
    let doc = serialize_interchange_timeline(&sample_timeline());
    assert_eq!(schema_of(&doc), "Timeline.1");
    assert_eq!(doc["name"], "Sequence 1");
    assert_eq!(schema_of(&doc["tracks"]), "Stack.1");
    assert_eq!(schema_of(&doc["tracks"]["children"][0]), "Track.1");
    assert_eq!(doc["tracks"]["children"][0]["kind"], "Video");
}

#[test]
fn a_gap_serializes_as_gap_1_with_a_source_range() {
    let doc = serialize_interchange_timeline(&sample_timeline());
    let gap = &doc["tracks"]["children"][0]["children"][0];
    assert_eq!(schema_of(gap), "Gap.1");
    assert_eq!(schema_of(&gap["source_range"]), "TimeRange.1");
    assert_eq!(
        schema_of(&gap["source_range"]["start_time"]),
        "RationalTime.1"
    );
    assert_eq!(gap["source_range"]["start_time"]["value"], 0.0);
    assert_eq!(gap["source_range"]["duration"]["value"], 1_000_000.0);
}

#[test]
fn a_clip_with_an_external_reference_serializes_as_clip_1_with_external_reference_1() {
    let doc = serialize_interchange_timeline(&sample_timeline());
    let clip = &doc["tracks"]["children"][0]["children"][1];
    assert_eq!(schema_of(clip), "Clip.1");
    assert_eq!(schema_of(&clip["media_reference"]), "ExternalReference.1");
    assert_eq!(
        clip["media_reference"]["target_url"],
        "file:///tmp/clip.mp4"
    );
    // Source range: 5 real seconds at the 1_000_000 interchange rate.
    assert_eq!(clip["source_range"]["duration"]["value"], 5_000_000.0);
    assert_eq!(clip["metadata"]["oca"]["speed_factor"], 1.5);
}

#[test]
fn a_clip_with_a_missing_reference_serializes_as_missing_reference_1() {
    let doc = serialize_interchange_timeline(&sample_timeline());
    let clip = &doc["tracks"]["children"][0]["children"][2];
    assert_eq!(schema_of(clip), "Clip.1");
    assert_eq!(schema_of(&clip["media_reference"]), "MissingReference.1");
}

#[test]
fn a_marker_serializes_as_marker_2_with_a_plain_string_color() {
    let doc = serialize_interchange_timeline(&sample_timeline());
    let marker = &doc["tracks"]["markers"][0];
    assert_eq!(schema_of(marker), "Marker.2");
    assert_eq!(marker["name"], "Highlight");
    assert_eq!(marker["color"], "YELLOW");
    assert_eq!(schema_of(&marker["marked_range"]), "TimeRange.1");
}

#[test]
fn an_empty_timeline_still_produces_a_well_formed_document() {
    let empty = InterchangeTimeline {
        name: "Empty".to_string(),
        tracks: vec![],
        markers: vec![],
    };
    let doc = serialize_interchange_timeline(&empty);
    assert_eq!(schema_of(&doc), "Timeline.1");
    assert_eq!(doc["tracks"]["children"].as_array().unwrap().len(), 0);
    assert_eq!(doc["tracks"]["markers"].as_array().unwrap().len(), 0);
}

#[test]
fn the_document_round_trips_through_a_real_json_parser() {
    // Confirms the emitted document is valid, parseable JSON, the way any real consumer (the
    // OTIO Python library, another NLE) would first check it -- `round_trips_through_parse_
    // otio_json` below goes the rest of the way, back into an InterchangeTimeline.
    let pretty = serialize_interchange_timeline_pretty(&sample_timeline());
    let reparsed: Value = serde_json::from_str(&pretty).expect("valid JSON");
    assert_eq!(schema_of(&reparsed), "Timeline.1");
}

#[test]
fn audio_tracks_carry_the_audio_kind() {
    let timeline = InterchangeTimeline {
        name: "Audio only".to_string(),
        tracks: vec![InterchangeTrack {
            name: "A1".to_string(),
            kind: InterchangeTrackKind::Audio,
            items: vec![],
        }],
        markers: vec![],
    };
    let doc = serialize_interchange_timeline(&timeline);
    assert_eq!(doc["tracks"]["children"][0]["kind"], "Audio");
}

// --- Import (parse_otio_json) ---

fn fixture(name: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/otio")
        .join(name);
    let bytes = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading fixture {}: {e}", path.display()));
    serde_json::from_str(&bytes).unwrap_or_else(|e| panic!("parsing fixture {name} as JSON: {e}"))
}

#[test]
fn round_trips_through_parse_otio_json() {
    let original = sample_timeline();
    let doc = serialize_interchange_timeline(&original);
    let result = parse_otio_json(&doc).expect("oca's own output must parse back cleanly");
    assert_eq!(result.warnings, Vec::<String>::new());
    assert_eq!(result.timeline, original);
}

#[test]
fn parses_a_real_simple_cut_otio_file() {
    // From the OpenTimelineIO project's own tests/sample_data/simple_cut.otio -- fetched real,
    // not hand-written -- one video track, four Clip.1 items, the last with a null
    // source_range (OTIO's own "use the full available_range of the media" convention).
    let doc = fixture("simple_cut.otio");
    let result = parse_otio_json(&doc).expect("a real, well-formed OTIO file must parse");
    assert_eq!(result.warnings, Vec::<String>::new());
    assert_eq!(result.timeline.name, "Figure 1 - Simple Cut List");
    assert_eq!(result.timeline.tracks.len(), 1);
    let track = &result.timeline.tracks[0];
    assert_eq!(track.kind, InterchangeTrackKind::Video);
    assert_eq!(track.items.len(), 4);
    for item in &track.items {
        assert!(
            matches!(item, InterchangeTrackItem::Clip(_)),
            "simple_cut.otio has no gaps -- every item should be a clip"
        );
    }
    let InterchangeTrackItem::Clip(clip4) = &track.items[3] else {
        unreachable!()
    };
    // Clip-004's own source_range is null; falls back to its media_reference's
    // available_range (start 100/24s, duration 6/24s -- see this module's own doc comment).
    assert!((clip4.source_range.start_time.to_seconds() - 100.0 / 24.0).abs() < 1e-9);
    assert!((clip4.source_range.duration.to_seconds() - 6.0 / 24.0).abs() < 1e-9);
    assert_eq!(
        clip4.media_reference,
        MediaReference::External {
            target_url: "file:///folder/credits.mov".to_string()
        }
    );
}

#[test]
fn parses_a_real_transition_otio_file() {
    // From the OpenTimelineIO project's own tests/sample_data/transition.otio: Clip-001,
    // Clip-002, a Transition.1 (SMPTE_Dissolve), Clip-003, Clip-004 (also a null source_range,
    // like simple_cut.otio's own Clip-004).
    let doc = fixture("transition.otio");
    let result = parse_otio_json(&doc).expect("a real, well-formed OTIO file must parse");
    let track = &result.timeline.tracks[0];
    // The Transition.1 item consumes no track slot of its own -- 4 clips in, 4 items out, not 5.
    assert_eq!(track.items.len(), 4);
    assert_eq!(
        result
            .warnings
            .iter()
            .filter(|w| w.contains("Transition.1"))
            .count(),
        1,
        "the Transition.1 item itself should be flagged exactly once: {:?}",
        result.warnings
    );
    let InterchangeTrackItem::Clip(clip3) = &track.items[2] else {
        panic!("expected the third item to be Clip-003, following the Transition.1");
    };
    assert_eq!(
        clip3.transition_in,
        Some(InterchangeTransitionKind::Fade),
        "the Transition.1 between Clip-002 and Clip-003 should attach to Clip-003"
    );
    let InterchangeTrackItem::Clip(clip1) = &track.items[0] else {
        panic!("expected the first item to be Clip-001");
    };
    assert_eq!(
        clip1.transition_in, None,
        "Clip-001 has no preceding Transition.1"
    );
}

#[test]
fn parses_a_real_multitrack_otio_file() {
    // From the OpenTimelineIO project's own tests/sample_data/multitrack.otio: three video
    // tracks, each a mix of real Clip.1 and Gap.1 ("Filler"/"ScopeReference") items.
    let doc = fixture("multitrack.otio");
    let result = parse_otio_json(&doc).expect("a real, well-formed OTIO file must parse");
    assert_eq!(result.warnings, Vec::<String>::new());
    assert_eq!(result.timeline.tracks.len(), 3);
    let expected_shapes: [&[bool]; 3] = [
        // false = Gap, true = Clip, in document order.
        &[true, false, true, false, true, false],
        &[false, true, false],
        &[false, true, false],
    ];
    for (track, expected) in result.timeline.tracks.iter().zip(expected_shapes) {
        assert_eq!(track.kind, InterchangeTrackKind::Video);
        let actual: Vec<bool> = track
            .items
            .iter()
            .map(|item| matches!(item, InterchangeTrackItem::Clip(_)))
            .collect();
        assert_eq!(actual, expected, "track '{}'", track.name);
    }
}

#[test]
fn a_document_that_is_not_a_timeline_1_is_a_hard_parse_error() {
    let not_a_timeline = json!({ "OTIO_SCHEMA": "Clip.1", "name": "not a timeline" });
    let err = parse_otio_json(&not_a_timeline).unwrap_err();
    assert_eq!(
        err,
        OtioParseError::UnrecognizedTopLevelSchema("Clip.1".to_string())
    );
}

#[test]
fn a_timeline_missing_its_tracks_field_is_a_hard_parse_error() {
    let malformed = json!({ "OTIO_SCHEMA": "Timeline.1", "name": "no tracks" });
    let err = parse_otio_json(&malformed).unwrap_err();
    assert_eq!(
        err,
        OtioParseError::MissingField {
            schema: "Timeline.1".to_string(),
            field: "tracks"
        }
    );
}

#[test]
fn an_unrecognized_media_reference_schema_is_treated_as_offline_with_a_warning() {
    let doc = json!({
        "OTIO_SCHEMA": "Timeline.1",
        "name": "t",
        "tracks": {
            "OTIO_SCHEMA": "Stack.1",
            "children": [{
                "OTIO_SCHEMA": "Track.1",
                "kind": "Video",
                "name": "V1",
                "children": [{
                    "OTIO_SCHEMA": "Clip.1",
                    "name": "Clip-1",
                    "source_range": {
                        "OTIO_SCHEMA": "TimeRange.1",
                        "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 0.0, "rate": 24.0 },
                        "duration": { "OTIO_SCHEMA": "RationalTime.1", "value": 24.0, "rate": 24.0 },
                    },
                    "media_reference": { "OTIO_SCHEMA": "ImageSequenceReference.1" },
                }],
            }],
            "markers": [],
        },
    });
    let result = parse_otio_json(&doc).unwrap();
    let InterchangeTrackItem::Clip(clip) = &result.timeline.tracks[0].items[0] else {
        panic!("expected one clip");
    };
    assert_eq!(clip.media_reference, MediaReference::Missing);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("ImageSequenceReference.1")),
        "{:?}",
        result.warnings
    );
}

#[test]
fn an_unsupported_nested_item_becomes_an_equal_duration_gap_with_a_warning() {
    // A nested Stack.1 (compound clip) sitting directly among a track's children, the real
    // shape confirmed against the OpenTimelineIO project's own nested_example.otio -- not yet
    // recursively imported (a separate, later slice), but its own on-timeline duration must
    // still be preserved so a following clip keeps its correct start time.
    let doc = json!({
        "OTIO_SCHEMA": "Timeline.1",
        "name": "t",
        "tracks": {
            "OTIO_SCHEMA": "Stack.1",
            "children": [{
                "OTIO_SCHEMA": "Track.1",
                "kind": "Video",
                "name": "V1",
                "children": [{
                    "OTIO_SCHEMA": "Stack.1",
                    "name": "Nested",
                    "source_range": {
                        "OTIO_SCHEMA": "TimeRange.1",
                        "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 0.0, "rate": 24.0 },
                        "duration": { "OTIO_SCHEMA": "RationalTime.1", "value": 48.0, "rate": 24.0 },
                    },
                    "children": [],
                }],
            }],
            "markers": [],
        },
    });
    let result = parse_otio_json(&doc).unwrap();
    let items = &result.timeline.tracks[0].items;
    assert_eq!(items.len(), 1);
    let InterchangeTrackItem::Gap(range) = &items[0] else {
        panic!("expected the nested Stack.1 to become a Gap");
    };
    assert!((range.duration.to_seconds() - 2.0).abs() < 1e-9);
    assert!(
        result.warnings.iter().any(|w| w.contains("Stack.1")),
        "{:?}",
        result.warnings
    );
}

#[test]
fn a_clip_2_with_multiple_media_references_keeps_only_the_active_one_and_warns() {
    let doc = json!({
        "OTIO_SCHEMA": "Timeline.1",
        "name": "t",
        "tracks": {
            "OTIO_SCHEMA": "Stack.1",
            "children": [{
                "OTIO_SCHEMA": "Track.1",
                "kind": "Video",
                "name": "V1",
                "children": [{
                    "OTIO_SCHEMA": "Clip.2",
                    "name": "Clip-1",
                    "source_range": {
                        "OTIO_SCHEMA": "TimeRange.1",
                        "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 0.0, "rate": 24.0 },
                        "duration": { "OTIO_SCHEMA": "RationalTime.1", "value": 24.0, "rate": 24.0 },
                    },
                    "active_media_reference_key": "PROXY",
                    "media_references": {
                        "DEFAULT_MEDIA": {
                            "OTIO_SCHEMA": "ExternalReference.1",
                            "target_url": "file:///full-res.mov",
                        },
                        "PROXY": {
                            "OTIO_SCHEMA": "ExternalReference.1",
                            "target_url": "file:///proxy.mov",
                        },
                    },
                }],
            }],
            "markers": [],
        },
    });
    let result = parse_otio_json(&doc).unwrap();
    let InterchangeTrackItem::Clip(clip) = &result.timeline.tracks[0].items[0] else {
        panic!("expected one clip");
    };
    assert_eq!(
        clip.media_reference,
        MediaReference::External {
            target_url: "file:///proxy.mov".to_string()
        }
    );
    assert!(
        result.warnings.iter().any(|w| w.contains("dropped")),
        "{:?}",
        result.warnings
    );
}

fn time(value: f64) -> serde_json::Value {
    serde_json::json!({"OTIO_SCHEMA": "RationalTime.1", "value": value, "rate": 24.0})
}

fn range(start: f64, duration: f64) -> serde_json::Value {
    serde_json::json!({
        "OTIO_SCHEMA": "TimeRange.1",
        "start_time": time(start),
        "duration": time(duration),
    })
}

fn document(tracks: serde_json::Value, markers: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "OTIO_SCHEMA": "Timeline.1",
        "name": "T",
        "tracks": {"OTIO_SCHEMA": "Stack.1", "children": tracks, "markers": markers},
    })
}

fn track(kind: &str, children: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"OTIO_SCHEMA": "Track.1", "name": "V1", "kind": kind, "children": children})
}

#[test]
fn structural_problems_are_hard_errors_with_readable_messages() {
    let not_object = parse_otio_json(&serde_json::json!([1])).unwrap_err();
    assert!(matches!(not_object, OtioParseError::NotAnObject(_)));
    assert!(not_object.to_string().contains("not a JSON object"));

    let no_tracks = parse_otio_json(&serde_json::json!({"OTIO_SCHEMA": "Timeline.1"})).unwrap_err();
    assert!(matches!(no_tracks, OtioParseError::MissingField { .. }));
    assert!(no_tracks.to_string().contains("tracks"));

    let wrong_root = parse_otio_json(&serde_json::json!({"OTIO_SCHEMA": "Other.1"})).unwrap_err();
    assert!(wrong_root.to_string().contains("Other.1"));

    let tracks_not_object = serde_json::json!({"OTIO_SCHEMA": "Timeline.1", "tracks": 5});
    assert!(matches!(
        parse_otio_json(&tracks_not_object).unwrap_err(),
        OtioParseError::NotAnObject(_)
    ));
    let wrong_stack = serde_json::json!({
        "OTIO_SCHEMA": "Timeline.1",
        "tracks": {"OTIO_SCHEMA": "Track.1"},
    });
    assert!(matches!(
        parse_otio_json(&wrong_stack).unwrap_err(),
        OtioParseError::UnrecognizedTopLevelSchema(_)
    ));
}

#[test]
fn unsupported_pieces_are_skipped_with_warnings_instead_of_failing() {
    let good_clip = serde_json::json!({
        "OTIO_SCHEMA": "Clip.1",
        "name": "Clip-7",
        "source_range": range(0.0, 48.0),
        "media_reference": {"OTIO_SCHEMA": "ExternalReference.1", "target_url": "file:///a.mp4"},
        "metadata": {"oca": {"speed_factor": 2.0, "transition_in": "Zoom"}},
    });
    let tracks = serde_json::json!([
        7,
        {"OTIO_SCHEMA": "Stack.1"},
        track("Subtitle", serde_json::json!([])),
        track(
            "Video",
            serde_json::json!([
                5,
                {"OTIO_SCHEMA": "Gap.1"},
                {"OTIO_SCHEMA": "Gap.1", "source_range": {"OTIO_SCHEMA": "TimeRange.1"}},
                {"OTIO_SCHEMA": "Transition.1", "transition_type": "SMPTE_Dissolve"},
                good_clip,
                {"OTIO_SCHEMA": "Clip.1", "name": "Clip-8", "media_reference": null},
                {"OTIO_SCHEMA": "Stack.1", "source_range": range(0.0, 24.0)},
                {"OTIO_SCHEMA": "Stack.1", "source_range": {"OTIO_SCHEMA": "TimeRange.1"}},
                {"OTIO_SCHEMA": "Stack.1"},
            ])
        ),
    ]);
    let markers = serde_json::json!([
        {"OTIO_SCHEMA": "Marker.1", "name": "m", "marked_range": range(0.0, 1.0), "color": "RED"},
        {"OTIO_SCHEMA": "Marker.1", "name": "no range"},
        7,
    ]);
    let result = parse_otio_json(&document(tracks, markers)).unwrap();
    assert_eq!(result.timeline.markers.len(), 1);
    assert_eq!(result.timeline.markers[0].kind, MarkerKind::ToDo);
    assert_eq!(result.timeline.tracks.len(), 1);
    let items = &result.timeline.tracks[0].items;
    // The clip kept its speed and picked up the transition from the item before it; the stack
    // with a range became a gap.
    let clips: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            InterchangeTrackItem::Clip(clip) => Some(clip),
            _ => None,
        })
        .collect();
    assert_eq!(clips.len(), 1);
    assert_eq!(clips[0].source_id, 7);
    assert_eq!(clips[0].speed_factor, 2.0);
    assert_eq!(
        clips[0].transition_in,
        Some(InterchangeTransitionKind::Fade)
    );
    assert!(items
        .iter()
        .any(|item| matches!(item, InterchangeTrackItem::Gap(_))));
    let warnings = result.warnings.join("\n");
    for needle in [
        "skipped a malformed marker",
        "non-object top-level track item",
        "unsupported top-level item",
        "unsupported kind 'Subtitle'",
        "non-object item",
        "malformed Gap.1",
        "Transition.1 approximated",
        "malformed Clip.1",
        "replaced with an equal-duration gap",
        "unreadable source_range",
        "no explicit source_range",
    ] {
        assert!(
            warnings.contains(needle),
            "missing warning: {needle}\n{warnings}"
        );
    }
}

#[test]
fn media_references_and_marker_colours_map_to_their_kinds() {
    use crate::interchange::{InterchangeMarker, MediaReference};
    let reference = |schema: &str| serde_json::json!({"OTIO_SCHEMA": schema});
    // A reference that is not an object, and an unrecognised schema, are offline with a warning.
    let not_object = media_reference_from_json(&serde_json::json!(5));
    assert_eq!(not_object.0, MediaReference::Missing);
    assert!(not_object.1.is_some());
    let (unknown, warning) = media_reference_from_json(&reference("ImageSequenceReference.1"));
    assert_eq!(unknown, MediaReference::Missing);
    assert!(warning.unwrap().contains("ImageSequenceReference.1"));
    assert_eq!(
        media_reference_from_json(&reference("MissingReference.1")),
        (MediaReference::Missing, None)
    );

    for (color, kind) in [
        ("RED", MarkerKind::ToDo),
        ("BLUE", MarkerKind::Chapter),
        ("YELLOW", MarkerKind::Highlight),
        ("GREEN", MarkerKind::Standard),
        ("PURPLE", MarkerKind::Standard),
    ] {
        assert_eq!(marker_kind_from_color(color), kind);
    }

    // Every marker kind serializes with its own colour.
    for (kind, colour) in [
        (MarkerKind::Standard, "GREEN"),
        (MarkerKind::ToDo, "RED"),
        (MarkerKind::Chapter, "BLUE"),
        (MarkerKind::Highlight, "YELLOW"),
    ] {
        let mut timeline = sample_timeline();
        timeline.markers = vec![InterchangeMarker {
            kind,
            ..timeline.markers[0].clone()
        }];
        let json = serialize_interchange_timeline(&timeline).to_string();
        let expected = format!("\"color\":\"{colour}\"");
        assert!(json.contains(&expected), "{colour}: {json}");
    }
}

#[test]
fn a_clip_two_keeps_only_its_active_media_reference_and_warns_about_the_rest() {
    let clip = serde_json::json!({
        "OTIO_SCHEMA": "Clip.2",
        "name": "Clip-3",
        "source_range": range(0.0, 24.0),
        "active_media_reference_key": "missing-key",
        "media_references": {
            "b": {"OTIO_SCHEMA": "ExternalReference.1", "target_url": "file:///b.mp4"},
            "a": {"OTIO_SCHEMA": "ImageSequenceReference.1"},
        },
    });
    let result = parse_otio_json(&document(
        serde_json::json!([track("Video", serde_json::json!([clip]))]),
        serde_json::json!([]),
    ))
    .unwrap();
    // The active key does not exist, so the alphabetically first reference wins.
    let warnings = result.warnings.join("\n");
    assert!(warnings.contains("only 'a' was kept"), "{warnings}");
    assert!(
        warnings.contains("unrecognized media reference schema"),
        "{warnings}"
    );

    // A clip with neither a source range nor an available range is malformed.
    let bare = serde_json::json!({"OTIO_SCHEMA": "Clip.2", "name": "Clip-4"});
    let result = parse_otio_json(&document(
        serde_json::json!([track("Video", serde_json::json!([bare]))]),
        serde_json::json!([]),
    ))
    .unwrap();
    assert!(result.warnings.join("\n").contains("malformed Clip.2"));
}
