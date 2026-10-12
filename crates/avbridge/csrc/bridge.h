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

#ifndef AVBRIDGE_BRIDGE_H
#define AVBRIDGE_BRIDGE_H

#include <stddef.h>
#include <stdint.h>

/* Returns libavformat's packed version number (same encoding as avformat_version()). */
uint32_t avbridge_version(void);

typedef enum {
    PROBE_OK = 0,
    /* avformat_open_input() failed — bad path, unreadable file, unrecognized container. */
    PROBE_ERR_OPEN = 1,
    /* avformat_find_stream_info() failed — container opened but streams couldn't be read. */
    PROBE_ERR_STREAM_INFO = 2,
    /* No video or audio stream in the file. */
    PROBE_ERR_NO_MEDIA_STREAM = 3,
} ProbeStatus;

#define PROBE_MAX_AUDIO_STREAMS 8

typedef struct {
    /* 1 if the picked stream is video, 0 if audio. */
    int has_video;
    /* 1 when the container has at least one audio stream, including video+audio files. */
    int has_audio;
    double duration_secs;
    /* Short codec name (e.g. "h264", "aac") — always NUL-terminated. */
    char codec_name[32];
    /* Bits per second; 0 if neither the container nor the stream reports one. */
    int64_t bit_rate;
    /* Both 0 when has_video is 0. */
    int width;
    int height;
    /* Frame rate as a fraction; both 0 when unknown or has_video is 0. */
    int fps_num;
    int fps_den;
    /* 0 when has_video is 1 or the rate is unknown. */
    int sample_rate_hz;
    /* Video colour description, all 0 (unspecified) when has_video is 0. The enums are FFmpeg's
       own AVPixelFormat/AVColorSpace/AVColorPrimaries/AVColorTransferCharacteristic values;
       `pixel_format` is -1 when unknown. `color_full_range` is 1 for full (JPEG) range. */
    int pixel_format;
    int bit_depth;
    int color_space;
    int color_primaries;
    int color_transfer;
    int color_full_range;
    /* Every audio stream of the file, in stream order (at most PROBE_MAX_AUDIO_STREAMS are
       described, `audio_stream_count` counts them all). */
    int audio_stream_count;
    struct {
        int stream_index;
        int channels;
        int sample_rate_hz;
        char codec_name[32];
        char title[64];
        char language[16];
    } audio_streams[PROBE_MAX_AUDIO_STREAMS];
} ProbeInfo;

/* Probes the media file at `path` (UTF-8, NUL-terminated) and fills `out` on success.
   Picks the first video stream if there is one, otherwise the first audio stream. */
ProbeStatus avbridge_probe(const char *path, ProbeInfo *out);

typedef enum {
    REMUX_OK = 0,
    /* avformat_open_input() failed on in_path. */
    REMUX_ERR_OPEN_INPUT = 1,
    /* avformat_find_stream_info() failed. */
    REMUX_ERR_STREAM_INFO = 2,
    /* Couldn't allocate/guess an output format for out_path. */
    REMUX_ERR_ALLOC_OUTPUT = 3,
    /* Failed to create an output stream matching one of the input's streams. */
    REMUX_ERR_NEW_STREAM = 4,
    /* avio_open() failed on out_path (e.g. unwritable directory). */
    REMUX_ERR_OPEN_OUTPUT = 5,
    /* avformat_write_header() failed. */
    REMUX_ERR_WRITE_HEADER = 6,
    /* av_interleaved_write_frame() failed partway through. */
    REMUX_ERR_WRITE_FRAME = 7,
} RemuxStatus;

/* Demuxes `in_path` and remuxes every stream to `out_path` unchanged — no decode, no encode,
   no filtering. Equivalent to `ffmpeg -i in_path -c copy out_path`. */
RemuxStatus avbridge_remux_copy(const char *in_path, const char *out_path);

/* Copies one audio stream of in_path (by container stream index, as the probe lists them) to
   out_path without decoding: a standalone audio file holding just that track. The output format
   follows out_path's extension (use .mka, which holds any codec). REMUX_ERR_NEW_STREAM when the
   index is not an audio stream. */
RemuxStatus avbridge_remux_audio_stream(const char *in_path, const char *out_path,
                                        int stream_index);

typedef enum {
    ENCODE_OK = 0,
    ENCODE_ERR_OPEN_INPUT = 1,
    ENCODE_ERR_STREAM_INFO = 2,
    ENCODE_ERR_ALLOC_OUTPUT = 3,
    ENCODE_ERR_NEW_STREAM = 4,
    ENCODE_ERR_OPEN_OUTPUT = 5,
    ENCODE_ERR_WRITE_HEADER = 6,
    ENCODE_ERR_WRITE_FRAME = 7,
    /* The input has no audio stream to normalize. */
    ENCODE_ERR_NO_AUDIO_STREAM = 8,
    /* Couldn't find/open the audio decoder. */
    ENCODE_ERR_DECODER = 9,
    /* Couldn't build the loudnorm/limiter filter graph. */
    ENCODE_ERR_FILTER_GRAPH = 10,
    /* Couldn't find/open the AAC encoder. */
    ENCODE_ERR_ENCODER = 11,
    /* A decode/filter/encode call failed mid-stream (not at setup). */
    ENCODE_ERR_PIPELINE = 12,
    /* *cancel became nonzero mid-render — out_path is a truncated/invalid file, not written
       past the header. Not a failure: matches render.rs's RenderOutcome::Cancelled. */
    ENCODE_CANCELLED = 13,
    /* A segment (avbridge_encode_timeline_export only) has no video stream. */
    ENCODE_ERR_NO_VIDEO_STREAM = 14,
    /* A later segment's audio (sample rate/format/channel layout) doesn't match the first
       segment's — avbridge_encode_timeline_export keeps one audio filter graph open across
       the whole timeline (so loudnorm sees it as one continuous stream), which requires every
       segment to decode to the same PCM shape. */
    ENCODE_ERR_AUDIO_FORMAT_MISMATCH = 15,
    /* avbridge_encode_timeline_export was called with segment_count <= 0. */
    ENCODE_ERR_EMPTY_TIMELINE = 16,
} EncodeStatus;

/* Called periodically during the read/decode loop with the input packet's position, in
   seconds, that's just been processed. Never called with a value past the file's duration. */
typedef void (*ProgressCallback)(void *user_data, double seconds_processed);

/* Renders `in_path` to `out_path`: video passthrough-copied, audio decoded, normalized
   (loudnorm to target_lufs + a true-peak safety limiter) and re-encoded to AAC 192kbps.
   Equivalent to:
   ffmpeg -i in_path -af
   "loudnorm=I=<target_lufs>:TP=-1.0:LRA=11,alimiter=limit=0.95:attack=5:release=50" -c:v copy -c:a
   aac -b:a 192k out_path Fails with ENCODE_ERR_NO_AUDIO_STREAM if `in_path` has no audio stream.

   progress_cb/progress_user_data may both be NULL to skip progress reporting.
   cancel may be NULL to disable cancellation; otherwise checked between packets — once
   `*cancel` is nonzero, stops and returns ENCODE_CANCELLED without writing a trailer. */
EncodeStatus avbridge_encode_export(const char *in_path, const char *out_path, float target_lufs,
                                    ProgressCallback progress_cb, void *progress_user_data,
                                    const uint8_t *cancel);

typedef struct {
    /* UTF-8, NUL-terminated. */
    const char *source_path;
    /* Trim range within source_path, in seconds. */
    double source_in_secs;
    double source_out_secs;
    /* Per-clip linear gain in dB, applied (as the `volume` filter's dB form) before the
       shared loudnorm/limiter chain sees this segment's audio. */
    float gain_db;
    /* Pre-built avfilter chain description (e.g. "lutyuv=y=...,hflip"), UTF-8,
       NUL-terminated. Empty string ("") means no clip-specific video effect — the segment
       still goes through the canvas-conform (scale/pad/fps) stage and gets re-encoded. Any
       scale/rotation/opacity keyframe animation (crate::keyframe module) is already spliced
       onto the front of this string by Rust (ClipInstance::keyframe_video_filter_chain) — C
       just splices the whole thing in at one point, same as before. */
    const char *video_filter;
    /* Nonzero holds the first decoded video frame at/after source_in_secs for this segment's
       whole trimmed duration (source_out_secs - source_in_secs) instead of playing through the
       range — a freeze frame. video_filter still applies to every held frame. Audio is
       unaffected — still decoded/played across the full source_in_secs..source_out_secs range
       regardless of this flag. */
    int frozen;
    /* Playback speed multiplier — 1.0 is normal speed. Video is handled by a setpts filter
       inserted before the canvas-conform fps stage (see bridge.c). Audio is handled by the
       atempo filter in the shared audio graph. Values are clamped to [0.5, 100.0] for the
       audio side (atempo's supported range); video setpts handles any positive value. When
       smooth_speed_ramp_end_factor is set (nonzero, below), this is instead the ramp's *start*
       speed. */
    float speed_factor;
    /* End speed of a smooth, continuous speed ramp across this segment's whole trimmed source
       duration — 0.0 (or any value <= 0.0) means "no ramp, use speed_factor unchanged" (same
       "<=0 means disabled" convention speed_factor's own 0.0-vs-default-1.0 handling elsewhere
       in this codebase already follows). When set, video gets a setpts expression that's the
       *integral* of 1/speed(t) over the linear speed(t) ramp (a natural-log term — see
       timeline_export.c's setpts_str construction) instead of the plain constant-multiplier
       setpts=PTS/speed_factor above. Audio has no continuous per-sample tempo-ramp primitive in
       this FFmpeg build (atempo takes a fixed parameter, not a `t`-keyed expression), so the
       audio side still uses one constant atempo value — the ramp's *average* speed
       ((speed_factor + smooth_speed_ramp_end_factor) / 2) — a deliberate, documented
       approximation, not a bug: a perfectly speed-synced audio ramp isn't achievable with this
       filter, so audio trades exact sync for staying audible and reasonably close throughout
       rather than jumping between discrete steps. */
    float smooth_speed_ramp_end_factor;
    /* Overlay-compositor x/y position expressions (avfilter expression syntax, e.g.
       "(0.25)*main_w"), UTF-8, NUL-terminated — built in Rust from this clip's position
       keyframes (crate::keyframe::position_overlay_xy_expr). Empty string ("") means no
       offset. Only consulted by avbridge_encode_timeline_export_multi's overlay path (a
       single/background-track clip has no compositing stage to apply this to) — ignored by
       avbridge_encode_timeline_export. Scale/rotation/opacity keyframes don't need their own
       fields here — they're already folded into video_filter above, built the same way. */
    const char *position_x_expr;
    const char *position_y_expr;
    /* Transition effect at the start of this clip:
       0 = None/HardCut (no effect), 1 = Fade (fade in from black), 2 = Slide (reveal from
       left via an animated drawbox wipe), 3 = Zoom (scale from 50% to 100%).
       Applied as an animated avfilter expression after the clip's video_filter and before
       the final format=yuv420p conform, so n=0 at each segment's filter graph start drives
       the per-frame animation. */
    int transition_in;
    /* Duration of the transition_in effect in seconds. Ignored when transition_in is 0.
       Converted to a frame count inside bridge.c using the canvas fps. */
    float transition_duration_secs;
    /* Start position of this clip on the shared timeline, in seconds.
       Used by avbridge_encode_timeline_export_multi to determine which overlay tracks are
       active at any given decoded-frame time. Ignored by avbridge_encode_timeline_export
       (single-track function concatenates in order, no gaps). */
    double timeline_start_secs;
    /* Path to a grayscale-as-luma alpha-matte video (see avbridge_encode_matte_video) for AI
       background removal, UTF-8, NUL-terminated. Empty string ("") means no matte — the
       segment composites with whatever alpha video_filter already produced (or fully opaque,
       if none). When set, avbridge_encode_timeline_export_multi decodes this alongside the
       segment's own source_path and alphamerges its luma onto this segment's video before
       compositing — replacing, not combining with, any alpha video_filter already produced
       (e.g. a mask_shape/chroma_key alpha on the same clip). The matte file's own internal
       timeline is 0-based and covers exactly [0, source_out_secs - source_in_secs) — the same
       range this segment's own source_path is trimmed to, not the timeline (post-speed_factor)
       duration — since it was generated by sampling source_path directly, unaffected by
       speed_factor. Only consulted by avbridge_encode_timeline_export_multi's overlay path
       (same reasoning as position_x_expr/position_y_expr above) — ignored by
       avbridge_encode_timeline_export. */
    const char *mask_video_path;
    /* FFmpeg blend filter mode name (e.g. "multiply", "screen" -- any name blend's all_mode
       option accepts), UTF-8, NUL-terminated. Empty string ("") means "no blend mode" -- this
       segment composites via plain alpha-over overlay, same as before this field existed.
       When set, avbridge_encode_timeline_export_multi's overlay graph builds a
       "blend=all_mode=<name>" stage in place of the usual "overlay=x=...:y=..." stage for
       this layer -- position_x_expr/position_y_expr above are ignored in that case (blend
       has no x/y placement option; it blends two same-size, already-aligned frames), so this
       layer composites at full canvas size, not at an offset position. Only consulted by
       avbridge_encode_timeline_export_multi's overlay path -- ignored by
       avbridge_encode_timeline_export. */
    const char *blend_mode;
    /* Fraction (0.0..=1.0) of this segment's own decoded frame that rotation/scale pivot
       around, instead of the frame's own center. 0.5/0.5 (the default -- every previous
       caller effectively used this) is exactly the old, hardcoded-center behavior: the
       canvas-conforming "scale=...,pad=cw:ch:(ow-iw)/2:(oh-ih)/2" stage every segment already
       goes through centers the decoded content within the canvas-sized buffer that a
       "rotate=..." stage in video_filter (when scale/rotation keyframes are set) then pivots
       around -- replacing that fixed "/2" offset with "ow/2-(anchor_x)*iw"/
       "oh/2-(anchor_y)*ih" moves the pivot to an arbitrary point on the content instead, while
       reducing to the identical expression at 0.5/0.5 (verified against real ffmpeg output,
       byte-identical). Applies uniformly to every clip in both avbridge_encode_timeline_export
       and avbridge_encode_timeline_export_multi -- unlike blend_mode/position_x_expr, this
       isn't overlay-only. */
    double anchor_x;
    double anchor_y;
    /* Fade from black (video) and from silence (audio) over this many seconds at the start /
       end of the clip, in output seconds; 0 = none. Video: `fade` appended to the segment's
       chain after any entry transition (in the overlay graph a layer fades to transparent through

       its alpha plane, the base clip to black). Audio: a sample ramp on the decoded frames in the
       single-track path, `afade` in the mix graph otherwise. */
    float fade_in_secs;
    float fade_out_secs;
} ClipSegment;

/* Renders an ordered sequence of trimmed clips (`segments`, `segment_count` of them) as one
   continuous export: video is decoded, each segment's own avfilter chain applied (prefixed
   with a canvas-conform scale/pad/fps stage so every segment lands on the same
   canvas_width/canvas_height/canvas_fps), and re-encoded via libopenh264 at
   canvas_bit_rate_bps — unlike
   avbridge_encode_export, video is never stream-copied here, since each clip may need a
   different filter chain. Audio across all segments is decoded, gain-adjusted per segment,
   and run through ONE continuous loudnorm+limiter graph (so normalization sees the whole
   timeline, not each clip in isolation) before being re-encoded to AAC 192kbps — this is why
   every segment's audio must share the same sample rate/format/channel layout
   (ENCODE_ERR_AUDIO_FORMAT_MISMATCH otherwise). Every segment must have both a video and
   an audio stream (ENCODE_ERR_NO_VIDEO_STREAM / ENCODE_ERR_NO_AUDIO_STREAM).

   progress_cb/progress_user_data may both be NULL to skip progress reporting; when set,
   called with the cumulative timeline position in seconds (sum of prior segments' trimmed
   durations plus progress within the current one), never past the sum of all segments'
   trimmed durations.
   cancel may be NULL to disable cancellation; otherwise checked between packets — once
   `*cancel` is nonzero, stops and returns ENCODE_CANCELLED without writing a trailer.

   gpu_encoder_preference selects the video encoder (one of the GPU_ENCODER_* values —
   see bridge_internal.h's GpuEncoderPreference; passed as plain int across the FFI
   boundary): AUTO/0 tries hardware encoders (NVENC, Quick Sync, VAAPI, then AMF, plus
   VideoToolbox on macOS) and falls back to the CPU (libopenh264) encoder if none open; CPU/1
   forces libopenh264; NVENC/2, QUICKSYNC/3, AMF/4, VAAPI/5, and VIDEOTOOLBOX/6 force that
   specific hardware encoder, still falling back to CPU if it can't open (no compatible
   GPU/driver present). */
EncodeStatus avbridge_encode_timeline_export(const ClipSegment *segments, int segment_count,
                                             int canvas_width, int canvas_height,
                                             int canvas_fps_num, int canvas_fps_den,
                                             int64_t canvas_bit_rate_bps, const char *out_path,
                                             float target_lufs, int gpu_encoder_preference,
                                             ProgressCallback progress_cb, void *progress_user_data,
                                             const uint8_t *cancel);

/* avbridge_encode_timeline_export, with graphs that run over the finished frames before the
   encoder: `post_filters` holds `post_filter_count` (at most 4) filter descriptions, applied in
   order, each taking its input as `in` and its output as `out` (the descriptions
   avbridge_text_overlay_filter and avbridge_shape_overlay_filter return). They replace the
   second decode, filter and encode of the whole file that avbridge_apply_text_overlays and
   avbridge_apply_shape_overlays need, and `t` inside them is the timeline's own time. Returns
   ENCODE_ERR_FILTER_GRAPH, before writing any frame, when one of them does not build. */
EncodeStatus avbridge_encode_timeline_export_with_post(
    const ClipSegment *segments, int segment_count, int canvas_width, int canvas_height,
    int canvas_fps_num, int canvas_fps_den, int64_t canvas_bit_rate_bps, const char *out_path,
    float target_lufs, int gpu_encoder_preference, ProgressCallback progress_cb,
    void *progress_user_data, const uint8_t *cancel, const char *const *post_filters,
    int post_filter_count);

/* Multi-track overlay compositor: composites n_tracks video tracks into a single output.
   track_segs[k] points to an array of track_n_segs[k] ClipSegment entries for track k.
   Track 0 is the background (drives the output duration and audio); tracks 1..n_tracks-1 are
   overlaid on top using avfilter's overlay filter whenever a clip from those tracks is active
   at the corresponding timeline position (determined by ClipSegment::timeline_start_secs).
   Overlays are applied in ascending track order, so later tracks appear above earlier tracks.
   This function's initial audio comes from track 0 only; the core timeline renderer uses
   avbridge_mix_audio_timeline + avbridge_mux_video_audio afterward when tracks 1+ contribute
   sound.
   For n_tracks == 1, delegates to avbridge_encode_timeline_export unchanged.
   gpu_encoder_preference has the same meaning as avbridge_encode_timeline_export's. */
EncodeStatus avbridge_encode_timeline_export_multi(
    const ClipSegment *const *track_segs, const int *track_n_segs, int n_tracks, int canvas_width,
    int canvas_height, int canvas_fps_num, int canvas_fps_den, int64_t canvas_bit_rate_bps,
    const char *out_path, float target_lufs, int gpu_encoder_preference,
    ProgressCallback progress_cb, void *progress_user_data, const uint8_t *cancel);

typedef struct {
    /* UTF-8, NUL-terminated source containing an audio stream. */
    const char *source_path;
    /* Trim range within source_path, in seconds. */
    double source_in_secs;
    double source_out_secs;
    /* Placement on the exported timeline, in seconds. */
    double timeline_start_secs;
    /* Per-clip gain and playback speed. */
    float gain_db;
    float speed_factor;
    /* Optional FFmpeg `volume` filter expression (linear multiplier, `t`-keyed) driving a
       keyframed gain ramp instead of the constant gain_db above -- NULL or empty means "use
       gain_db unchanged", non-empty overrides it via `volume=<expr>:eval=frame` (see
       audio_mix.c's build_mix_graph). Built by avcore::keyframe::gain_filter_db_expr from a
       ClipInstance's gain_keyframes. */
    const char *gain_keyframe_expr;
    /* Audio-ducking role (P2 item 6, "Auto Ducking"): 0 = normal (mixed in as-is), 1 = trigger
       (the sidechain signal that ducks target branches, and still plays itself), 2 = target
       (ducked under trigger branches via sidechaincompress). Derived from the source track's
       avcore::timeline::AudioRole (Mic -> trigger, Music -> target, everything else -> normal)
       -- see audio_mix.c's build_mix_graph for the filter-graph topology this drives. */
    int duck_role;
    /* CF-03 "Gameplay Voice" cleanup (spec/architecture/competitive-feature-plan.md), from
       avcore::timeline::ClipInstance::voice_cleanup_enabled. 0 = off (this branch's audio is
       unchanged beyond gain/tempo, same as before this field existed); non-zero inserts
       highpass=f=80,afftdn=nf=<voice_cleanup_noise_floor_db>,acompressor=threshold=
       <voice_cleanup_compressor_threshold_db>dB:ratio=<voice_cleanup_compressor_ratio>:
       attack=10:release=250:makeup=1.5,alimiter=limit=<voice_cleanup_ceiling_linear> between
       this branch's own volume and delay stages, before it joins the final mix -- see
       audio_mix.c's build_mix_graph. Values come from scripts/Watch-Gameplay.ps1's own proven
       chain, not guessed. */
    int voice_cleanup_enabled;
    float voice_cleanup_noise_floor_db;
    float voice_cleanup_compressor_threshold_db;
    float voice_cleanup_compressor_ratio;
    float voice_cleanup_ceiling_linear;
    /* Fade in / out over this many output seconds, applied right after the branch's own volume
       stage; 0 = none. */
    float fade_in_secs;
    float fade_out_secs;
    /* Stereo balance, -1 (left only) to 1 (right only), 0 = centred and untouched: the side the
       pan moves away from is attenuated with a quarter-sine taper, the other stays at unity.
       Applied after the branch's voice cleanup, before it joins the mix. */
    float pan;
    /* A mixer strip's tone and dynamics, applied after the pan: low shelf at 120 Hz, mid bell at
       1 kHz and high shelf at 6 kHz (gains in dB, 0 = no filter), high-pass and low-pass corners in
       Hz (0 = open), and a compressor (threshold in dB, ratio; a ratio <= 1 is off). */
    float eq_low_db;
    float eq_mid_db;
    float eq_high_db;
    float highpass_hz;
    float lowpass_hz;
    float comp_threshold_db;
    float comp_ratio;
} AudioSegment;

typedef enum {
    AUDIO_MIX_OK = 0,
    AUDIO_MIX_ERR_OPEN_INPUT = 1,
    AUDIO_MIX_ERR_ALLOC_OUTPUT = 2,
    AUDIO_MIX_ERR_NO_AUDIO = 3,
    AUDIO_MIX_ERR_FILTER_GRAPH = 4,
    AUDIO_MIX_ERR_ENCODER = 5,
    AUDIO_MIX_ERR_OPEN_OUTPUT = 6,
    AUDIO_MIX_ERR_WRITE_HEADER = 7,
    AUDIO_MIX_ERR_PIPELINE = 8,
    AUDIO_MIX_CANCELLED = 9,
} AudioMixStatus;

/* Output formats of the audio mix. */
enum {
    AUDIO_MIX_FORMAT_AAC = 0,   /* 192 kbps, the audio of a video export */
    AUDIO_MIX_FORMAT_WAV24 = 1, /* uncompressed 24-bit PCM */
    AUDIO_MIX_FORMAT_FLAC = 2,  /* lossless, 24-bit */
    AUDIO_MIX_FORMAT_MP3 = 3,   /* 256 kbps */
    AUDIO_MIX_FORMAT_OPUS = 4   /* 160 kbps */
};

/* Mixes every segment that contains audio onto one timeline and writes it in `format` (one of
   AUDIO_MIX_FORMAT_*; the container follows out_path's extension) to an audio-only file.
   Per-segment trim, gain, speed, and timeline placement are applied before the shared
   afftdn/loudnorm/limiter chain. Inputs without an audio stream are skipped; returns
   AUDIO_MIX_ERR_NO_AUDIO when none remain. timeline_duration_secs trims the final mix so an audio
   clip cannot extend the output past the rendered video. */
AudioMixStatus avbridge_mix_audio_timeline(const AudioSegment *segments, int segment_count,
                                           double timeline_duration_secs, const char *out_path,
                                           float target_lufs, int format, const uint8_t *cancel);

typedef enum {
    MEDIA_MUX_OK = 0,
    MEDIA_MUX_ERR_OPEN_INPUT = 1,
    MEDIA_MUX_ERR_ALLOC_OUTPUT = 2,
    MEDIA_MUX_ERR_MISSING_STREAM = 3,
    MEDIA_MUX_ERR_NEW_STREAM = 4,
    MEDIA_MUX_ERR_OPEN_OUTPUT = 5,
    MEDIA_MUX_ERR_WRITE_HEADER = 6,
    MEDIA_MUX_ERR_WRITE_FRAME = 7,
} MediaMuxStatus;

/* Stream-copies the first video stream from video_path and the first audio stream from
   audio_path into out_path. Used after avbridge_mix_audio_timeline so replacing a timeline's
   audio never incurs another lossy video encode. */
MediaMuxStatus avbridge_mux_video_audio(const char *video_path, const char *audio_path,
                                        const char *out_path);

typedef enum {
    LOUDNESS_OK = 0,
    LOUDNESS_ERR_OPEN_INPUT = 1,
    LOUDNESS_ERR_STREAM_INFO = 2,
    LOUDNESS_ERR_NO_AUDIO_STREAM = 3,
    LOUDNESS_ERR_DECODER = 4,
    LOUDNESS_ERR_FILTER_GRAPH = 5,
    /* A decode/filter call failed mid-stream (not at setup). */
    LOUDNESS_ERR_PIPELINE = 6,
    /* The pipeline ran to completion but no loudnorm JSON report was captured. */
    LOUDNESS_ERR_NO_REPORT = 7,
} LoudnessStatus;

/* Measures integrated loudness / true peak / loudness range via a single-pass `loudnorm`
   analysis (I=-16:TP=-1.5:LRA=11 — matches ffmpeg's
   `-af loudnorm=I=-16:TP=-1.5:LRA=11:print_format=json -f null -`). Nothing is written or
   re-encoded — analysis only.

   On success, writes the raw JSON report (NUL-terminated, truncated to fit if longer than
   out_json_len - 1 bytes) into `out_json` for the caller to parse. The loudnorm filter has no
   queryable struct API for its final stats — it only prints them via av_log() when it
   processes EOF — so this is the only way to get them out of libavfilter directly.

   NOT thread-safe: installs a process-global libavutil log callback for the call's duration
   (reset to av_log_default_callback before returning) — do not call this from multiple
   threads concurrently. */
LoudnessStatus avbridge_measure_loudness(const char *in_path, char *out_json, size_t out_json_len);

typedef enum {
    PROXY_OK = 0,
    PROXY_ERR_OPEN_INPUT = 1,
    PROXY_ERR_STREAM_INFO = 2,
    /* The input has no video stream to make a proxy of. */
    PROXY_ERR_NO_VIDEO_STREAM = 3,
    PROXY_ERR_ALLOC_OUTPUT = 4,
    PROXY_ERR_NEW_STREAM = 5,
    PROXY_ERR_OPEN_OUTPUT = 6,
    PROXY_ERR_WRITE_HEADER = 7,
    PROXY_ERR_WRITE_FRAME = 8,
    /* Couldn't find/open the video or audio decoder. */
    PROXY_ERR_DECODER = 9,
    /* Couldn't find/open the libopenh264 video encoder or the AAC audio encoder. */
    PROXY_ERR_ENCODER = 10,
    /* Couldn't build the video scaler (libswscale). */
    PROXY_ERR_SCALER = 11,
    /* Couldn't build the (filterless, format-conversion-only) audio graph. */
    PROXY_ERR_FILTER_GRAPH = 12,
    /* A decode/scale/encode call failed mid-stream (not at setup). */
    PROXY_ERR_PIPELINE = 13,
} ProxyStatus;

/* Generates a downscaled editing proxy of in_path's video (height = target_height, width
   computed to preserve source aspect ratio and rounded to the nearest even number, matching
   ffmpeg's `scale=-2:<height>`) via libopenh264 (BSD-licensed — not libx264/GPL, which this
   LGPL FFmpeg build doesn't have). Any audio stream is re-encoded to AAC 128kbps unchanged
   otherwise (decode -> format-match the encoder -> encode, no filtering). Equivalent to:
   ffmpeg -i in_path -vf scale=-2:<target_height> -c:v libopenh264 -c:a aac -b:a 128k out_path
   Fails with PROXY_ERR_NO_VIDEO_STREAM if in_path has no video stream. Audio is optional —
   a video-only input produces a video-only proxy, no error. */
ProxyStatus avbridge_generate_proxy(const char *in_path, const char *out_path, int target_height);

typedef enum {
    WAVEFORM_OK = 0,
    WAVEFORM_ERR_OPEN_INPUT = 1,
    WAVEFORM_ERR_STREAM_INFO = 2,
    /* The input has no audio stream to compute a waveform from. */
    WAVEFORM_ERR_NO_AUDIO_STREAM = 3,
    /* Couldn't find/open the audio decoder. */
    WAVEFORM_ERR_DECODER = 4,
    /* Couldn't build the mono-downmix filter graph. */
    WAVEFORM_ERR_FILTER_GRAPH = 5,
    /* A decode/filter call failed mid-stream (not at setup), or bucket_count was <= 0. */
    WAVEFORM_ERR_PIPELINE = 6,
} WaveformStatus;

/* Computes per-bucket min/max amplitude peaks (each in [-1, 1]) of in_path's audio stream,
   downmixed to mono, for waveform rendering. The full duration is divided into bucket_count
   equal-length buckets by sample index (not wall-clock time) — out_min/out_max must each point
   at a writable array of at least bucket_count floats; both are fully overwritten (a bucket
   with no samples in it, e.g. because the duration estimate undershoots, is left at 0.0f).
   Equivalent in spirit to
   ffmpeg -i in_path -af "aformat=sample_fmts=flt:channel_layouts=mono" -f null -
   with peak tracking bolted on, but nothing is written or re-encoded.
   Fails with WAVEFORM_ERR_NO_AUDIO_STREAM if in_path has no audio stream. */
WaveformStatus avbridge_generate_waveform(const char *in_path, int bucket_count, float *out_min,
                                          float *out_max);

typedef enum {
    TEXT_OVERLAY_OK = 0,
    /* avformat_open_input() or avformat_find_stream_info() failed. */
    TEXT_OVERLAY_ERR_OPEN_INPUT = 1,
    /* Couldn't allocate the output context or open the output file for writing. */
    TEXT_OVERLAY_ERR_ALLOC_OUTPUT = 2,
    /* Couldn't build the PNG overlay filter graph. */
    TEXT_OVERLAY_ERR_FILTER_GRAPH = 3,
    /* A decode/filter/encode call failed mid-stream. */
    TEXT_OVERLAY_ERR_PIPELINE = 4,
} TextOverlayStatus;

/* One pre-rasterized text overlay to composite over an already-rendered export video. */
typedef struct {
    /* Timeline position in seconds where this text becomes visible. */
    double start_secs;
    /* How long the text stays visible, in seconds. */
    double duration_secs;
    /* UTF-8, NUL-terminated path to a full-canvas RGBA PNG. Must not be NULL. */
    const char *overlay_path;
    /* NULL or empty means "always fully opaque, same as before this field existed" -- a
       complete geq-expression-language fragment (e.g. "if(lt((T-1.000000),..." from
       avcore::keyframe::text_opacity_alpha_expr) multiplied against the PNG's own alpha
       channel. See avbridge_apply_text_overlays' doc comment for where this is spliced in. */
    const char *opacity_keyframe_expr;
    /* NULL or empty means "no offset, same x=0/y=0 placement as before these fields existed" --
       complete overlay-expression-language fragments (e.g. "if(lt((t-1.000000),..." from
       avcore::keyframe::text_position_offset_expr) added to the overlay node's x/y position.
       See avbridge_apply_text_overlays' doc comment for where these are spliced in. */
    const char *position_keyframe_expr_x;
    const char *position_keyframe_expr_y;
    /* NULL, empty, or either-one-empty means "no remap, same pixel-for-pixel raster as before
       these fields existed" -- complete geq-expression-language fragments (e.g.
       "(960.000000+(X-(960.000000))/(if(lt((T-1.000000),..." from
       avcore::keyframe::text_scale_sample_exprs) used as the (X,Y) coordinates a geq stage
       samples the raster's own r/g/b/a channels at, remapping around the raster's baked
       position anchor -- an inverse-sample zoom, not a literal `scale` filter (see
       avbridge_apply_text_overlays' doc comment for why). See that same doc comment for where
       these are spliced in. */
    const char *scale_keyframe_expr_x;
    const char *scale_keyframe_expr_y;
    /* NULL, empty, or either-one-empty means "no remap, same pixel-for-pixel raster as before
       these fields existed" -- complete geq-expression-language fragments (e.g.
       "(960.0000+(X-(960.0000))*cos(...)+(Y-(540.0000))*sin(...))" from
       avcore::keyframe::text_rotation_sample_exprs) used as the (X,Y) coordinates a geq stage
       samples the raster's own r/g/b/a channels at, remapping around the same raster-baked
       position anchor the scale remap above uses -- an inverse-sample rotation, composing with
       the scale remap independently (isotropic scale and rotation around the same center
       commute) since neither changes the raster's own canvas-sized frame. See
       avbridge_apply_text_overlays' doc comment for where these are spliced in. */
    const char *rotation_keyframe_expr_x;
    const char *rotation_keyframe_expr_y;
} TextSegment;

/* Opens `in_path` (an already-rendered H.264/AAC mp4), composites PNG overlays for every
   segment in `segments` using an enable='between(t,start,end)' avfilter expression, and writes
   the result to `out_path`. Each overlay is already rasterized as a transparent PNG so preview
   and export share the exact same font/background renderer. Up to three optional geq stages sit
   between the movie source and the overlay node, in this order: when a segment's
   scale_keyframe_expr_x/_y is non-empty, a geq stage inverse-samples the raster at those
   coordinates instead of (X,Y) directly, remapping it around its own baked position anchor
   (this is an inverse-sample zoom -- output frame size never changes -- deliberately not a
   literal `scale` filter with `eval=frame`, which reliably corrupted the heap in a real export
   elsewhere in this codebase, see CLAUDE.md); when rotation_keyframe_expr_x/_y is non-empty, a
   geq stage inverse-samples the (possibly already scale-remapped) raster around the same
   anchor, at an angle instead of a factor; when opacity_keyframe_expr is non-empty, a geq
   stage multiplies the (possibly already remapped) alpha channel by that expression. When
   position_keyframe_expr_x/_y is non-empty, the overlay node's x/y is that expression instead
   of the literal 0 it otherwise defaults to. All of these are in the same filtergraph with a
   shared time origin (t=0 at graph start, same as the main video), so geq's per-pixel T and
   overlay's own per-frame t are both timeline-absolute, matching what the expressions were
   built against. Video is decoded, filtered, and re-encoded via libopenh264; audio is
   stream-copied unchanged.
   canvas_width/canvas_height and canvas_fps_num/den are
   used only to size the output encoder context — they must match the actual rendered video.

   Returns TEXT_OVERLAY_OK immediately if segment_count <= 0 without touching any files. */
TextOverlayStatus avbridge_apply_text_overlays(const char *in_path, const char *out_path,
                                               const TextSegment *segments, int segment_count,
                                               int canvas_width, int canvas_height,
                                               int canvas_fps_num, int canvas_fps_den);

/* One geometric shape to composite over an already-rendered export video. `filter_desc` is a
   *complete*, ready-to-chain avfilter node description (a `geq=lum=...:cb=...:cr=...:enable=
   between(t,start,end)` string) built in Rust — see `avcore::shape_render`, which is where the
   actual per-shape-kind geometry (rectangle/ellipse/triangle/trapezoid/arrow/custom polygon,
   rotation, outline thickness) lives, the same "Rust builds the filter string, C just chains
   it" split `ClipInstance::video_filter_chain` already uses for the main export path. This
   keeps shape_overlay.c a thin, shape-kind-agnostic chainer, same reasoning as `TextSegment`
   already being pre-formatted before crossing the FFI boundary. */
typedef struct {
    /* UTF-8, NUL-terminated. Must not be NULL. A single `geq=...` filter node description, no
       trailing/leading comma. */
    const char *filter_desc;
} ShapeSegment;

/* Same shape as [`avbridge_apply_text_overlays`] (same TextOverlayStatus return codes, same
   decode/filter/re-encode-video + stream-copy-audio approach), but chains each segment's
   pre-built `geq` filter node instead of loading a pre-rasterized PNG. Returns
   TEXT_OVERLAY_OK immediately if segment_count <= 0 without touching any files. */
TextOverlayStatus avbridge_apply_shape_overlays(const char *in_path, const char *out_path,
                                                const ShapeSegment *segments, int segment_count,
                                                int canvas_width, int canvas_height,
                                                int canvas_fps_num, int canvas_fps_den);

/* The graph descriptions avbridge_apply_text_overlays / avbridge_apply_shape_overlays build, for
   avbridge_encode_timeline_export_with_post. Each returns a string to release with
   avbridge_free_string, or NULL when it does not build or there are no segments. */
/* Name of the video encoder the calling thread opened last ("h264_nvenc", "libopenh264", ...),
   or "" if it opened none. The pointer is valid until that thread opens another. */
const char *avbridge_last_video_encoder(void);

char *avbridge_text_overlay_filter(const TextSegment *segments, int segment_count);
char *avbridge_shape_overlay_filter(const ShapeSegment *segments, int segment_count);
/* The same for one privacy-blur segment: `matte_path` is a matte video at the canvas's size and
   frame rate and as long as the export. NULL when the arguments are unusable. */
char *avbridge_privacy_blur_filter(const char *matte_path, double blur_sigma, int canvas_width,
                                   int canvas_height);
void avbridge_free_string(char *s);

typedef enum {
    MATTE_OK = 0,
    MATTE_ERR_ALLOC_OUTPUT = 1,
    /* Couldn't find/open a supported CPU H.264 encoder. */
    MATTE_ERR_ENCODER = 2,
    MATTE_ERR_NEW_STREAM = 3,
    MATTE_ERR_OPEN_OUTPUT = 4,
    MATTE_ERR_WRITE_HEADER = 5,
    /* An encode/write call failed mid-stream (not at setup). */
    MATTE_ERR_PIPELINE = 6,
    /* frame_count <= 0, or width/height/fps_num/fps_den <= 0. */
    MATTE_ERR_EMPTY = 7,
    /* width or height is odd — invalid for yuv420p's 2x-subsampled chroma planes. */
    MATTE_ERR_ODD_DIMENSIONS = 8,
} MatteStatus;

/* Encodes `frame_count` grayscale-as-luma frames — a per-clip AI background-removal alpha
   matte (see `avcore::background_removal::segment_person`), not meant to ever be shown to the
   user directly — into a plain H.264 video at `out_path`, via the available CPU H.264 encoder
   (forced, no GPU
   attempt: this is a small internal artifact, not worth the hardware-encoder fallback ladder
   `open_video_encoder` uses for the main export path).

   `luma_frames` holds `frame_count` frames back-to-back, each `width * height` bytes
   (row-major, one byte per pixel — frame `i` starts at `luma_frames + (size_t)i * width *
   height`). Each output frame is encoded as YUV420P with the supplied bytes as the luma plane
   and both chroma planes filled with the neutral value 128 ("no color") — "grayscale-as-luma"
   rather than a true single-plane GRAY8 stream, since whether this FFmpeg build's
   encoder accepts `AV_PIX_FMT_GRAY8` input is unverified. Frame `i` gets pts `i`
   at `fps_num/fps_den` — this function has no notion of any original clip's timing, just its
   own frame count and rate.

   Returns MATTE_ERR_EMPTY immediately if frame_count <= 0 or width/height/fps_num/fps_den <=
   0, and MATTE_ERR_ODD_DIMENSIONS if width or height is odd, without touching out_path either
   way. */
MatteStatus avbridge_encode_matte_video(const uint8_t *luma_frames, int frame_count, int width,
                                        int height, int fps_num, int fps_den, const char *out_path);

typedef enum {
    PRIVACY_BLUR_OK = 0,
    /* avformat_open_input() or avformat_find_stream_info() failed on in_path. */
    PRIVACY_BLUR_ERR_OPEN_INPUT = 1,
    /* Couldn't allocate the output context or open the output file for writing. */
    PRIVACY_BLUR_ERR_ALLOC_OUTPUT = 2,
    /* Couldn't build the blur/mask filter graph (bad matte_path, or the installed FFmpeg build
       lacks gblur/maskedmerge — both are standard libavfilter filters, so this would only
       happen against an unusually stripped-down build). */
    PRIVACY_BLUR_ERR_FILTER_GRAPH = 3,
    /* A decode/filter/encode call failed mid-stream. */
    PRIVACY_BLUR_ERR_PIPELINE = 4,
} PrivacyBlurStatus;

/* Opens `in_path` (an already-rendered H.264/AAC mp4, same "post-process pass over an
   already-rendered export" shape as `avbridge_apply_text_overlays`/`_shape_overlays`), applies a
   Gaussian blur of `blur_sigma` everywhere `matte_path`'s own per-frame luma is non-zero, and
   writes the result to `out_path`. `matte_path` is expected to be exactly what
   `avbridge_encode_matte_video` produces (grayscale-as-luma, neutral chroma) — typically from
   `avcore::mask_propagation::rasterize_to_matte_frames`'s output for CF-09's arbitrary-object
   privacy-blur use case, but any matte video of that same shape works. `matte_path`'s own
   frame rate/duration must already match `in_path`'s (`canvas_fps_num`/`_den`) — this function
   does no timing reconciliation between the two, just reads `matte_path` frame-for-frame via
   FFmpeg's own `movie` filter source alongside the main decode loop.

   The filter graph (`gblur` for the blur, `maskedmerge` for the per-pixel blend against the
   matte's luma) is built from fixed, non-user-supplied strings except `matte_path` itself
   (single-quote-escaped the same way `avbridge_apply_text_overlays` escapes `overlay_path`) and
   `blur_sigma` (a plain `%f`-formatted double) — there is no dynamic geq expression construction
   here the way the other overlay functions have, since blur/mask compositing needs none. Video
   is decoded, filtered, and re-encoded via libopenh264; audio is stream-copied unchanged, exactly
   like the other two overlay functions.

   `blur_sigma` <= 0 is treated as PRIVACY_BLUR_ERR_FILTER_GRAPH (gblur requires a positive
   sigma) rather than silently passing the frame through unfiltered — a caller wanting "no blur"
   should skip calling this function entirely, the same convention
   `avbridge_apply_text_overlays`/`_shape_overlays` use for `segment_count <= 0`. */
PrivacyBlurStatus avbridge_apply_privacy_blur(const char *in_path, const char *out_path,
                                              const char *matte_path, double blur_sigma,
                                              int canvas_width, int canvas_height,
                                              int canvas_fps_num, int canvas_fps_den);

typedef enum {
    PCM_OK = 0,
    PCM_ERR_OPEN_INPUT = 1,
    PCM_ERR_STREAM_INFO = 2,
    /* The input has no audio stream to decode. */
    PCM_ERR_NO_AUDIO_STREAM = 3,
    /* Couldn't find/open the audio decoder. */
    PCM_ERR_DECODER = 4,
    /* Couldn't build the resample/mono-downmix filter graph. */
    PCM_ERR_FILTER_GRAPH = 5,
    /* A decode/filter call failed mid-stream (not at setup). */
    PCM_ERR_PIPELINE = 6,
} PcmStatus;

/* Decodes `in_path`'s first audio stream to 16kHz mono 32-bit float PCM samples — the exact
   input format whisper.cpp (via the `whisper-rs` crate) requires. Equivalent to
   `ffmpeg -i in_path -af "aresample=16000,aformat=sample_fmts=flt:channel_layouts=mono" -f f32le
   -`.

   On success, `*out_samples` points to a buffer of `*out_sample_count` floats, allocated with
   malloc() and owned by the caller — free it with `avbridge_free_pcm_buffer()`, not `free()`
   directly (keeps the allocator paired with whichever C runtime avbridge itself was linked
   against). Both out-params are left untouched on any non-OK status.
   Fails with `PCM_ERR_NO_AUDIO_STREAM` if `in_path` has no audio stream. */
PcmStatus avbridge_extract_pcm_16k_mono(const char *in_path, float **out_samples,
                                        int64_t *out_sample_count);

/* Frees a buffer previously returned via `avbridge_extract_pcm_16k_mono`'s `out_samples`.
   Safe to call with `samples == NULL` (no-op). */
void avbridge_free_pcm_buffer(float *samples);

/* Opaque one-video-stream demux/decode session used by Bush preview. */
typedef struct AvbridgeDecodeSession AvbridgeDecodeSession;

typedef enum {
    DECODE_OK = 0,
    DECODE_EOF = 1,
    DECODE_CANCELLED = 2,
    DECODE_ERR_OPEN_INPUT = 3,
    DECODE_ERR_STREAM_INFO = 4,
    DECODE_ERR_NO_VIDEO_STREAM = 5,
    DECODE_ERR_DECODER = 6,
    DECODE_ERR_SEEK = 7,
    DECODE_ERR_PIPELINE = 8,
    /* Dimensions or a packed RGBA frame exceed Bush's native allocation limit. */
    DECODE_ERR_FRAME_SIZE = 9,
    /* The input has no audio stream. */
    DECODE_ERR_NO_AUDIO_STREAM = 10,
} DecodeStatus;

typedef struct {
    int width;
    int height;
    int fps_num;
    int fps_den;
    double duration_secs;
    /* Nonzero when frames are decoded on a hardware device. */
    int hardware;
} DecodeInfo;

typedef struct {
    uint8_t *pixels;
    size_t pixel_len;
    int width;
    int height;
    int stride_bytes;
    double timestamp_secs;
    /* Nonzero when `pixels` is the buffer the caller handed in (see below) rather than a new
       allocation: avbridge_decode_free_frame then leaves it alone. */
    int borrowed;
} DecodedRgbaFrame;

/* avbridge_decode_next_rgba and avbridge_decode_take_rgba convert into a caller-owned buffer when
   `out_frame->pixels` / `pixel_len` hold one on entry and it is large enough for the frame (the
   frame is then returned with `borrowed` set); otherwise they allocate as before, so a caller
   that only guesses the size loses nothing. This saves the allocation and the copy out of it. */

/* Opens the first video stream. `out_session` is set only on DECODE_OK and must be released by
   avbridge_decode_close. `out_info` is initialized only on DECODE_OK. */
DecodeStatus avbridge_decode_open(const char *path, AvbridgeDecodeSession **out_session,
                                  DecodeInfo *out_info);

/* Like avbridge_decode_open; when `hardware` is nonzero the first hardware device the codec
   supports is tried, and any failure (no device, unsupported stream, driver refusal) falls back
   to software decode. `out_info->hardware` reports which path was taken. */
DecodeStatus avbridge_decode_open_ex(const char *path, int hardware,
                                     AvbridgeDecodeSession **out_session, DecodeInfo *out_info);

/* Flushes decoder state and seeks to the nearest preceding keyframe at timestamp_secs. */
DecodeStatus avbridge_decode_seek(AvbridgeDecodeSession *session, double timestamp_secs);

/* Decodes one packed RGBA frame. On DECODE_OK the caller owns out_frame->pixels and releases it
   with avbridge_decode_free_frame. Every loop checks cancel when it is non-NULL. */
DecodeStatus avbridge_decode_next_rgba(AvbridgeDecodeSession *session, const uint8_t *cancel,
                                       DecodedRgbaFrame *out_frame);
/* Cheap frame-accurate seeking: decodes the next frame WITHOUT converting it to RGBA and
   reports its timestamp. The previously decoded frame is kept as the "held" frame. Convert only
   the frame you present with avbridge_decode_take_rgba (newest != 0: the frame just advanced to;
   newest == 0: the held frame before it). A frame left unconverted is returned by the next
   avbridge_decode_next_rgba. DECODE_EOF leaves the newest frame untouched. */
DecodeStatus avbridge_decode_advance(AvbridgeDecodeSession *session, const uint8_t *cancel,
                                     double *out_timestamp_secs);
int avbridge_decode_has_held(AvbridgeDecodeSession *session);
DecodeStatus avbridge_decode_take_rgba(AvbridgeDecodeSession *session, int newest,
                                       DecodedRgbaFrame *out_frame);
/* Converts later frames to `width` x `height` instead of the decoded size (the picture is scaled,
   not cropped, so pass the stream's aspect ratio). A size larger than the decoded one, or 0, goes
   back to the decoded size. Frames already decoded but not yet converted use the new size. */
void avbridge_decode_set_output_size(AvbridgeDecodeSession *session, int width, int height);
void avbridge_decode_free_frame(DecodedRgbaFrame *frame);
void avbridge_decode_close(AvbridgeDecodeSession *session);

/* Releases the process-wide hardware devices shared by hardware decode sessions. Sessions that
   are still open keep their own reference; call after closing them to free the device. */
void avbridge_hw_device_cache_clear(void);

/* Opaque one-audio-stream demux/decode session used by Bush preview. Output is always packed
   32-bit float at the requested rate and channel count (1 or 2). */
typedef struct AvbridgeAudioSession AvbridgeAudioSession;

typedef struct {
    int sample_rate;
    int channels;
    int source_sample_rate;
    int source_channels;
    double duration_secs;
} AudioDecodeInfo;

typedef struct {
    float *samples;
    int64_t frame_count;
    /* Presentation time of the first frame; may be slightly negative (codec priming). */
    double timestamp_secs;
} DecodedAudioBlock;

/* `out_session` is set only on DECODE_OK and must be released by avbridge_audio_close. */
DecodeStatus avbridge_audio_open(const char *path, int target_sample_rate, int target_channels,
                                 AvbridgeAudioSession **out_session, AudioDecodeInfo *out_info);
/* Seeks to the packet at or before timestamp_secs and resets resampler state. Following blocks
   may start before the target; callers trim to sample accuracy. */
DecodeStatus avbridge_audio_seek(AvbridgeAudioSession *session, double timestamp_secs);
/* Produces the next filtered block (owned by the caller, release with
   avbridge_audio_free_block). Checks cancel when non-NULL. */
DecodeStatus avbridge_audio_next_block(AvbridgeAudioSession *session, const uint8_t *cancel,
                                       DecodedAudioBlock *out_block);
void avbridge_audio_free_block(DecodedAudioBlock *block);
void avbridge_audio_close(AvbridgeAudioSession *session);

typedef enum {
    SMART_CUT_OK = 0,
    /* Nothing was written: the clip or the cut does not suit a copy (another codec, a variable
       frame rate, an open GOP, no whole GOP between the cut points). Export the usual way. */
    SMART_CUT_NOT_APPLICABLE = 1,
    SMART_CUT_CANCELLED = 2,
    /* Writing started and failed: delete `out_path` and export the usual way. */
    SMART_CUT_ERROR = 3
} SmartCutStatus;

/* Writes the video of `in_path` for the `range_count` ranges [in_secs[i], out_secs[i]) played one
   after the other to `out_path` (video only, no audio), frame accurate, without encoding what lies
   between the keyframes next to the cut points: that part is copied packet for packet, and only
   the pictures at the ends are decoded and encoded again. A range too short to hold a copy is
   encoded whole; at least one range must hold a copy. Needs an H.264 4:2:0 stream of the canvas's
   size and a constant frame rate equal to the canvas's; `canvas_bit_rate_bps` is for the
   re-encoded pictures. Times are source timestamps, as in ClipSegment. progress_cb may be NULL;
   `cancel` may be NULL. */
SmartCutStatus avbridge_smart_cut_video(const char *in_path, const double *in_secs,
                                        const double *out_secs, int range_count, int canvas_width,
                                        int canvas_height, int canvas_fps_num, int canvas_fps_den,
                                        int64_t canvas_bit_rate_bps, const char *out_path,
                                        ProgressCallback progress_cb, void *progress_user_data,
                                        const uint8_t *cancel);

#endif
