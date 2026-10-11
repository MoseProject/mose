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
#include <libavfilter/avfilter.h>
#include <libavfilter/buffersink.h>
#include <libavfilter/buffersrc.h>
#include <libavformat/avformat.h>
#include <libavutil/avutil.h>
#include <libavutil/channel_layout.h>

#define PCM_TARGET_SAMPLE_RATE 16000

/* Resamples to 16kHz and downmixes to mono float — same graph-building shape as
   waveform.c's init_waveform_filter_chain, just with an aresample stage prepended since
   whisper.cpp requires exactly 16kHz input regardless of the source's native rate. */
static int init_pcm_filter_chain(AVCodecContext *dec_ctx, AudioFilterChain *chain) {
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

    ret = avfilter_graph_parse_ptr(
        chain->graph,
        "aresample=" AV_STRINGIFY(
            PCM_TARGET_SAMPLE_RATE) ",aformat=sample_fmts=flt:channel_layouts=mono",
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

/* Growable output buffer for the decoded PCM samples — total sample count isn't known ahead
   of decode (duration estimates aren't exact), so this doubles capacity as needed rather than
   requiring the caller to pre-size a buffer the way avbridge_generate_waveform's fixed
   bucket_count does. */
typedef struct {
    float *data;
    int64_t len;
    int64_t cap;
} PcmBuffer;

static int pcm_buffer_reserve(PcmBuffer *buf, int64_t additional) {
    if (buf->len + additional <= buf->cap) {
        return 0;
    }
    int64_t new_cap = buf->cap == 0 ? 4096 : buf->cap;
    while (new_cap < buf->len + additional) {
        new_cap *= 2;
    }
    float *grown = realloc(buf->data, (size_t)new_cap * sizeof(float));
    if (!grown) {
        return -1;
    }
    buf->data = grown;
    buf->cap = new_cap;
    return 0;
}

static int append_pcm_frame(PcmBuffer *buf, const AVFrame *frame) {
    if (frame->nb_samples <= 0) {
        return 0;
    }
    if (!frame->data[0] || pcm_buffer_reserve(buf, frame->nb_samples) < 0) {
        return -1;
    }
    memcpy(buf->data + buf->len, frame->data[0], (size_t)frame->nb_samples * sizeof(float));
    buf->len += frame->nb_samples;
    return 0;
}

/* Pushes one decoded frame through the resample/mono-downmix graph and appends every filtered
   sample to `buf`. Pass frame=NULL to flush. */
static int drain_pcm_frame(AudioFilterChain *chain, AVFrame *frame, AVFrame *filt_frame,
                           PcmBuffer *buf) {
    int ret = av_buffersrc_add_frame(chain->buffersrc_ctx, frame);
    if (ret < 0) return ret;

    while (1) {
        ret = av_buffersink_get_frame(chain->buffersink_ctx, filt_frame);
        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
            return 0;
        } else if (ret < 0) {
            return ret;
        }
        if (append_pcm_frame(buf, filt_frame) < 0) {
            av_frame_unref(filt_frame);
            return AVERROR(ENOMEM);
        }
        av_frame_unref(filt_frame);
    }
}

typedef struct {
    AudioFilterChain *chain;
    AVFrame *filtered_frame;
    PcmBuffer *buffer;
} PcmFrameContext;

static int consume_pcm_frame(AVFrame *frame, void *user_data) {
    PcmFrameContext *context = user_data;
    return drain_pcm_frame(context->chain, frame, context->filtered_frame, context->buffer);
}

PcmStatus avbridge_extract_pcm_16k_mono(const char *in_path, float **out_samples,
                                        int64_t *out_sample_count) {
    AVFormatContext *in_ctx = NULL;
    AVCodecContext *dec_ctx = NULL;
    AudioFilterChain chain = {0};
    AVPacket *pkt = NULL;
    AVFrame *dec_frame = NULL;
    AVFrame *filt_frame = NULL;
    PcmBuffer buf = {0};
    PcmStatus status = PCM_OK;
    int audio_in_index = -1;

    switch (open_input(in_path, &in_ctx)) {
    case -1:
        return PCM_ERR_OPEN_INPUT;
    case -2:
        return PCM_ERR_STREAM_INFO;
    }

    for (unsigned int i = 0; i < in_ctx->nb_streams; i++) {
        if (in_ctx->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_AUDIO) {
            audio_in_index = (int)i;
            break;
        }
    }
    if (audio_in_index < 0) {
        avformat_close_input(&in_ctx);
        return PCM_ERR_NO_AUDIO_STREAM;
    }

    {
        AVCodecParameters *par = in_ctx->streams[audio_in_index]->codecpar;
        const AVCodec *decoder = avcodec_find_decoder(par->codec_id);
        if (!decoder) {
            status = PCM_ERR_DECODER;
            goto cleanup;
        }
        dec_ctx = avcodec_alloc_context3(decoder);
        if (!dec_ctx || avcodec_parameters_to_context(dec_ctx, par) < 0) {
            status = PCM_ERR_DECODER;
            goto cleanup;
        }
        dec_ctx->pkt_timebase = in_ctx->streams[audio_in_index]->time_base;
        if (avcodec_open2(dec_ctx, decoder, NULL) < 0) {
            status = PCM_ERR_DECODER;
            goto cleanup;
        }
    }

    if (init_pcm_filter_chain(dec_ctx, &chain) < 0) {
        status = PCM_ERR_FILTER_GRAPH;
        goto cleanup;
    }

    pkt = av_packet_alloc();
    dec_frame = av_frame_alloc();
    filt_frame = av_frame_alloc();
    if (!pkt || !dec_frame || !filt_frame) {
        status = PCM_ERR_PIPELINE;
        goto cleanup;
    }

    PcmFrameContext pcm_context = {&chain, filt_frame, &buf};
    if (decode_audio_stream(in_ctx, dec_ctx, audio_in_index, pkt, dec_frame, consume_pcm_frame,
                            &pcm_context) < 0) {
        status = PCM_ERR_PIPELINE;
        goto cleanup;
    }

cleanup:
    free_audio_filter_chain(&chain);
    avcodec_free_context(&dec_ctx);
    av_frame_free(&dec_frame);
    av_frame_free(&filt_frame);
    av_packet_free(&pkt);
    avformat_close_input(&in_ctx);

    if (status == PCM_OK) {
        *out_samples = buf.data;
        *out_sample_count = buf.len;
    } else {
        free(buf.data);
    }
    return status;
}

void avbridge_free_pcm_buffer(float *samples) {
    free(samples);
}
