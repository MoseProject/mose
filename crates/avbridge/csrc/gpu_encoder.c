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

#include "bridge.h"
#include "bridge_internal.h"

#include <stdlib.h>
#include <string.h>

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/hwcontext.h>
#include <libavutil/log.h>

#ifdef __linux__
#include <unistd.h>
#endif

/* h264_qsv rejects AV_PIX_FMT_YUV420P as a direct software-frame input (empirically observed:
   "Specified pixel format yuv420p is not supported by the h264_qsv encoder", wants nv12/qsv
   instead) — every other candidate here accepts yuv420p. See open_video_encoder's doc comment
   in bridge_internal.h for the full story. */
static enum AVPixelFormat pix_fmt_for_encoder_name(const char *encoder_name) {
    if (strcmp(encoder_name, "h264_qsv") == 0) {
        return AV_PIX_FMT_NV12;
    }
    return AV_PIX_FMT_YUV420P;
}

const AVCodec *find_cpu_h264_encoder(void) {
    static const char *const candidates[] = {"libopenh264", "libx264"};
    for (size_t i = 0; i < sizeof(candidates) / sizeof(candidates[0]); i++) {
        const AVCodec *encoder = avcodec_find_encoder_by_name(candidates[i]);
        if (encoder) {
            return encoder;
        }
    }
    return NULL;
}

/* Tries to open a video encoder by name at the given canvas geometry. Returns NULL (freeing
   any partially-allocated context) if the encoder isn't compiled into this FFmpeg build, or if
   avcodec_open2 fails — the latter is the common case for a hardware encoder name on a machine
   without that vendor's GPU/driver present. On success, if `out_pix_fmt` is non-NULL, reports
   the pixel format the opened context actually uses — callers need this to make the filter
   graph feeding the encoder target the same format. */
static AVCodecContext *try_open_encoder(const char *encoder_name, int canvas_width,
                                        int canvas_height, AVRational canvas_fps,
                                        int64_t canvas_bit_rate_bps, int global_header,
                                        enum AVPixelFormat *out_pix_fmt) {
    const AVCodec *enc = avcodec_find_encoder_by_name(encoder_name);
    if (!enc) {
        return NULL;
    }
    AVCodecContext *ctx = avcodec_alloc_context3(enc);
    if (!ctx) {
        return NULL;
    }
    ctx->width = canvas_width;
    ctx->height = canvas_height;
    ctx->pix_fmt = pix_fmt_for_encoder_name(encoder_name);
    ctx->time_base = av_inv_q(canvas_fps);
    ctx->framerate = canvas_fps;
    ctx->gop_size = (canvas_fps.num / canvas_fps.den) * KEYFRAME_INTERVAL_SECS;
    ctx->max_b_frames = 0;
    ctx->bit_rate = canvas_bit_rate_bps;
    /* Software encoders (libopenh264, libx264) run on one thread unless told otherwise; a
       hardware encoder ignores this. */
    ctx->thread_count = SOFTWARE_ENCODER_THREADS;
    if (global_header) {
        ctx->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
    }
    if (avcodec_open2(ctx, enc, NULL) < 0) {
        avcodec_free_context(&ctx);
        return NULL;
    }
    if (out_pix_fmt) {
        *out_pix_fmt = ctx->pix_fmt;
    }
    return ctx;
}

/* Opens h264_vaapi with the hardware-frame pool required by that encoder. The ordinary
   software encoders above consume the filter graph's AVFrame directly; VAAPI instead consumes
   AV_PIX_FMT_VAAPI surfaces backed by a device. filters.c recognizes hw_frames_ctx and uploads
   each NV12 software frame into this pool immediately before avcodec_send_frame(). */
#ifdef __linux__
static AVCodecContext *try_open_vaapi_device(const char *device_path, int canvas_width,
                                             int canvas_height, AVRational canvas_fps,
                                             int64_t canvas_bit_rate_bps, int global_header,
                                             enum AVPixelFormat *out_filter_pix_fmt) {
    const AVCodec *enc = avcodec_find_encoder_by_name("h264_vaapi");
    AVBufferRef *device_ref = NULL;
    AVBufferRef *frames_ref = NULL;
    AVCodecContext *ctx = NULL;

    if (!enc ||
        av_hwdevice_ctx_create(&device_ref, AV_HWDEVICE_TYPE_VAAPI, device_path, NULL, 0) < 0) {
        return NULL;
    }

    ctx = avcodec_alloc_context3(enc);
    if (!ctx) {
        goto fail;
    }
    ctx->width = canvas_width;
    ctx->height = canvas_height;
    ctx->pix_fmt = AV_PIX_FMT_VAAPI;
    ctx->time_base = av_inv_q(canvas_fps);
    ctx->framerate = canvas_fps;
    ctx->gop_size = (canvas_fps.num / canvas_fps.den) * KEYFRAME_INTERVAL_SECS;
    ctx->max_b_frames = 0;
    ctx->bit_rate = canvas_bit_rate_bps;
    if (global_header) {
        ctx->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
    }

    frames_ref = av_hwframe_ctx_alloc(device_ref);
    if (!frames_ref) {
        goto fail;
    }
    AVHWFramesContext *frames_ctx = (AVHWFramesContext *)frames_ref->data;
    frames_ctx->format = AV_PIX_FMT_VAAPI;
    frames_ctx->sw_format = AV_PIX_FMT_NV12;
    frames_ctx->width = canvas_width;
    frames_ctx->height = canvas_height;
    frames_ctx->initial_pool_size = 20;
    if (av_hwframe_ctx_init(frames_ref) < 0) {
        goto fail;
    }
    ctx->hw_frames_ctx = av_buffer_ref(frames_ref);
    if (!ctx->hw_frames_ctx || avcodec_open2(ctx, enc, NULL) < 0) {
        goto fail;
    }

    av_buffer_unref(&frames_ref);
    av_buffer_unref(&device_ref);
    if (out_filter_pix_fmt) {
        *out_filter_pix_fmt = AV_PIX_FMT_NV12;
    }
    return ctx;

fail:
    av_buffer_unref(&frames_ref);
    av_buffer_unref(&device_ref);
    avcodec_free_context(&ctx);
    return NULL;
}
#endif

/* Tries an explicit override first, then Linux DRM render nodes in deterministic order, and
   finally FFmpeg's own default-device resolution. Vendor VAAPI drivers remain an operating-
   system/GPU responsibility; absence or permission failure is intentionally just
   "unavailable" so open_video_encoder can fall back to a CPU encoder. */
static AVCodecContext *try_open_vaapi_encoder(int canvas_width, int canvas_height,
                                              AVRational canvas_fps, int64_t canvas_bit_rate_bps,
                                              int global_header,
                                              enum AVPixelFormat *out_filter_pix_fmt) {
#ifdef __linux__
    const char *override_path = getenv("OCA_VAAPI_DEVICE");
    if (override_path && override_path[0]) {
        return try_open_vaapi_device(override_path, canvas_width, canvas_height, canvas_fps,
                                     canvas_bit_rate_bps, global_header, out_filter_pix_fmt);
    }

    char render_node[64];
    for (int node = 128; node <= 191; node++) {
        snprintf(render_node, sizeof(render_node), "/dev/dri/renderD%d", node);
        if (access(render_node, R_OK | W_OK) != 0) {
            continue;
        }
        AVCodecContext *ctx =
            try_open_vaapi_device(render_node, canvas_width, canvas_height, canvas_fps,
                                  canvas_bit_rate_bps, global_header, out_filter_pix_fmt);
        if (ctx) {
            return ctx;
        }
    }

    return try_open_vaapi_device(NULL, canvas_width, canvas_height, canvas_fps, canvas_bit_rate_bps,
                                 global_header, out_filter_pix_fmt);
#else
    (void)canvas_width;
    (void)canvas_height;
    (void)canvas_fps;
    (void)canvas_bit_rate_bps;
    (void)global_header;
    (void)out_filter_pix_fmt;
    return NULL;
#endif
}

AVCodecContext *open_video_encoder(GpuEncoderPreference preference, int canvas_width,
                                   int canvas_height, AVRational canvas_fps,
                                   int64_t canvas_bit_rate_bps, int global_header,
                                   int *out_used_gpu, enum AVPixelFormat *out_pix_fmt) {
    static const char *const nvenc = "h264_nvenc";
    static const char *const quicksync = "h264_qsv";
    static const char *const amf = "h264_amf";
    static const char *const vaapi = "h264_vaapi";
    static const char *const videotoolbox = "h264_videotoolbox";

    const char *hw_candidates[5] = {NULL, NULL, NULL, NULL, NULL};
    int hw_candidate_count = 0;
    switch (preference) {
    case GPU_ENCODER_NVENC:
        hw_candidates[hw_candidate_count++] = nvenc;
        break;
    case GPU_ENCODER_QUICKSYNC:
        hw_candidates[hw_candidate_count++] = quicksync;
        break;
    case GPU_ENCODER_AMF:
        hw_candidates[hw_candidate_count++] = amf;
        break;
    case GPU_ENCODER_VAAPI:
        hw_candidates[hw_candidate_count++] = vaapi;
        break;
    case GPU_ENCODER_VIDEOTOOLBOX:
        hw_candidates[hw_candidate_count++] = videotoolbox;
        break;
    case GPU_ENCODER_AUTO:
        hw_candidates[hw_candidate_count++] = nvenc;
        hw_candidates[hw_candidate_count++] = quicksync;
        hw_candidates[hw_candidate_count++] = vaapi;
        hw_candidates[hw_candidate_count++] = amf;
#ifdef __APPLE__
        hw_candidates[hw_candidate_count++] = videotoolbox;
#endif
        break;
    case GPU_ENCODER_CPU:
    default:
        break;
    }

    for (int i = 0; i < hw_candidate_count; i++) {
        AVCodecContext *ctx =
            strcmp(hw_candidates[i], vaapi) == 0
                ? try_open_vaapi_encoder(canvas_width, canvas_height, canvas_fps,
                                         canvas_bit_rate_bps, global_header, out_pix_fmt)
                : try_open_encoder(hw_candidates[i], canvas_width, canvas_height, canvas_fps,
                                   canvas_bit_rate_bps, global_header, out_pix_fmt);
        if (ctx) {
            /* No existing way to observe which encoder a call actually opened from outside this
               function -- `out_used_gpu` is a plain bool every current caller passes NULL for
               (avcodec_open2 itself is silent on success, unlike its very vocal failure paths for
               qsv/amf), which made "did real GPU hardware verification ever happen" impossible to
               answer definitively (ROADMAP.md P4 item 19). This one log line is the minimal fix:
               purely additive, no signature/ABI change, safe to leave in permanently. */
            av_log(NULL, AV_LOG_INFO, "oca: opened video encoder %s (hardware)\n",
                   hw_candidates[i]);
            if (out_used_gpu) {
                *out_used_gpu = 1;
            }
            return ctx;
        }
    }

    const AVCodec *cpu = find_cpu_h264_encoder();
    AVCodecContext *ctx = cpu ? try_open_encoder(cpu->name, canvas_width, canvas_height, canvas_fps,
                                                 canvas_bit_rate_bps, global_header, out_pix_fmt)
                              : NULL;
    if (ctx) {
        av_log(NULL, AV_LOG_INFO, "oca: opened video encoder %s (cpu fallback)\n", cpu->name);
    }
    if (out_used_gpu) {
        *out_used_gpu = 0;
    }
    return ctx;
}

AVCodecContext *open_overlay_pass_encoder(int width, int height, AVRational fps,
                                          int64_t bit_rate_bps, int global_header) {
    enum AVPixelFormat pix_fmt = AV_PIX_FMT_NONE;
    AVCodecContext *ctx = open_video_encoder(GPU_ENCODER_AUTO, width, height, fps, bit_rate_bps,
                                             global_header, NULL, &pix_fmt);
    if (ctx && pix_fmt != AV_PIX_FMT_YUV420P) {
        /* Quick Sync and VAAPI want other formats or hardware surfaces than the yuv420p these
           passes' filter graphs end in. */
        avcodec_free_context(&ctx);
        ctx = open_video_encoder(GPU_ENCODER_CPU, width, height, fps, bit_rate_bps, global_header,
                                 NULL, &pix_fmt);
    }
    return ctx;
}
