// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

#include "bridge.h"

#include <math.h>
#include <string.h>

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/hwcontext.h>
#include <libavutil/imgutils.h>
#include <libavutil/pixdesc.h>
#include <libavutil/mem.h>
#include <libswscale/swscale.h>

enum { MAX_RGBA_FRAME_BYTES = 256 * 1024 * 1024 };

struct AvbridgeDecodeSession {
    AVFormatContext *format;
    AVCodecContext *decoder;
    AVFrame *decoded;
    /* Software copy of a hardware frame; unused for software decode. */
    AVFrame *transfer;
    AVBufferRef *hw_device;
    enum AVPixelFormat hw_pix_fmt;
    AVPacket *packet;
    struct SwsContext *sws;
    int stream_index;
    int draining;
};

static void close_session(AvbridgeDecodeSession *session) {
    if (!session) return;
    sws_freeContext(session->sws);
    av_packet_free(&session->packet);
    av_frame_free(&session->transfer);
    av_frame_free(&session->decoded);
    avcodec_free_context(&session->decoder);
    av_buffer_unref(&session->hw_device);
    avformat_close_input(&session->format);
    av_free(session);
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
        AVBufferRef *device = NULL;
        if (av_hwdevice_ctx_create(&device, config->device_type, NULL, NULL, 0) < 0) continue;
        session->hw_device = device;
        session->hw_pix_fmt = config->pix_fmt;
        session->decoder->hw_device_ctx = av_buffer_ref(device);
        session->decoder->opaque = (void *)(intptr_t)config->pix_fmt;
        session->decoder->get_format = select_format;
        return 1;
    }
}

DecodeStatus avbridge_decode_open_ex(const char *path, int hardware,
                                     AvbridgeDecodeSession **out_session,
                                     DecodeInfo *out_info) {
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
    session->stream_index = av_find_best_stream(session->format, AVMEDIA_TYPE_VIDEO, -1, -1,
                                                NULL, 0);
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
            avcodec_parameters_to_context(session->decoder, stream->codecpar) < 0 ||
            avcodec_open2(session->decoder, codec, NULL) < 0) {
            close_session(session);
            return DECODE_ERR_DECODER;
        }
    }
    session->decoded = av_frame_alloc();
    session->transfer = av_frame_alloc();
    session->packet = av_packet_alloc();
    if (!session->decoded || !session->transfer || !session->packet) {
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
    int64_t target = av_rescale_q((int64_t)(timestamp_secs * AV_TIME_BASE), AV_TIME_BASE_Q,
                                  stream->time_base);
    if (av_seek_frame(session->format, session->stream_index, target, AVSEEK_FLAG_BACKWARD) < 0)
        return DECODE_ERR_SEEK;
    avcodec_flush_buffers(session->decoder);
    av_packet_unref(session->packet);
    av_frame_unref(session->decoded);
    session->draining = 0;
    return DECODE_OK;
}

static DecodeStatus copy_rgba(AvbridgeDecodeSession *session, AVFrame *frame,
                              DecodedRgbaFrame *out_frame) {
    int width = frame->width;
    int height = frame->height;
    if (width <= 0 || height <= 0) return DECODE_ERR_FRAME_SIZE;
    int stride = av_image_get_linesize(AV_PIX_FMT_RGBA, width, 0);
    int bytes = av_image_get_buffer_size(AV_PIX_FMT_RGBA, width, height, 1);
    if (stride <= 0 || bytes <= 0 || bytes > MAX_RGBA_FRAME_BYTES) return DECODE_ERR_FRAME_SIZE;

    uint8_t *pixels = av_malloc((size_t)bytes);
    if (!pixels) return DECODE_ERR_FRAME_SIZE;
    uint8_t *dst_data[4] = {0};
    int dst_linesize[4] = {0};
    if (av_image_fill_arrays(dst_data, dst_linesize, pixels, AV_PIX_FMT_RGBA, width, height, 1) < 0) {
        av_free(pixels);
        return DECODE_ERR_FRAME_SIZE;
    }
    session->sws = sws_getCachedContext(session->sws, width, height, (enum AVPixelFormat)frame->format,
                                         width, height, AV_PIX_FMT_RGBA, SWS_BILINEAR, NULL, NULL, NULL);
    if (!session->sws) {
        av_free(pixels);
        return DECODE_ERR_PIPELINE;
    }
    if (sws_scale(session->sws, (const uint8_t *const *)frame->data, frame->linesize, 0, height,
                  dst_data, dst_linesize) != height) {
        av_free(pixels);
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
    return DECODE_OK;
}

DecodeStatus avbridge_decode_next_rgba(AvbridgeDecodeSession *session, const uint8_t *cancel,
                                       DecodedRgbaFrame *out_frame) {
    if (!session || !out_frame) return DECODE_ERR_PIPELINE;
    memset(out_frame, 0, sizeof(*out_frame));
    for (;;) {
        if (cancel && *cancel) return DECODE_CANCELLED;
        int receive = avcodec_receive_frame(session->decoder, session->decoded);
        if (receive == 0) {
            AVFrame *source = session->decoded;
            if (session->hw_pix_fmt != AV_PIX_FMT_NONE && source->format == session->hw_pix_fmt) {
                av_frame_unref(session->transfer);
                if (av_hwframe_transfer_data(session->transfer, source, 0) < 0) {
                    av_frame_unref(session->decoded);
                    return DECODE_ERR_PIPELINE;
                }
                av_frame_copy_props(session->transfer, source);
                source = session->transfer;
            }
            DecodeStatus status = copy_rgba(session, source, out_frame);
            av_frame_unref(session->transfer);
            av_frame_unref(session->decoded);
            return status;
        }
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

void avbridge_decode_free_frame(DecodedRgbaFrame *frame) {
    if (!frame) return;
    av_freep(&frame->pixels);
    memset(frame, 0, sizeof(*frame));
}

void avbridge_decode_close(AvbridgeDecodeSession *session) { close_session(session); }
