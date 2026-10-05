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

#include <math.h>

#include <libavcodec/avcodec.h>
#include <libavfilter/avfilter.h>
#include <libavfilter/buffersink.h>
#include <libavfilter/buffersrc.h>
#include <libavformat/avformat.h>
#include <libavutil/bprint.h>
#include <libavutil/channel_layout.h>
#include <libavutil/mem.h>
#include <libavutil/pixdesc.h>

/* Timeline-elapsed time after `source_elapsed_secs` of `seg`'s own source time have played —
   the C-side twin of core::keyframe::smooth_speed_ramp_duration_secs, needed anywhere this file
   converts a decoded frame's source-relative position into a timeline position for `seg` (e.g.
   deciding which overlay track is active "right now"). Falls back to the plain
   `source_elapsed_secs / speed_factor` a constant-speed segment already used before this field
   existed. */
static double smooth_speed_ramp_timeline_elapsed(const ClipSegment *seg,
                                                 double source_elapsed_secs) {
    double v0 = (double)seg->speed_factor;
    double v1 = (double)seg->smooth_speed_ramp_end_factor;
    double d = seg->source_out_secs - seg->source_in_secs;
    if (seg->smooth_speed_ramp_end_factor > 0.0f && fabs(v1 - v0) > 1e-4 && v0 > 0.0 && d > 1e-9) {
        double t = source_elapsed_secs;
        if (t < 0.0) t = 0.0;
        if (t > d) t = d;
        double speed_at_t = v0 + (v1 - v0) * t / d;
        return (d / (v1 - v0)) * log(speed_at_t / v0);
    }
    double spd = v0 > 0.0 ? v0 : 1.0;
    return source_elapsed_secs / spd;
}

/* =========================================================================
   avbridge_encode_timeline_export_multi — multi-track overlay compositor.

   Track 0 drives the output: its segments play in order (same as the single-track
   function), providing both audio and the background video.  Tracks 1..n_tracks-1
   are overlaid on top wherever their clips' [timeline_start_secs, timeline_end)
   windows overlap with the current track-0 frame's timeline position.

   When a frame from track 0 is at timeline time T:
   - Find the active segment on every higher track, opening/seeking each decoder once per
     segment boundary.
   - Push the background plus every active layer through one dynamically-built overlay chain,
     in ascending track order, then pull the composited result and encode it.
   - Otherwise: fall back to a plain VideoFilterChain for track 0 (same as the
     existing single-track function).

   This low-level compositor's initial audio comes from track 0 only.  Core's full timeline
   export replaces it afterward with avbridge_mix_audio_timeline + avbridge_mux_video_audio
   whenever another video/audio track contributes sound.
   ========================================================================= */

/* Build the per-track filter string for use INSIDE an overlay graph — same as
   build_segment_vfilter but omits the trailing format=yuv420p (added after the
   overlay stage) and omits transitions (n counter semantics differ in multi-input
   graphs). Returns 0 on success, -1 on truncation -- see build_segment_vfilter's doc
   comment for why this is checked rather than silently accepted. */
static int build_overlay_vfilter(const ClipSegment *seg, int cw, int ch, int fps_num, int fps_den,
                                 char *buf, size_t cap) {
    const char *cf = (seg->video_filter && seg->video_filter[0]) ? seg->video_filter : "";
    char setpts[192];
    build_setpts_str(seg, seg->source_out_secs - seg->source_in_secs, setpts, sizeof(setpts));

    char post[4096] = "";
    if (cf[0]) {
        int post_written = snprintf(post, sizeof(post), ",%s", cf);
        if (post_written < 0 || (size_t)post_written >= sizeof(post)) return -1;
    }

    /* pad's x/y offsets place this segment's own anchor point (a fraction of its decoded
       content, not the padded canvas buffer) at the padded buffer's center (ow/2, oh/2) -- at
       anchor 0.5/0.5 this reduces to the original "(ow-iw)/2" centering exactly (verified
       against real ffmpeg output, byte-identical), so a segment with the default anchor
       renders identically to before this field existed. Any rotate=... stage later in cf
       (from scale/rotation keyframes) then pivots around this same buffer's own center -- i.e.
       around whatever point anchor_x/anchor_y designated -- since rotate always rotates around
       its own input frame's center. */
    char pad_x[96], pad_y[96];
    snprintf(pad_x, sizeof(pad_x), "ow/2-(%g)*iw", seg->anchor_x);
    snprintf(pad_y, sizeof(pad_y), "oh/2-(%g)*ih", seg->anchor_y);

    int buf_written = snprintf(buf, cap,
                               "%sscale=%d:%d:force_original_aspect_ratio=decrease,pad=%d:%d:%s:%s,"
                               "fps=%d/%d%s",
                               setpts, cw, ch, cw, ch, pad_x, pad_y, fps_num, fps_den, post);
    if (buf_written < 0 || (size_t)buf_written >= cap) return -1;
    return 0;
}

typedef struct {
    AVFormatContext *in_ctx;
    AVCodecContext *vdec_ctx;
    int video_in_index;
    int seg_open;     /* index of the segment this decoder was opened for, or -1 */
    int done;         /* av_read_frame hit EOF / source_out_secs reached */
    AVFrame *pending; /* last decoded frame still within range, or NULL */
} OverlayDecoder;

typedef struct {
    OverlayDecoder video;
    OverlayDecoder matte;
    const ClipSegment *segment;
    ClipSegment matte_segment;
    int active_segment_index;
    int matte_active;
    AVPacket *video_packet;
    AVPacket *matte_packet;
    AVFrame *video_frame;
    AVFrame *matte_frame;
    AVFilterContext *video_source;
    AVFilterContext *matte_source;
} OverlayTrackState;

static void free_overlay_decoder(OverlayDecoder *d) {
    if (d->pending) {
        av_frame_free(&d->pending);
    }
    avcodec_free_context(&d->vdec_ctx);
    avformat_close_input(&d->in_ctx);
    d->video_in_index = -1;
    d->seg_open = -1;
    d->done = 0;
}

/* Open the overlay decoder for `seg`, seeking to `src_seek`. Returns 0 on success. */
static int open_overlay_decoder(const ClipSegment *seg, double src_seek, OverlayDecoder *d,
                                int seg_idx) {
    free_overlay_decoder(d);
    switch (open_input(seg->source_path, &d->in_ctx)) {
    case -1:
        return -1;
    case -2:
        return -2;
    }
    for (unsigned s = 0; s < d->in_ctx->nb_streams; s++) {
        if (d->in_ctx->streams[s]->codecpar->codec_type == AVMEDIA_TYPE_VIDEO) {
            d->video_in_index = (int)s;
            break;
        }
    }
    if (d->video_in_index < 0) {
        avformat_close_input(&d->in_ctx);
        return -3;
    }
    AVCodecParameters *vpar = d->in_ctx->streams[d->video_in_index]->codecpar;
    const AVCodec *vd = avcodec_find_decoder(vpar->codec_id);
    if (!vd) {
        avformat_close_input(&d->in_ctx);
        return -4;
    }
    d->vdec_ctx = avcodec_alloc_context3(vd);
    if (!d->vdec_ctx || avcodec_parameters_to_context(d->vdec_ctx, vpar) < 0) {
        avformat_close_input(&d->in_ctx);
        return -5;
    }
    d->vdec_ctx->pkt_timebase = d->in_ctx->streams[d->video_in_index]->time_base;
    if (avcodec_open2(d->vdec_ctx, vd, NULL) < 0) {
        avcodec_free_context(&d->vdec_ctx);
        avformat_close_input(&d->in_ctx);
        return -6;
    }
    if (src_seek > 0.0) {
        av_seek_frame(d->in_ctx, -1, (int64_t)(src_seek * AV_TIME_BASE), AVSEEK_FLAG_BACKWARD);
        avcodec_flush_buffers(d->vdec_ctx);
    }
    d->seg_open = seg_idx;
    d->done = 0;
    return 0;
}

/* Advance `d` until it has a frame at or past `src_target`, within [seg->source_in_secs,
   seg->source_out_secs).  Stores the frame in d->pending (caller must NOT free it —
   free_overlay_decoder handles lifetime).  Returns 1 if a frame is ready, 0 if done. */
static int advance_overlay_decoder(OverlayDecoder *d, const ClipSegment *seg, double src_target,
                                   AVPacket *tmp_pkt, AVFrame *tmp_frame) {
    if (d->done) return (d->pending != NULL);
    while (1) {
        if (av_read_frame(d->in_ctx, tmp_pkt) < 0) {
            d->done = 1;
            break;
        }
        if (tmp_pkt->stream_index != d->video_in_index) {
            av_packet_unref(tmp_pkt);
            continue;
        }
        int ret = avcodec_send_packet(d->vdec_ctx, tmp_pkt);
        av_packet_unref(tmp_pkt);
        if (ret < 0) {
            d->done = 1;
            break;
        }
        ret = avcodec_receive_frame(d->vdec_ctx, tmp_frame);
        if (ret == AVERROR(EAGAIN)) continue;
        if (ret < 0) {
            d->done = 1;
            break;
        }
        double fsecs = tmp_frame->pts * av_q2d(d->vdec_ctx->pkt_timebase);
        if (fsecs < seg->source_in_secs) {
            av_frame_unref(tmp_frame);
            continue;
        }
        if (fsecs >= seg->source_out_secs) {
            av_frame_unref(tmp_frame);
            d->done = 1;
            break;
        }
        /* Store as pending, replacing any previous pending frame. */
        if (d->pending)
            av_frame_unref(d->pending);
        else
            d->pending = av_frame_alloc();
        if (d->pending)
            av_frame_move_ref(d->pending, tmp_frame);
        else
            av_frame_unref(tmp_frame);
        if (fsecs >= src_target - 0.02) break; /* close enough — stop advancing */
    }
    return (d->pending != NULL);
}

static void reset_overlay_graph_sources(OverlayTrackState *tracks, int track_count) {
    for (int i = 0; i < track_count; i++) {
        tracks[i].video_source = NULL;
        tracks[i].matte_source = NULL;
    }
}

static void close_overlay_track(OverlayTrackState *track) {
    free_overlay_decoder(&track->video);
    free_overlay_decoder(&track->matte);
    track->segment = NULL;
    track->active_segment_index = -1;
    track->matte_active = 0;
    track->video_source = NULL;
    track->matte_source = NULL;
}

static void free_overlay_track(OverlayTrackState *track) {
    close_overlay_track(track);
    av_packet_free(&track->video_packet);
    av_packet_free(&track->matte_packet);
    av_frame_free(&track->video_frame);
    av_frame_free(&track->matte_frame);
}

static int create_video_buffer_source(AVFilterGraph *graph, const char *name,
                                      AVCodecContext *decoder, AVFilterContext **out_source) {
    const AVFilter *buffer_source = avfilter_get_by_name("buffer");
    AVRational sar = decoder->sample_aspect_ratio;
    char args[512];
    if (sar.num <= 0) sar = (AVRational){1, 1};
    snprintf(args, sizeof(args), "video_size=%dx%d:pix_fmt=%d:time_base=%d/%d:pixel_aspect=%d/%d",
             decoder->width, decoder->height, decoder->pix_fmt, decoder->pkt_timebase.num,
             decoder->pkt_timebase.den, sar.num, sar.den);
    return avfilter_graph_create_filter(out_source, buffer_source, name, args, NULL, graph);
}

static int add_graph_input(AVFilterInOut **inputs, AVFilterContext *source, const char *label) {
    AVFilterInOut *input = avfilter_inout_alloc();
    if (!input) return AVERROR(ENOMEM);
    input->name = av_strdup(label);
    if (!input->name) {
        avfilter_inout_free(&input);
        return AVERROR(ENOMEM);
    }
    input->filter_ctx = source;
    input->pad_idx = 0;
    input->next = *inputs;
    *inputs = input;
    return 0;
}

/* Build and configure one dynamic overlay graph. Every active entry in `tracks` contributes a
   video input; entries with an AI-background-removal matte contribute one additional input.
   Layers are applied in array order, which is the same bottom-to-top order as tracks 1..N.
   alphamerge replaces (not combines with) any alpha that the layer's own filter chain already
   produced (e.g. a mask_shape/chroma_key alpha on the same clip); combining multiple
   simultaneous alpha sources on one clip isn't supported.
   pix_fmt_name must match whatever the video encoder that will consume this graph's output was
   actually opened with (see open_video_encoder's out_pix_fmt) — yuv420p for most encoders, nv12
   for h264_qsv. Caller owns the returned graph + contexts; free with avfilter_graph_free(). */
static int init_overlay_graph(AVCodecContext *vdec0, const char *f0, OverlayTrackState *tracks,
                              int track_count, int canvas_width, int canvas_height, int fps_num,
                              int fps_den, const char *pix_fmt_name, AVFilterGraph **out_graph,
                              AVFilterContext **out_src0, AVFilterContext **out_sink) {
    *out_graph = avfilter_graph_alloc();
    *out_src0 = *out_sink = NULL;
    reset_overlay_graph_sources(tracks, track_count);
    if (!*out_graph) return AVERROR(ENOMEM);

    const AVFilter *bufsink = avfilter_get_by_name("buffersink");
    int ret = create_video_buffer_source(*out_graph, "src0", vdec0, out_src0);
    if (ret < 0) goto fail;
    ret = avfilter_graph_create_filter(out_sink, bufsink, "snk", NULL, NULL, *out_graph);
    if (ret < 0) goto fail;

    AVFilterInOut *graph_inputs = NULL;
    AVFilterInOut *graph_output = avfilter_inout_alloc();
    if (!graph_output) {
        ret = AVERROR(ENOMEM);
        goto fail;
    }
    graph_output->name = av_strdup("out");
    if (!graph_output->name) {
        avfilter_inout_free(&graph_output);
        ret = AVERROR(ENOMEM);
        goto fail;
    }
    graph_output->filter_ctx = *out_sink;
    graph_output->pad_idx = 0;

    if (add_graph_input(&graph_inputs, *out_src0, "in0") < 0) {
        avfilter_inout_free(&graph_output);
        ret = AVERROR(ENOMEM);
        goto fail;
    }

    AVBPrint description;
    av_bprint_init(&description, 4096, AV_BPRINT_SIZE_UNLIMITED);
    av_bprintf(&description, "[in0]%s[base0];", f0);
    int layer_number = 0;
    for (int i = 0; i < track_count; i++) {
        OverlayTrackState *track = &tracks[i];
        if (!track->segment) continue;

        char source_name[32], input_label[32], layer_filter[8192];
        snprintf(source_name, sizeof(source_name), "src%d", i + 1);
        snprintf(input_label, sizeof(input_label), "in%d", i + 1);
        ret = create_video_buffer_source(*out_graph, source_name, track->video.vdec_ctx,
                                         &track->video_source);
        if (ret < 0 || add_graph_input(&graph_inputs, track->video_source, input_label) < 0) {
            ret = ret < 0 ? ret : AVERROR(ENOMEM);
            goto graph_build_fail;
        }
        if (build_overlay_vfilter(track->segment, canvas_width, canvas_height, fps_num, fps_den,
                                  layer_filter, sizeof(layer_filter)) < 0) {
            ret = AVERROR(EINVAL);
            goto graph_build_fail;
        }
        av_bprintf(&description, "[%s]%s[layer%d];", input_label, layer_filter, i + 1);

        const char *layer_label = "layer";
        char composited_label[32];
        snprintf(composited_label, sizeof(composited_label), "layer%d", i + 1);
        if (track->matte_active) {
            char matte_source_name[32], matte_input_label[32], matte_filter[8192];
            snprintf(matte_source_name, sizeof(matte_source_name), "matte_src%d", i + 1);
            snprintf(matte_input_label, sizeof(matte_input_label), "matte_in%d", i + 1);
            ret = create_video_buffer_source(*out_graph, matte_source_name, track->matte.vdec_ctx,
                                             &track->matte_source);
            if (ret < 0 ||
                add_graph_input(&graph_inputs, track->matte_source, matte_input_label) < 0) {
                ret = ret < 0 ? ret : AVERROR(ENOMEM);
                goto graph_build_fail;
            }
            if (build_overlay_vfilter(&track->matte_segment, canvas_width, canvas_height, fps_num,
                                      fps_den, matte_filter, sizeof(matte_filter)) < 0) {
                ret = AVERROR(EINVAL);
                goto graph_build_fail;
            }
            av_bprintf(&description,
                       "[%s]%s,format=gray[matte%d];[layer%d][matte%d]alphamerge[layera%d];",
                       matte_input_label, matte_filter, i + 1, i + 1, i + 1, i + 1);
            layer_label = "layera";
            snprintf(composited_label, sizeof(composited_label), "%s%d", layer_label, i + 1);
        }

        /* A blend mode replaces overlay entirely for this layer, per ClipSegment::blend_mode's
           own doc comment (bridge.h): blend has no x/y placement option (it blends two
           same-size, already-aligned frames), so position_x_expr/position_y_expr are ignored
           here -- this layer composites at full canvas size, not at an offset position.
           blend's two inputs are labeled "top" (#0) and "bottom" (#1) -- verified against a
           real ffmpeg build (`ffmpeg -h filter=blend`), not assumed. This layer (the one whose
           blend mode is set) is deliberately "top": that's the mental model every NLE's own
           per-layer blend mode uses (a layer blends onto whatever's beneath it), and for an
           asymmetric mode (subtract, divide, burn, dodge, overlay, ...) the operand order
           changes the result -- swapping it would silently produce the wrong picture for
           exactly those modes. */
        if (track->segment->blend_mode && track->segment->blend_mode[0]) {
            av_bprintf(&description, "[%s][base%d]blend=all_mode=%s[base%d];", composited_label,
                       layer_number, track->segment->blend_mode, layer_number + 1);
        } else {
            const char *px = (track->segment->position_x_expr && track->segment->position_x_expr[0])
                                 ? track->segment->position_x_expr
                                 : "0";
            const char *py = (track->segment->position_y_expr && track->segment->position_y_expr[0])
                                 ? track->segment->position_y_expr
                                 : "0";
            av_bprintf(&description, "[base%d][%s]overlay=x='%s':y='%s'[base%d];", layer_number,
                       composited_label, px, py, layer_number + 1);
        }
        layer_number++;
    }
    av_bprintf(&description, "[base%d]format=%s[out]", layer_number, pix_fmt_name);
    if (!av_bprint_is_complete(&description)) {
        ret = AVERROR(ENOMEM);
        goto graph_build_fail;
    }

    char *filter_description = NULL;
    ret = av_bprint_finalize(&description, &filter_description);
    if (ret < 0 || !filter_description) {
        av_free(filter_description);
        avfilter_inout_free(&graph_output);
        avfilter_inout_free(&graph_inputs);
        ret = AVERROR(ENOMEM);
        goto fail;
    }
    ret = avfilter_graph_parse_ptr(*out_graph, filter_description, &graph_output, &graph_inputs,
                                   NULL);
    av_free(filter_description);
    avfilter_inout_free(&graph_output);
    avfilter_inout_free(&graph_inputs);
    if (ret < 0) goto fail;
    ret = avfilter_graph_config(*out_graph, NULL);
    if (ret < 0) goto fail;
    return 0;

graph_build_fail:
    av_bprint_finalize(&description, NULL);
    avfilter_inout_free(&graph_output);
    avfilter_inout_free(&graph_inputs);
fail:
    avfilter_graph_free(out_graph);
    *out_src0 = *out_sink = NULL;
    reset_overlay_graph_sources(tracks, track_count);
    return ret;
}

/* Tells every input of an overlay graph that its stream has ended and encodes the frames the
   graph was still holding back: `blend`, `overlay` and `fps` are framesync-backed and keep their
   newest frames until the next arrives, so a graph that is freed instead loses them. Returns
   ENCODE_OK or ENCODE_ERR_PIPELINE. */
static EncodeStatus drain_overlay_graph(AVFilterContext *src0, OverlayTrackState *tracks,
                                        int track_count, AVFilterContext *sink, AVFrame *filt_frame,
                                        AVFormatContext *out_ctx, AVCodecContext *venc_ctx,
                                        AVStream *vout_stream, AVPacket *enc_pkt,
                                        int64_t *next_vpts) {
    if (av_buffersrc_add_frame_flags(src0, NULL, 0) < 0) {
        return ENCODE_ERR_PIPELINE;
    }
    for (int i = 0; i < track_count; i++) {
        OverlayTrackState *track = &tracks[i];
        if (track->video_source && av_buffersrc_add_frame_flags(track->video_source, NULL, 0) < 0) {
            return ENCODE_ERR_PIPELINE;
        }
        if (track->matte_source && av_buffersrc_add_frame_flags(track->matte_source, NULL, 0) < 0) {
            return ENCODE_ERR_PIPELINE;
        }
    }
    EncodeStatus status = ENCODE_OK;
    while (status == ENCODE_OK && av_buffersink_get_frame(sink, filt_frame) == 0) {
        filt_frame->pts = (*next_vpts)++;
        if (encode_write_packet(out_ctx, venc_ctx, vout_stream, filt_frame, enc_pkt) < 0) {
            status = ENCODE_ERR_PIPELINE;
        }
        av_frame_unref(filt_frame);
    }
    return status;
}

EncodeStatus avbridge_encode_timeline_export_multi(
    const ClipSegment *const *track_segs, const int *track_n_segs, int n_tracks, int canvas_width,
    int canvas_height, int canvas_fps_num, int canvas_fps_den, int64_t canvas_bit_rate_bps,
    const char *out_path, float target_lufs, int gpu_encoder_preference,
    ProgressCallback progress_cb, void *progress_user_data, const uint8_t *cancel) {
    if (n_tracks <= 0 || track_n_segs[0] <= 0) return ENCODE_ERR_EMPTY_TIMELINE;

    /* N=1: delegate to the established single-track implementation. */
    if (n_tracks == 1) {
        return avbridge_encode_timeline_export(
            track_segs[0], track_n_segs[0], canvas_width, canvas_height, canvas_fps_num,
            canvas_fps_den, canvas_bit_rate_bps, out_path, target_lufs, gpu_encoder_preference,
            progress_cb, progress_user_data, cancel);
    }

    AVFormatContext *out_ctx = NULL;
    AVCodecContext *venc_ctx = NULL, *aenc_ctx = NULL;
    AudioFilterChain achain = {0};
    AudioWorker *audio_worker = NULL;
    AVStream *vout_stream = NULL, *aout_stream = NULL;
    AVPacket *pkt = NULL;
    AVFrame *dec_frame = NULL, *filt_frame = NULL;
    AVPacket *enc_pkt = NULL;
    EncodeStatus status = ENCODE_OK;
    AVRational canvas_fps = {canvas_fps_num, canvas_fps_den};
    int64_t next_vpts = 0;
    double elapsed = 0.0;
    int canonical_sr = 0;
    enum AVSampleFormat canonical_fmt = AV_SAMPLE_FMT_NONE;
    AVChannelLayout canonical_ch = {0};
    const int overlay_track_count = n_tracks - 1;
    OverlayTrackState *overlay_tracks = NULL;
    int *active_segment_indices = NULL;

    avformat_alloc_output_context2(&out_ctx, NULL, NULL, out_path);
    if (!out_ctx) return ENCODE_ERR_ALLOC_OUTPUT;

    /* Video encoder — same hardware-with-CPU-fallback setup as the single-track function.
       venc_pix_fmt is the software pixel format the opened path wants (yuv420p, or nv12 for
       h264_qsv and VAAPI's hardware upload) — every filter graph built below must conform to
       it. */
    enum AVPixelFormat venc_pix_fmt = AV_PIX_FMT_YUV420P;
    const char *venc_pix_fmt_name = "yuv420p";
    {
        int global_header = (out_ctx->oformat->flags & AVFMT_GLOBALHEADER) != 0;
        venc_ctx = open_video_encoder((GpuEncoderPreference)gpu_encoder_preference, canvas_width,
                                      canvas_height, canvas_fps, canvas_bit_rate_bps, global_header,
                                      NULL, &venc_pix_fmt);
        if (!venc_ctx) {
            status = ENCODE_ERR_ENCODER;
            goto cleanup;
        }
        vout_stream = avformat_new_stream(out_ctx, NULL);
        if (!vout_stream || avcodec_parameters_from_context(vout_stream->codecpar, venc_ctx) < 0) {
            status = ENCODE_ERR_NEW_STREAM;
            goto cleanup;
        }
        vout_stream->time_base = venc_ctx->time_base;
        const char *name = av_get_pix_fmt_name(venc_pix_fmt);
        if (name) {
            venc_pix_fmt_name = name;
        }
    }

    overlay_tracks = av_calloc((size_t)overlay_track_count, sizeof(*overlay_tracks));
    active_segment_indices =
        av_malloc_array((size_t)overlay_track_count, sizeof(*active_segment_indices));
    if (!overlay_tracks || !active_segment_indices) {
        status = ENCODE_ERR_PIPELINE;
        goto cleanup;
    }
    for (int i = 0; i < overlay_track_count; i++) {
        OverlayTrackState *track = &overlay_tracks[i];
        track->video.seg_open = -1;
        track->matte.seg_open = -1;
        track->active_segment_index = -1;
        track->video_packet = av_packet_alloc();
        track->matte_packet = av_packet_alloc();
        track->video_frame = av_frame_alloc();
        track->matte_frame = av_frame_alloc();
        if (!track->video_packet || !track->matte_packet || !track->video_frame ||
            !track->matte_frame) {
            status = ENCODE_ERR_PIPELINE;
            goto cleanup;
        }
    }

    pkt = av_packet_alloc();
    dec_frame = av_frame_alloc();
    filt_frame = av_frame_alloc();
    enc_pkt = av_packet_alloc();
    if (!pkt || !dec_frame || !filt_frame || !enc_pkt) {
        status = ENCODE_ERR_PIPELINE;
        goto cleanup;
    }

    /* Outer loop: process track-0 segments in order. */
    for (int si = 0; si < track_n_segs[0] && status == ENCODE_OK; si++) {
        const ClipSegment *seg0 = &track_segs[0][si];
        AVFormatContext *in_ctx0 = NULL;
        AVCodecContext *vdec_ctx0 = NULL, *adec_ctx0 = NULL;
        VideoFilterChain vchain = {0};
        AVFilterGraph *ov_graph = NULL;
        AVFilterContext *ov_src0 = NULL, *ov_sink = NULL;
        int vidx0 = -1, aidx0 = -1;
        /* Filter graph mode: 0 = not built, 1 = single-track vchain, 2 = overlay graph. */
        int cur_mode = 0;
        /* Pts of the main-source frame that opened the current overlay graph: every frame pushed
           into the graph is stamped with its time since then, in that source's own time base. */
        int64_t ov_base_pts = 0;

        double tl_start = seg0->timeline_start_secs;

        switch (open_input(seg0->source_path, &in_ctx0)) {
        case -1:
            status = ENCODE_ERR_OPEN_INPUT;
            break;
        case -2:
            status = ENCODE_ERR_STREAM_INFO;
            break;
        }
        if (status != ENCODE_OK) break;

        for (unsigned s = 0; s < in_ctx0->nb_streams; s++) {
            enum AVMediaType mt = in_ctx0->streams[s]->codecpar->codec_type;
            if (vidx0 < 0 && mt == AVMEDIA_TYPE_VIDEO)
                vidx0 = (int)s;
            else if (aidx0 < 0 && mt == AVMEDIA_TYPE_AUDIO)
                aidx0 = (int)s;
        }
        if (vidx0 < 0) {
            avformat_close_input(&in_ctx0);
            status = ENCODE_ERR_NO_VIDEO_STREAM;
            break;
        }
        if (aidx0 < 0) {
            avformat_close_input(&in_ctx0);
            status = ENCODE_ERR_NO_AUDIO_STREAM;
            break;
        }

        { /* Video decoder */
            AVCodecParameters *vp = in_ctx0->streams[vidx0]->codecpar;
            const AVCodec *vd = avcodec_find_decoder(vp->codec_id);
            if (!vd) {
                status = ENCODE_ERR_DECODER;
                goto seg_cleanup;
            }
            vdec_ctx0 = avcodec_alloc_context3(vd);
            if (!vdec_ctx0 || avcodec_parameters_to_context(vdec_ctx0, vp) < 0) {
                status = ENCODE_ERR_DECODER;
                goto seg_cleanup;
            }
            vdec_ctx0->pkt_timebase = in_ctx0->streams[vidx0]->time_base;
            bridge_enable_decoder_threads(vdec_ctx0);
            if (avcodec_open2(vdec_ctx0, vd, NULL) < 0) {
                status = ENCODE_ERR_DECODER;
                goto seg_cleanup;
            }
        }
        { /* Audio decoder */
            AVCodecParameters *ap = in_ctx0->streams[aidx0]->codecpar;
            const AVCodec *ad = avcodec_find_decoder(ap->codec_id);
            if (!ad) {
                status = ENCODE_ERR_DECODER;
                goto seg_cleanup;
            }
            adec_ctx0 = avcodec_alloc_context3(ad);
            if (!adec_ctx0 || avcodec_parameters_to_context(adec_ctx0, ap) < 0) {
                status = ENCODE_ERR_DECODER;
                goto seg_cleanup;
            }
            adec_ctx0->pkt_timebase = in_ctx0->streams[aidx0]->time_base;
            if (avcodec_open2(adec_ctx0, ad, NULL) < 0) {
                status = ENCODE_ERR_DECODER;
                goto seg_cleanup;
            }
        }

        if (si == 0) { /* First segment: set up audio chain + encoder + output header */
            canonical_sr = adec_ctx0->sample_rate;
            canonical_fmt = adec_ctx0->sample_fmt;
            av_channel_layout_copy(&canonical_ch, &adec_ctx0->ch_layout);
            status = timeline_open_audio_output(out_ctx, adec_ctx0, target_lufs, out_path, &achain,
                                                &aenc_ctx, &aout_stream, &audio_worker);
            if (status != ENCODE_OK) {
                goto seg_cleanup;
            }
        } else if (adec_ctx0->sample_rate != canonical_sr ||
                   adec_ctx0->sample_fmt != canonical_fmt ||
                   av_channel_layout_compare(&adec_ctx0->ch_layout, &canonical_ch) != 0) {
            status = ENCODE_ERR_AUDIO_FORMAT_MISMATCH;
            goto seg_cleanup;
        }

        timeline_queue_segment_audio_commands(audio_worker, seg0);

        if (seg0->source_in_secs > 0.0) {
            av_seek_frame(in_ctx0, -1, (int64_t)(seg0->source_in_secs * AV_TIME_BASE),
                          AVSEEK_FLAG_BACKWARD);
            avcodec_flush_buffers(vdec_ctx0);
            avcodec_flush_buffers(adec_ctx0);
        }

        { /* Per-segment decode loop */
            int vdone = 0, adone = 0;
            int video_flushed = 0;
            while ((!vdone || !adone) && status == ENCODE_OK) {
                if (cancel && *cancel) {
                    status = ENCODE_CANCELLED;
                    break;
                }
                if (av_read_frame(in_ctx0, pkt) < 0) {
                    if (video_flushed || vdone) break;
                    /* End of input: an empty packet flushes the threaded decoder and the
                       receive loop below delivers the frames it was still holding. */
                    video_flushed = 1;
                    av_packet_unref(pkt);
                    pkt->data = NULL;
                    pkt->size = 0;
                    pkt->stream_index = vidx0;
                }

                if (pkt->stream_index == vidx0 && !vdone) {
                    int ret = avcodec_send_packet(vdec_ctx0, pkt);
                    av_packet_unref(pkt);
                    if (ret < 0) {
                        status = ENCODE_ERR_PIPELINE;
                        break;
                    }
                    while (1) {
                        ret = avcodec_receive_frame(vdec_ctx0, dec_frame);
                        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) break;
                        if (ret < 0) {
                            status = ENCODE_ERR_PIPELINE;
                            break;
                        }
                        double fsrc = dec_frame->pts * av_q2d(vdec_ctx0->pkt_timebase);
                        if (fsrc < seg0->source_in_secs) {
                            av_frame_unref(dec_frame);
                            continue;
                        }
                        if (fsrc >= seg0->source_out_secs) {
                            vdone = 1;
                            av_frame_unref(dec_frame);
                            continue;
                        }

                        double ftl = tl_start + smooth_speed_ramp_timeline_elapsed(
                                                    seg0, fsrc - seg0->source_in_secs);

                        int want_overlay = 0;
                        int overlay_set_changed = (cur_mode != 2);
                        for (int i = 0; i < overlay_track_count; i++) {
                            int track_index = i + 1;
                            int active_index = -1;
                            for (int s = 0; s < track_n_segs[track_index]; s++) {
                                const ClipSegment *candidate = &track_segs[track_index][s];
                                double speed =
                                    candidate->speed_factor > 0.0f ? candidate->speed_factor : 1.0f;
                                double end =
                                    candidate->timeline_start_secs +
                                    (candidate->source_out_secs - candidate->source_in_secs) /
                                        speed;
                                if (ftl >= candidate->timeline_start_secs && ftl < end) {
                                    active_index = s;
                                    break;
                                }
                            }
                            active_segment_indices[i] = active_index;
                            if (active_index >= 0) want_overlay = 1;
                            if (active_index != overlay_tracks[i].active_segment_index)
                                overlay_set_changed = 1;
                        }

                        /* Rebuild whenever any layer enters, leaves, or changes clip. */
                        if (want_overlay && overlay_set_changed) {
                            if (ov_graph) {
                                status = drain_overlay_graph(ov_src0, overlay_tracks,
                                                             overlay_track_count, ov_sink,
                                                             filt_frame, out_ctx, venc_ctx,
                                                             vout_stream, enc_pkt, &next_vpts);
                                if (status != ENCODE_OK) {
                                    av_frame_unref(dec_frame);
                                    break;
                                }
                                avfilter_graph_free(&ov_graph);
                                ov_src0 = ov_sink = NULL;
                                reset_overlay_graph_sources(overlay_tracks, overlay_track_count);
                            }
                            if (vchain.graph && filter_encode_write_video_frame(
                                                    out_ctx, &vchain, venc_ctx, vout_stream, NULL,
                                                    filt_frame, &next_vpts, enc_pkt) < 0) {
                                status = ENCODE_ERR_PIPELINE;
                                av_frame_unref(dec_frame);
                                break;
                            }
                            free_video_filter_chain(&vchain);

                            for (int i = 0; i < overlay_track_count; i++) {
                                OverlayTrackState *track = &overlay_tracks[i];
                                int segment_index = active_segment_indices[i];
                                if (segment_index == track->active_segment_index) continue;
                                close_overlay_track(track);
                                if (segment_index < 0) continue;

                                const ClipSegment *segment = &track_segs[i + 1][segment_index];
                                double speed =
                                    segment->speed_factor > 0.0f ? segment->speed_factor : 1.0f;
                                double source_seek = segment->source_in_secs +
                                                     (ftl - segment->timeline_start_secs) * speed;
                                if (open_overlay_decoder(segment, source_seek, &track->video,
                                                         segment_index) < 0) {
                                    status = ENCODE_ERR_DECODER;
                                    av_frame_unref(dec_frame);
                                    break;
                                }
                                track->segment = segment;
                                track->active_segment_index = segment_index;

                                /* A matte is optional. It covers the clip's source-trim range
                                   and is decoded in lockstep with that layer. A missing/stale
                                   matte degrades to the unmasked layer instead of aborting. */
                                if (segment->mask_video_path && segment->mask_video_path[0]) {
                                    track->matte_segment = *segment;
                                    track->matte_segment.source_path = segment->mask_video_path;
                                    track->matte_segment.source_in_secs = 0.0;
                                    track->matte_segment.source_out_secs =
                                        segment->source_out_secs - segment->source_in_secs;
                                    track->matte_segment.video_filter = "";
                                    track->matte_segment.transition_in = 0;
                                    double matte_seek = source_seek - segment->source_in_secs;
                                    if (open_overlay_decoder(&track->matte_segment, matte_seek,
                                                             &track->matte, segment_index) == 0) {
                                        track->matte_active = 1;
                                    }
                                }
                            }
                            if (status != ENCODE_OK) break;

                            char f0[8192];
                            if (build_overlay_vfilter(seg0, canvas_width, canvas_height,
                                                      canvas_fps_num, canvas_fps_den, f0,
                                                      sizeof(f0)) < 0) {
                                status = ENCODE_ERR_FILTER_GRAPH;
                                av_frame_unref(dec_frame);
                                break;
                            }
                            if (init_overlay_graph(
                                    vdec_ctx0, f0, overlay_tracks, overlay_track_count,
                                    canvas_width, canvas_height, canvas_fps_num, canvas_fps_den,
                                    venc_pix_fmt_name, &ov_graph, &ov_src0, &ov_sink) < 0) {
                                int had_matte = 0;
                                for (int i = 0; i < overlay_track_count; i++) {
                                    OverlayTrackState *track = &overlay_tracks[i];
                                    if (!track->matte_active) continue;
                                    had_matte = 1;
                                    track->matte_active = 0;
                                    free_overlay_decoder(&track->matte);
                                }
                                /* Matte alpha is optional. If any matte made the combined graph
                                   invalid, retry all active layers without matte inputs. */
                                if (!had_matte ||
                                    init_overlay_graph(
                                        vdec_ctx0, f0, overlay_tracks, overlay_track_count,
                                        canvas_width, canvas_height, canvas_fps_num, canvas_fps_den,
                                        venc_pix_fmt_name, &ov_graph, &ov_src0, &ov_sink) < 0) {
                                    status = ENCODE_ERR_FILTER_GRAPH;
                                    av_frame_unref(dec_frame);
                                    break;
                                }
                            }
                            ov_base_pts = dec_frame->pts;
                            cur_mode = 2;
                        } else if (!want_overlay && cur_mode != 1) {
                            /* Switch to single-track mode */
                            if (ov_graph) {
                                status = drain_overlay_graph(ov_src0, overlay_tracks,
                                                             overlay_track_count, ov_sink,
                                                             filt_frame, out_ctx, venc_ctx,
                                                             vout_stream, enc_pkt, &next_vpts);
                                if (status != ENCODE_OK) {
                                    av_frame_unref(dec_frame);
                                    break;
                                }
                                avfilter_graph_free(&ov_graph);
                                ov_src0 = ov_sink = NULL;
                            }
                            free_video_filter_chain(&vchain);
                            for (int i = 0; i < overlay_track_count; i++)
                                close_overlay_track(&overlay_tracks[i]);

                            char vfd[16384];
                            if (build_segment_vfilter(seg0, canvas_width, canvas_height,
                                                      canvas_fps_num, canvas_fps_den,
                                                      venc_pix_fmt_name, vfd, sizeof(vfd)) < 0) {
                                status = ENCODE_ERR_FILTER_GRAPH;
                                av_frame_unref(dec_frame);
                                break;
                            }
                            if (init_video_filter_chain(vdec_ctx0, vfd, &vchain) < 0) {
                                status = ENCODE_ERR_FILTER_GRAPH;
                                av_frame_unref(dec_frame);
                                break;
                            }
                            cur_mode = 1;
                        }

                        if (cur_mode == 2) {
                            for (int i = 0; i < overlay_track_count; i++) {
                                OverlayTrackState *track = &overlay_tracks[i];
                                if (!track->segment) continue;
                                double speed = track->segment->speed_factor > 0.0f
                                                   ? track->segment->speed_factor
                                                   : 1.0f;
                                double source_target =
                                    track->segment->source_in_secs +
                                    (ftl - track->segment->timeline_start_secs) * speed;
                                advance_overlay_decoder(&track->video, track->segment,
                                                        source_target, track->video_packet,
                                                        track->video_frame);
                                if (track->matte_active) {
                                    advance_overlay_decoder(
                                        &track->matte, &track->matte_segment,
                                        source_target - track->segment->source_in_secs,
                                        track->matte_packet, track->matte_frame);
                                }
                            }

                            /* The layers advance in lockstep with this frame, so every input is
                               stamped with the same moment, each in its own source's time base. */
                            dec_frame->pts -= ov_base_pts;
                            int64_t layer_moment = dec_frame->pts;
                            if (av_buffersrc_add_frame_flags(ov_src0, dec_frame,
                                                             AV_BUFFERSRC_FLAG_KEEP_REF) < 0) {
                                status = ENCODE_ERR_PIPELINE;
                            }
                            for (int i = 0; i < overlay_track_count && status == ENCODE_OK; i++) {
                                OverlayTrackState *track = &overlay_tracks[i];
                                if (!track->segment) continue;
                                if (track->video.pending) {
                                    track->video.pending->pts =
                                        av_rescale_q(layer_moment, vdec_ctx0->pkt_timebase,
                                                     track->video.vdec_ctx->pkt_timebase);
                                    if (av_buffersrc_add_frame_flags(
                                            track->video_source, track->video.pending,
                                            AV_BUFFERSRC_FLAG_KEEP_REF) < 0) {
                                        status = ENCODE_ERR_PIPELINE;
                                        break;
                                    }
                                }
                                if (track->matte_active && track->matte.pending) {
                                    track->matte.pending->pts =
                                        av_rescale_q(layer_moment, vdec_ctx0->pkt_timebase,
                                                     track->matte.vdec_ctx->pkt_timebase);
                                    if (av_buffersrc_add_frame_flags(
                                            track->matte_source, track->matte.pending,
                                            AV_BUFFERSRC_FLAG_KEEP_REF) < 0) {
                                        status = ENCODE_ERR_PIPELINE;
                                        break;
                                    }
                                }
                            }

                            /* Pull composite frames from overlay sink. */
                            while (status == ENCODE_OK &&
                                   av_buffersink_get_frame(ov_sink, filt_frame) == 0) {
                                filt_frame->pts = next_vpts++;
                                if (encode_write_packet(out_ctx, venc_ctx, vout_stream, filt_frame,
                                                        enc_pkt) < 0) {
                                    status = ENCODE_ERR_PIPELINE;
                                }
                                av_frame_unref(filt_frame);
                                if (status != ENCODE_OK) break;
                            }
                            if (progress_cb)
                                progress_cb(progress_user_data,
                                            elapsed + smooth_speed_ramp_timeline_elapsed(
                                                          seg0, fsrc - seg0->source_in_secs));
                        } else { /* single-track vchain */
                            if (filter_encode_write_video_frame(out_ctx, &vchain, venc_ctx,
                                                                vout_stream, dec_frame, filt_frame,
                                                                &next_vpts, enc_pkt) < 0) {
                                status = ENCODE_ERR_PIPELINE;
                            }
                            if (progress_cb)
                                progress_cb(progress_user_data,
                                            elapsed + smooth_speed_ramp_timeline_elapsed(
                                                          seg0, fsrc - seg0->source_in_secs));
                        }
                        av_frame_unref(dec_frame);
                        if (status != ENCODE_OK) break;
                    }
                } else if (pkt->stream_index == aidx0 && !adone) {
                    int ret = avcodec_send_packet(adec_ctx0, pkt);
                    av_packet_unref(pkt);
                    if (ret < 0) {
                        status = ENCODE_ERR_PIPELINE;
                        break;
                    }
                    while (1) {
                        ret = avcodec_receive_frame(adec_ctx0, dec_frame);
                        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) break;
                        if (ret < 0) {
                            status = ENCODE_ERR_PIPELINE;
                            break;
                        }
                        double fs = dec_frame->pts * av_q2d(adec_ctx0->pkt_timebase);
                        if (fs < seg0->source_in_secs) {
                            av_frame_unref(dec_frame);
                            continue;
                        }
                        if (fs >= seg0->source_out_secs) {
                            adone = 1;
                            av_frame_unref(dec_frame);
                            continue;
                        }
                        if (audio_worker_push_frame(audio_worker, dec_frame) < 0) {
                            status = ENCODE_ERR_PIPELINE;
                            break;
                        }
                        av_frame_unref(dec_frame);
                    }
                } else {
                    av_packet_unref(pkt);
                }
                if (status != ENCODE_OK) break;
            }
        }

        /* `blend` is framesync-backed and may retain its final frames until every input reaches
           EOF. Drain the graph before freeing it so a short blend-only segment cannot leave an
           otherwise valid export with audio but no video stream. */
        if (status == ENCODE_OK && cur_mode == 2 && ov_graph) {
            status = drain_overlay_graph(ov_src0, overlay_tracks, overlay_track_count, ov_sink,
                                         filt_frame, out_ctx, venc_ctx, vout_stream, enc_pkt,
                                         &next_vpts);
        }

        if (status == ENCODE_OK && cur_mode == 1 && vchain.graph &&
            filter_encode_write_video_frame(out_ctx, &vchain, venc_ctx, vout_stream, NULL,
                                            filt_frame, &next_vpts, enc_pkt) < 0) {
            status = ENCODE_ERR_PIPELINE;
        }

        elapsed +=
            smooth_speed_ramp_timeline_elapsed(seg0, seg0->source_out_secs - seg0->source_in_secs);

    seg_cleanup:
        if (ov_graph) {
            avfilter_graph_free(&ov_graph);
            ov_src0 = ov_sink = NULL;
            reset_overlay_graph_sources(overlay_tracks, overlay_track_count);
        }
        free_video_filter_chain(&vchain);
        avcodec_free_context(&vdec_ctx0);
        avcodec_free_context(&adec_ctx0);
        avformat_close_input(&in_ctx0);
    }

    /* Flush audio encoder + loudnorm graph. */
    if (status == ENCODE_OK && audio_worker_finish(&audio_worker, 1) < 0) {
        status = ENCODE_ERR_PIPELINE;
    }
    /* Flush video encoder. */
    if (status == ENCODE_OK &&
        encode_write_packet(out_ctx, venc_ctx, vout_stream, NULL, enc_pkt) < 0) {
        status = ENCODE_ERR_PIPELINE;
    }
    if (status == ENCODE_OK) av_write_trailer(out_ctx);

cleanup:
    audio_worker_finish(&audio_worker, 0);
    if (overlay_tracks) {
        for (int i = 0; i < overlay_track_count; i++)
            free_overlay_track(&overlay_tracks[i]);
    }
    av_free(overlay_tracks);
    av_free(active_segment_indices);
    av_packet_free(&pkt);
    av_packet_free(&enc_pkt);
    av_frame_free(&dec_frame);
    av_frame_free(&filt_frame);
    if (out_ctx && out_ctx->pb && !(out_ctx->oformat->flags & AVFMT_NOFILE))
        avio_closep(&out_ctx->pb);
    free_audio_filter_chain(&achain);
    av_channel_layout_uninit(&canonical_ch);
    avcodec_free_context(&venc_ctx);
    avcodec_free_context(&aenc_ctx);
    if (out_ctx) avformat_free_context(out_ctx);
    return status;
}
