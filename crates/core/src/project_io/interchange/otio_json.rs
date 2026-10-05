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

//! CF-05's deferred slice, now unblocked: real OpenTimelineIO `.otio` JSON serialization.
//! `interchange.rs`'s own doc comment explained why this didn't happen in slice 1 — no network
//! path in that sandbox to fetch or verify OTIO's actual wire format, and writing one from
//! memory alone would be exactly the kind of guessing `CLAUDE.md`'s "do not guess APIs,
//! versions, flags, or package names" rule forbids. This session has real network access, so the
//! schema below is verified against real files from the OpenTimelineIO project's own
//! `tests/sample_data/` directory (fetched via `curl`, not summarized through a lossy
//! intermediary) — `simple_cut.otio`, `screening_example.otio`, `transition.otio`,
//! `multitrack.otio`, `nested_example.otio` — not reconstructed from documentation prose.
//!
//! **Schema versions used, and why** — verified not just against sample files but against the
//! real `opentimelineio` 0.18.1 Python reference implementation itself (`pip install
//! opentimelineio`, this session had real network access): `Timeline.1`/`Stack.1`/`Track.1`/
//! `Gap.1`/`Transition.1`/`TimeRange.1`/`RationalTime.1`/`ExternalReference.1`/
//! `MissingReference.1` are unambiguous. `Clip` has two live versions in the real ecosystem:
//! `Clip.1` (a single `media_reference` field) and `Clip.2` (a `media_references` map +
//! `active_media_reference_key`, for multi-reference workflows like stereo or proxy/full-res
//! pairs — confirmed as what 0.18.1 itself constructs by default via `otio.schema.Clip()`).
//! This module writes `Clip.1` deliberately, not because it's current but because
//! [`super::MediaReference`] only ever models one reference per clip, the same shape `Clip.1`
//! has — writing `Clip.2` would mean inventing a fake single-entry map to satisfy a field oca
//! has no real multi-reference data for. Confirmed for real, not assumed: `opentimelineio`
//! 0.18.1's own `read_from_file` successfully loads `Clip.1` (tested directly against
//! `simple_cut.otio`, a `Clip.1` file from the project's own `tests/sample_data/`) — backward
//! compatibility, not a guess. `Marker` genuinely needed a real-verification catch: an initial
//! attempt used `Marker.3` with a nested `Color.1` object, matching a sample fetched from the
//! OpenTimelineIO GitHub repo's `main` branch — but 0.18.1's own `otio.schema.Marker()` emits
//! `Marker.2` with a *plain string* `color` field and a `comment` field, and rejected the
//! `Marker.3` version outright (`UnsupportedSchemaError`) when the generated file was loaded
//! back through the real library. `main`'s sample data was ahead of the actual released/
//! installable library version — exactly the kind of gap only a real load-it-back-and-see check
//! (not just eyeballing a sample file) catches. Fixed to `Marker.2`, then re-verified: the real
//! library loads every object this module emits without error.
//!
//! **The read direction ([`parse_otio_json`]) is now implemented too**, CF-05's previously-
//! deferred next slice. Verified against real third-party files, not just this module's own
//! output: `simple_cut.otio`, `transition.otio`, `multitrack.otio`, and `nested_example.otio`
//! (fetched from the OpenTimelineIO project's own `tests/sample_data/`, loaded through the real
//! `opentimelineio` 0.18.1 Python library first to confirm what each one actually contains, not
//! assumed from the file alone). Two real-world shapes this module's own writer never produces
//! needed real verification before import could handle them correctly, not just type-check:
//!
//! - **`Transition.1`** track items carry no `source_range` of their own — checked directly
//!   against `transition.otio`'s real `in_offset`/`out_offset` fields and the neighboring
//!   clips' own `source_range`s: a transition's overlap comes out of the two clips it sits
//!   between, not extra timeline duration. [`parse_otio_json`] treats a `Transition.1` as
//!   attaching `transition_in` (best-effort mapped to [`super::InterchangeTransitionKind::Fade`]
//!   — real OTIO's own transition taxonomy is coarser than oca's, effectively just "some kind of
//!   dissolve") to the clip immediately following it, and does **not** advance the track's
//!   timing cursor for it — confirmed correct against the real file's own clip start times.
//! - **A compound clip is a nested `Stack.1`/`Track.1` sitting directly among a track's own
//!   `children`** — confirmed against `nested_example.otio`'s real shape (`walk`ed via the
//!   `opentimelineio` library, not guessed from the schema docs alone). Full recursive import
//!   into oca's own `nested_sequence_id` model is out of scope for this slice (a separate, later
//!   one); a nested item is instead replaced with a same-duration [`InterchangeTrackItem::Gap`]
//!   when its own `source_range` is explicit (preserving every later item's timing exactly) or
//!   skipped with a stronger warning when it isn't (the doc's own "preserve unsupported fields
//!   as warnings, never silently approximate them" rule, extended here to "approximate the
//!   *timing* honestly even when the *content* can't be recovered").
//!
//! Anything else structurally unrecognized (an unknown top-level schema, a malformed individual
//! clip/marker, an unresolvable media reference) is reported in [`OtioImportResult::warnings`]
//! and skipped rather than failing the whole document — matching
//! [`super::InterchangeImportResult::warnings`]'s own convention. Only a genuinely malformed
//! top-level document (not a `Timeline.1`/`Stack.1` at all, or missing a structurally required
//! field) returns [`OtioParseError`].

use serde_json::{json, Value};

use super::{
    InterchangeClip, InterchangeMarker, InterchangeTimeline, InterchangeTrack,
    InterchangeTrackItem, InterchangeTrackKind, InterchangeTransitionKind, MediaReference,
    RationalTime, TimeRange,
};
use crate::timeline::MarkerKind;

fn rational_time_json(t: RationalTime) -> Value {
    json!({
        "OTIO_SCHEMA": "RationalTime.1",
        "rate": t.rate,
        "value": t.value,
    })
}

fn time_range_json(r: TimeRange) -> Value {
    json!({
        "OTIO_SCHEMA": "TimeRange.1",
        "start_time": rational_time_json(r.start_time),
        "duration": rational_time_json(r.duration),
    })
}

fn media_reference_json(reference: &MediaReference) -> Value {
    match reference {
        MediaReference::External { target_url } => json!({
            "OTIO_SCHEMA": "ExternalReference.1",
            "name": "",
            "metadata": {},
            "available_range": null,
            "available_image_bounds": null,
            "target_url": target_url,
        }),
        MediaReference::Missing => json!({
            "OTIO_SCHEMA": "MissingReference.1",
            "name": "",
            "metadata": {},
            "available_range": null,
            "available_image_bounds": null,
        }),
    }
}

/// OTIO's `Marker.2` convention doesn't mandate any particular color-per-marker-kind meaning —
/// this mapping is oca's own judgment call for readability when a marker is opened in another
/// NLE, not a spec requirement. Named colors match the standard string constants
/// `opentimelineio.schema.MarkerColor` defines (`RED`/`GREEN`/`BLUE`/`YELLOW`).
fn marker_color_name(kind: MarkerKind) -> &'static str {
    match kind {
        MarkerKind::Standard => "GREEN",
        MarkerKind::ToDo => "RED",
        MarkerKind::Chapter => "BLUE",
        MarkerKind::Highlight => "YELLOW",
    }
}

fn marker_json(marker: &InterchangeMarker) -> Value {
    json!({
        "OTIO_SCHEMA": "Marker.2",
        "name": marker.name,
        "metadata": {},
        "color": marker_color_name(marker.kind),
        "marked_range": time_range_json(marker.marked_range),
        "comment": "",
    })
}

fn clip_json(clip: &InterchangeClip) -> Value {
    json!({
        "OTIO_SCHEMA": "Clip.1",
        "name": format!("Clip-{}", clip.source_id),
        "metadata": {
            // `speed_factor`/`transition_in` have no OTIO-native field at this schema version
            // (see this module's doc comment on what's carried vs. approximated) -- kept in
            // metadata rather than silently dropped, so a round-trip back into oca (once import
            // lands) or a careful human reader in another tool can still recover them.
            "oca": {
                "speed_factor": clip.speed_factor,
                "transition_in": clip.transition_in.map(|t| format!("{t:?}")),
            }
        },
        "effects": [],
        "markers": [],
        "enabled": true,
        "source_range": time_range_json(clip.source_range),
        "media_reference": media_reference_json(&clip.media_reference),
    })
}

fn gap_json(range: TimeRange) -> Value {
    json!({
        "OTIO_SCHEMA": "Gap.1",
        "name": "Gap",
        "metadata": {},
        "effects": [],
        "markers": [],
        "enabled": true,
        "source_range": time_range_json(range),
    })
}

fn track_item_json(item: &InterchangeTrackItem) -> Value {
    match item {
        InterchangeTrackItem::Clip(clip) => clip_json(clip),
        InterchangeTrackItem::Gap(range) => gap_json(*range),
    }
}

fn track_json(track: &InterchangeTrack) -> Value {
    let kind = match track.kind {
        InterchangeTrackKind::Video => "Video",
        InterchangeTrackKind::Audio => "Audio",
    };
    json!({
        "OTIO_SCHEMA": "Track.1",
        "name": track.name,
        "metadata": {},
        "source_range": null,
        "effects": [],
        "markers": [],
        "enabled": true,
        "kind": kind,
        "children": track.items.iter().map(track_item_json).collect::<Vec<_>>(),
    })
}

/// Serializes `timeline` as a real, schema-verified OpenTimelineIO `.otio` JSON document — see
/// this module's own doc comment for exactly which schema versions and why. The top-level
/// `Timeline` carries `timeline.markers` on its `tracks` `Stack` (OTIO has no separate
/// timeline-level marker list distinct from the stack's own) rather than dropping them.
pub fn serialize_interchange_timeline(timeline: &InterchangeTimeline) -> Value {
    json!({
        "OTIO_SCHEMA": "Timeline.1",
        "name": timeline.name,
        "metadata": {},
        "global_start_time": null,
        "tracks": {
            "OTIO_SCHEMA": "Stack.1",
            "name": "tracks",
            "metadata": {},
            "source_range": null,
            "effects": [],
            "enabled": true,
            "markers": timeline.markers.iter().map(marker_json).collect::<Vec<_>>(),
            "children": timeline.tracks.iter().map(track_json).collect::<Vec<_>>(),
        },
    })
}

/// [`serialize_interchange_timeline`], pretty-printed — what a caller writing a `.otio` file to
/// disk actually wants (OTIO's own Python library writes indented JSON too, and a hand-readable
/// file is one of interchange's own practical benefits over a binary format).
pub fn serialize_interchange_timeline_pretty(timeline: &InterchangeTimeline) -> String {
    serde_json::to_string_pretty(&serialize_interchange_timeline(timeline))
        .expect("serde_json::Value serialization is infallible")
}

// --- Import (JSON -> InterchangeTimeline) ---

/// A genuine document error [`parse_otio_json`] cannot recover from — a required structural
/// field missing or the wrong JSON type, or a top-level shape that isn't `Timeline.1`/`Stack.1`
/// at all. A *recognized but unsupported* shape (a `Transition.1` item, a nested compound clip,
/// an unresolvable media reference, one malformed clip/marker among otherwise-good siblings)
/// is not this — those complete import and land in [`OtioImportResult::warnings`] instead, per
/// this module's own doc comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OtioParseError {
    NotAnObject(String),
    MissingField { schema: String, field: &'static str },
    UnrecognizedTopLevelSchema(String),
}

impl std::fmt::Display for OtioParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAnObject(what) => write!(f, "{what} is not a JSON object"),
            Self::MissingField { schema, field } => {
                write!(f, "{schema} is missing its required '{field}' field")
            }
            Self::UnrecognizedTopLevelSchema(schema) => {
                write!(f, "unrecognized top-level OTIO_SCHEMA '{schema}'")
            }
        }
    }
}

impl std::error::Error for OtioParseError {}

/// [`parse_otio_json`]'s result: the reconstructed [`InterchangeTimeline`] plus every
/// recognized-but-unsupported shape it had to skip or approximate along the way, in document
/// order.
#[derive(Debug, Clone, PartialEq)]
pub struct OtioImportResult {
    pub timeline: InterchangeTimeline,
    pub warnings: Vec<String>,
}

fn schema_tag(obj: &serde_json::Map<String, Value>) -> &str {
    obj.get("OTIO_SCHEMA").and_then(Value::as_str).unwrap_or("")
}

fn rational_time_from_json(v: &Value, schema: &str) -> Result<RationalTime, OtioParseError> {
    let obj = v
        .as_object()
        .ok_or_else(|| OtioParseError::NotAnObject(schema.to_string()))?;
    let value =
        obj.get("value")
            .and_then(Value::as_f64)
            .ok_or_else(|| OtioParseError::MissingField {
                schema: schema.to_string(),
                field: "value",
            })?;
    let rate = obj
        .get("rate")
        .and_then(Value::as_f64)
        .filter(|r| *r > 0.0)
        .ok_or_else(|| OtioParseError::MissingField {
            schema: schema.to_string(),
            field: "rate",
        })?;
    Ok(RationalTime { value, rate })
}

fn time_range_from_json(v: &Value, schema: &str) -> Result<TimeRange, OtioParseError> {
    let obj = v
        .as_object()
        .ok_or_else(|| OtioParseError::NotAnObject(schema.to_string()))?;
    let start_time = rational_time_from_json(
        obj.get("start_time")
            .ok_or_else(|| OtioParseError::MissingField {
                schema: schema.to_string(),
                field: "start_time",
            })?,
        schema,
    )?;
    let duration = rational_time_from_json(
        obj.get("duration")
            .ok_or_else(|| OtioParseError::MissingField {
                schema: schema.to_string(),
                field: "duration",
            })?,
        schema,
    )?;
    Ok(TimeRange {
        start_time,
        duration,
    })
}

/// `None` means "recognized reference kind, genuinely offline" ([`MediaReference::Missing`]);
/// `Some(warning)` alongside it flags an unrecognized reference schema (e.g.
/// `ImageSequenceReference.1`) that this slice conservatively treats the same way rather than
/// guessing at a URL from fields it doesn't model.
fn media_reference_from_json(v: &Value) -> (MediaReference, Option<String>) {
    let Some(obj) = v.as_object() else {
        return (
            MediaReference::Missing,
            Some("media reference is not a JSON object; treated as offline".to_string()),
        );
    };
    match schema_tag(obj) {
        "ExternalReference.1" => {
            let target_url = obj
                .get("target_url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            (MediaReference::External { target_url }, None)
        }
        "MissingReference.1" => (MediaReference::Missing, None),
        other => (
            MediaReference::Missing,
            Some(format!(
                "unrecognized media reference schema '{other}'; treated as offline"
            )),
        ),
    }
}

fn marker_kind_from_color(color: &str) -> MarkerKind {
    match color {
        "RED" => MarkerKind::ToDo,
        "BLUE" => MarkerKind::Chapter,
        "YELLOW" => MarkerKind::Highlight,
        // GREEN (this module's own default) and anything unrecognized.
        _ => MarkerKind::Standard,
    }
}

fn marker_from_json(v: &Value) -> Result<InterchangeMarker, OtioParseError> {
    let obj = v
        .as_object()
        .ok_or_else(|| OtioParseError::NotAnObject("Marker".to_string()))?;
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let marked_range = time_range_from_json(
        obj.get("marked_range")
            .ok_or_else(|| OtioParseError::MissingField {
                schema: "Marker".to_string(),
                field: "marked_range",
            })?,
        "Marker.marked_range",
    )?;
    let color = obj.get("color").and_then(Value::as_str).unwrap_or("GREEN");
    Ok(InterchangeMarker {
        name,
        marked_range,
        kind: marker_kind_from_color(color),
    })
}

/// oca's own `Clip.1`/`Clip.2` writer round-trips `speed_factor`/`transition_in` through
/// `metadata.oca` (see [`clip_json`]'s own comment on why — neither field has a native OTIO
/// slot at this schema version). Reading it back here is what makes an oca-exported `.otio`
/// round-trip exactly; a file from another tool simply won't have this key, so both fields fall
/// back to their ordinary defaults (`speed_factor` 1.0, no transition) exactly as if the
/// metadata block were absent, which for a third-party file it is.
fn oca_metadata_from_json(
    obj: &serde_json::Map<String, Value>,
) -> (f32, Option<InterchangeTransitionKind>) {
    let Some(oca) = obj.get("metadata").and_then(|m| m.get("oca")) else {
        return (1.0, None);
    };
    let speed_factor = oca
        .get("speed_factor")
        .and_then(Value::as_f64)
        .map(|v| v as f32)
        .unwrap_or(1.0);
    let transition_in = oca
        .get("transition_in")
        .and_then(Value::as_str)
        .and_then(|s| match s {
            "Fade" => Some(InterchangeTransitionKind::Fade),
            "HardCut" => Some(InterchangeTransitionKind::HardCut),
            "Slide" => Some(InterchangeTransitionKind::Slide),
            "Zoom" => Some(InterchangeTransitionKind::Zoom),
            _ => None,
        });
    (speed_factor, transition_in)
}

fn clip_from_json(
    v: &Value,
    schema: &str,
    warnings: &mut Vec<String>,
) -> Result<InterchangeClip, OtioParseError> {
    let obj = v
        .as_object()
        .ok_or_else(|| OtioParseError::NotAnObject(schema.to_string()))?;
    let name = obj.get("name").and_then(Value::as_str).unwrap_or("");
    let source_id = name
        .strip_prefix("Clip-")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    // Resolved before `source_range` -- a clip with no `source_range` of its own (real, common:
    // OTIO's own convention is "use the full length of the media") falls back to this
    // reference's `available_range`, so the raw JSON has to be in hand first.
    let (media_reference, media_reference_json) = match schema {
        "Clip.1" => match obj.get("media_reference") {
            Some(v) if !v.is_null() => {
                let (reference, warning) = media_reference_from_json(v);
                if let Some(warning) = warning {
                    warnings.push(format!("clip '{name}': {warning}"));
                }
                (reference, Some(v))
            }
            _ => (MediaReference::Missing, None),
        },
        // Clip.2: a `media_references` map keyed by name plus an `active_media_reference_key`
        // pointing at the one actually in use -- oca's own model only ever carries one
        // reference per clip, so every entry but the active one is genuinely dropped, not just
        // deferred; flagged whenever the map holds more than the one we keep.
        _ => {
            let map = obj.get("media_references").and_then(Value::as_object);
            match map {
                Some(map) if !map.is_empty() => {
                    let active_key = obj
                        .get("active_media_reference_key")
                        .and_then(Value::as_str);
                    let (chosen_key, chosen_value) =
                        match active_key.and_then(|k| map.get(k).map(|v| (k.to_string(), v))) {
                            Some((key, value)) => (key, value),
                            None => {
                                let mut keys: Vec<&String> = map.keys().collect();
                                keys.sort();
                                let key = keys[0].clone();
                                (key.clone(), &map[&key])
                            }
                        };
                    if map.len() > 1 {
                        warnings.push(format!(
                            "clip '{name}': Clip.2 has {} media references; only '{chosen_key}' was kept, the rest were dropped",
                            map.len()
                        ));
                    }
                    let (reference, warning) = media_reference_from_json(chosen_value);
                    if let Some(warning) = warning {
                        warnings.push(format!("clip '{name}': {warning}"));
                    }
                    (reference, Some(chosen_value))
                }
                _ => (MediaReference::Missing, None),
            }
        }
    };

    // A null/absent `source_range` is OTIO's own "use the full available_range of the media"
    // convention (confirmed against a real Clip-004 in the reference project's own
    // `simple_cut.otio` sample), not a malformed clip -- fall back to the resolved reference's
    // `available_range` before giving up.
    let source_range_json = obj.get("source_range").filter(|v| !v.is_null());
    let source_range = match source_range_json {
        Some(v) => time_range_from_json(v, "Clip.source_range")?,
        None => {
            let available_range = media_reference_json
                .and_then(|r| r.as_object())
                .and_then(|r| r.get("available_range"))
                .filter(|v| !v.is_null());
            match available_range {
                Some(v) => time_range_from_json(v, "Clip.media_reference.available_range")?,
                None => {
                    return Err(OtioParseError::MissingField {
                        schema: "Clip".to_string(),
                        field: "source_range",
                    })
                }
            }
        }
    };

    let (speed_factor, transition_in) = oca_metadata_from_json(obj);

    Ok(InterchangeClip {
        source_id,
        source_range,
        media_reference,
        speed_factor,
        transition_in,
    })
}

/// Maps a real OTIO `Transition.1`'s `transition_type` onto oca's own coarser taxonomy. Every
/// value seen in practice (`SMPTE_Dissolve`, the only one the standard names) is some form of
/// cross-fade, so this is deliberately not trying to distinguish "Fade" from "Slide"/"Zoom" from
/// a `Transition.1` alone -- those are oca-specific visual effects with no OTIO equivalent, not
/// a real ambiguity this mapping is failing to resolve.
fn transition_kind_from_json(v: &Value) -> InterchangeTransitionKind {
    let _ = v;
    InterchangeTransitionKind::Fade
}

/// Parses one track's `children` array, in document order. Returns the track's own
/// [`InterchangeTrackItem`]s; anything unsupported is pushed onto `warnings` (with `track_name`
/// for context) and either skipped outright or replaced with a same-duration
/// [`InterchangeTrackItem::Gap`] so later items keep their real on-timeline position — see this
/// module's own doc comment for exactly which shapes get which treatment.
fn track_items_from_json(
    children: &[Value],
    track_name: &str,
    warnings: &mut Vec<String>,
) -> Vec<InterchangeTrackItem> {
    let mut items = Vec::with_capacity(children.len());
    // A `Transition.1` item never advances the track's own timing cursor (see this module's doc
    // comment) -- it attaches to whichever real clip comes right after it.
    let mut pending_transition: Option<InterchangeTransitionKind> = None;

    for child in children {
        let Some(obj) = child.as_object() else {
            warnings.push(format!("track '{track_name}': skipped a non-object item"));
            continue;
        };
        let schema = schema_tag(obj);
        match schema {
            "Gap.1" => {
                match obj.get("source_range").ok_or(OtioParseError::MissingField {
                    schema: "Gap.1".to_string(),
                    field: "source_range",
                }) {
                    Ok(range_val) => match time_range_from_json(range_val, "Gap.1") {
                        Ok(range) => items.push(InterchangeTrackItem::Gap(range)),
                        Err(e) => warnings.push(format!(
                            "track '{track_name}': skipped a malformed Gap.1 ({e})"
                        )),
                    },
                    Err(e) => warnings.push(format!(
                        "track '{track_name}': skipped a malformed Gap.1 ({e})"
                    )),
                }
            }
            "Clip.1" | "Clip.2" => match clip_from_json(child, schema, warnings) {
                Ok(mut clip) => {
                    if let Some(kind) = pending_transition.take() {
                        clip.transition_in = Some(kind);
                    }
                    items.push(InterchangeTrackItem::Clip(clip));
                }
                Err(e) => warnings.push(format!(
                    "track '{track_name}': skipped a malformed {schema} ({e})"
                )),
            },
            "Transition.1" => {
                warnings.push(format!(
                    "track '{track_name}': Transition.1 approximated as a plain transition on \
                     the following clip -- real overlap/offset timing is not modeled"
                ));
                pending_transition = Some(transition_kind_from_json(
                    child.get("transition_type").unwrap_or(&Value::Null),
                ));
            }
            other => {
                // A nested compound clip (Stack.1/Track.1 as a track child) or any other
                // unrecognized item schema. Recursive compound-clip import is a separate, later
                // slice -- but its own on-timeline duration is still real and must be preserved
                // so every later sibling keeps its correct start time.
                match obj.get("source_range").filter(|v| !v.is_null()) {
                    Some(range_val) => match time_range_from_json(range_val, other) {
                        Ok(range) => {
                            warnings.push(format!(
                                "track '{track_name}': unsupported item schema '{other}' \
                                 replaced with an equal-duration gap; its own content was dropped"
                            ));
                            items.push(InterchangeTrackItem::Gap(range));
                        }
                        Err(e) => warnings.push(format!(
                            "track '{track_name}': skipped unsupported item schema '{other}' \
                             with an unreadable source_range ({e}); later items on this track \
                             may be misaligned"
                        )),
                    },
                    None => warnings.push(format!(
                        "track '{track_name}': skipped unsupported item schema '{other}' with \
                         no explicit source_range; later items on this track may be misaligned"
                    )),
                }
            }
        }
    }
    items
}

/// Parses a real OpenTimelineIO `.otio` JSON document (already deserialized to a
/// [`serde_json::Value`], e.g. via `serde_json::from_str`) into an [`InterchangeTimeline`] —
/// the reverse of [`serialize_interchange_timeline`]. See this module's own doc comment for
/// exactly what's supported, what's approximated-with-a-warning, and what's a hard
/// [`OtioParseError`].
pub fn parse_otio_json(value: &Value) -> Result<OtioImportResult, OtioParseError> {
    let obj = value
        .as_object()
        .ok_or_else(|| OtioParseError::NotAnObject("Timeline".to_string()))?;
    let schema = schema_tag(obj);
    if schema != "Timeline.1" {
        return Err(OtioParseError::UnrecognizedTopLevelSchema(
            schema.to_string(),
        ));
    }
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let stack_val = obj
        .get("tracks")
        .ok_or_else(|| OtioParseError::MissingField {
            schema: "Timeline.1".to_string(),
            field: "tracks",
        })?;
    let stack_obj = stack_val
        .as_object()
        .ok_or_else(|| OtioParseError::NotAnObject("Stack".to_string()))?;
    let stack_schema = schema_tag(stack_obj);
    if stack_schema != "Stack.1" {
        return Err(OtioParseError::UnrecognizedTopLevelSchema(
            stack_schema.to_string(),
        ));
    }

    let mut warnings = Vec::new();

    let markers = stack_obj
        .get("markers")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|m| match marker_from_json(m) {
                    Ok(marker) => Some(marker),
                    Err(e) => {
                        warnings.push(format!("skipped a malformed marker: {e}"));
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let children = stack_obj
        .get("children")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut tracks = Vec::with_capacity(children.len());
    for child in &children {
        let Some(track_obj) = child.as_object() else {
            warnings.push("skipped a non-object top-level track item".to_string());
            continue;
        };
        let track_schema = schema_tag(track_obj);
        if track_schema != "Track.1" {
            warnings.push(format!(
                "skipped unsupported top-level item '{track_schema}' -- only Track.1 children \
                 of the root Stack are imported"
            ));
            continue;
        }
        let track_name = track_obj
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let kind = match track_obj
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("Video")
        {
            "Video" => InterchangeTrackKind::Video,
            "Audio" => InterchangeTrackKind::Audio,
            other => {
                warnings.push(format!(
                    "skipped track '{track_name}' with unsupported kind '{other}'"
                ));
                continue;
            }
        };
        let track_children = track_obj
            .get("children")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let items = track_items_from_json(&track_children, &track_name, &mut warnings);
        tracks.push(InterchangeTrack {
            name: track_name,
            kind,
            items,
        });
    }

    Ok(OtioImportResult {
        timeline: InterchangeTimeline {
            name,
            tracks,
            markers,
        },
        warnings,
    })
}

#[cfg(test)]
#[path = "otio_json_test.rs"]
mod tests;
