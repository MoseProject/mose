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

#include "bridge_internal.h"
#include "bridge_thread.h"

#include <libavfilter/buffersink.h>
#include <libavfilter/buffersrc.h>
#include <libavutil/channel_layout.h>
#include <libavutil/hwcontext.h>
#include <libavutil/opt.h>

void free_audio_filter_chain(AudioFilterChain *chain) {
    avfilter_graph_free(&chain->graph);
    chain->buffersrc_ctx = NULL;
    chain->buffersink_ctx = NULL;
}

/* Builds `filter_descr` (e.g. "loudnorm=...,alimiter=..." or just "anull" for a passthrough
   format-conversion-only chain) between an abuffer source shaped like dec_ctx's decoded audio
   and an abuffersink constrained to a format `encoder` accepts (so the graph auto-inserts any
   needed aformat/aresample conversion, per FFmpeg's own transcode_aac.c example) — without
   this, avcodec_open2() on the encoder can reject whatever raw format the filter chain
   happens to produce. */
int init_audio_filter_chain(AVCodecContext *dec_ctx, const AVCodec *encoder,
                            const char *filter_descr, AudioFilterChain *chain) {
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

    /* PCM in Matroska has only a channel count ("2 channels"), which no encoder accepts: give it
       the usual layout for that count. Decoded frames follow the decoder's layout. */
    if (dec_ctx->ch_layout.order == AV_CHANNEL_ORDER_UNSPEC && dec_ctx->ch_layout.nb_channels > 0) {
        av_channel_layout_default(&dec_ctx->ch_layout, dec_ctx->ch_layout.nb_channels);
    }
    av_channel_layout_describe(&dec_ctx->ch_layout, chlayout_str, sizeof(chlayout_str));
    snprintf(args, sizeof(args), "time_base=1/%d:sample_rate=%d:sample_fmt=%s:channel_layout=%s",
             dec_ctx->sample_rate, dec_ctx->sample_rate,
             av_get_sample_fmt_name(dec_ctx->sample_fmt), chlayout_str);

    ret = avfilter_graph_create_filter(&chain->buffersrc_ctx, buffersrc, "in", args, NULL,
                                       chain->graph);
    if (ret < 0) goto end;

    /* abuffersink, built in two steps (alloc, then init) instead of the one-shot
       avfilter_graph_create_filter(): "sample_fmts" is a pre-init-only option — setting it
       after the filter is initialized fails with "not a runtime option". */
    chain->buffersink_ctx = avfilter_graph_alloc_filter(chain->graph, buffersink, "out");
    if (!chain->buffersink_ctx) {
        ret = AVERROR(ENOMEM);
        goto end;
    }

    /* Constrain the sink to formats/rates `encoder` actually accepts, so the graph inserts
       any needed aformat/aresample conversion automatically (per FFmpeg's own
       transcode_aac.c example) instead of avcodec_open2() rejecting whatever the filter
       chain happens to produce. As of this FFmpeg version these are proper array-typed
       options set via av_opt_set_array() — the older av_opt_set_bin()-with-a-raw-array
       idiom ("sample_fmts") is deprecated and, here, silently fails to constrain anything. */
    {
        const enum AVSampleFormat *sample_fmts = NULL;
        int num_sample_fmts = 0;
        if (avcodec_get_supported_config(NULL, encoder, AV_CODEC_CONFIG_SAMPLE_FORMAT, 0,
                                         (const void **)&sample_fmts, &num_sample_fmts) == 0 &&
            sample_fmts && num_sample_fmts > 0) {
            ret =
                av_opt_set_array(chain->buffersink_ctx, "sample_formats", AV_OPT_SEARCH_CHILDREN, 0,
                                 (unsigned)num_sample_fmts, AV_OPT_TYPE_SAMPLE_FMT, sample_fmts);
            if (ret < 0) goto end;
        }
    }

    {
        const int *sample_rates = NULL;
        int num_sample_rates = 0;
        if (avcodec_get_supported_config(NULL, encoder, AV_CODEC_CONFIG_SAMPLE_RATE, 0,
                                         (const void **)&sample_rates, &num_sample_rates) == 0 &&
            sample_rates && num_sample_rates > 0) {
            /* Prefer the source's own rate when the encoder already accepts it — negotiation
               otherwise doesn't necessarily pick the closest/cheapest match from the allowed
               set, and will happily insert a gratuitous upsample (seen: 44100 -> 96000, the
               first entry in AAC's supported-rates list) when no rate is preferred. */
            int source_rate_ok = 0;
            for (int i = 0; i < num_sample_rates; i++) {
                if (sample_rates[i] == dec_ctx->sample_rate) {
                    source_rate_ok = 1;
                    break;
                }
            }
            if (source_rate_ok) {
                ret = av_opt_set_array(chain->buffersink_ctx, "samplerates", AV_OPT_SEARCH_CHILDREN,
                                       0, 1, AV_OPT_TYPE_INT, &dec_ctx->sample_rate);
            } else {
                ret =
                    av_opt_set_array(chain->buffersink_ctx, "samplerates", AV_OPT_SEARCH_CHILDREN,
                                     0, (unsigned)num_sample_rates, AV_OPT_TYPE_INT, sample_rates);
            }
            if (ret < 0) goto end;
        }
    }

    ret = avfilter_init_str(chain->buffersink_ctx, NULL);
    if (ret < 0) goto end;

    outputs->name = av_strdup("in");
    outputs->filter_ctx = chain->buffersrc_ctx;
    outputs->pad_idx = 0;
    outputs->next = NULL;

    inputs->name = av_strdup("out");
    inputs->filter_ctx = chain->buffersink_ctx;
    inputs->pad_idx = 0;
    inputs->next = NULL;

    ret = avfilter_graph_parse_ptr(chain->graph, filter_descr, &inputs, &outputs, NULL);
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

/* Drains every packet the encoder has ready and writes it. Pass frame=NULL once, at the end,
   to flush. */
int encode_write_packet(AVFormatContext *out_ctx, AVCodecContext *enc_ctx, AVStream *out_stream,
                        AVFrame *frame, AVPacket *enc_pkt) {
    AVFrame *hw_frame = NULL;
    AVFrame *frame_to_send = frame;

    /* A video frame whose size differs from the encoder's (a clip filter that changes the size
       and was not conformed back) makes the encoders copy past the frame's rows: FFmpeg then
       aborts the whole process. Refuse it as a pipeline error instead. */
    if (frame && enc_ctx->codec_type == AVMEDIA_TYPE_VIDEO &&
        (frame->width != enc_ctx->width || frame->height != enc_ctx->height)) {
        return AVERROR(EINVAL);
    }

    /* Hardware encoders such as h264_vaapi consume hardware surfaces rather than the NV12
       software frames emitted by the filter graph. Upload only when the encoder exposes a
       hardware-frame pool; audio and all existing software encoders keep the old zero-copy
       path. avcodec_send_frame takes its own references, so the temporary surface can be
       released immediately after the call. */
    if (frame && enc_ctx->codec_type == AVMEDIA_TYPE_VIDEO && enc_ctx->hw_frames_ctx &&
        frame->format != enc_ctx->pix_fmt) {
        hw_frame = av_frame_alloc();
        if (!hw_frame) return AVERROR(ENOMEM);
        hw_frame->format = enc_ctx->pix_fmt;
        hw_frame->width = frame->width;
        hw_frame->height = frame->height;
        int upload_ret = av_hwframe_get_buffer(enc_ctx->hw_frames_ctx, hw_frame, 0);
        if (upload_ret >= 0) {
            upload_ret = av_hwframe_transfer_data(hw_frame, frame, 0);
        }
        if (upload_ret >= 0) {
            upload_ret = av_frame_copy_props(hw_frame, frame);
        }
        if (upload_ret < 0) {
            av_frame_free(&hw_frame);
            return upload_ret;
        }
        frame_to_send = hw_frame;
    }

    int ret = avcodec_send_frame(enc_ctx, frame_to_send);
    av_frame_free(&hw_frame);
    if (ret < 0 && ret != AVERROR_EOF) return ret;

    while (1) {
        ret = avcodec_receive_packet(enc_ctx, enc_pkt);
        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
            return 0;
        } else if (ret < 0) {
            return ret;
        }
        enc_pkt->stream_index = out_stream->index;
        av_packet_rescale_ts(enc_pkt, enc_ctx->time_base, out_stream->time_base);
        /* Audio and video are encoded on different threads while an AudioWorker runs. */
        BridgeMutex *mux_lock = (BridgeMutex *)out_ctx->opaque;
        if (mux_lock) bridge_mutex_lock(mux_lock);
        ret = av_interleaved_write_frame(out_ctx, enc_pkt);
        if (mux_lock) bridge_mutex_unlock(mux_lock);
        if (ret < 0) return ret;
    }
}

/* Pushes one decoded frame through the filter graph and encodes+writes every frame it
   produces in response (a loudnorm/alimiter chain doesn't emit 1:1 with its input). Pass
   frame=NULL once, at the end, to flush the filter graph itself. */
int filter_encode_write_frame(AVFormatContext *out_ctx, AudioFilterChain *chain,
                              AVCodecContext *enc_ctx, AVStream *out_stream, AVFrame *frame,
                              AVFrame *filt_frame, AVPacket *enc_pkt) {
    int ret = av_buffersrc_add_frame(chain->buffersrc_ctx, frame);
    if (ret < 0) return ret;

    while (1) {
        ret = av_buffersink_get_frame(chain->buffersink_ctx, filt_frame);
        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
            return 0;
        } else if (ret < 0) {
            return ret;
        }
        ret = encode_write_packet(out_ctx, enc_ctx, out_stream, filt_frame, enc_pkt);
        av_frame_unref(filt_frame);
        if (ret < 0) return ret;
    }
}

void free_video_filter_chain(VideoFilterChain *chain) {
    avfilter_graph_free(&chain->graph);
    chain->buffersrc_ctx = NULL;
    chain->buffersink_ctx = NULL;
}

/* Builds `filter_descr` between a "buffer" source shaped like dec_ctx's decoded video and a
   plain "buffersink" — unlike init_audio_filter_chain, no encoder-format negotiation is
   needed here: `filter_descr` itself always ends in an explicit "format=yuv420p" (added by
   the caller), which pins the sink's output format directly instead of constraining the sink
   via options. */
int init_video_filter_chain(AVCodecContext *dec_ctx, const char *filter_descr,
                            VideoFilterChain *chain) {
    char args[512];
    const AVFilter *buffersrc = avfilter_get_by_name("buffer");
    const AVFilter *buffersink = avfilter_get_by_name("buffersink");
    AVFilterInOut *outputs = avfilter_inout_alloc();
    AVFilterInOut *inputs = avfilter_inout_alloc();
    AVRational sar = dec_ctx->sample_aspect_ratio;
    int ret;

    if (sar.num <= 0 || sar.den <= 0) {
        sar = (AVRational){1, 1};
    }

    chain->buffersrc_ctx = NULL;
    chain->buffersink_ctx = NULL;
    chain->graph = avfilter_graph_alloc();
    if (!outputs || !inputs || !chain->graph) {
        ret = AVERROR(ENOMEM);
        goto end;
    }

    /* The source says how its frames are to be read: without the matrix and range, filters that
       look at the link instead of the frame (scale) treat full-range video as limited. */
    snprintf(
        args, sizeof(args),
        "video_size=%dx%d:pix_fmt=%d:time_base=%d/%d:pixel_aspect=%d/%d:colorspace=%d:range=%d",
        dec_ctx->width, dec_ctx->height, dec_ctx->pix_fmt, dec_ctx->pkt_timebase.num,
        dec_ctx->pkt_timebase.den, sar.num, sar.den, (int)dec_ctx->colorspace,
        (int)dec_ctx->color_range);

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

    ret = avfilter_graph_parse_ptr(chain->graph, filter_descr, &inputs, &outputs, NULL);
    if (ret < 0) goto end;

    ret = avfilter_graph_config(chain->graph, NULL);

end:
    avfilter_inout_free(&inputs);
    avfilter_inout_free(&outputs);
    if (ret < 0) {
        free_video_filter_chain(chain);
    }
    return ret;
}

/* Pushes one decoded video frame through the filter graph and encodes+writes every frame it
   produces in response (the canvas-conform stage's `fps` filter doesn't emit 1:1 with its
   input — it duplicates/drops frames to hit canvas_fps). Every emitted frame gets a fresh
   sequential pts in `*next_pts` (in venc_ctx's time_base, i.e. one canvas frame) instead of
   whatever pts the filter graph assigns — same idea as scale_video_frame's, extended across
   segment boundaries so concatenated segments produce continuous timestamps. Pass frame=NULL
   once, at the end, to flush the filter graph itself. */
int filter_encode_write_video_frame(AVFormatContext *out_ctx, VideoFilterChain *chain,
                                    AVCodecContext *enc_ctx, AVStream *out_stream, AVFrame *frame,
                                    AVFrame *filt_frame, int64_t *next_pts, AVPacket *enc_pkt) {
    int ret = av_buffersrc_add_frame(chain->buffersrc_ctx, frame);
    if (ret < 0) return ret;

    while (1) {
        ret = av_buffersink_get_frame(chain->buffersink_ctx, filt_frame);
        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
            return 0;
        } else if (ret < 0) {
            return ret;
        }
        filt_frame->pts = (*next_pts)++;
        ret = encode_video_frame_through(chain->post, chain->worker, out_ctx, enc_ctx, out_stream,
                                         filt_frame, enc_pkt);
        av_frame_unref(filt_frame);
        if (ret < 0) return ret;
    }
}

void free_post_filter_chain(VideoFilterChain *chain) {
    avfilter_graph_free(&chain->graph);
    av_frame_free(&chain->scratch);
    chain->buffersrc_ctx = NULL;
    chain->buffersink_ctx = NULL;
}

int init_post_filter_chain(int width, int height, enum AVPixelFormat pix_fmt, AVRational time_base,
                           const char *filter_descr, VideoFilterChain *chain) {
    char args[256];
    const AVFilter *buffersrc = avfilter_get_by_name("buffer");
    const AVFilter *buffersink = avfilter_get_by_name("buffersink");
    AVFilterInOut *outputs = avfilter_inout_alloc();
    AVFilterInOut *inputs = avfilter_inout_alloc();
    int ret;

    chain->buffersrc_ctx = NULL;
    chain->buffersink_ctx = NULL;
    chain->post = NULL;
    chain->graph = avfilter_graph_alloc();
    chain->scratch = av_frame_alloc();
    if (!outputs || !inputs || !chain->graph || !chain->scratch || !buffersrc || !buffersink) {
        ret = AVERROR(ENOMEM);
        goto end;
    }
    snprintf(args, sizeof(args), "video_size=%dx%d:pix_fmt=%d:time_base=%d/%d:pixel_aspect=1/1",
             width, height, (int)pix_fmt, time_base.num, time_base.den);
    ret = avfilter_graph_create_filter(&chain->buffersrc_ctx, buffersrc, "in", args, NULL,
                                       chain->graph);
    if (ret < 0) goto end;

    /* The sink is held to the encoder's own format (pixel_formats is a pre-init-only option, so
       the sink is allocated and initialised in two steps): an overlay left to negotiate freely
       can hand a packed or RGBA frame to an encoder that wants planar YUV. */
    chain->buffersink_ctx = avfilter_graph_alloc_filter(chain->graph, buffersink, "out");
    if (!chain->buffersink_ctx) {
        ret = AVERROR(ENOMEM);
        goto end;
    }
    ret = av_opt_set_array(chain->buffersink_ctx, "pixel_formats", AV_OPT_SEARCH_CHILDREN, 0, 1,
                           AV_OPT_TYPE_PIXEL_FMT, &pix_fmt);
    if (ret < 0) goto end;
    ret = avfilter_init_str(chain->buffersink_ctx, NULL);
    if (ret < 0) goto end;

    outputs->name = av_strdup("in");
    outputs->filter_ctx = chain->buffersrc_ctx;
    outputs->pad_idx = 0;
    outputs->next = NULL;
    inputs->name = av_strdup("out");
    inputs->filter_ctx = chain->buffersink_ctx;
    inputs->pad_idx = 0;
    inputs->next = NULL;

    ret = avfilter_graph_parse_ptr(chain->graph, filter_descr, &inputs, &outputs, NULL);
    if (ret < 0) goto end;
    ret = avfilter_graph_config(chain->graph, NULL);

end:
    avfilter_inout_free(&inputs);
    avfilter_inout_free(&outputs);
    if (ret < 0) {
        free_post_filter_chain(chain);
    }
    return ret;
}

int encode_video_frame_through(VideoFilterChain *post, VideoEncodeWorker *worker,
                               AVFormatContext *out_ctx, AVCodecContext *enc_ctx,
                               AVStream *out_stream, AVFrame *frame, AVPacket *enc_pkt) {
    if (!post) {
        return worker ? video_worker_push(worker, frame)
                      : encode_write_packet(out_ctx, enc_ctx, out_stream, frame, enc_pkt);
    }
    int ret = av_buffersrc_add_frame_flags(post->buffersrc_ctx, frame, 0);
    if (ret < 0) return ret;
    while (1) {
        ret = av_buffersink_get_frame(post->buffersink_ctx, post->scratch);
        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) return 0;
        if (ret < 0) return ret;
        post->scratch->pict_type = AV_PICTURE_TYPE_NONE;
        ret = encode_video_frame_through(post->post, worker, out_ctx, enc_ctx, out_stream,
                                         post->scratch, enc_pkt);
        av_frame_unref(post->scratch);
        if (ret < 0) return ret;
    }
}

int flush_post_filter_chains(VideoFilterChain *post, VideoEncodeWorker *worker,
                             AVFormatContext *out_ctx, AVCodecContext *enc_ctx,
                             AVStream *out_stream, AVPacket *enc_pkt) {
    for (; post; post = post->post) {
        int ret = av_buffersrc_add_frame_flags(post->buffersrc_ctx, NULL, 0);
        if (ret < 0) return ret;
        while (1) {
            ret = av_buffersink_get_frame(post->buffersink_ctx, post->scratch);
            if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) break;
            if (ret < 0) return ret;
            post->scratch->pict_type = AV_PICTURE_TYPE_NONE;
            ret = encode_video_frame_through(post->post, worker, out_ctx, enc_ctx, out_stream,
                                             post->scratch, enc_pkt);
            av_frame_unref(post->scratch);
            if (ret < 0) return ret;
        }
    }
    return 0;
}
