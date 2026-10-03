// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

#include "bridge.h"

#include <math.h>
#include <stdio.h>
#include <string.h>

#include <libavcodec/avcodec.h>
#include <libavfilter/avfilter.h>
#include <libavfilter/buffersink.h>
#include <libavfilter/buffersrc.h>
#include <libavformat/avformat.h>
#include <libavutil/channel_layout.h>
#include <libavutil/mem.h>
#include <libavutil/opt.h>

/* A filtered block is at most a few thousand frames, but bound it anyway so hostile metadata
   can never request an unchecked allocation. */
enum { MAX_AUDIO_BLOCK_FRAMES = 1 << 20, MAX_AUDIO_CHANNELS = 2 };

struct AvbridgeAudioSession {
    AVFormatContext *format;
    AVCodecContext *decoder;
    AVFrame *decoded;
    AVFrame *filtered;
    AVPacket *packet;
    AVFilterGraph *graph;
    AVFilterContext *src;
    AVFilterContext *sink;
    int stream_index;
    int draining;
    int flushed;
    int target_rate;
    int target_channels;
};

static void free_graph(AvbridgeAudioSession *session) {
    avfilter_graph_free(&session->graph);
    session->src = NULL;
    session->sink = NULL;
}

static void close_session(AvbridgeAudioSession *session) {
    if (!session) return;
    free_graph(session);
    av_packet_free(&session->packet);
    av_frame_free(&session->filtered);
    av_frame_free(&session->decoded);
    avcodec_free_context(&session->decoder);
    avformat_close_input(&session->format);
    av_free(session);
}

/* abuffer -> aresample -> aformat(flt packed, target layout/rate) -> abuffersink. Rebuilt after
   every seek so resampler state never leaks across a discontinuity. */
static int build_graph(AvbridgeAudioSession *session) {
    char args[512];
    char layout[64];
    char descr[256];
    const AVFilter *abuffer = avfilter_get_by_name("abuffer");
    const AVFilter *abuffersink = avfilter_get_by_name("abuffersink");
    AVFilterInOut *outputs = avfilter_inout_alloc();
    AVFilterInOut *inputs = avfilter_inout_alloc();
    int ret = AVERROR(ENOMEM);

    free_graph(session);
    session->graph = avfilter_graph_alloc();
    if (!outputs || !inputs || !session->graph) goto end;

    AVCodecContext *dec = session->decoder;
    av_channel_layout_describe(&dec->ch_layout, layout, sizeof(layout));
    snprintf(args, sizeof(args),
             "time_base=1/%d:sample_rate=%d:sample_fmt=%s:channel_layout=%s", dec->sample_rate,
             dec->sample_rate, av_get_sample_fmt_name(dec->sample_fmt), layout);
    ret = avfilter_graph_create_filter(&session->src, abuffer, "in", args, NULL, session->graph);
    if (ret < 0) goto end;
    ret = avfilter_graph_create_filter(&session->sink, abuffersink, "out", NULL, NULL,
                                       session->graph);
    if (ret < 0) goto end;

    outputs->name = av_strdup("in");
    outputs->filter_ctx = session->src;
    outputs->pad_idx = 0;
    outputs->next = NULL;
    inputs->name = av_strdup("out");
    inputs->filter_ctx = session->sink;
    inputs->pad_idx = 0;
    inputs->next = NULL;

    snprintf(descr, sizeof(descr),
             "aresample=%d,aformat=sample_fmts=flt:sample_rates=%d:channel_layouts=%s",
             session->target_rate, session->target_rate,
             session->target_channels == 1 ? "mono" : "stereo");
    ret = avfilter_graph_parse_ptr(session->graph, descr, &inputs, &outputs, NULL);
    if (ret < 0) goto end;
    ret = avfilter_graph_config(session->graph, NULL);

end:
    avfilter_inout_free(&inputs);
    avfilter_inout_free(&outputs);
    if (ret < 0) free_graph(session);
    return ret;
}

DecodeStatus avbridge_audio_open(const char *path, int target_sample_rate, int target_channels,
                                 AvbridgeAudioSession **out_session,
                                 AudioDecodeInfo *out_info) {
    if (!path || !out_session || !out_info) return DECODE_ERR_OPEN_INPUT;
    *out_session = NULL;
    if (target_sample_rate < 8000 || target_sample_rate > 192000 || target_channels < 1 ||
        target_channels > MAX_AUDIO_CHANNELS)
        return DECODE_ERR_DECODER;

    AvbridgeAudioSession *session = av_calloc(1, sizeof(*session));
    if (!session) return DECODE_ERR_DECODER;
    session->target_rate = target_sample_rate;
    session->target_channels = target_channels;
    if (avformat_open_input(&session->format, path, NULL, NULL) < 0) {
        close_session(session);
        return DECODE_ERR_OPEN_INPUT;
    }
    if (avformat_find_stream_info(session->format, NULL) < 0) {
        close_session(session);
        return DECODE_ERR_STREAM_INFO;
    }
    session->stream_index = av_find_best_stream(session->format, AVMEDIA_TYPE_AUDIO, -1, -1,
                                                NULL, 0);
    if (session->stream_index < 0) {
        close_session(session);
        return DECODE_ERR_NO_AUDIO_STREAM;
    }

    AVStream *stream = session->format->streams[session->stream_index];
    const AVCodec *codec = avcodec_find_decoder(stream->codecpar->codec_id);
    if (!codec) {
        close_session(session);
        return DECODE_ERR_DECODER;
    }
    session->decoder = avcodec_alloc_context3(codec);
    if (!session->decoder ||
        avcodec_parameters_to_context(session->decoder, stream->codecpar) < 0) {
        close_session(session);
        return DECODE_ERR_DECODER;
    }
    session->decoder->pkt_timebase = stream->time_base;
    if (avcodec_open2(session->decoder, codec, NULL) < 0 || session->decoder->sample_rate <= 0 ||
        session->decoder->ch_layout.nb_channels <= 0) {
        close_session(session);
        return DECODE_ERR_DECODER;
    }
    session->decoded = av_frame_alloc();
    session->filtered = av_frame_alloc();
    session->packet = av_packet_alloc();
    if (!session->decoded || !session->filtered || !session->packet ||
        build_graph(session) < 0) {
        close_session(session);
        return DECODE_ERR_PIPELINE;
    }

    memset(out_info, 0, sizeof(*out_info));
    out_info->sample_rate = target_sample_rate;
    out_info->channels = target_channels;
    out_info->source_sample_rate = session->decoder->sample_rate;
    out_info->source_channels = session->decoder->ch_layout.nb_channels;
    if (stream->duration != AV_NOPTS_VALUE) {
        out_info->duration_secs = stream->duration * av_q2d(stream->time_base);
    } else if (session->format->duration != AV_NOPTS_VALUE) {
        out_info->duration_secs = (double)session->format->duration / AV_TIME_BASE;
    }
    *out_session = session;
    return DECODE_OK;
}

DecodeStatus avbridge_audio_seek(AvbridgeAudioSession *session, double timestamp_secs) {
    if (!session || !isfinite(timestamp_secs) || timestamp_secs < 0.0) return DECODE_ERR_SEEK;
    AVStream *stream = session->format->streams[session->stream_index];
    int64_t target = av_rescale_q((int64_t)(timestamp_secs * AV_TIME_BASE), AV_TIME_BASE_Q,
                                  stream->time_base);
    if (av_seek_frame(session->format, session->stream_index, target, AVSEEK_FLAG_BACKWARD) < 0)
        return DECODE_ERR_SEEK;
    avcodec_flush_buffers(session->decoder);
    av_packet_unref(session->packet);
    av_frame_unref(session->decoded);
    av_frame_unref(session->filtered);
    session->draining = 0;
    session->flushed = 0;
    if (build_graph(session) < 0) return DECODE_ERR_PIPELINE;
    return DECODE_OK;
}

static DecodeStatus take_filtered(AvbridgeAudioSession *session, DecodedAudioBlock *out_block) {
    AVFrame *frame = session->filtered;
    int channels = frame->ch_layout.nb_channels;
    if (frame->nb_samples <= 0 || frame->nb_samples > MAX_AUDIO_BLOCK_FRAMES ||
        channels != session->target_channels || frame->format != AV_SAMPLE_FMT_FLT)
        return DECODE_ERR_PIPELINE;
    size_t bytes = (size_t)frame->nb_samples * (size_t)channels * sizeof(float);
    float *samples = av_malloc(bytes);
    if (!samples) return DECODE_ERR_FRAME_SIZE;
    memcpy(samples, frame->data[0], bytes);
    AVRational tb = av_buffersink_get_time_base(session->sink);
    out_block->samples = samples;
    out_block->frame_count = frame->nb_samples;
    out_block->timestamp_secs =
        frame->pts == AV_NOPTS_VALUE ? 0.0 : (double)frame->pts * av_q2d(tb);
    return DECODE_OK;
}

DecodeStatus avbridge_audio_next_block(AvbridgeAudioSession *session, const uint8_t *cancel,
                                       DecodedAudioBlock *out_block) {
    if (!session || !out_block) return DECODE_ERR_PIPELINE;
    memset(out_block, 0, sizeof(*out_block));
    for (;;) {
        if (cancel && *cancel) return DECODE_CANCELLED;

        int got = av_buffersink_get_frame(session->sink, session->filtered);
        if (got >= 0) {
            DecodeStatus status = take_filtered(session, out_block);
            av_frame_unref(session->filtered);
            return status;
        }
        if (got == AVERROR_EOF) return DECODE_EOF;
        if (got != AVERROR(EAGAIN)) return DECODE_ERR_PIPELINE;

        /* The graph needs more input: pull a decoded frame, or read another packet. */
        int receive = avcodec_receive_frame(session->decoder, session->decoded);
        if (receive == 0) {
            AVStream *stream = session->format->streams[session->stream_index];
            int64_t pts = session->decoded->best_effort_timestamp;
            if (pts != AV_NOPTS_VALUE) {
                session->decoded->pts = av_rescale_q(
                    pts, stream->time_base,
                    (AVRational){1, session->decoder->sample_rate});
            }
            int add = av_buffersrc_add_frame(session->src, session->decoded);
            av_frame_unref(session->decoded);
            if (add < 0) return DECODE_ERR_PIPELINE;
            continue;
        }
        if (receive == AVERROR_EOF) {
            if (!session->flushed) {
                session->flushed = 1;
                if (av_buffersrc_add_frame(session->src, NULL) < 0) return DECODE_ERR_PIPELINE;
                continue;
            }
            return DECODE_EOF;
        }
        if (receive != AVERROR(EAGAIN)) return DECODE_ERR_PIPELINE;
        /* A draining decoder must yield EOF, never EAGAIN; bail out rather than spin. */
        if (session->draining) return DECODE_ERR_PIPELINE;

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

void avbridge_audio_free_block(DecodedAudioBlock *block) {
    if (!block) return;
    av_freep(&block->samples);
    memset(block, 0, sizeof(*block));
}

void avbridge_audio_close(AvbridgeAudioSession *session) { close_session(session); }
