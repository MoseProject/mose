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

#include <stdio.h>
#include <string.h>

#include <libavfilter/buffersink.h>
#include <libavfilter/buffersrc.h>
#include <libavutil/mem.h>
#include <libavutil/pixdesc.h>

enum AVColorSpace avbridge_resolve_colorspace_for(enum AVColorSpace tagged, int width, int height) {
    switch (tagged) {
    case AVCOL_SPC_BT709:
    case AVCOL_SPC_BT470BG:
    case AVCOL_SPC_SMPTE170M:
    case AVCOL_SPC_SMPTE240M:
    case AVCOL_SPC_BT2020_NCL:
    case AVCOL_SPC_BT2020_CL:
        return tagged;
    default:
        break;
    }
    return (width >= 1280 || height >= 720) ? AVCOL_SPC_BT709 : AVCOL_SPC_SMPTE170M;
}

enum AVColorSpace avbridge_resolve_colorspace(const AVFrame *frame) {
    return avbridge_resolve_colorspace_for(frame->colorspace, frame->width, frame->height);
}

int avbridge_frame_is_full_range(const AVFrame *frame) {
    if (frame->color_range == AVCOL_RANGE_JPEG) return 1;
    switch (frame->format) {
    case AV_PIX_FMT_YUVJ420P:
    case AV_PIX_FMT_YUVJ422P:
    case AV_PIX_FMT_YUVJ444P:
    case AV_PIX_FMT_YUVJ440P:
        return 1;
    default:
        return 0;
    }
}

int avbridge_frame_is_hdr(const AVFrame *frame) {
    return frame->color_trc == AVCOL_TRC_SMPTE2084 || frame->color_trc == AVCOL_TRC_ARIB_STD_B67;
}

int avbridge_tone_map_graph(AVFilterGraph **graph, AVFilterContext **source, AVFilterContext **sink,
                            int key[7], const AVFrame *frame, int width, int height,
                            const char *output_format) {
    int hlg = frame->color_trc == AVCOL_TRC_ARIB_STD_B67;
    int full = avbridge_frame_is_full_range(frame);
    int wanted[7] = {frame->width, frame->height, frame->format, width, height, hlg, full};
    if (*graph && memcmp(wanted, key, sizeof(wanted)) == 0) return 0;

    avfilter_graph_free(graph);
    *graph = avfilter_graph_alloc();
    if (!*graph) return AVERROR(ENOMEM);
    char args[256];
    snprintf(args, sizeof(args),
             "video_size=%dx%d:pix_fmt=%d:time_base=1/1000:pixel_aspect=1/1:colorspace=%d:range=%s",
             frame->width, frame->height, frame->format, (int)AVCOL_SPC_BT2020_NCL,
             full ? "pc" : "tv");
    char chain[1024];
    snprintf(chain, sizeof(chain),
             "scale=%d:%d:flags=area,"
             "zscale=tin=%s:min=2020_ncl:pin=2020:rin=%s:t=linear:npl=203,"
             "format=gbrpf32le,zscale=p=bt709,"
             "tonemap=mobius:param=0.5:peak=5:desat=2,"
             "zscale=t=bt709:m=bt709:r=tv,format=%s",
             width, height, hlg ? "arib-std-b67" : "smpte2084", full ? "pc" : "tv", output_format);
    AVFilterInOut *outputs = avfilter_inout_alloc();
    AVFilterInOut *inputs = avfilter_inout_alloc();
    int ret = AVERROR(ENOMEM);
    if (outputs && inputs) {
        ret = avfilter_graph_create_filter(source, avfilter_get_by_name("buffer"), "in", args, NULL,
                                           *graph);
        if (ret >= 0) {
            ret = avfilter_graph_create_filter(sink, avfilter_get_by_name("buffersink"), "out",
                                               NULL, NULL, *graph);
        }
        if (ret >= 0) {
            outputs->name = av_strdup("in");
            outputs->filter_ctx = *source;
            outputs->pad_idx = 0;
            outputs->next = NULL;
            inputs->name = av_strdup("out");
            inputs->filter_ctx = *sink;
            inputs->pad_idx = 0;
            inputs->next = NULL;
            ret = avfilter_graph_parse_ptr(*graph, chain, &inputs, &outputs, NULL);
        }
        if (ret >= 0) ret = avfilter_graph_config(*graph, NULL);
    }
    avfilter_inout_free(&inputs);
    avfilter_inout_free(&outputs);
    if (ret < 0) {
        avfilter_graph_free(graph);
        return ret;
    }
    memcpy(key, wanted, sizeof(wanted));
    return 0;
}

int avbridge_tone_map_run(AVFilterContext *source, AVFilterContext *sink, const AVFrame *frame,
                          AVFrame *result) {
    AVFrame *input = av_frame_clone(frame);
    if (!input) return AVERROR(ENOMEM);
    input->pts = 0;
    int ret = av_buffersrc_add_frame_flags(source, input, 0);
    av_frame_free(&input);
    if (ret < 0) return ret;
    return av_buffersink_get_frame(sink, result);
}

void avbridge_fill_missing_color(AVFrame *frame, const AVCodecParameters *stream) {
    if (frame->colorspace == AVCOL_SPC_UNSPECIFIED) frame->colorspace = stream->color_space;
    if (frame->color_primaries == AVCOL_PRI_UNSPECIFIED)
        frame->color_primaries = stream->color_primaries;
    if (frame->color_trc == AVCOL_TRC_UNSPECIFIED) frame->color_trc = stream->color_trc;
    if (frame->color_range == AVCOL_RANGE_UNSPECIFIED) frame->color_range = stream->color_range;
}
