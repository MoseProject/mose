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
#include <libavfilter/buffersink.h>
#include <libavformat/avformat.h>
#include <libswscale/swscale.h>

/* Scales one decoded frame into `scaled_frame` (already sized/formatted for the encoder,
   allocated once by the caller) and stamps a sequential pts — proxies don't need to preserve
   the source's original timestamps, just monotonically increasing ones for the encoder/muxer. */
static int scale_video_frame(struct SwsContext *sws_ctx, AVFrame *src_frame, AVFrame *scaled_frame,
                             int64_t *next_pts) {
    int ret = av_frame_make_writable(scaled_frame);
    if (ret < 0) return ret;

    ret = sws_scale_frame(sws_ctx, scaled_frame, src_frame);
    if (ret < 0) return ret;

    scaled_frame->pts = (*next_pts)++;
    return 0;
}

ProxyStatus avbridge_generate_proxy(const char *in_path, const char *out_path, int target_height) {
    AVFormatContext *in_ctx = NULL;
    AVFormatContext *out_ctx = NULL;
    AVCodecContext *vdec_ctx = NULL;
    AVCodecContext *venc_ctx = NULL;
    AVCodecContext *adec_ctx = NULL;
    AVCodecContext *aenc_ctx = NULL;
    struct SwsContext *sws_ctx = NULL;
    AudioFilterChain achain = {0};
    AVPacket *pkt = NULL;
    AVFrame *dec_frame = NULL;
    AVFrame *scaled_frame = NULL;
    AVFrame *filt_frame = NULL;
    AVPacket *enc_pkt = NULL;
    ProxyStatus status = PROXY_OK;
    int video_in_index = -1;
    int audio_in_index = -1;
    int video_out_index = -1;
    int audio_out_index = -1;
    int64_t next_video_pts = 0;

    switch (open_input(in_path, &in_ctx)) {
    case -1:
        return PROXY_ERR_OPEN_INPUT;
    case -2:
        return PROXY_ERR_STREAM_INFO;
    }

    for (unsigned int i = 0; i < in_ctx->nb_streams; i++) {
        enum AVMediaType type = in_ctx->streams[i]->codecpar->codec_type;
        if (video_in_index < 0 && type == AVMEDIA_TYPE_VIDEO) {
            video_in_index = (int)i;
        } else if (audio_in_index < 0 && type == AVMEDIA_TYPE_AUDIO) {
            audio_in_index = (int)i;
        }
    }
    if (video_in_index < 0) {
        avformat_close_input(&in_ctx);
        return PROXY_ERR_NO_VIDEO_STREAM;
    }

    avformat_alloc_output_context2(&out_ctx, NULL, NULL, out_path);
    if (!out_ctx) {
        status = PROXY_ERR_ALLOC_OUTPUT;
        goto cleanup;
    }

    /* Video decoder. */
    {
        AVCodecParameters *vpar = in_ctx->streams[video_in_index]->codecpar;
        const AVCodec *decoder = avcodec_find_decoder(vpar->codec_id);
        if (!decoder) {
            status = PROXY_ERR_DECODER;
            goto cleanup;
        }
        vdec_ctx = avcodec_alloc_context3(decoder);
        if (!vdec_ctx || avcodec_parameters_to_context(vdec_ctx, vpar) < 0) {
            status = PROXY_ERR_DECODER;
            goto cleanup;
        }
        vdec_ctx->pkt_timebase = in_ctx->streams[video_in_index]->time_base;
        bridge_enable_decoder_threads(vdec_ctx);
        if (avcodec_open2(vdec_ctx, decoder, NULL) < 0) {
            status = PROXY_ERR_DECODER;
            goto cleanup;
        }
    }

    /* Video encoder (prefer libopenh264, with libx264 support for GPL-enabled dev builds) +
       scaler, dimensions keeping the source's aspect ratio with height fixed
       at target_height, rounded to the nearest even number (odd dimensions are invalid for
       yuv420p — chroma planes are subsampled 2x in both directions). */
    {
        const AVCodec *venc = find_cpu_h264_encoder();
        if (!venc) {
            status = PROXY_ERR_ENCODER;
            goto cleanup;
        }

        int src_w = vdec_ctx->width;
        int src_h = vdec_ctx->height;
        int dst_h = target_height > 0 ? target_height : src_h;
        int dst_w = (int)((int64_t)src_w * dst_h / src_h);
        dst_w += dst_w % 2;
        dst_h += dst_h % 2;

        venc_ctx = avcodec_alloc_context3(venc);
        if (!venc_ctx) {
            status = PROXY_ERR_ENCODER;
            goto cleanup;
        }
        venc_ctx->width = dst_w;
        venc_ctx->height = dst_h;
        venc_ctx->pix_fmt = AV_PIX_FMT_YUV420P;
        AVRational frame_rate = in_ctx->streams[video_in_index]->avg_frame_rate;
        if (frame_rate.num <= 0 || frame_rate.den <= 0) {
            frame_rate = (AVRational){30, 1};
        }
        venc_ctx->time_base = av_inv_q(frame_rate);
        venc_ctx->framerate = frame_rate;
        venc_ctx->gop_size = (frame_rate.num / frame_rate.den) * KEYFRAME_INTERVAL_SECS;
        venc_ctx->max_b_frames = 0; /* keeps proxy decoding lightweight across CPU encoders */
        /* Proxy target: small and fast to decode while scrubbing, not archival quality.
           A pixel-count-derived bitrate keeps output predictable across the supported CPU
           encoders, rather than relying on encoder-specific quality controls.
           knob, sized off the (already downscaled) pixel count so both a 540p and a 360p
           proxy land in a reasonable range rather than one fixed number over- or
           under-shooting depending on target_height. */
        venc_ctx->bit_rate = (int64_t)dst_w * dst_h * 2;
        if (out_ctx->oformat->flags & AVFMT_GLOBALHEADER) {
            venc_ctx->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
        }
        /* The encoder is the other large share; libopenh264 and libx264 both use this many. */
        venc_ctx->thread_count = SOFTWARE_ENCODER_THREADS;
        if (avcodec_open2(venc_ctx, venc, NULL) < 0) {
            status = PROXY_ERR_ENCODER;
            goto cleanup;
        }

        /* Scaling a 4K or 1080p60 frame down is a large share of the work and runs on one
           thread unless asked: `threads=0` uses one per core. */
        sws_ctx = sws_alloc_context();
        if (!sws_ctx) {
            status = PROXY_ERR_SCALER;
            goto cleanup;
        }
        av_opt_set_int(sws_ctx, "srcw", src_w, 0);
        av_opt_set_int(sws_ctx, "srch", src_h, 0);
        av_opt_set_int(sws_ctx, "src_format", vdec_ctx->pix_fmt, 0);
        av_opt_set_int(sws_ctx, "dstw", dst_w, 0);
        av_opt_set_int(sws_ctx, "dsth", dst_h, 0);
        av_opt_set_int(sws_ctx, "dst_format", AV_PIX_FMT_YUV420P, 0);
        av_opt_set_int(sws_ctx, "sws_flags", SWS_BILINEAR, 0);
        av_opt_set_int(sws_ctx, "threads", 0, 0);
        if (sws_init_context(sws_ctx, NULL, NULL) < 0) {
            sws_free_context(&sws_ctx);
            status = PROXY_ERR_SCALER;
            goto cleanup;
        }

        AVStream *out_stream = avformat_new_stream(out_ctx, NULL);
        if (!out_stream || avcodec_parameters_from_context(out_stream->codecpar, venc_ctx) < 0) {
            status = PROXY_ERR_NEW_STREAM;
            goto cleanup;
        }
        out_stream->time_base = venc_ctx->time_base;
        video_out_index = out_stream->index;
    }

    /* Audio decoder + encoder + filterless (format-conversion-only) graph, only if the
       source has an audio stream — a video-only source produces a video-only proxy. */
    if (audio_in_index >= 0) {
        AVCodecParameters *apar = in_ctx->streams[audio_in_index]->codecpar;
        const AVCodec *adecoder = avcodec_find_decoder(apar->codec_id);
        if (!adecoder) {
            status = PROXY_ERR_DECODER;
            goto cleanup;
        }
        adec_ctx = avcodec_alloc_context3(adecoder);
        if (!adec_ctx || avcodec_parameters_to_context(adec_ctx, apar) < 0) {
            status = PROXY_ERR_DECODER;
            goto cleanup;
        }
        adec_ctx->pkt_timebase = in_ctx->streams[audio_in_index]->time_base;
        if (avcodec_open2(adec_ctx, adecoder, NULL) < 0) {
            status = PROXY_ERR_DECODER;
            goto cleanup;
        }

        const AVCodec *aenc = avcodec_find_encoder(AV_CODEC_ID_AAC);
        if (!aenc) {
            status = PROXY_ERR_ENCODER;
            goto cleanup;
        }

        /* "anull": no actual filtering, just reuses init_audio_filter_chain's
           encoder-format-negotiation (sample_formats/samplerates constraint on the sink) so
           avcodec_open2() on the AAC encoder doesn't reject whatever raw format decoding
           happens to produce — see that function's doc comment. */
        if (init_audio_filter_chain(adec_ctx, aenc, "anull", &achain) < 0) {
            status = PROXY_ERR_FILTER_GRAPH;
            goto cleanup;
        }

        aenc_ctx = avcodec_alloc_context3(aenc);
        if (!aenc_ctx) {
            status = PROXY_ERR_ENCODER;
            goto cleanup;
        }
        aenc_ctx->sample_rate = av_buffersink_get_sample_rate(achain.buffersink_ctx);
        av_buffersink_get_ch_layout(achain.buffersink_ctx, &aenc_ctx->ch_layout);
        aenc_ctx->sample_fmt = (enum AVSampleFormat)av_buffersink_get_format(achain.buffersink_ctx);
        aenc_ctx->bit_rate = PROXY_AUDIO_BITRATE_BPS;
        aenc_ctx->time_base = av_buffersink_get_time_base(achain.buffersink_ctx);
        if (out_ctx->oformat->flags & AVFMT_GLOBALHEADER) {
            aenc_ctx->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
        }
        if (avcodec_open2(aenc_ctx, aenc, NULL) < 0) {
            status = PROXY_ERR_ENCODER;
            goto cleanup;
        }
        if (aenc_ctx->frame_size > 0) {
            av_buffersink_set_frame_size(achain.buffersink_ctx, (unsigned)aenc_ctx->frame_size);
        }

        AVStream *out_stream = avformat_new_stream(out_ctx, NULL);
        if (!out_stream || avcodec_parameters_from_context(out_stream->codecpar, aenc_ctx) < 0) {
            status = PROXY_ERR_NEW_STREAM;
            goto cleanup;
        }
        out_stream->time_base = aenc_ctx->time_base;
        audio_out_index = out_stream->index;
    }

    if (!(out_ctx->oformat->flags & AVFMT_NOFILE)) {
        if (avio_open(&out_ctx->pb, out_path, AVIO_FLAG_WRITE) < 0) {
            status = PROXY_ERR_OPEN_OUTPUT;
            goto cleanup_output_io;
        }
    }
    if (avformat_write_header(out_ctx, NULL) < 0) {
        status = PROXY_ERR_WRITE_HEADER;
        goto cleanup_output_io;
    }

    pkt = av_packet_alloc();
    dec_frame = av_frame_alloc();
    scaled_frame = av_frame_alloc();
    filt_frame = av_frame_alloc();
    enc_pkt = av_packet_alloc();
    if (!pkt || !dec_frame || !scaled_frame || !filt_frame || !enc_pkt) {
        status = PROXY_ERR_PIPELINE;
        goto cleanup_output_io;
    }
    scaled_frame->format = venc_ctx->pix_fmt;
    scaled_frame->width = venc_ctx->width;
    scaled_frame->height = venc_ctx->height;
    if (av_frame_get_buffer(scaled_frame, 0) < 0) {
        status = PROXY_ERR_PIPELINE;
        goto cleanup_output_io;
    }

    while (av_read_frame(in_ctx, pkt) >= 0) {
        if (pkt->stream_index == video_in_index) {
            AVStream *out_stream = out_ctx->streams[video_out_index];
            int ret = avcodec_send_packet(vdec_ctx, pkt);
            av_packet_unref(pkt);
            if (ret < 0) {
                status = PROXY_ERR_PIPELINE;
                goto cleanup_output_io;
            }
            while (1) {
                ret = avcodec_receive_frame(vdec_ctx, dec_frame);
                if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) break;
                if (ret < 0) {
                    status = PROXY_ERR_PIPELINE;
                    goto cleanup_output_io;
                }
                if (scale_video_frame(sws_ctx, dec_frame, scaled_frame, &next_video_pts) < 0 ||
                    encode_write_packet(out_ctx, venc_ctx, out_stream, scaled_frame, enc_pkt) < 0) {
                    status = PROXY_ERR_PIPELINE;
                    goto cleanup_output_io;
                }
                av_frame_unref(dec_frame);
            }
        } else if (pkt->stream_index == audio_in_index) {
            AVStream *out_stream = out_ctx->streams[audio_out_index];
            int ret = avcodec_send_packet(adec_ctx, pkt);
            av_packet_unref(pkt);
            if (ret < 0) {
                status = PROXY_ERR_PIPELINE;
                goto cleanup_output_io;
            }
            while (1) {
                ret = avcodec_receive_frame(adec_ctx, dec_frame);
                if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) break;
                if (ret < 0) {
                    status = PROXY_ERR_PIPELINE;
                    goto cleanup_output_io;
                }
                if (filter_encode_write_frame(out_ctx, &achain, aenc_ctx, out_stream, dec_frame,
                                              filt_frame, enc_pkt) < 0) {
                    status = PROXY_ERR_PIPELINE;
                    goto cleanup_output_io;
                }
            }
        } else {
            av_packet_unref(pkt);
        }
    }

    /* Flush video: decoder -> scale+encode remaining buffered frames -> encoder. */
    {
        AVStream *out_stream = out_ctx->streams[video_out_index];
        avcodec_send_packet(vdec_ctx, NULL);
        while (avcodec_receive_frame(vdec_ctx, dec_frame) >= 0) {
            if (scale_video_frame(sws_ctx, dec_frame, scaled_frame, &next_video_pts) < 0 ||
                encode_write_packet(out_ctx, venc_ctx, out_stream, scaled_frame, enc_pkt) < 0) {
                status = PROXY_ERR_PIPELINE;
                goto cleanup_output_io;
            }
            av_frame_unref(dec_frame);
        }
        if (encode_write_packet(out_ctx, venc_ctx, out_stream, NULL, enc_pkt) < 0) {
            status = PROXY_ERR_PIPELINE;
            goto cleanup_output_io;
        }
    }

    /* Flush audio, if present: decoder -> filter -> encoder. */
    if (audio_in_index >= 0) {
        AVStream *out_stream = out_ctx->streams[audio_out_index];
        avcodec_send_packet(adec_ctx, NULL);
        while (avcodec_receive_frame(adec_ctx, dec_frame) >= 0) {
            if (filter_encode_write_frame(out_ctx, &achain, aenc_ctx, out_stream, dec_frame,
                                          filt_frame, enc_pkt) < 0) {
                status = PROXY_ERR_PIPELINE;
                goto cleanup_output_io;
            }
        }
        if (filter_encode_write_frame(out_ctx, &achain, aenc_ctx, out_stream, NULL, filt_frame,
                                      enc_pkt) < 0 ||
            encode_write_packet(out_ctx, aenc_ctx, out_stream, NULL, enc_pkt) < 0) {
            status = PROXY_ERR_PIPELINE;
            goto cleanup_output_io;
        }
    }

    if (status == PROXY_OK) {
        av_write_trailer(out_ctx);
    }

cleanup_output_io:
    av_packet_free(&pkt);
    av_packet_free(&enc_pkt);
    av_frame_free(&dec_frame);
    av_frame_free(&scaled_frame);
    av_frame_free(&filt_frame);
    if (out_ctx && !(out_ctx->oformat->flags & AVFMT_NOFILE)) {
        avio_closep(&out_ctx->pb);
    }
cleanup:
    if (sws_ctx) {
        sws_freeContext(sws_ctx);
    }
    free_audio_filter_chain(&achain);
    avcodec_free_context(&vdec_ctx);
    avcodec_free_context(&venc_ctx);
    avcodec_free_context(&adec_ctx);
    avcodec_free_context(&aenc_ctx);
    if (out_ctx) {
        avformat_free_context(out_ctx);
    }
    avformat_close_input(&in_ctx);
    return status;
}
