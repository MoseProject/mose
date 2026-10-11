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

#include <libavcodec/avcodec.h>
#include <libavfilter/avfilter.h>
#include <libavfilter/buffersink.h>
#include <libavfilter/buffersrc.h>
#include <libavformat/avformat.h>
#include <libavutil/channel_layout.h>

/* Same shape as init_measure_filter_chain() but the filter graph downmixes to mono float
   instead of running loudnorm — the filtered samples themselves are what gets read here,
   not discarded. */
static int init_waveform_filter_chain(AVCodecContext *dec_ctx, AudioFilterChain *chain) {
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

    ret = avfilter_graph_parse_ptr(chain->graph, "aformat=sample_fmts=flt:channel_layouts=mono",
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

/* Peak-accumulation state threaded through the waveform decode/filter loop below — a bucket
   index is purely sample-index math (sample_index / bucket_size_samples), so it can't drift
   from timestamp jitter mid-decode the way a wall-clock computation could. */
typedef struct {
    float *out_min;
    float *out_max;
    int bucket_count;
    int64_t bucket_size_samples;
    int64_t sample_index;
} WaveformAccumulator;

static void accumulate_waveform_frame(WaveformAccumulator *acc, const AVFrame *frame) {
    const float *samples = (const float *)frame->data[0];
    for (int i = 0; i < frame->nb_samples; i++) {
        int64_t bucket = acc->sample_index / acc->bucket_size_samples;
        if (bucket >= acc->bucket_count) {
            bucket = acc->bucket_count - 1;
        }
        float s = samples[i];
        if (s < acc->out_min[bucket]) acc->out_min[bucket] = s;
        if (s > acc->out_max[bucket]) acc->out_max[bucket] = s;
        acc->sample_index++;
    }
}

/* Pushes one decoded frame through the mono-downmix graph and accumulates every filtered
   sample into `acc`'s min/max buckets. Pass frame=NULL to flush. */
static int drain_waveform_frame(AudioFilterChain *chain, AVFrame *frame, AVFrame *filt_frame,
                                WaveformAccumulator *acc) {
    int ret = av_buffersrc_add_frame(chain->buffersrc_ctx, frame);
    if (ret < 0) return ret;

    while (1) {
        ret = av_buffersink_get_frame(chain->buffersink_ctx, filt_frame);
        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
            return 0;
        } else if (ret < 0) {
            return ret;
        }
        accumulate_waveform_frame(acc, filt_frame);
        av_frame_unref(filt_frame);
    }
}

typedef struct {
    AudioFilterChain *chain;
    AVFrame *filtered_frame;
    WaveformAccumulator *accumulator;
} WaveformFrameContext;

static int consume_waveform_frame(AVFrame *frame, void *user_data) {
    WaveformFrameContext *context = user_data;
    return drain_waveform_frame(context->chain, frame, context->filtered_frame,
                                context->accumulator);
}

WaveformStatus avbridge_generate_waveform(const char *in_path, int bucket_count, float *out_min,
                                          float *out_max) {
    AVFormatContext *in_ctx = NULL;
    AVCodecContext *dec_ctx = NULL;
    AudioFilterChain chain = {0};
    AVPacket *pkt = NULL;
    AVFrame *dec_frame = NULL;
    AVFrame *filt_frame = NULL;
    WaveformAccumulator acc;
    WaveformStatus status = WAVEFORM_OK;
    int audio_in_index = -1;
    int64_t total_samples;
    double duration_secs = 0.0;
    AVStream *audio_stream;

    if (bucket_count <= 0) {
        return WAVEFORM_ERR_PIPELINE;
    }
    for (int i = 0; i < bucket_count; i++) {
        out_min[i] = 0.0f;
        out_max[i] = 0.0f;
    }

    switch (open_input(in_path, &in_ctx)) {
    case -1:
        return WAVEFORM_ERR_OPEN_INPUT;
    case -2:
        return WAVEFORM_ERR_STREAM_INFO;
    }

    for (unsigned int i = 0; i < in_ctx->nb_streams; i++) {
        if (in_ctx->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_AUDIO) {
            audio_in_index = (int)i;
            break;
        }
    }
    if (audio_in_index < 0) {
        avformat_close_input(&in_ctx);
        return WAVEFORM_ERR_NO_AUDIO_STREAM;
    }

    audio_stream = in_ctx->streams[audio_in_index];
    if (in_ctx->duration != (int64_t)AV_NOPTS_VALUE) {
        duration_secs = (double)in_ctx->duration / AV_TIME_BASE;
    } else if (audio_stream->duration != (int64_t)AV_NOPTS_VALUE) {
        duration_secs = audio_stream->duration * av_q2d(audio_stream->time_base);
    }

    {
        const AVCodec *decoder = avcodec_find_decoder(audio_stream->codecpar->codec_id);
        if (!decoder) {
            status = WAVEFORM_ERR_DECODER;
            goto cleanup;
        }
        dec_ctx = avcodec_alloc_context3(decoder);
        if (!dec_ctx || avcodec_parameters_to_context(dec_ctx, audio_stream->codecpar) < 0) {
            status = WAVEFORM_ERR_DECODER;
            goto cleanup;
        }
        dec_ctx->pkt_timebase = audio_stream->time_base;
        if (avcodec_open2(dec_ctx, decoder, NULL) < 0) {
            status = WAVEFORM_ERR_DECODER;
            goto cleanup;
        }
    }

    if (init_waveform_filter_chain(dec_ctx, &chain) < 0) {
        status = WAVEFORM_ERR_FILTER_GRAPH;
        goto cleanup;
    }

    pkt = av_packet_alloc();
    dec_frame = av_frame_alloc();
    filt_frame = av_frame_alloc();
    if (!pkt || !dec_frame || !filt_frame) {
        status = WAVEFORM_ERR_PIPELINE;
        goto cleanup;
    }

    total_samples = (int64_t)(duration_secs * dec_ctx->sample_rate);
    acc.out_min = out_min;
    acc.out_max = out_max;
    acc.bucket_count = bucket_count;
    acc.bucket_size_samples = total_samples > 0 ? total_samples / bucket_count : 0;
    if (acc.bucket_size_samples <= 0) {
        acc.bucket_size_samples = 1;
    }
    acc.sample_index = 0;

    WaveformFrameContext waveform_context = {&chain, filt_frame, &acc};
    if (decode_audio_stream(in_ctx, dec_ctx, audio_in_index, pkt, dec_frame, consume_waveform_frame,
                            &waveform_context) < 0) {
        status = WAVEFORM_ERR_PIPELINE;
        goto cleanup;
    }

cleanup:
    free_audio_filter_chain(&chain);
    avcodec_free_context(&dec_ctx);
    av_frame_free(&dec_frame);
    av_frame_free(&filt_frame);
    av_packet_free(&pkt);
    avformat_close_input(&in_ctx);
    return status;
}
