// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

#include "bridge.h"

#include <math.h>
#include <string.h>

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/hwcontext.h>
#include <libavutil/imgutils.h>
#include <libavutil/mem.h>
#include <libavutil/opt.h>
#include <libavutil/pixdesc.h>
#include <libswscale/swscale.h>

enum { MAX_RGBA_FRAME_BYTES = 256 * 1024 * 1024 };

struct AvbridgeDecodeSession {
    AVFormatContext *format;
    AVCodecContext *decoder;
    AVFrame *decoded;
    /* Unconverted frames for cheap frame-accurate seeks: the newest decoded frame and the one
       before it. Only the frame that is finally presented is converted to RGBA. */
    AVFrame *latest;
    AVFrame *held;
    int has_latest;
    int has_held;
    /* Software copy of a hardware frame; unused for software decode. */
    AVFrame *transfer;
    AVBufferRef *hw_device;
    enum AVPixelFormat hw_pix_fmt;
    AVPacket *packet;
    struct SwsContext *sws;
    int sws_width;
    int sws_height;
    int sws_format;
    int stream_index;
    int draining;
};

static void close_session(AvbridgeDecodeSession *session) {
    if (!session) return;
    sws_freeContext(session->sws);
    av_packet_free(&session->packet);
    av_frame_free(&session->transfer);
    av_frame_free(&session->latest);
    av_frame_free(&session->held);
    av_frame_free(&session->decoded);
    avcodec_free_context(&session->decoder);
    av_buffer_unref(&session->hw_device);
    avformat_close_input(&session->format);
    av_free(session);
}

/* Creating a hardware device costs hundreds of milliseconds, so one device per type is created
   lazily and shared (reference counted) by every session in the process. The lock is a plain
   spin lock: it is only held while looking up or creating a device. */
#ifdef _WIN32
#include <windows.h>
static volatile LONG device_lock;
#define LOCK_DEVICES()                                                                             \
    while (InterlockedExchange(&device_lock, 1))                                                   \
    Sleep(0)
#define UNLOCK_DEVICES() InterlockedExchange(&device_lock, 0)
#else
static volatile int device_lock;
#define LOCK_DEVICES()                                                                             \
    while (__sync_lock_test_and_set(&device_lock, 1)) {                                            \
    }
#define UNLOCK_DEVICES() __sync_lock_release(&device_lock)
#endif

enum { MAX_CACHED_DEVICE_TYPES = 32 };
static AVBufferRef *cached_devices[MAX_CACHED_DEVICE_TYPES];

/* A new reference to the shared device of `type`, creating it on first use; NULL on failure. */
static AVBufferRef *acquire_device(enum AVHWDeviceType type) {
    if ((int)type < 0 || (int)type >= MAX_CACHED_DEVICE_TYPES) return NULL;
    LOCK_DEVICES();
    if (!cached_devices[type]) {
        AVBufferRef *created = NULL;
        if (av_hwdevice_ctx_create(&created, type, NULL, NULL, 0) >= 0)
            cached_devices[type] = created;
    }
    AVBufferRef *reference = cached_devices[type] ? av_buffer_ref(cached_devices[type]) : NULL;
    UNLOCK_DEVICES();
    return reference;
}

void avbridge_hw_device_cache_clear(void) {
    LOCK_DEVICES();
    for (int type = 0; type < MAX_CACHED_DEVICE_TYPES; type++)
        av_buffer_unref(&cached_devices[type]);
    UNLOCK_DEVICES();
}

/* Software decode uses every core: frame threads for throughput, slice threads where the codec
   supports them. Output order is unchanged, so seek-and-decode-forward stays exact. */
static void enable_decoder_threads(AVCodecContext *decoder) {
    decoder->thread_count = 0;
    decoder->thread_type = FF_THREAD_FRAME | FF_THREAD_SLICE;
}

/* Colour conversion is a full-frame pass; slice-threaded swscale keeps it off the critical path
   for 1080p and larger frames. */
static struct SwsContext *threaded_sws(int width, int height, enum AVPixelFormat format) {
    struct SwsContext *context = sws_alloc_context();
    if (!context) return NULL;
    av_opt_set_int(context, "srcw", width, 0);
    av_opt_set_int(context, "srch", height, 0);
    av_opt_set_int(context, "src_format", format, 0);
    av_opt_set_int(context, "dstw", width, 0);
    av_opt_set_int(context, "dsth", height, 0);
    av_opt_set_int(context, "dst_format", AV_PIX_FMT_RGBA, 0);
    av_opt_set_int(context, "sws_flags", SWS_BILINEAR, 0);
    av_opt_set_int(context, "threads", 0, 0);
    if (sws_init_context(context, NULL, NULL) < 0) {
        sws_freeContext(context);
        return NULL;
    }
    return context;
}

/* Picks the negotiated hardware surface format when the driver offers it, otherwise the first
   software format so decoding degrades instead of failing. */
static enum AVPixelFormat select_format(AVCodecContext *context,
                                        const enum AVPixelFormat *formats) {
    enum AVPixelFormat wanted = (enum AVPixelFormat)(intptr_t)context->opaque;
    const enum AVPixelFormat *first_software = NULL;
    for (const enum AVPixelFormat *format = formats; *format != AV_PIX_FMT_NONE; format++) {
        if (*format == wanted) return *format;
        const AVPixFmtDescriptor *descriptor = av_pix_fmt_desc_get(*format);
        if (!first_software && descriptor && !(descriptor->flags & AV_PIX_FMT_FLAG_HWACCEL))
            first_software = format;
    }
    return first_software ? *first_software : AV_PIX_FMT_NONE;
}

/* Attaches the first hardware device the codec supports. Returns 1 on success; on any failure
   the decoder is left untouched so the caller proceeds with software decode. */
static int try_hardware(AvbridgeDecodeSession *session, const AVCodec *codec) {
    for (int index = 0;; index++) {
        const AVCodecHWConfig *config = avcodec_get_hw_config(codec, index);
        if (!config) return 0;
        if (!(config->methods & AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX)) continue;
        AVBufferRef *device = acquire_device(config->device_type);
        if (!device) continue;
        session->hw_device = device;
        session->hw_pix_fmt = config->pix_fmt;
        session->decoder->hw_device_ctx = av_buffer_ref(device);
        session->decoder->opaque = (void *)(intptr_t)config->pix_fmt;
        session->decoder->get_format = select_format;
        return 1;
    }
}

DecodeStatus avbridge_decode_open_ex(const char *path, int hardware,
                                     AvbridgeDecodeSession **out_session, DecodeInfo *out_info) {
    if (!path || !out_session || !out_info) return DECODE_ERR_OPEN_INPUT;
    *out_session = NULL;

    AvbridgeDecodeSession *session = av_calloc(1, sizeof(*session));
    if (!session) return DECODE_ERR_DECODER;
    if (avformat_open_input(&session->format, path, NULL, NULL) < 0) {
        close_session(session);
        return DECODE_ERR_OPEN_INPUT;
    }
    if (avformat_find_stream_info(session->format, NULL) < 0) {
        close_session(session);
        return DECODE_ERR_STREAM_INFO;
    }
    session->stream_index =
        av_find_best_stream(session->format, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    if (session->stream_index < 0) {
        close_session(session);
        return DECODE_ERR_NO_VIDEO_STREAM;
    }

    AVStream *stream = session->format->streams[session->stream_index];
    const AVCodec *codec = avcodec_find_decoder(stream->codecpar->codec_id);
    if (!codec) {
        close_session(session);
        return DECODE_ERR_DECODER;
    }
    session->hw_pix_fmt = AV_PIX_FMT_NONE;
    session->decoder = avcodec_alloc_context3(codec);
    if (!session->decoder ||
        avcodec_parameters_to_context(session->decoder, stream->codecpar) < 0) {
        close_session(session);
        return DECODE_ERR_DECODER;
    }
    int using_hardware = hardware ? try_hardware(session, codec) : 0;
    if (!using_hardware) enable_decoder_threads(session->decoder);
    if (avcodec_open2(session->decoder, codec, NULL) < 0) {
        if (!using_hardware) {
            close_session(session);
            return DECODE_ERR_DECODER;
        }
        /* The driver refused the stream: retry once, software only. */
        avcodec_free_context(&session->decoder);
        av_buffer_unref(&session->hw_device);
        session->hw_pix_fmt = AV_PIX_FMT_NONE;
        using_hardware = 0;
        session->decoder = avcodec_alloc_context3(codec);
        if (!session->decoder ||
            avcodec_parameters_to_context(session->decoder, stream->codecpar) < 0) {
            close_session(session);
            return DECODE_ERR_DECODER;
        }
        enable_decoder_threads(session->decoder);
        if (avcodec_open2(session->decoder, codec, NULL) < 0) {
            close_session(session);
            return DECODE_ERR_DECODER;
        }
    }
    session->decoded = av_frame_alloc();
    session->transfer = av_frame_alloc();
    session->latest = av_frame_alloc();
    session->held = av_frame_alloc();
    session->packet = av_packet_alloc();
    if (!session->decoded || !session->transfer || !session->latest || !session->held ||
        !session->packet) {
        close_session(session);
        return DECODE_ERR_DECODER;
    }

    AVRational fps = stream->avg_frame_rate.num ? stream->avg_frame_rate : stream->r_frame_rate;
    memset(out_info, 0, sizeof(*out_info));
    out_info->width = session->decoder->width;
    out_info->height = session->decoder->height;
    out_info->fps_num = fps.num;
    out_info->fps_den = fps.den;
    out_info->hardware = using_hardware;
    if (session->format->duration != AV_NOPTS_VALUE) {
        out_info->duration_secs = (double)session->format->duration / AV_TIME_BASE;
    } else if (stream->duration != AV_NOPTS_VALUE) {
        out_info->duration_secs = stream->duration * av_q2d(stream->time_base);
    }
    *out_session = session;
    return DECODE_OK;
}

DecodeStatus avbridge_decode_open(const char *path, AvbridgeDecodeSession **out_session,
                                  DecodeInfo *out_info) {
    return avbridge_decode_open_ex(path, 0, out_session, out_info);
}

DecodeStatus avbridge_decode_seek(AvbridgeDecodeSession *session, double timestamp_secs) {
    if (!session || !isfinite(timestamp_secs) || timestamp_secs < 0.0) return DECODE_ERR_SEEK;
    AVStream *stream = session->format->streams[session->stream_index];
    int64_t target =
        av_rescale_q((int64_t)(timestamp_secs * AV_TIME_BASE), AV_TIME_BASE_Q, stream->time_base);
    if (av_seek_frame(session->format, session->stream_index, target, AVSEEK_FLAG_BACKWARD) < 0)
        return DECODE_ERR_SEEK;
    avcodec_flush_buffers(session->decoder);
    av_packet_unref(session->packet);
    av_frame_unref(session->decoded);
    av_frame_unref(session->latest);
    av_frame_unref(session->held);
    session->has_latest = 0;
    session->has_held = 0;
    session->draining = 0;
    return DECODE_OK;
}

static DecodeStatus copy_rgba(AvbridgeDecodeSession *session, AVFrame *frame,
                              DecodedRgbaFrame *out_frame, uint8_t *given, size_t given_cap) {
    int width = frame->width;
    int height = frame->height;
    if (width <= 0 || height <= 0) return DECODE_ERR_FRAME_SIZE;
    int stride = av_image_get_linesize(AV_PIX_FMT_RGBA, width, 0);
    int bytes = av_image_get_buffer_size(AV_PIX_FMT_RGBA, width, height, 1);
    if (stride <= 0 || bytes <= 0 || bytes > MAX_RGBA_FRAME_BYTES) return DECODE_ERR_FRAME_SIZE;

    int borrowed = given && given_cap >= (size_t)bytes;
    uint8_t *pixels = borrowed ? given : av_malloc((size_t)bytes);
    if (!pixels) return DECODE_ERR_FRAME_SIZE;
    uint8_t *dst_data[4] = {0};
    int dst_linesize[4] = {0};
    if (av_image_fill_arrays(dst_data, dst_linesize, pixels, AV_PIX_FMT_RGBA, width, height, 1) <
        0) {
        if (!borrowed) av_free(pixels);
        return DECODE_ERR_FRAME_SIZE;
    }
    if (!session->sws || session->sws_width != width || session->sws_height != height ||
        session->sws_format != frame->format) {
        sws_freeContext(session->sws);
        session->sws = threaded_sws(width, height, (enum AVPixelFormat)frame->format);
        session->sws_width = width;
        session->sws_height = height;
        session->sws_format = frame->format;
    }
    if (!session->sws) {
        if (!borrowed) av_free(pixels);
        return DECODE_ERR_PIPELINE;
    }
    if (sws_scale(session->sws, (const uint8_t *const *)frame->data, frame->linesize, 0, height,
                  dst_data, dst_linesize) != height) {
        if (!borrowed) av_free(pixels);
        return DECODE_ERR_PIPELINE;
    }
    AVStream *stream = session->format->streams[session->stream_index];
    int64_t pts = frame->best_effort_timestamp;
    if (pts == AV_NOPTS_VALUE) pts = frame->pts;
    memset(out_frame, 0, sizeof(*out_frame));
    out_frame->pixels = pixels;
    out_frame->pixel_len = (size_t)bytes;
    out_frame->width = width;
    out_frame->height = height;
    out_frame->stride_bytes = stride;
    out_frame->timestamp_secs = pts == AV_NOPTS_VALUE ? 0.0 : pts * av_q2d(stream->time_base);
    out_frame->borrowed = borrowed;
    return DECODE_OK;
}

/* Presents `frame` (hardware surfaces are copied to system memory first) as packed RGBA. */
static DecodeStatus convert_frame(AvbridgeDecodeSession *session, AVFrame *frame,
                                  DecodedRgbaFrame *out_frame, uint8_t *given, size_t given_cap) {
    AVFrame *source = frame;
    if (session->hw_pix_fmt != AV_PIX_FMT_NONE && source->format == session->hw_pix_fmt) {
        av_frame_unref(session->transfer);
        if (av_hwframe_transfer_data(session->transfer, source, 0) < 0) return DECODE_ERR_PIPELINE;
        av_frame_copy_props(session->transfer, source);
        source = session->transfer;
    }
    DecodeStatus status = copy_rgba(session, source, out_frame, given, given_cap);
    av_frame_unref(session->transfer);
    return status;
}

/* Decodes the next frame into session->decoded, reading and feeding packets as needed. */
static DecodeStatus receive_decoded(AvbridgeDecodeSession *session, const uint8_t *cancel) {
    for (;;) {
        if (cancel && *cancel) return DECODE_CANCELLED;
        int receive = avcodec_receive_frame(session->decoder, session->decoded);
        if (receive == 0) return DECODE_OK;
        if (receive == AVERROR_EOF) return DECODE_EOF;
        if (receive != AVERROR(EAGAIN)) return DECODE_ERR_PIPELINE;
        if (session->draining) return DECODE_EOF;

        int read = av_read_frame(session->format, session->packet);
        if (read == AVERROR_EOF) {
            if (avcodec_send_packet(session->decoder, NULL) < 0) return DECODE_ERR_PIPELINE;
            session->draining = 1;
            continue;
        }
        if (read < 0) return DECODE_ERR_PIPELINE;
        if (session->packet->stream_index != session->stream_index) {
            av_packet_unref(session->packet);
            continue;
        }
        int send = avcodec_send_packet(session->decoder, session->packet);
        av_packet_unref(session->packet);
        if (send < 0 && send != AVERROR(EAGAIN)) return DECODE_ERR_PIPELINE;
    }
}

static double frame_seconds(AvbridgeDecodeSession *session, const AVFrame *frame) {
    AVStream *stream = session->format->streams[session->stream_index];
    int64_t pts = frame->best_effort_timestamp;
    if (pts == AV_NOPTS_VALUE) pts = frame->pts;
    return pts == AV_NOPTS_VALUE ? 0.0 : pts * av_q2d(stream->time_base);
}

DecodeStatus avbridge_decode_next_rgba(AvbridgeDecodeSession *session, const uint8_t *cancel,
                                       DecodedRgbaFrame *out_frame) {
    if (!session || !out_frame) return DECODE_ERR_PIPELINE;
    uint8_t *given = out_frame->pixels;
    size_t given_cap = out_frame->pixel_len;
    memset(out_frame, 0, sizeof(*out_frame));
    /* A frame decoded ahead by avbridge_decode_advance is next in stream order. */
    session->has_held = 0;
    av_frame_unref(session->held);
    if (session->has_latest) {
        DecodeStatus status = convert_frame(session, session->latest, out_frame, given, given_cap);
        av_frame_unref(session->latest);
        session->has_latest = 0;
        return status;
    }
    DecodeStatus status = receive_decoded(session, cancel);
    if (status != DECODE_OK) return status;
    status = convert_frame(session, session->decoded, out_frame, given, given_cap);
    av_frame_unref(session->decoded);
    return status;
}

DecodeStatus avbridge_decode_advance(AvbridgeDecodeSession *session, const uint8_t *cancel,
                                     double *out_timestamp_secs) {
    if (!session || !out_timestamp_secs) return DECODE_ERR_PIPELINE;
    DecodeStatus status = receive_decoded(session, cancel);
    if (status != DECODE_OK) return status;
    if (session->has_latest) {
        av_frame_unref(session->held);
        av_frame_move_ref(session->held, session->latest);
        session->has_held = 1;
    }
    av_frame_move_ref(session->latest, session->decoded);
    session->has_latest = 1;
    *out_timestamp_secs = frame_seconds(session, session->latest);
    return DECODE_OK;
}

int avbridge_decode_has_held(AvbridgeDecodeSession *session) {
    return session && session->has_held;
}

DecodeStatus avbridge_decode_take_rgba(AvbridgeDecodeSession *session, int newest,
                                       DecodedRgbaFrame *out_frame) {
    if (!session || !out_frame) return DECODE_ERR_PIPELINE;
    uint8_t *given = out_frame->pixels;
    size_t given_cap = out_frame->pixel_len;
    memset(out_frame, 0, sizeof(*out_frame));
    AVFrame *frame = newest ? session->latest : session->held;
    int *present = newest ? &session->has_latest : &session->has_held;
    if (!*present) return DECODE_EOF;
    DecodeStatus status = convert_frame(session, frame, out_frame, given, given_cap);
    av_frame_unref(frame);
    *present = 0;
    return status;
}

void avbridge_decode_free_frame(DecodedRgbaFrame *frame) {
    if (!frame) return;
    if (!frame->borrowed) av_freep(&frame->pixels);
    memset(frame, 0, sizeof(*frame));
}

void avbridge_decode_close(AvbridgeDecodeSession *session) {
    close_session(session);
}
