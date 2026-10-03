# Video Editor Domain Model — `avcore` (crate `core`)

Ground truth against `crates/core/src/project.rs`, `crates/core/src/timeline.rs`,
`crates/core/src/media.rs`. Field lists are abbreviated (types simplified); read the real
struct for exact types before relying on this for a signature.

## CURRENTLY IMPLEMENTED

### Project (`project.rs`)
```
Project { id, name, last_edited: Recency, summary, media_library: Vec<MediaAsset>,
          sequences: Vec<Sequence>, active_sequence: usize,
          file_path: Option<PathBuf> (#[serde(skip)]),
          panel_layout: Option<PanelLayout>, smart_bins: Vec<SmartBin> }
```
One project holds many sequences and one shared media library. `active_sequence` is an index,
not an id — the editor's sequence tabs switch it.

### Sequence (`project.rs`)
```
Sequence { id, name, timeline: Timeline, export_settings: SequenceExportSettings }
```
Each sequence is an independently editable timeline with its own export settings.

### Timeline (`timeline.rs`)
```
Timeline { tracks: Vec<Track>, playhead_secs: f64, markers: Vec<Marker>,
           multicam_groups: Vec<MulticamGroup> }
```
`playhead_secs` is **persisted**, per-sequence — switching sequence tabs keeps each one's
playhead independently.

### Track (`timeline.rs`)
```
Track { id, name, kind: TrackKind, clips: Vec<ClipInstance>, text_clips: Vec<TextClip>,
        shape_clips: Vec<ShapeClip>, visible: bool, audio_role: AudioRole,
        color_label: Option<[u8;3]> }
```
Video/overlay clips, text clips, and shape clips are three separate `Vec`s on the same track,
not one polymorphic clip list.

### ClipInstance (`timeline.rs`)
```
ClipInstance { id, asset_id, start_secs, source_in_secs, source_out_secs,
               nested_sequence_id: Option<u64>, composite_id, color_label, gain_db, frozen,
               speed_factor, speed_ramp_end_factor,
               crop_x/y/w/h (+ per-axis Vec<Keyframe<f32>>), mask/chroma-key fields,
               scale/rotation/opacity keyframe vectors, ... }
```
A clip references a `MediaAsset` by id **or**, if `nested_sequence_id: Some(seq_id)`, is a
**compound clip** whose visible content is another `Sequence` in the same `Project` — `asset_id`
is then ignored/stale. `source_in_secs`/`source_out_secs` trim into the nested sequence's own
rendered timeline in that case. `crate::nested_sequence::materialize_nested_sequences` recursively
renders nested sequences (with cycle detection) to a cached temp file exposed as a synthetic
`MediaAsset`, so every downstream consumer (preview resolution, export segment resolution) needs
zero special-casing for compound clips. There is **no separate "compound clip" type** — it is
this same struct with one field set.

### MediaAsset (`media.rs`)
```
MediaAsset { id, file_name, source_path: PathBuf, kind: MediaKind, has_audio, duration_secs,
             codec, source_bitrate_mbps, resolution: Option<(u32,u32)>, fps: Option<f32>,
             sample_rate_khz, loudness: Option<LoudnessMetrics>,
             proxy_path: Option<PathBuf> (#[serde(skip)], derived cache),
             waveform_peaks: Option<Vec<(f32,f32)>> (serialized) }
```
Imported/generated media, distinct from `ClipInstance` — the domain rule "imported media stays
distinct from timeline instances" is real and enforced by this being a separate id space entirely
(`ClipInstance::asset_id` is a foreign key, never an inline copy).

### Effects / Keyframes (`keyframe.rs`)
Generic, not a per-property enum:
```
Keyframe<T> { time_fraction: f32, value: T }
trait Lerp { fn lerp(self, other: Self, t: f32) -> Self }  // impl'd per T (f32, Position, ...)
fn evaluate_keyframes<T: Lerp + Copy>(keyframes: &[Keyframe<T>], time_fraction: f32, default: T) -> T
```
Individual animatable properties are plain typed fields on `ClipInstance`
(`crop_x_keyframes: Vec<Keyframe<f32>>`, etc.), not entries in one shared effect list. **Keyframes
currently drive export only** — the Bush live preview animates most of them (intensity curves, gain keyframes and mattes are still approximations or missing, see `spec/matrix/bush.md`) (this is a real,
documented gap, not an oversight to silently work around).

### Markers (`timeline.rs`)
```
Marker { id, position_secs, label, kind: MarkerKind, completed }
MarkerKind { Standard, ToDo, Chapter, Highlight }
```

### Playhead
Lives on `Timeline` (`playhead_secs`), persisted, per-sequence.

### Selection
**Not** part of the domain model. `selected_clip_id: Option<u64>` and
`multi_selected_clip_ids: HashSet<u64>` live on `ui::App`, never serialized into `.ocproj`. The
generic rule "selection is UI/editor state, not persistent project data" is confirmed true here —
keep it that way when adding new selection-like state (don't add a "selected" field to a domain
struct).

### Preview vs. export — two independent rendering paths
- `crates/core/src/preview.rs` — removed (GStreamer is gone). Real-time preview is `crates/bush`
  (FFmpeg decode, CPU/GPU compositor, cpal audio), driven from
  `apps/desktop/src-tauri/src/bush_adapter/`, streaming JPEG frames to the webview.
- `crates/core/src/render.rs`/`export.rs` — calls into `avbridge`'s FFmpeg/avfilter chain
  (`csrc/export.c`, `filters.c`, `timeline_export.c`) for the actual export, with `loudnorm`
  normalization.

These are maintained and extended **independently** — several `ClipInstance` fields are
documented as "wired into export's avfilter chain, no live preview effect yet." When adding a new
per-clip visual effect, decide explicitly whether it needs both paths or is export-only (as
keyframes currently are), and say so — don't assume parity exists.

### Undo/redo (`undo.rs`)
Full-**`Sequence`**-snapshot based, explicitly not command-pattern:
```
UndoStack { capacity, undo: Vec<Sequence>, redo: Vec<Sequence> }
```
Scoped to one sequence, not the whole project; cleared on sequence switch.
`App::push_undo_snapshot` clones the current `Sequence` before a mutation.
`App::push_undo_snapshot_for_drag` coalesces continuous drag/slider edits into one push per
gesture. Do not design a feature assuming per-field undo granularity — it doesn't exist.

### Persistence (`persistence.rs`)
4-byte magic + 1-byte version + gzip-compressed **struct-map-mode** MessagePack, shared by three
sidecar kinds: `OCPJ` (`.ocproj`), `OCQU` (`.ocqueue`), `OCTR` (`.octr`, transcript). Struct-map
mode keeps field names in the encoding so `#[serde(default)]` gives real forward compatibility
when a field is added later — this is why new `ClipInstance`/`Project` fields must be
`#[serde(default)]` and never renamed without a migration story. Decompression is capped
(`MAX_DECOMPRESSED_BYTES`) against gzip-bomb DoS — any new sidecar-like persisted format should
reuse `to_framed_bytes`/`from_framed_bytes`, not invent its own framing.

## PLANNED / REQUIRED (per `spec/ROADMAP.md`, not yet built as of this writing)
See `spec/ROADMAP.md` and `spec/architecture/competitive-feature-plan.md` for the live backlog —
this file intentionally does not duplicate a fast-moving list. Notable domain-model implication
already visible: CF-02 (gameplay event ingestion) will need a new persisted sidecar type, which
should reuse the `OCxx` framing convention above rather than a new one.

## NOT YET DESIGNED
Nothing at the domain-model layer is currently flagged as architecturally undecided; open
questions are feature-scoped and tracked in `spec/ROADMAP.md`/`spec/matrix/*.md`, not here.
