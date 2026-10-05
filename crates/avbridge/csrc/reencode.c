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
#include <libavutil/opt.h>

/* The re-encode pass the text, shape and privacy blur overlays share: decode the exported file,
   run one filter graph over its video, encode the result (hardware when it opens), and copy the
   audio through untouched. Those three used to carry this scaffolding three times; each now
   builds only its own filter description. */

ReencodeStatus avbridge_reencode_with_filter(const char *in_path, const char *out_path,
                                             const char *filter_desc, int canvas_width,
                                             int canvas_height, int canvas_fps_num,
                                             int canvas_fps_den,
                                             int bound_output_to_input_frames) {
    ReencodeStatus status = REENCODE_OK;
    AVFormatContext *in_ctx = NULL;
    AVFormatContext *out_ctx = NULL;
    AVCodecContext *vdec_ctx = NULL;
    AVCodecContext *venc_ctx = NULL;
    AVFilterGraph *filter_graph = NULL;
    AVFilterContext *buffersrc_ctx = NULL;
    AVFilterContext *buffersink_ctx = NULL;
    AVPacket *pkt = NULL;
    AVFrame *dec_frame = NULL;
    AVFrame *filt_frame = NULL;
    AVPacket *enc_pkt = NULL;
    int video_in_idx = -1, audio_in_idx = -1;
    int video_out_idx = -1, audio_out_idx = -1;
    int64_t next_video_pts = 0;
    int64_t frames_fed = 0; /* frames pushed into the filtergraph */
    AVRational canvas_fps = {canvas_fps_num, canvas_fps_den};

        /* Open input */
    if (open_input(in_path, &in_ctx) != 0) {
        status = REENCODE_ERR_OPEN_INPUT;
        goto cleanup;
    }
    for (unsigned int i = 0; i < in_ctx->nb_streams; i++) {
        enum AVMediaType mt = in_ctx->streams[i]->codecpar->codec_type;
        if (video_in_idx < 0 && mt == AVMEDIA_TYPE_VIDEO) video_in_idx = (int)i;
        if (audio_in_idx < 0 && mt == AVMEDIA_TYPE_AUDIO) audio_in_idx = (int)i;
    }
    if (video_in_idx < 0) {
        status = REENCODE_ERR_OPEN_INPUT;
        goto cleanup;
    }

    /* Open video decoder */
    {
        AVStream *vs = in_ctx->streams[video_in_idx];
        const AVCodec *vdec = avcodec_find_decoder(vs->codecpar->codec_id);
        if (!vdec) {
            status = REENCODE_ERR_OPEN_INPUT;
            goto cleanup;
        }
        vdec_ctx = avcodec_alloc_context3(vdec);
        if (!vdec_ctx) {
            status = REENCODE_ERR_OPEN_INPUT;
            goto cleanup;
        }
        avcodec_parameters_to_context(vdec_ctx, vs->codecpar);
        vdec_ctx->time_base = vs->time_base;
        bridge_enable_decoder_threads(vdec_ctx);
        if (avcodec_open2(vdec_ctx, vdec, NULL) < 0) {
            status = REENCODE_ERR_OPEN_INPUT;
            goto cleanup;
        }
    }

    /* Set up filtergraph: buffer + PNG movie sources -> overlay chain -> buffersink. */
    {
        AVStream *vs = in_ctx->streams[video_in_idx];
        filter_graph = avfilter_graph_alloc();
        if (!filter_graph) {
            status = REENCODE_ERR_FILTER_GRAPH;
            goto cleanup;
        }

        const AVFilter *buffersrc = avfilter_get_by_name("buffer");
        const AVFilter *buffersink = avfilter_get_by_name("buffersink");
        if (!buffersrc || !buffersink) {
            status = REENCODE_ERR_FILTER_GRAPH;
            goto cleanup;
        }

        char args[256];
        snprintf(args, sizeof(args),
                 "video_size=%dx%d:pix_fmt=%d:time_base=%d/%d:pixel_aspect=%d/%d", vdec_ctx->width,
                 vdec_ctx->height, (int)vdec_ctx->pix_fmt, vs->time_base.num, vs->time_base.den,
                 vdec_ctx->sample_aspect_ratio.num,
                 vdec_ctx->sample_aspect_ratio.den > 0 ? vdec_ctx->sample_aspect_ratio.den : 1);

        if (avfilter_graph_create_filter(&buffersrc_ctx, buffersrc, "in", args, NULL,
                                         filter_graph) < 0) {
            status = REENCODE_ERR_FILTER_GRAPH;
            goto cleanup;
        }

        /* overlay=format=auto may otherwise negotiate a packed/RGBA output when the PNG input
           has no opacity-expression geq stage. FFmpeg's libopenh264 wrapper forwards that
           frame as I420, leaving chroma planes/strides invalid and causing OpenH264's
           BuildSpatialPicList to reject the first frame. Constrain the sink to the same
           planar format configured on venc_ctx so the graph inserts conversion as needed.
           pixel_formats is a pre-init-only option, so allocate and initialize the sink
           separately. */
        buffersink_ctx = avfilter_graph_alloc_filter(filter_graph, buffersink, "out");
        if (!buffersink_ctx) {
            status = REENCODE_ERR_FILTER_GRAPH;
            goto cleanup;
        }
        enum AVPixelFormat sink_pix_fmt = AV_PIX_FMT_YUV420P;
        if (av_opt_set_array(buffersink_ctx, "pixel_formats", AV_OPT_SEARCH_CHILDREN, 0, 1,
                             AV_OPT_TYPE_PIXEL_FMT, &sink_pix_fmt) < 0 ||
            avfilter_init_str(buffersink_ctx, NULL) < 0) {
            status = REENCODE_ERR_FILTER_GRAPH;
            goto cleanup;
        }

        AVFilterInOut *filt_out = avfilter_inout_alloc();
        AVFilterInOut *filt_in = avfilter_inout_alloc();
        if (!filt_out || !filt_in) {
            avfilter_inout_free(&filt_out);
            avfilter_inout_free(&filt_in);
            status = REENCODE_ERR_FILTER_GRAPH;
            goto cleanup;
        }
        filt_out->name = av_strdup("in");
        filt_out->filter_ctx = buffersrc_ctx;
        filt_out->pad_idx = 0;
        filt_out->next = NULL;
        filt_in->name = av_strdup("out");
        filt_in->filter_ctx = buffersink_ctx;
        filt_in->pad_idx = 0;
        filt_in->next = NULL;

        int ret = avfilter_graph_parse_ptr(filter_graph, filter_desc, &filt_in, &filt_out, NULL);
        avfilter_inout_free(&filt_in);
        avfilter_inout_free(&filt_out);
        if (ret < 0 || avfilter_graph_config(filter_graph, NULL) < 0) {
            status = REENCODE_ERR_FILTER_GRAPH;
            goto cleanup;
        }
    }

    /* Set up output context: libopenh264 for video, stream-copy for audio */
    {
        avformat_alloc_output_context2(&out_ctx, NULL, NULL, out_path);
        if (!out_ctx) {
            status = REENCODE_ERR_ALLOC_OUTPUT;
            goto cleanup;
        }

        AVStream *vin = in_ctx->streams[video_in_idx];
        /* Hardware encoder when one opens (this pass re-encodes the whole file), else CPU. */
        venc_ctx = open_overlay_pass_encoder(
            canvas_width, canvas_height, canvas_fps,
            vin->codecpar->bit_rate > 0 ? vin->codecpar->bit_rate : 4000000LL,
            (out_ctx->oformat->flags & AVFMT_GLOBALHEADER) != 0);
        if (!venc_ctx) {
            status = REENCODE_ERR_PIPELINE;
            goto cleanup;
        }
        AVStream *vout = avformat_new_stream(out_ctx, NULL);
        if (!vout || avcodec_parameters_from_context(vout->codecpar, venc_ctx) < 0) {
            status = REENCODE_ERR_ALLOC_OUTPUT;
            goto cleanup;
        }
        vout->time_base = venc_ctx->time_base;
        video_out_idx = vout->index;

        if (audio_in_idx >= 0) {
            AVStream *ain = in_ctx->streams[audio_in_idx];
            AVStream *aout = avformat_new_stream(out_ctx, NULL);
            if (!aout || avcodec_parameters_copy(aout->codecpar, ain->codecpar) < 0) {
                status = REENCODE_ERR_ALLOC_OUTPUT;
                goto cleanup;
            }
            aout->time_base = ain->time_base;
            audio_out_idx = aout->index;
        }

        if (!(out_ctx->oformat->flags & AVFMT_NOFILE)) {
            if (avio_open(&out_ctx->pb, out_path, AVIO_FLAG_WRITE) < 0) {
                status = REENCODE_ERR_ALLOC_OUTPUT;
                goto cleanup;
            }
        }
        if (avformat_write_header(out_ctx, NULL) < 0) {
            status = REENCODE_ERR_ALLOC_OUTPUT;
            goto cleanup_output_io;
        }
    }

    pkt = av_packet_alloc();
    dec_frame = av_frame_alloc();
    filt_frame = av_frame_alloc();
    enc_pkt = av_packet_alloc();
    if (!pkt || !dec_frame || !filt_frame || !enc_pkt) {
        status = REENCODE_ERR_PIPELINE;
        goto cleanup_output_io;
    }

    /* Main decode -> filter -> encode loop */
    while (av_read_frame(in_ctx, pkt) >= 0) {
        if (pkt->stream_index == video_in_idx) {
            int ret = avcodec_send_packet(vdec_ctx, pkt);
            av_packet_unref(pkt);
            if (ret < 0) {
                status = REENCODE_ERR_PIPELINE;
                goto cleanup_output_io;
            }
            while (1) {
                ret = avcodec_receive_frame(vdec_ctx, dec_frame);
                if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) break;
                if (ret < 0) {
                    status = REENCODE_ERR_PIPELINE;
                    goto cleanup_output_io;
                }
                dec_frame->pts = dec_frame->best_effort_timestamp;
                ret = av_buffersrc_add_frame_flags(buffersrc_ctx, dec_frame,
                                                   AV_BUFFERSRC_FLAG_KEEP_REF);
                av_frame_unref(dec_frame);
                if (ret < 0) {
                    status = REENCODE_ERR_PIPELINE;
                    goto cleanup_output_io;
                }
                frames_fed++;
                while (1) {
                    ret = av_buffersink_get_frame(buffersink_ctx, filt_frame);
                    if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) break;
                    if (ret < 0) {
                        av_frame_unref(filt_frame);
                        status = REENCODE_ERR_PIPELINE;
                        goto cleanup_output_io;
                    }
                    filt_frame->pts = next_video_pts++;
                    filt_frame->pict_type = AV_PICTURE_TYPE_NONE;
                    if (encode_write_packet(out_ctx, venc_ctx, out_ctx->streams[video_out_idx],
                                            filt_frame, enc_pkt) < 0) {
                        av_frame_unref(filt_frame);
                        status = REENCODE_ERR_PIPELINE;
                        goto cleanup_output_io;
                    }
                    av_frame_unref(filt_frame);
                }
            }
        } else if (pkt->stream_index == audio_in_idx && audio_out_idx >= 0) {
            /* Audio: rescale timestamps and stream-copy to avoid a second lossy AAC encode */
            AVStream *ain = in_ctx->streams[audio_in_idx];
            AVStream *aout = out_ctx->streams[audio_out_idx];
            av_packet_rescale_ts(pkt, ain->time_base, aout->time_base);
            pkt->stream_index = audio_out_idx;
            if (av_interleaved_write_frame(out_ctx, pkt) < 0) {
                av_packet_unref(pkt);
                status = REENCODE_ERR_PIPELINE;
                goto cleanup_output_io;
            }
        } else {
            av_packet_unref(pkt);
        }
    }

    /* Flush decoder into filtergraph */
    avcodec_send_packet(vdec_ctx, NULL);
    while (avcodec_receive_frame(vdec_ctx, dec_frame) >= 0) {
        dec_frame->pts = dec_frame->best_effort_timestamp;
        if (av_buffersrc_add_frame_flags(buffersrc_ctx, dec_frame, AV_BUFFERSRC_FLAG_KEEP_REF) >=
            0) {
            frames_fed++;
            while (av_buffersink_get_frame(buffersink_ctx, filt_frame) >= 0) {
                filt_frame->pts = next_video_pts++;
                filt_frame->pict_type = AV_PICTURE_TYPE_NONE;
                encode_write_packet(out_ctx, venc_ctx, out_ctx->streams[video_out_idx], filt_frame,
                                    enc_pkt);
                av_frame_unref(filt_frame);
            }
        }
        av_frame_unref(dec_frame);
    }
    /* Flush filtergraph */
    if (av_buffersrc_add_frame_flags(buffersrc_ctx, NULL, 0) < 0) {
        status = REENCODE_ERR_PIPELINE;
    }
    /* A looping source (the privacy matte, `loop=0`) never reports EOF on its own: such a pass
       stops once one output frame exists per input frame. */
    while ((!bound_output_to_input_frames || next_video_pts < frames_fed) &&
           av_buffersink_get_frame(buffersink_ctx, filt_frame) >= 0) {
        filt_frame->pts = next_video_pts++;
        filt_frame->pict_type = AV_PICTURE_TYPE_NONE;
        encode_write_packet(out_ctx, venc_ctx, out_ctx->streams[video_out_idx], filt_frame,
                            enc_pkt);
        av_frame_unref(filt_frame);
    }
    /* Flush encoder */
    encode_write_packet(out_ctx, venc_ctx, out_ctx->streams[video_out_idx], NULL, enc_pkt);

    if (status == REENCODE_OK) av_write_trailer(out_ctx);

cleanup_output_io:
    av_packet_free(&pkt);
    av_packet_free(&enc_pkt);
    av_frame_free(&dec_frame);
    av_frame_free(&filt_frame);
    if (out_ctx && !(out_ctx->oformat->flags & AVFMT_NOFILE)) avio_closep(&out_ctx->pb);
cleanup:
    avfilter_graph_free(&filter_graph);
    avcodec_free_context(&vdec_ctx);
    avcodec_free_context(&venc_ctx);
    if (out_ctx) avformat_free_context(out_ctx);
    avformat_close_input(&in_ctx);
    return status;
}
