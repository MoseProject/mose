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

#include <stdarg.h>
#include <string.h>

#ifdef _WIN32
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
static SRWLOCK loudness_log_mutex = SRWLOCK_INIT;
static void lock_loudness_log(void) {
    AcquireSRWLockExclusive(&loudness_log_mutex);
}
static void unlock_loudness_log(void) {
    ReleaseSRWLockExclusive(&loudness_log_mutex);
}
#else
#include <pthread.h>
static pthread_mutex_t loudness_log_mutex = PTHREAD_MUTEX_INITIALIZER;
static void lock_loudness_log(void) {
    pthread_mutex_lock(&loudness_log_mutex);
}
static void unlock_loudness_log(void) {
    pthread_mutex_unlock(&loudness_log_mutex);
}
#endif

#include <libavcodec/avcodec.h>
#include <libavfilter/avfilter.h>
#include <libavfilter/buffersink.h>
#include <libavfilter/buffersrc.h>
#include <libavformat/avformat.h>
#include <libavutil/channel_layout.h>
#include <libavutil/log.h>

/* Measurement calls remain serialized by the caller. Every callback, including logs from
   unrelated FFmpeg workers, shares this lock with capture setup and teardown. */
static char *g_loudness_log_buf = NULL;
static size_t g_loudness_log_cap = 0;
static size_t g_loudness_log_len = 0;
static int g_loudness_log_print_prefix = 1;
static AVFilterContext *g_loudness_log_filter = NULL;

static void loudness_log_callback(void *avcl, int level, const char *fmt, va_list vl) {
    lock_loudness_log();
    if (!g_loudness_log_filter || avcl != g_loudness_log_filter || level > AV_LOG_INFO) {
        unlock_loudness_log();
        av_log_default_callback(avcl, level, fmt, vl);
        return;
    }
    if (!g_loudness_log_buf || g_loudness_log_cap == 0 ||
        g_loudness_log_len >= g_loudness_log_cap - 1) {
        unlock_loudness_log();
        return;
    }
    char line[1024];
    int written =
        av_log_format_line2(avcl, level, fmt, vl, line, sizeof(line), &g_loudness_log_print_prefix);
    if (written <= 0) {
        unlock_loudness_log();
        return;
    }
    /* The formatter returns the required size, not the number stored in `line`. */
    size_t available = (size_t)written < sizeof(line) ? (size_t)written : sizeof(line) - 1;
    size_t remaining = g_loudness_log_cap - g_loudness_log_len - 1;
    size_t to_copy = available < remaining ? available : remaining;
    memcpy(g_loudness_log_buf + g_loudness_log_len, line, to_copy);
    g_loudness_log_len += to_copy;
    g_loudness_log_buf[g_loudness_log_len] = '\0';
    unlock_loudness_log();
}

/* Simpler than init_audio_filter_chain(): analysis only, nothing downstream needs the
   filtered samples in a specific format (they're discarded, not encoded), so no
   sample_formats/samplerates constraint on the sink. */
static int init_measure_filter_chain(AVCodecContext *dec_ctx, AudioFilterChain *chain) {
    char args[512];
    char chlayout_str[64];
    const AVFilter *buffersrc = avfilter_get_by_name("abuffer");
    const AVFilter *buffersink = avfilter_get_by_name("abuffersink");
    AVFilterInOut *outputs = avfilter_inout_alloc();
    AVFilterInOut *inputs = avfilter_inout_alloc();
    int ret;

    chain->buffersrc_ctx = NULL;
    chain->buffersink_ctx = NULL;
    chain->graph = avfilter_graph_alloc();
    if (!outputs || !inputs || !chain->graph) {
        ret = AVERROR(ENOMEM);
        goto end;
    }

    av_channel_layout_describe(&dec_ctx->ch_layout, chlayout_str, sizeof(chlayout_str));
    snprintf(args, sizeof(args), "time_base=1/%d:sample_rate=%d:sample_fmt=%s:channel_layout=%s",
             dec_ctx->sample_rate, dec_ctx->sample_rate,
             av_get_sample_fmt_name(dec_ctx->sample_fmt), chlayout_str);

    ret = avfilter_graph_create_filter(&chain->buffersrc_ctx, buffersrc, "in", args, NULL,
                                       chain->graph);
    if (ret < 0) goto end;

    ret = avfilter_graph_create_filter(&chain->buffersink_ctx, buffersink, "out", NULL, NULL,
                                       chain->graph);
    if (ret < 0) goto end;

    outputs->name = av_strdup("in");
    outputs->filter_ctx = chain->buffersrc_ctx;
    outputs->pad_idx = 0;
    outputs->next = NULL;

    inputs->name = av_strdup("out");
    inputs->filter_ctx = chain->buffersink_ctx;
    inputs->pad_idx = 0;
    inputs->next = NULL;

    ret = avfilter_graph_parse_ptr(chain->graph, "loudnorm=I=-16:TP=-1.5:LRA=11:print_format=json",
                                   &inputs, &outputs, NULL);
    if (ret < 0) goto end;

    ret = avfilter_graph_config(chain->graph, NULL);

end:
    avfilter_inout_free(&inputs);
    avfilter_inout_free(&outputs);
    if (ret < 0) {
        free_audio_filter_chain(chain);
    }
    return ret;
}

/* Pushes one frame through the filter graph and drains+discards whatever comes out (the
   filtered samples themselves aren't needed, only the JSON report loudnorm logs on EOF).
   Pass frame=NULL to flush the graph. */
static int drain_measure_frame(AudioFilterChain *chain, AVFrame *frame, AVFrame *filt_frame) {
    int ret = av_buffersrc_add_frame(chain->buffersrc_ctx, frame);
    if (ret < 0) return ret;

    while (1) {
        ret = av_buffersink_get_frame(chain->buffersink_ctx, filt_frame);
        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
            return 0;
        } else if (ret < 0) {
            return ret;
        }
        av_frame_unref(filt_frame);
    }
}

typedef struct {
    AudioFilterChain *chain;
    AVFrame *filtered_frame;
} MeasureFrameContext;

static int consume_measure_frame(AVFrame *frame, void *user_data) {
    MeasureFrameContext *context = user_data;
    return drain_measure_frame(context->chain, frame, context->filtered_frame);
}

LoudnessStatus avbridge_measure_loudness(const char *in_path, char *out_json, size_t out_json_len) {
    AVFormatContext *in_ctx = NULL;
    AVCodecContext *dec_ctx = NULL;
    AudioFilterChain chain = {0};
    AVPacket *pkt = NULL;
    AVFrame *dec_frame = NULL;
    AVFrame *filt_frame = NULL;
    LoudnessStatus status = LOUDNESS_OK;
    int audio_in_index = -1;
    int log_installed = 0;

    if (out_json && out_json_len > 0) {
        out_json[0] = '\0';
    }

    switch (open_input(in_path, &in_ctx)) {
    case -1:
        return LOUDNESS_ERR_OPEN_INPUT;
    case -2:
        return LOUDNESS_ERR_STREAM_INFO;
    }

    for (unsigned int i = 0; i < in_ctx->nb_streams; i++) {
        if (in_ctx->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_AUDIO) {
            audio_in_index = (int)i;
            break;
        }
    }
    if (audio_in_index < 0) {
        avformat_close_input(&in_ctx);
        return LOUDNESS_ERR_NO_AUDIO_STREAM;
    }

    {
        const AVCodec *decoder =
            avcodec_find_decoder(in_ctx->streams[audio_in_index]->codecpar->codec_id);
        if (!decoder) {
            status = LOUDNESS_ERR_DECODER;
            goto cleanup;
        }
        dec_ctx = avcodec_alloc_context3(decoder);
        if (!dec_ctx ||
            avcodec_parameters_to_context(dec_ctx, in_ctx->streams[audio_in_index]->codecpar) < 0) {
            status = LOUDNESS_ERR_DECODER;
            goto cleanup;
        }
        dec_ctx->pkt_timebase = in_ctx->streams[audio_in_index]->time_base;
        if (avcodec_open2(dec_ctx, decoder, NULL) < 0) {
            status = LOUDNESS_ERR_DECODER;
            goto cleanup;
        }
    }

    if (init_measure_filter_chain(dec_ctx, &chain) < 0) {
        status = LOUDNESS_ERR_FILTER_GRAPH;
        goto cleanup;
    }

    pkt = av_packet_alloc();
    dec_frame = av_frame_alloc();
    filt_frame = av_frame_alloc();
    if (!pkt || !dec_frame || !filt_frame) {
        status = LOUDNESS_ERR_PIPELINE;
        goto cleanup;
    }

    /* Capture only this graph's loudnorm filter, including its report during teardown. */
    lock_loudness_log();
    const AVFilter *loudnorm = avfilter_get_by_name("loudnorm");
    for (unsigned int i = 0; i < chain.graph->nb_filters; i++) {
        if (chain.graph->filters[i]->filter == loudnorm) {
            g_loudness_log_filter = chain.graph->filters[i];
            break;
        }
    }
    g_loudness_log_buf = out_json;
    g_loudness_log_cap = out_json_len;
    g_loudness_log_len = 0;
    g_loudness_log_print_prefix = 1;
    unlock_loudness_log();
    av_log_set_callback(loudness_log_callback);
    log_installed = 1;

    MeasureFrameContext measure_context = {&chain, filt_frame};
    if (decode_audio_stream(in_ctx, dec_ctx, audio_in_index, pkt, dec_frame,
                            consume_measure_frame, &measure_context) < 0) {
        status = LOUDNESS_ERR_PIPELINE;
        goto cleanup;
    }

cleanup:
    /* free_audio_filter_chain() (avfilter_graph_free) MUST run while the log capture is still
       installed: loudnorm prints its final JSON report during filter uninit — i.e. graph
       teardown — not while processing the EOF flush above. Resetting the callback first was
       an earlier bug here: the report would print to the console via
       av_log_default_callback instead of landing in out_json. */
    free_audio_filter_chain(&chain);

    if (log_installed) {
        lock_loudness_log();
        if (status == LOUDNESS_OK && (!out_json || out_json_len == 0 || out_json[0] == '\0')) {
            status = LOUDNESS_ERR_NO_REPORT;
        }
        g_loudness_log_buf = NULL;
        g_loudness_log_cap = 0;
        g_loudness_log_len = 0;
        g_loudness_log_filter = NULL;
        unlock_loudness_log();
        av_log_set_callback(av_log_default_callback);
    }
    avcodec_free_context(&dec_ctx);
    av_frame_free(&dec_frame);
    av_frame_free(&filt_frame);
    av_packet_free(&pkt);
    avformat_close_input(&in_ctx);
    return status;
}
