# Audio mixer, HDR and color management

Scope, findings, sequencing and acceptance criteria for three gaps between oca and the editors it
competes with: a real audio mixer, HDR footage, and color management and grading. Status stays in
[ROADMAP.md](../ROADMAP.md); this file defines what to build and how to know it works.

The product is a gameplay editor. The footage that matters most is OBS and NVIDIA/Xbox captures:
often HEVC or AV1, often 10-bit HDR (BT.2020, PQ) on a Windows HDR display, often several audio
tracks (game, microphone, chat) in one file. Each slice below is justified against that footage
before any broader editor-parity argument.

## Status (2026-10-12)

| slice | state |
|---|---|
| CM-00 SDR decode matrix | done: preview and proxies use the tagged (or size-based) matrix and range, tested against five generated clips |
| CM-01 colour metadata | done: probe, `MediaAsset::streams`, project format 5 (v3 and v4 files still open, golden files pin both), HDR mark in the library |
| CM-02 HDR to SDR | done for preview, proxies and export (single and layered); no UI to choose the handling yet (the field exists, default Auto) |
| CM-03 output tagging | done: every export is tagged Rec.709 limited range and SD or full-range sources are converted to it |
| MX-01 multi-stream audio | done as copies: each extra track is copied out as its own audio asset (see below); no per-clip stream selector |
| MX-02 track strip | done for fader and pan per track (project format 6, preview and export, a mixer button in the track header); no per-track meter and no master fader yet |
| CM-06 scopes | done: waveform, RGB parade, histogram and vectorscope with reference lines, a strip under the monitor, drawn on request from the frame on screen at a small size (the Tauri shell had no scopes at all before) |
| CM-04, CM-05, MX-00, MX-03, MX-04 | not started |

Decisions taken while building:

- Tone curve: **Mobius with a knee at half of SDR white**, not Hable. Measured on a 200,60,40 colour
  stored as PQ: Hable gave 129,39,25 (darkened), Mobius 199,60,39, and a 1000-nit white still maps to 255.
- Preview cost of HDR: 1080p native 27 ms per frame against 4.8 ms for SDR (4 vCPU), paid at the
  monitor size because the scale runs first; proxies of HDR are SDR so scrubbing never pays it.
- Multi-stream: instead of threading a stream index through the waveform, loudness, preview and
  export code (four C pipelines), each extra track is copied out without decoding (`avbridge_remux_audio_stream`)
  into the project cache and imported as an ordinary audio asset that remembers its recording
  (`MediaAsset::audio_stream`). A lost copy is made again when the project loads.
- Pan law: the side the pan moves away from falls off as a quarter sine, the other stays at unity, so the
  centre changes nothing. One law in the model, the preview mixer and the export graph, pinned by a test.
  The export normalizes the whole mix to its loudness target, so a fader changes a track relative to the others
  there; the preview plays the absolute level.
- Scopes are served as JPEG without chroma subsampling: the 4:2:0 encoding of a photo smeared a one-pixel
  red parade trace to a dim blur (red read 40 instead of 255).
- Found on the way, all fixed: proxies failed for FLAC, Opus and PCM audio in Matroska; ten protocol route tests that had not compiled since a module regrouping (now wired, with a guard test); a failed proxy
  left a partial file that passed for a finished one; graph sources did not carry the decoder's matrix
  and range, so full-range footage was exported with shifted colours.

## 1. Findings

Everything here was read from the code or measured in the sandbox (4 vCPU, no GPU). Anything that
needs hardware is marked as such.

### 1.1 Color: what exists

- Probe (`crates/core/src/av/probe.rs`, `ProbedMedia`) reads codec, resolution, fps, bitrate and
  sample rate. **No pixel format, bit depth, color matrix, primaries, transfer or range** is read
  or stored anywhere in `avbridge`, `core`, `bush` or `desktop`.
- Preview decode (`avbridge/csrc/decode_session.c`, `threaded_sws`) converts every frame to
  8-bit `RGBA` with swscale and sets only the source and destination formats. Compositing is 8-bit
  (`bush/src/blend8.rs`, `cpu_renderer.rs`, and the wgpu path).
- Export (`filters.c`, `gpu_encoder.c`, `matte_encode.c`) always ends in `yuv420p` (`nv12` or a
  hardware surface for some GPU encoders) and H.264 (`libopenh264`, `libx264` if present, NVENC,
  QSV, VAAPI). No 10-bit, no HEVC, no ProRes, no tagging of color metadata.
- Grading tools (`ColorSection.tsx`, `ClipInstance`): brightness, contrast, saturation, color
  filters, 3D LUT (`.cube`), vignette, chroma key. No exposure, temperature, tint, lift/gamma/gain,
  curves or hue controls. Scopes (`core/src/graphics/scopes.rs`) are a grayscale luma waveform and
  a vectorscope with no graticule, no RGB parade, no histogram.

### 1.2 Color: measured

A flat color clip (sRGB 200, 60, 40) encoded to YUV with the BT.709 matrix, limited range, lossless
(FFV1), decoded by the preview path (`FrameSampler`, the same `DecodeSource` the preview uses):

| clip | preview shows (R, G, B) | expected |
|---|---|---|
| BT.709, tagged | 186, 46, 41 | 200, 60, 40 |
| BT.709, untagged | 186, 46, 41 | 200, 60, 40 |
| HDR10 (BT.2020, PQ, 10-bit) | 119, 82, 62 | tone mapped to about 200, 60, 40 |

The first two rows are a **bug in ordinary SDR footage**: the values match decoding BT.709 YUV
with the BT.601 matrix (computed independently: 188, 47, 42). Every HD clip is shifted, in the
preview only. Export keeps the YUV data as it is, so the preview and the exported file disagree.
The third row is the expected washed-out look of HDR decoded as SDR. The probe is
`color_probe` in `core/src/av/frame_sampler/frame_sampler_test.rs`.

Not measured here: what the export pipeline does with the same clips (needs the same probe run
over an exported file) and anything on a real HDR capture or a GPU.

### 1.3 Audio: what exists

- Per clip: gain (dB) with keyframes, speed, fade in and out, voice cleanup (high-pass 80 Hz,
  `afftdn` denoise, compressor, limiter), freeze.
- Per track: `muted`, `solo`, `audio_role` (Mic, Music, others), which drives ducking
  (`sidechaincompress`: Mic ducks Music).
- Master: `loudnorm` to a target LUFS plus a limiter at 0.95 (`MASTERING_*` in
  `bridge_internal.h`), 48 kHz stereo, AAC 192 kb/s. Measurement and normalization per asset and
  per export. K-weighted momentary LUFS and peaks in the preview meter.
- Export graph (`audio_mix.c`): one `amix` of every audible clip.

### 1.4 Audio: gaps

1. **One audio stream per file.** `audio_mix.c`, `loudness.c`, `waveform.c`, `timeline_export.c`
   and the decode session all take the first (or "best") audio stream. An OBS file with game,
   microphone and chat on separate tracks plays and exports only one of them. Probe does not
   report the stream count.
2. **Preview and export disagree.** Ducking and voice cleanup exist only in the export graph
   (`bush` and `desktop` contain neither). What the user hears while editing is not what is
   exported. `core/src/av/voice_cleanup_preview.rs` renders a short snippet, not live playback.
3. No track fader, no pan, no per-track meters, no buses or sends, no master fader.
4. No EQ, no gate, no per-track compressor as user controls (the filters exist in FFmpeg).
5. Stereo only. A mono microphone is not placed in a stereo field deliberately.
6. No audio crossfade at cuts (fades exist per clip), no audio-only export (FLAC, WAV, MP3, Opus
   encoders are present in the FFmpeg build).

### 1.5 Constraints

- The shipped FFmpeg is a static **LGPL** build (`spec/matrix/packaging.md`): no `libx264` or
  `libx265`. Available video encoders in the reference build: `libopenh264`, `prores_ks`,
  `dnxhd`, `libsvtav1`, `libaom-av1`, and the hardware encoders `h264_nvenc`, `hevc_nvenc`,
  `av1_nvenc`. So 10-bit HEVC output means a hardware encoder, and ProRes/DNxHR are software and
  safe to ship. Filters present: `zscale` (needs zimg, present), `tonemap`, `colorspace`,
  `lut3d`, `libplacebo` (needs Vulkan), `equalizer`, `anequalizer`, `acompressor`, `agate`,
  `sidechaincompress`, `acrossfade`, `pan`, `stereotools`, `loudnorm`, `alimiter`.
- The `.ocproj` layout is positional postcard. Adding a field to `MediaAsset`, `Track` or
  `ClipInstance` needs a `PROJECT_FORMAT_VERSION` bump and an upgrade of the old layout, the way
  `upgrade_v3` does. `golden_project_test.rs` fails if a field changes without that.
- Preview is 8-bit by design and stays so in this plan. HDR in the preview means a tone-mapped
  SDR image, not an HDR display path.
- GPU encoders, HDR displays and real captures cannot be verified in the sandbox. Each slice says
  what is verifiable here.

## 2. Principles

1. **SDR correctness before HDR.** The 601/709 bug affects all footage and is the cheapest fix.
2. **Preview equals export.** A color or audio feature ships only when both paths produce the
   same result, checked by a probe like `color_probe` for pixels and an equivalent for audio.
3. **The project has a working color space (Rec.709 SDR) and always exports tagged.** Sources
   are converted into it on decode; the user decides how HDR sources are treated.
4. **No silent quality loss.** Anything that tone maps, clips or downmixes says so in the UI and
   in the export summary.
5. **Reuse FFmpeg filters** for export and write the matching preview path in `bush`, instead of
   a second implementation of the same math.

## 3. Slices

Order: CM-00, CM-01, CM-02, MX-00, MX-01, MX-02, then the rest by value. Each slice is split by
layer in commits (`avbridge`, `core`, `desktop` backend, `desktop` frontend, tests).

### CM-00: correct SDR decode (bug)

Convert with the frame's own matrix and range: BT.709 for HD when untagged (the usual rule is
height 720 and above, or width 1280 and above), BT.601 for SD, BT.2020 when tagged, and the
source range (`tv` or `pc`) honored, in `threaded_sws` through `sws_setColorspaceDetails` or the
frame properties, whichever the FFmpeg 7.1+ API requires (verify, do not assume).

Acceptance:
- `color_probe` shows 200, 60, 40 within 1 per channel for BT.709 tagged and untagged, and for
  BT.601 and full-range variants (add those clips to the probe).
- The preview of an exported file matches the preview of its source for a color chart clip.
- No measurable decode slowdown (`decode_throughput` example, same clip, before and after).

### CM-01: color metadata

Probe reads pixel format, bit depth, matrix, primaries, transfer, range and the audio stream
count. `MediaAsset` stores them (`#[serde(default)]` is not enough for postcard: bump
`PROJECT_FORMAT_VERSION` to 5, add `upgrade_v4`, regenerate the golden file on purpose). The media
library shows an "HDR" badge and the detail row shows the transfer and bit depth.

Acceptance: probing the clips from section 1.2 reports the right values; opening a v4 project
works; the golden test pins v5.

### CM-02: HDR to SDR for preview and export

Per-asset **HDR handling**: Auto (default: tone map when the transfer is PQ or HLG), Tone map, or
Treat as SDR (the user's escape hatch). Tone mapping targets Rec.709 with a configurable
reference white (203 nits default) and a tone curve (Hable or Mobius, not clipping).

- Export: `zscale` to linear, `tonemap`, `zscale` back to BT.709 at the head of each segment graph
  (`filters.c`), only for flagged assets, so SDR export is untouched.
- Preview: swscale cannot tone map. Options, in order of preference: run the same filter graph
  inside the decode session for flagged assets (one cost per decoded frame, measured), or decode
  to 16-bit and tone map on the CPU with a precomputed 3D LUT (the project already has trilinear
  LUT code, `preview_effects`). Pick by measurement; budget: flagged 1080p60 must not drop below
  real time on 4 vCPU, otherwise the proxy path bakes the tone mapping in (proxies are SDR).
- Proxies of HDR sources are always generated tone mapped, so scrubbing is cheap.

Acceptance: the HDR clip from 1.2 decodes to within a few percent of 200, 60, 40 in preview and
in an exported file; SDR clips are bit-identical to before; a clip tagged HDR but set to "Treat
as SDR" looks like today's output. Needs a real HDR capture to judge highlights (not verifiable
here beyond synthetic clips).

### CM-03: project color space and output tagging

Sequences carry a working space (Rec.709 only at first; the field exists so Rec.2020 PQ can be
added). Every export tags matrix, primaries and transfer on the stream and in the container so
players do not guess. Export summary says "SDR Rec.709" or the HDR mode.

Acceptance: `ffprobe` of an export shows the tags; a player-independent probe decodes the export
to the same RGB as the preview.

### CM-04: 10-bit and HDR output (phase 2, needs a product decision)

Deliverables beyond SDR H.264: HEVC 10-bit through `hevc_nvenc` (hardware only), ProRes 422 HQ
(`prores_ks`) and DNxHR (`dnxhd`) for hand-off to other editors, AV1 through SVT-AV1. HDR10 output
needs the filter graph in 10-bit (`yuv420p10le`) end to end, mastering display and content light
metadata, and the bitrate-matching logic taught about the new codecs. The preview stays 8-bit.
Verifiable here: ProRes, DNxHR, 10-bit software paths. Not verifiable: NVENC.

### CM-05: grading tools

Add to `ClipInstance` and `ColorSection`: exposure, temperature, tint, hue, highlights and
shadows, lift/gamma/gain (three wheels), RGB curves. Export through `colorbalance`,
`colortemperature`, `curves`, `eq`/`colorlevels`; preview through the existing `layer_mods` and
`preview_effects` per-frame passes (and the GPU shader), written against the same formulas.
Keyframes follow the existing `Keyframe<T>` model. Presets and "copy grade to clip" reuse paste
formatting.

Acceptance: per tool, a preview-versus-export probe on a gradient chart within 2 levels per
channel; undo is one step per drag (existing coalescing).

### CM-06: scopes

Graticule (IRE or percent), RGB parade, histogram, false-color and zebra overlay on the monitor,
scope selection in a panel. Pure CPU on the already decoded frame (`scopes.rs`), same budget rule
as any per-frame work: computed on a downscaled frame and only while the panel is visible.

### MX-00: preview equals export for audio

Live ducking and voice cleanup in the preview mixer (`bush/src/audio_playback.rs`): a DSP chain
per clip (high-pass, denoise or a simpler gate, compressor, limiter) and sidechain ducking
between roles, implemented in Rust with the same parameters as `audio_mix.c`.

Acceptance: an audio probe renders the same 10 s project through the preview mixer and through the
export graph and the two differ by less than 0.5 dB RMS and 0.5 LU integrated; both run over a
fixture with a speech-like signal and music. Where the exact filter (`afftdn`) has no cheap
equivalent, the preview may use a documented approximation and the probe states its tolerance.

### MX-01: multi-stream audio

Probe lists audio streams (index, channels, language, title). Importing a file with several
streams offers "one clip per stream" (default for 2+ streams: separate audio tracks named from the
stream title, e.g. "Game", "Mic"), and `ClipInstance` or the audio segment carries the stream
index through decode, waveform, loudness, mix and proxy. The first-stream behavior remains the
default for single-stream files.

Acceptance: an OBS-style fixture with three streams imports as three clips, waveforms differ,
export contains all three mixed, and soloing one track exports only it.

### MX-02: track strip and master

`Track` gets `volume_db` and `pan`; the timeline header (or a collapsible mixer panel) shows a
fader, pan knob, mute, solo and a meter per audio track, plus a master fader and the existing
loudness readout. Export maps them to `volume` and `pan`/`stereotools`; preview mixer applies the
same gains. Fader and pan are automatable later (reuse `Keyframe<f32>`).

Acceptance: a fader at -6 dB halves amplitude in preview and export (probe); pan hard left leaves
the right channel silent; mono sources are placed through `pan`; undo is one step per drag.

### MX-03: EQ and dynamics

Per track (and optionally per clip): 3-band parametric EQ plus high-pass and low-pass, gate,
compressor, with presets (Voice, Game, Music). Export through `equalizer`, `highpass`,
`lowpass`, `agate`, `acompressor`; preview through biquad and envelope code in Rust, verified by
the same audio probe as MX-00.

### MX-04: crossfades and audio-only export

`acrossfade` at cuts between audio clips (default 10 ms to avoid clicks, user adjustable), and an
"Export audio" option with WAV 24-bit, FLAC, MP3 and Opus using the encoders already in the build.

## 4. Data model and format changes (summary)

| change | where | format impact |
|---|---|---|
| color fields, audio stream list | `MediaAsset` | bump to v5, `upgrade_v4` |
| HDR handling, working space | `MediaAsset`, `Sequence` | same bump |
| audio stream index | `ClipInstance`, `AudioSegment` | same bump |
| `volume_db`, `pan` | `Track` | same bump |
| grading fields and keyframes | `ClipInstance` | one bump per release batch |
| EQ and dynamics settings | `Track` | idem |

Land all fields of one release in one version bump where possible; every bump regenerates
`golden.ocproj` in a commit of its own so the diff is reviewable.

## 5. Verification

Verifiable in the sandbox (software only):
- Pixel probes against generated chart clips (FFV1, ProRes, 10-bit) for CM-00, 01, 02, 05.
- Audio probes against generated signals for MX-00 to MX-04, plus multi-stream fixtures made with
  FFmpeg.
- Golden project file, the QA sweeps, frontend component and visual tests.

Needs the user's machine:
- Real HDR captures (OBS HEVC 10-bit) for CM-02 highlights and for tone map taste.
- NVENC HEVC 10-bit and AV1 (CM-04), GPU preview of any new pass.
- Playback of exports in the players that matter (browser, YouTube upload, phone).

## 6. Open decisions

1. Default for HDR sources: tone map automatically (recommended) or ask on import.
2. Which tone curve to ship first (Hable is the safer default for games).
3. Whether CM-04 is in scope for the next release, given the LGPL build: HEVC means hardware only.
4. Mixer placement: inline in track headers (compact, recommended) or a separate panel.
5. Multi-stream import default: separate tracks (recommended) or mix down.

## 7. Deliberate non-goals

HDR display output in the preview, Dolby Vision, ACES and full OCIO pipelines, surround (5.1 and
beyond), VST/AU plug-in hosting, automatic loudness riding (the export normalization stays), and
node-based grading.
