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

/* CF-09 slice 4 ("Support privacy blur as the first end-to-end effect before general selective
   effects", spec/architecture/competitive-feature-plan.md) — structurally identical to
   avbridge_apply_text_overlays/_shape_overlays (see those functions' comments for the full
   decode/filter/encode pipeline shape); only the filter-string-building step differs, and unlike
   those two this graph has two sources (the main video via buffersrc, the matte video via a
   `movie=` filter source) feeding one `maskedmerge` node instead of chaining N overlay/geq
   segments onto one source. */

/* Escapes `in` for use as a single-quoted avfilter movie filename — identical in behavior to
   text_overlay.c's own static helper of the same name (each csrc file that needs this keeps its
   own copy rather than sharing one across translation units, matching this codebase's existing
   convention: see shape_overlay.c's own escape-free but otherwise self-contained structure).
   Writes into `out` (size `out_size`); returns bytes written excluding NUL, or -1 on overflow. */
static int escape_filter_path(char *out, size_t out_size, const char *in) {
    size_t pos = 0;
    for (size_t i = 0; in[i] != '\0'; i++) {
        char c = in[i];
        int needs_escape =
            (c == '\\' || c == '\'' || c == ':' || c == ',' || c == ';' || c == '[' || c == ']');
        if (pos + (size_t)(needs_escape ? 2 : 1) + 1 > out_size) return -1;
        if (needs_escape) out[pos++] = '\\';
        out[pos++] = c;
    }
    out[pos] = '\0';
    return (int)pos;
}

PrivacyBlurStatus avbridge_apply_privacy_blur(const char *in_path, const char *out_path,
                                              const char *matte_path, double blur_sigma,
                                              int canvas_width, int canvas_height,
                                              int canvas_fps_num, int canvas_fps_den) {

    /* No meaningful "zero blur" — a caller wanting the frame untouched should skip calling this
       function entirely, same convention avbridge_apply_text_overlays/_shape_overlays use for
       segment_count <= 0. gblur itself also rejects a non-positive sigma at graph-config time,
       but checking here gives a specific, actionable status instead of a generic filter-graph
       failure. */
    if (blur_sigma <= 0.0) return PRIVACY_BLUR_ERR_FILTER_GRAPH;

    PrivacyBlurStatus status = PRIVACY_BLUR_OK;
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
    char *filter_str = NULL;
    int video_in_idx = -1, audio_in_idx = -1;
    int video_out_idx = -1, audio_out_idx = -1;
    int64_t next_video_pts = 0;
    int64_t frames_fed = 0; /* frames pushed into the filtergraph */
    AVRational canvas_fps = {canvas_fps_num, canvas_fps_den};

    if (open_input(in_path, &in_ctx) != 0) {
        status = PRIVACY_BLUR_ERR_OPEN_INPUT;
        goto cleanup;
    }
    for (unsigned int i = 0; i < in_ctx->nb_streams; i++) {
        enum AVMediaType mt = in_ctx->streams[i]->codecpar->codec_type;
        if (video_in_idx < 0 && mt == AVMEDIA_TYPE_VIDEO) video_in_idx = (int)i;
        if (audio_in_idx < 0 && mt == AVMEDIA_TYPE_AUDIO) audio_in_idx = (int)i;
    }
    if (video_in_idx < 0) {
        status = PRIVACY_BLUR_ERR_OPEN_INPUT;
        goto cleanup;
    }

    {
        AVStream *vs = in_ctx->streams[video_in_idx];
        const AVCodec *vdec = avcodec_find_decoder(vs->codecpar->codec_id);
        if (!vdec) {
            status = PRIVACY_BLUR_ERR_OPEN_INPUT;
            goto cleanup;
        }
        vdec_ctx = avcodec_alloc_context3(vdec);
        if (!vdec_ctx) {
            status = PRIVACY_BLUR_ERR_OPEN_INPUT;
            goto cleanup;
        }
        avcodec_parameters_to_context(vdec_ctx, vs->codecpar);
        vdec_ctx->time_base = vs->time_base;
        bridge_enable_decoder_threads(vdec_ctx);
        if (avcodec_open2(vdec_ctx, vdec, NULL) < 0) {
            status = PRIVACY_BLUR_ERR_OPEN_INPUT;
            goto cleanup;
        }
    }

    /* Builds:
         movie=filename='<matte>':loop=0,format=yuv420p,scale=W:H[mask];
         [in]format=yuv420p,split=2[base][to_blur];
         [to_blur]gblur=sigma=<sigma>[blurred];
         [base][blurred][mask]maskedmerge[out]

       `format=yuv420p` on both the main branch and the matte movie source keeps every input to
       maskedmerge in the same planar format (maskedmerge's own docs require base/overlay/mask to
       share pixel format) — the matte is already produced as yuv420p by
       avbridge_encode_matte_video, but a caller-supplied matte of a different format is
       normalized here rather than trusted. `scale=W:H` on the matte guards against a matte
       encoded at a different resolution than the main video (e.g. built from downscaled sample
       frames) silently failing maskedmerge's own equal-dimensions requirement. `loop=0` repeats
       the matte's last frame if it runs shorter than the main video, rather than the movie
       source hitting EOF and stalling the graph early — a caller building the matte from fewer
       sample points than the export's own frame count (a coarser sampling cadence, not every
       frame) is expected and shouldn't truncate the output. */
    {
        char escaped_matte[1024];
        if (escape_filter_path(escaped_matte, sizeof(escaped_matte), matte_path) < 0) {
            status = PRIVACY_BLUR_ERR_FILTER_GRAPH;
            goto cleanup;
        }
        size_t filter_buf_size = sizeof(escaped_matte) + 512;
        filter_str = av_malloc(filter_buf_size);
        if (!filter_str) {
            status = PRIVACY_BLUR_ERR_FILTER_GRAPH;
            goto cleanup;
        }
        int written = snprintf(filter_str, filter_buf_size,
                               "movie=filename='%s':loop=0,format=yuv420p,scale=%d:%d[mask];"
                               "[in]format=yuv420p,split=2[base][to_blur];"
                               "[to_blur]gblur=sigma=%f[blurred];"
                               "[base][blurred][mask]maskedmerge[out]",
                               escaped_matte, canvas_width, canvas_height, blur_sigma);
        if (written < 0 || (size_t)written >= filter_buf_size) {
            status = PRIVACY_BLUR_ERR_FILTER_GRAPH;
            goto cleanup;
        }
    }

    {
        AVStream *vs = in_ctx->streams[video_in_idx];
        filter_graph = avfilter_graph_alloc();
        if (!filter_graph) {
            status = PRIVACY_BLUR_ERR_FILTER_GRAPH;
            goto cleanup;
        }

        const AVFilter *buffersrc = avfilter_get_by_name("buffer");
        const AVFilter *buffersink = avfilter_get_by_name("buffersink");
        if (!buffersrc || !buffersink) {
            status = PRIVACY_BLUR_ERR_FILTER_GRAPH;
            goto cleanup;
        }

        char args[256];
        snprintf(args, sizeof(args),
                 "video_size=%dx%d:pix_fmt=%d:time_base=%d/%d:pixel_aspect=%d/%d", vdec_ctx->width,
                 vdec_ctx->height, (int)vdec_ctx->pix_fmt, vs->time_base.num, vs->time_base.den,
                 vdec_ctx->sample_aspect_ratio.num,
                 vdec_ctx->sample_aspect_ratio.den > 0 ? vdec_ctx->sample_aspect_ratio.den : 1);

        if (avfilter_graph_create_filter(&buffersrc_ctx, buffersrc, "in", args, NULL,
                                         filter_graph) < 0 ||
            avfilter_graph_create_filter(&buffersink_ctx, buffersink, "out", NULL, NULL,
                                         filter_graph) < 0) {
            status = PRIVACY_BLUR_ERR_FILTER_GRAPH;
            goto cleanup;
        }

        AVFilterInOut *filt_out = avfilter_inout_alloc();
        AVFilterInOut *filt_in = avfilter_inout_alloc();
        if (!filt_out || !filt_in) {
            avfilter_inout_free(&filt_out);
            avfilter_inout_free(&filt_in);
            status = PRIVACY_BLUR_ERR_FILTER_GRAPH;
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

        int ret = avfilter_graph_parse_ptr(filter_graph, filter_str, &filt_in, &filt_out, NULL);
        avfilter_inout_free(&filt_in);
        avfilter_inout_free(&filt_out);
        if (ret < 0 || avfilter_graph_config(filter_graph, NULL) < 0) {
            status = PRIVACY_BLUR_ERR_FILTER_GRAPH;
            goto cleanup;
        }
    }

    {
        avformat_alloc_output_context2(&out_ctx, NULL, NULL, out_path);
        if (!out_ctx) {
            status = PRIVACY_BLUR_ERR_ALLOC_OUTPUT;
            goto cleanup;
        }

        AVStream *vin = in_ctx->streams[video_in_idx];
        /* Hardware encoder when one opens (this pass re-encodes the whole file), else CPU. */
        venc_ctx = open_overlay_pass_encoder(
            canvas_width, canvas_height, canvas_fps,
            vin->codecpar->bit_rate > 0 ? vin->codecpar->bit_rate : 4000000LL,
            (out_ctx->oformat->flags & AVFMT_GLOBALHEADER) != 0);
        if (!venc_ctx) {
            status = PRIVACY_BLUR_ERR_PIPELINE;
            goto cleanup;
        }
        AVStream *vout = avformat_new_stream(out_ctx, NULL);
        if (!vout || avcodec_parameters_from_context(vout->codecpar, venc_ctx) < 0) {
            status = PRIVACY_BLUR_ERR_ALLOC_OUTPUT;
            goto cleanup;
        }
        vout->time_base = venc_ctx->time_base;
        video_out_idx = vout->index;

        if (audio_in_idx >= 0) {
            AVStream *ain = in_ctx->streams[audio_in_idx];
            AVStream *aout = avformat_new_stream(out_ctx, NULL);
            if (!aout || avcodec_parameters_copy(aout->codecpar, ain->codecpar) < 0) {
                status = PRIVACY_BLUR_ERR_ALLOC_OUTPUT;
                goto cleanup;
            }
            aout->time_base = ain->time_base;
            audio_out_idx = aout->index;
        }

        if (!(out_ctx->oformat->flags & AVFMT_NOFILE)) {
            if (avio_open(&out_ctx->pb, out_path, AVIO_FLAG_WRITE) < 0) {
                status = PRIVACY_BLUR_ERR_ALLOC_OUTPUT;
                goto cleanup;
            }
        }
        if (avformat_write_header(out_ctx, NULL) < 0) {
            status = PRIVACY_BLUR_ERR_ALLOC_OUTPUT;
            goto cleanup_output_io;
        }
    }

    pkt = av_packet_alloc();
    dec_frame = av_frame_alloc();
    filt_frame = av_frame_alloc();
    enc_pkt = av_packet_alloc();
    if (!pkt || !dec_frame || !filt_frame || !enc_pkt) {
        status = PRIVACY_BLUR_ERR_PIPELINE;
        goto cleanup_output_io;
    }

    while (av_read_frame(in_ctx, pkt) >= 0) {
        if (pkt->stream_index == video_in_idx) {
            int ret = avcodec_send_packet(vdec_ctx, pkt);
            av_packet_unref(pkt);
            if (ret < 0) {
                status = PRIVACY_BLUR_ERR_PIPELINE;
                goto cleanup_output_io;
            }
            while (1) {
                ret = avcodec_receive_frame(vdec_ctx, dec_frame);
                if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) break;
                if (ret < 0) {
                    status = PRIVACY_BLUR_ERR_PIPELINE;
                    goto cleanup_output_io;
                }
                dec_frame->pts = dec_frame->best_effort_timestamp;
                ret = av_buffersrc_add_frame_flags(buffersrc_ctx, dec_frame,
                                                   AV_BUFFERSRC_FLAG_KEEP_REF);
                av_frame_unref(dec_frame);
                if (ret < 0) {
                    status = PRIVACY_BLUR_ERR_PIPELINE;
                    goto cleanup_output_io;
                }
                frames_fed++;
                while (1) {
                    ret = av_buffersink_get_frame(buffersink_ctx, filt_frame);
                    if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) break;
                    if (ret < 0) {
                        av_frame_unref(filt_frame);
                        status = PRIVACY_BLUR_ERR_PIPELINE;
                        goto cleanup_output_io;
                    }
                    filt_frame->pts = next_video_pts++;
                    filt_frame->pict_type = AV_PICTURE_TYPE_NONE;
                    if (encode_write_packet(out_ctx, venc_ctx, out_ctx->streams[video_out_idx],
                                            filt_frame, enc_pkt) < 0) {
                        av_frame_unref(filt_frame);
                        status = PRIVACY_BLUR_ERR_PIPELINE;
                        goto cleanup_output_io;
                    }
                    av_frame_unref(filt_frame);
                }
            }
        } else if (pkt->stream_index == audio_in_idx && audio_out_idx >= 0) {
            /* Audio: rescale timestamps and stream-copy to avoid a second lossy AAC encode. */
            AVStream *ain = in_ctx->streams[audio_in_idx];
            AVStream *aout = out_ctx->streams[audio_out_idx];
            av_packet_rescale_ts(pkt, ain->time_base, aout->time_base);
            pkt->stream_index = audio_out_idx;
            if (av_interleaved_write_frame(out_ctx, pkt) < 0) {
                av_packet_unref(pkt);
                status = PRIVACY_BLUR_ERR_PIPELINE;
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
        status = PRIVACY_BLUR_ERR_PIPELINE;
    }
    /* The matte movie loops forever (`loop=0`), so the graph never reports EOF on its own: stop
       once one output frame exists per input frame. */
    while (next_video_pts < frames_fed && av_buffersink_get_frame(buffersink_ctx, filt_frame) >= 0) {
        filt_frame->pts = next_video_pts++;
        filt_frame->pict_type = AV_PICTURE_TYPE_NONE;
        encode_write_packet(out_ctx, venc_ctx, out_ctx->streams[video_out_idx], filt_frame,
                            enc_pkt);
        av_frame_unref(filt_frame);
    }
    /* Flush encoder */
    encode_write_packet(out_ctx, venc_ctx, out_ctx->streams[video_out_idx], NULL, enc_pkt);

    if (status == PRIVACY_BLUR_OK) av_write_trailer(out_ctx);

cleanup_output_io:
    av_packet_free(&pkt);
    av_packet_free(&enc_pkt);
    av_frame_free(&dec_frame);
    av_frame_free(&filt_frame);
    if (out_ctx && !(out_ctx->oformat->flags & AVFMT_NOFILE)) avio_closep(&out_ctx->pb);
cleanup:
    av_free(filter_str);
    avfilter_graph_free(&filter_graph);
    avcodec_free_context(&vdec_ctx);
    avcodec_free_context(&venc_ctx);
    if (out_ctx) avformat_free_context(out_ctx);
    avformat_close_input(&in_ctx);
    return status;
}
