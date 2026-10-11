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

#ifndef AVBRIDGE_BRIDGE_INTERNAL_H
#define AVBRIDGE_BRIDGE_INTERNAL_H

/* Declarations shared across bridge.c's split translation units. Nothing here is part of
   the public API (see bridge.h) — these are internal helpers reused by more than one .c
   file, so they can no longer be `static`. */

#include <libavcodec/avcodec.h>
#include <libavfilter/avfilter.h>
#include <libavformat/avformat.h>

#include "bridge.h"

typedef int (*AudioFrameConsumer)(AVFrame *frame, void *user_data);

/* Decodes one demuxed audio stream, forwards each frame to `consume`, flushes the decoder,
   then calls `consume(NULL, user_data)` to flush its downstream filter graph. */
int decode_audio_stream(AVFormatContext *input, AVCodecContext *decoder, int stream_index,
                        AVPacket *packet, AVFrame *frame, AudioFrameConsumer consume,
                        void *user_data);

/* Lets a video decoder use every core: frame threading plus slice threading. A decoder opened by
   hand through avcodec_alloc_context3 runs on one thread unless this is called before
   avcodec_open2(). Frame threading holds back up to one frame per thread, so the caller must
   flush the decoder (empty packet, then drain) at the end of its input. */
static inline void bridge_enable_decoder_threads(AVCodecContext *ctx) {
    ctx->thread_count = 0; /* one per core */
    ctx->thread_type = FF_THREAD_FRAME | FF_THREAD_SLICE;
}

/* Settings the exports, proxies and passes share. */

/* Mastering every export's audio goes through: denoise, loudness to the target, true-peak limit. */
#define MASTERING_LOUDNORM_ARGS_FORMAT "I=%.1f:TP=-1.0:LRA=11"
#define MASTERING_LIMITER_ARGS "limit=0.95:attack=5:release=50"
#define MASTERING_FILTER_FORMAT                                                                    \
    "afftdn,loudnorm=" MASTERING_LOUDNORM_ARGS_FORMAT ",alimiter=" MASTERING_LIMITER_ARGS

/* Sample rate and AAC bitrate of an exported mix, and the bitrate of a proxy's audio. */
#define EXPORT_AUDIO_SAMPLE_RATE 48000
#define EXPORT_AUDIO_BITRATE_BPS 192000
#define PROXY_AUDIO_BITRATE_BPS 128000

/* Threads a software H.264 encoder is given, and the keyframe spacing of every encode. */
#define SOFTWARE_ENCODER_THREADS 8
#define KEYFRAME_INTERVAL_SECS 2

/* Video bitrate of a re-encode pass when its input does not say. */
#define DEFAULT_REENCODE_BITRATE_BPS 4000000LL

typedef enum {
    /* Try platform-available hardware encoders in a fixed order (NVENC, Quick Sync, VAAPI,
       AMF, then VideoToolbox on macOS), falling back to the first available CPU H.264 encoder
       if none of them open successfully. */
    GPU_ENCODER_AUTO = 0,
    /* Force the CPU (libopenh264) encoder — no hardware attempt. */
    GPU_ENCODER_CPU = 1,
    /* Force NVIDIA NVENC (h264_nvenc), falling back to CPU if it can't open. */
    GPU_ENCODER_NVENC = 2,
    /* Force Intel Quick Sync (h264_qsv), falling back to CPU if it can't open. */
    GPU_ENCODER_QUICKSYNC = 3,
    /* Force AMD AMF (h264_amf), falling back to CPU if it can't open. */
    GPU_ENCODER_AMF = 4,
    /* Force Linux VAAPI (h264_vaapi), falling back to CPU if it can't open. */
    GPU_ENCODER_VAAPI = 5,
    /* Force Apple VideoToolbox (h264_videotoolbox), falling back to CPU if it can't open. */
    GPU_ENCODER_VIDEOTOOLBOX = 6,
} GpuEncoderPreference;

/* Opens a video H.264 encoder AVCodecContext sized for canvas_width/canvas_height/canvas_fps
   at canvas_bit_rate_bps, honoring `preference`. Tries the requested hardware encoder(s) first
   (AUTO tries all four cross-platform candidates and VideoToolbox on macOS). Each software-frame
   candidate's requested pixel format comes from gpu_encoder.c's pix_fmt_for_encoder_name():
   AV_PIX_FMT_YUV420P for every encoder except h264_qsv, which gets AV_PIX_FMT_NV12. The filter
   chains conform each segment to the selected format. This split exists because h264_qsv was
   empirically observed on this dev machine's FFmpeg build to reject yuv420p outright ("Specified
   pixel format yuv420p is not supported by the h264_qsv encoder", listing only nv12/qsv as
   supported) even without real Quick Sync hardware present — a mismatch indistinguishable, before
   this fix, from "no GPU/driver available", so it always fell through to the CPU fallback.
   avcodec_open2() still fails at runtime for the ordinary "no compatible GPU/driver" case too —
   that's still treated identically here as "unavailable, fall back to CPU", it just no longer also
   catches Quick Sync's format mismatch.

   NVENC/AMF didn't reach the pix_fmt check at all on this machine (no NVIDIA GPU; AMD's
   amfrt64.dll isn't present), so whether they accept yuv420p directly on real hardware remains
   unverified — left unchanged since no observed failure justifies changing them. Likewise,
   fixing the yuv420p/nv12 mismatch does not by itself prove h264_qsv successfully *encodes* on
   real Quick Sync hardware — this function (like the rest of the GPU-encoder path) can't be
   exercised against real GPU hardware in this environment. The CPU fallback path itself IS
   exercised end-to-end for every preference value (see encode_test.rs's
   every_gpu_encoder_preference_falls_back_to_a_working_export), since forcing any hardware
   preference on this machine deterministically falls through to it. VAAPI is different from
   the other candidates: h264_vaapi consumes AV_PIX_FMT_VAAPI hardware surfaces. Its setup
   creates a VAAPI device and an NV12-backed AVHWFramesContext; filters.c uploads the software
   NV12 filter output into that pool before avcodec_send_frame. On Linux, OCA_VAAPI_DEVICE may
   name an explicit render node; otherwise /dev/dri/renderD128..191 are tried before FFmpeg's
   default device resolution. Missing drivers/devices/permissions all fall through to CPU.

   If none of the attempted encoders open, falls back to the first available CPU H.264 encoder — the
   "fallback pro encode por CPU" from the Fase 5 spec. Sets *out_used_gpu to 1 if a hardware
   encoder was actually opened, 0 if the CPU fallback (or explicit GPU_ENCODER_CPU) was used.
   If `out_pix_fmt` is non-NULL, sets it to the software pixel format the filter graph must emit
   (yuv420p or nv12). For VAAPI this deliberately differs from enc_ctx->pix_fmt
   (AV_PIX_FMT_VAAPI), because encode_write_packet performs the software-to-hardware upload.
   Callers must build their filter graph's final `format=...` conform stage to target this
   instead of hardcoding yuv420p. `global_header` should be nonzero when the output format wants
   AV_CODEC_FLAG_GLOBAL_HEADER set (out_ctx->oformat->flags & AVFMT_GLOBALHEADER).

   Returns NULL only if even the CPU fallback couldn't open (when neither libopenh264 nor
   libx264 is available). Caller owns the returned AVCodecContext (not yet
   attached to any AVStream). Defined in gpu_encoder.c. */
AVCodecContext *open_video_encoder(GpuEncoderPreference preference, int canvas_width,
                                   int canvas_height, AVRational canvas_fps,
                                   int64_t canvas_bit_rate_bps, int global_header,
                                   int *out_used_gpu, enum AVPixelFormat *out_pix_fmt);

/* Speed handling shared by the single- and multi-track timeline exports. All defined in
   timeline_common.c. */
float smooth_speed_ramp_average_speed(const ClipSegment *seg);
void build_setpts_str(const ClipSegment *seg, double source_duration_secs, char *out,
                      size_t out_size);

/* Opens the encoder for a pass that re-encodes an already exported file (text, shape and privacy
   blur overlays): hardware when one opens with yuv420p input, else the CPU encoder. Their filter
   graphs end in yuv420p, so an encoder that wants another format is not used. NULL on failure. */
AVCodecContext *open_overlay_pass_encoder(int width, int height, AVRational fps,
                                          int64_t bit_rate_bps, int global_header);

/* Returns the preferred available software H.264 encoder: libopenh264 when present, otherwise
   libx264. Returns NULL when the FFmpeg build provides neither. Defined in gpu_encoder.c. */
const AVCodec *find_cpu_h264_encoder(void);

/* Opens `path` and reads its stream info into a new AVFormatContext.
   Returns 0 on success (caller owns *fmt_ctx_out and must close it),
   -1 if avformat_open_input failed, -2 if avformat_find_stream_info failed
   (*fmt_ctx_out is NULL in both error cases). Defined in common.c. */
int open_input(const char *path, AVFormatContext **fmt_ctx_out);

typedef struct {
    AVFilterContext *buffersrc_ctx;
    AVFilterContext *buffersink_ctx;
    AVFilterGraph *graph;
} AudioFilterChain;

/* All defined in filters.c. */
void free_audio_filter_chain(AudioFilterChain *chain);
int init_audio_filter_chain(AVCodecContext *dec_ctx, const AVCodec *encoder,
                            const char *filter_descr, AudioFilterChain *chain);
int encode_write_packet(AVFormatContext *out_ctx, AVCodecContext *enc_ctx, AVStream *out_stream,
                        AVFrame *frame, AVPacket *enc_pkt);
int filter_encode_write_frame(AVFormatContext *out_ctx, AudioFilterChain *chain,
                              AVCodecContext *enc_ctx, AVStream *out_stream, AVFrame *frame,
                              AVFrame *filt_frame, AVPacket *enc_pkt);

/* Runs the audio graph and encoder of an export on their own thread (audio_worker.c). While one
   exists, out_ctx->opaque holds the mutex encode_write_packet takes around muxer writes. */
typedef struct AudioWorker AudioWorker;
AudioWorker *audio_worker_start(AVFormatContext *out_ctx, AudioFilterChain *chain,
                                AVCodecContext *encoder, AVStream *stream);
/* Queues a reference to `frame`; blocks while the queue is full. Negative once the worker failed.
 */
int audio_worker_push_frame(AudioWorker *worker, const AVFrame *frame);
/* Queues a filter command, applied in order with the frames pushed around it. */
int audio_worker_command(AudioWorker *worker, const char *target, const char *command,
                         const char *argument);
/* Stops and frees `*worker` (set to NULL). With `flush`, the graph and encoder are drained first;
   without, whatever is still queued is dropped. Returns negative if the worker had failed. */
int audio_worker_finish(AudioWorker **worker, int flush);

/* Speed handling and audio output shared by the single- and multi-track timeline exports. All
   defined in timeline_common.c. */
float smooth_speed_ramp_average_speed(const ClipSegment *seg);
void build_setpts_str(const ClipSegment *seg, double source_duration_secs, char *out,
                      size_t out_size);
int build_entry_transition(const ClipSegment *seg, int fps_num, int fps_den, double frame_offset,
                           int alpha, char *transition_str, size_t cap);
int build_segment_vfilter(const ClipSegment *seg, int canvas_width, int canvas_height, int fps_num,
                          int fps_den, const char *pix_fmt_name, char *buf, size_t cap);
/* Builds the export's audio graph and AAC encoder from `adec_ctx`, adds the audio stream, opens
   the output file, writes the container header (the video stream must already exist) and starts
   the audio worker. `*aenc_ctx`, `*audio_stream` and `*worker` are set as far as setup got, so
   the caller's cleanup can free what exists after a failure. */
EncodeStatus timeline_open_audio_output(AVFormatContext *out_ctx, AVCodecContext *adec_ctx,
                                        float target_lufs, const char *out_path,
                                        AudioFilterChain *achain, AVCodecContext **aenc_ctx,
                                        AVStream **audio_stream, AudioWorker **worker);
/* Queues the gain and tempo changes `seg` needs, ahead of its audio frames. */
void timeline_queue_segment_audio_commands(AudioWorker *worker, const ClipSegment *seg);
/* The segment's length in output seconds: the trimmed source length divided by the speed (the
   integral of 1/speed for a ramp), the time base the fades are specified in. */
double segment_output_secs(const ClipSegment *seg);
/* Multiplies the samples of a decoded audio frame by the segment's fade-in / fade-out ramp.
   `frame_start_secs` is the frame's first sample in source time. A no-op when the segment has no
   fades or the sample format is not one of the plain PCM/float layouts. */
void timeline_apply_audio_fade(AVFrame *frame, const ClipSegment *seg, double frame_start_secs);

/* Result of a re-encode pass; the values equal TextOverlayStatus and PrivacyBlurStatus, which
   the callers convert it to. */
typedef enum {
    REENCODE_OK = 0,
    REENCODE_ERR_OPEN_INPUT = 1,
    REENCODE_ERR_ALLOC_OUTPUT = 2,
    REENCODE_ERR_FILTER_GRAPH = 3,
    REENCODE_ERR_PIPELINE = 4,
} ReencodeStatus;

/* Decodes `in_path`, runs `filter_desc` (a graph from [in] to [out]) over its video, encodes the
   result to `out_path` and copies the audio unchanged. With `bound_output_to_input_frames` the
   output stops at one frame per input frame, for graphs with a source that never ends. Defined
   in reencode.c. */
ReencodeStatus avbridge_reencode_with_filter(const char *in_path, const char *out_path,
                                             const char *filter_desc, int canvas_width,
                                             int canvas_height, int canvas_fps_num,
                                             int canvas_fps_den, int bound_output_to_input_frames);

typedef struct VideoFilterChain {
    AVFilterContext *buffersrc_ctx;
    AVFilterContext *buffersink_ctx;
    AVFilterGraph *graph;
    /* A graph that runs over the whole export after this one (the text and shape overlays),
       or NULL. Owned by the export, not by this chain: it outlives every segment's chain. */
    struct VideoFilterChain *post;
    AVFrame *scratch; /* only set on a post chain: where its output frames land */
} VideoFilterChain;

/* All defined in filters.c. */
void free_video_filter_chain(VideoFilterChain *chain);
int init_video_filter_chain(AVCodecContext *dec_ctx, const char *filter_descr,
                            VideoFilterChain *chain);
/* Builds `chain` as a graph that runs over the finished frames of an export: the given size and
   pixel format in, `time_base` for `t` (one frame per tick), the same pixel format out. Returns
   a negative value when the description does not build. */
int init_post_filter_chain(int width, int height, enum AVPixelFormat pix_fmt, AVRational time_base,
                           const char *filter_descr, VideoFilterChain *chain);
void free_post_filter_chain(VideoFilterChain *chain);
/* Encodes `frame`, or first runs it through the `post` chains when there are any. */
int encode_video_frame_through(VideoFilterChain *post, AVFormatContext *out_ctx,
                               AVCodecContext *enc_ctx, AVStream *out_stream, AVFrame *frame,
                               AVPacket *enc_pkt);
/* Tells the `post` chains their input has ended and encodes what they still hold. */
int flush_post_filter_chains(VideoFilterChain *post, AVFormatContext *out_ctx,
                             AVCodecContext *enc_ctx, AVStream *out_stream, AVPacket *enc_pkt);
int filter_encode_write_video_frame(AVFormatContext *out_ctx, VideoFilterChain *chain,
                                    AVCodecContext *enc_ctx, AVStream *out_stream, AVFrame *frame,
                                    AVFrame *filt_frame, int64_t *next_pts, AVPacket *enc_pkt);

#endif
