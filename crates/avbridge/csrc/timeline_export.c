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
#include <libavformat/avformat.h>
#include <libavutil/channel_layout.h>
#include <libavutil/pixdesc.h>

EncodeStatus avbridge_encode_timeline_export(const ClipSegment *segments, int segment_count,
                                             int canvas_width, int canvas_height,
                                             int canvas_fps_num, int canvas_fps_den,
                                             int64_t canvas_bit_rate_bps, const char *out_path,
                                             float target_lufs, int gpu_encoder_preference,
                                             ProgressCallback progress_cb, void *progress_user_data,
                                             const uint8_t *cancel) {
    if (segment_count <= 0) {
        return ENCODE_ERR_EMPTY_TIMELINE;
    }

    AVFormatContext *out_ctx = NULL;
    AVCodecContext *venc_ctx = NULL;
    AVCodecContext *aenc_ctx = NULL;
    AudioFilterChain achain = {0};
    AudioWorker *audio_worker = NULL;
    AVStream *video_out_stream = NULL;
    AVStream *audio_out_stream = NULL;
    AVPacket *pkt = NULL;
    AVFrame *dec_frame = NULL;
    AVFrame *filt_frame = NULL;
    AVPacket *enc_pkt = NULL;
    EncodeStatus status = ENCODE_OK;
    AVRational canvas_fps = {canvas_fps_num, canvas_fps_den};
    int64_t next_video_pts = 0;
    double elapsed_before_segment = 0.0;
    int canonical_sample_rate = 0;
    enum AVSampleFormat canonical_sample_fmt = AV_SAMPLE_FMT_NONE;
    AVChannelLayout canonical_ch_layout = {0};

    avformat_alloc_output_context2(&out_ctx, NULL, NULL, out_path);
    if (!out_ctx) {
        return ENCODE_ERR_ALLOC_OUTPUT;
    }

    /* Video encoder for the whole timeline's canvas — hardware-accelerated per
       gpu_encoder_preference with a CPU (libopenh264) fallback, see open_video_encoder.
       venc_pix_fmt is the software pixel format the opened path wants (yuv420p, or nv12 for
       h264_qsv and VAAPI's hardware upload) — the per-segment filter graph below must conform
       to it. */
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
        video_out_stream = avformat_new_stream(out_ctx, NULL);
        if (!video_out_stream ||
            avcodec_parameters_from_context(video_out_stream->codecpar, venc_ctx) < 0) {
            status = ENCODE_ERR_NEW_STREAM;
            goto cleanup;
        }
        video_out_stream->time_base = venc_ctx->time_base;
        const char *name = av_get_pix_fmt_name(venc_pix_fmt);
        if (name) {
            venc_pix_fmt_name = name;
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

    for (int seg_i = 0; seg_i < segment_count && status == ENCODE_OK; seg_i++) {
        const ClipSegment *seg = &segments[seg_i];
        AVFormatContext *in_ctx = NULL;
        AVCodecContext *vdec_ctx = NULL;
        AVCodecContext *adec_ctx = NULL;
        VideoFilterChain vchain = {0};
        int video_in_index = -1;
        int audio_in_index = -1;

        switch (open_input(seg->source_path, &in_ctx)) {
        case -1:
            status = ENCODE_ERR_OPEN_INPUT;
            break;
        case -2:
            status = ENCODE_ERR_STREAM_INFO;
            break;
        }
        if (status != ENCODE_OK) break;
        for (unsigned int s = 0; s < in_ctx->nb_streams; s++) {
            enum AVMediaType type = in_ctx->streams[s]->codecpar->codec_type;
            if (video_in_index < 0 && type == AVMEDIA_TYPE_VIDEO) {
                video_in_index = (int)s;
            } else if (audio_in_index < 0 && type == AVMEDIA_TYPE_AUDIO) {
                audio_in_index = (int)s;
            }
        }
        if (video_in_index < 0) {
            avformat_close_input(&in_ctx);
            status = ENCODE_ERR_NO_VIDEO_STREAM;
            break;
        }
        if (audio_in_index < 0) {
            avformat_close_input(&in_ctx);
            status = ENCODE_ERR_NO_AUDIO_STREAM;
            break;
        }

        /* Video decoder for this segment. */
        {
            AVCodecParameters *vpar = in_ctx->streams[video_in_index]->codecpar;
            const AVCodec *vdecoder = avcodec_find_decoder(vpar->codec_id);
            if (!vdecoder) {
                status = ENCODE_ERR_DECODER;
                goto segment_cleanup;
            }
            vdec_ctx = avcodec_alloc_context3(vdecoder);
            if (!vdec_ctx || avcodec_parameters_to_context(vdec_ctx, vpar) < 0) {
                status = ENCODE_ERR_DECODER;
                goto segment_cleanup;
            }
            vdec_ctx->pkt_timebase = in_ctx->streams[video_in_index]->time_base;
            /* A frozen segment reads a single anchor frame and never drains, so it keeps the
               one-frame-in, one-frame-out decoder; every other segment drains below. */
            if (!seg->frozen) {
                bridge_enable_decoder_threads(vdec_ctx);
            }
            if (avcodec_open2(vdec_ctx, vdecoder, NULL) < 0) {
                status = ENCODE_ERR_DECODER;
                goto segment_cleanup;
            }
        }

        /* Audio decoder for this segment. */
        {
            AVCodecParameters *apar = in_ctx->streams[audio_in_index]->codecpar;
            const AVCodec *adecoder = avcodec_find_decoder(apar->codec_id);
            if (!adecoder) {
                status = ENCODE_ERR_DECODER;
                goto segment_cleanup;
            }
            adec_ctx = avcodec_alloc_context3(adecoder);
            if (!adec_ctx || avcodec_parameters_to_context(adec_ctx, apar) < 0) {
                status = ENCODE_ERR_DECODER;
                goto segment_cleanup;
            }
            adec_ctx->pkt_timebase = in_ctx->streams[audio_in_index]->time_base;
            if (avcodec_open2(adec_ctx, adecoder, NULL) < 0) {
                status = ENCODE_ERR_DECODER;
                goto segment_cleanup;
            }
        }

        if (seg_i == 0) {
            /* First segment sets up the ONE audio filter graph + AAC encoder reused for the
               rest of the timeline — loudnorm needs to see the whole concatenated stream, not
               each clip normalized in isolation, so this graph is never rebuilt per segment
               (unlike the video filter chain, which is). Everything downstream requires every
               later segment's decoded audio to match this format exactly. */
            canonical_sample_rate = adec_ctx->sample_rate;
            canonical_sample_fmt = adec_ctx->sample_fmt;
            av_channel_layout_copy(&canonical_ch_layout, &adec_ctx->ch_layout);

            status = timeline_open_audio_output(out_ctx, adec_ctx, target_lufs, out_path, &achain,
                                                &aenc_ctx, &audio_out_stream, &audio_worker);
            if (status != ENCODE_OK) {
                goto segment_cleanup;
            }
        } else if (adec_ctx->sample_rate != canonical_sample_rate ||
                   adec_ctx->sample_fmt != canonical_sample_fmt ||
                   av_channel_layout_compare(&adec_ctx->ch_layout, &canonical_ch_layout) != 0) {
            status = ENCODE_ERR_AUDIO_FORMAT_MISMATCH;
            goto segment_cleanup;
        }

        timeline_queue_segment_audio_commands(audio_worker, seg);

        /* Per-segment video filter chain: canvas-conform (scale/pad/fps, so every segment
           lands on the same output dimensions/frame rate) + this clip's own effect filters +
           optional entry transition + a final format lock so the encoder always receives
           yuv420p. Rebuilt every segment since clip filters and transitions differ.
           The n counter in avfilter resets to 0 at each segment's graph instantiation, which
           is what drives per-frame transition animation. */
        {
            /* Generous margins throughout this block: clip_filter (video_filter_chain()'s
               output, e.g. a RoundedRect mask's geq expression) and the Zoom transition's own
               geq expression (see case 3 below) can each independently run past a thousand
               bytes. */
            char vfilter_descr[16384];
            const char *clip_filter =
                (seg->video_filter && seg->video_filter[0]) ? seg->video_filter : "";
            char setpts_str[192];
            build_setpts_str(seg, seg->source_out_secs - seg->source_in_secs, setpts_str,
                              sizeof(setpts_str));
            /* Build the transition filter string for this segment's entry effect. A comma is
               only a filter-chain separator OUTSIDE quotes — every comma below sits inside a
               single-quoted option value (lum='...', w='...', etc.), so lt()/gte()'s own
               comma-separated arguments are safe (verified against a real ffmpeg build, not
               just read off the docs). n/N reset to 0 at the start of each segment's filter
               graph, so they count frames from this clip's first frame. */
            /* A Zoom transition expands `inside` three times, alongside six coordinate
               expressions. Keep this comfortably above their bounded worst-case expansion. */
            char transition_str[8192] = "";
            if (seg->transition_in != 0) {
                double tf = (double)seg->transition_duration_secs * (double)canvas_fps.num /
                            (double)canvas_fps.den;
                if (tf < 1.0) tf = 1.0;
                switch (seg->transition_in) {
                case 1: /* Fade: fade in from black over transition_duration_secs. */
                    snprintf(transition_str, sizeof(transition_str), "fade=t=in:st=0:d=%.4f",
                             (double)seg->transition_duration_secs);
                    break;
                case 2:
                    /* Slide: reveal the clip from left to right. Originally an animated
                       drawbox (covers the frame with black, retreats rightward each frame)
                       — switched to a geq per-pixel expression because drawbox's x/y don't
                       expose a frame-count variable in the FFmpeg build this project links
                       against ("n" evaluates as an undefined constant there), unlike
                       crop/scale's n or geq's own N. inside is 1 while X sits left of the
                       reveal edge (min(W,N*W/tf), clamped so it never exceeds the frame),
                       else 0; luma is zeroed and chroma pinned to neutral (128) outside it,
                       matching drawbox's opaque black rectangle. */
                    snprintf(transition_str, sizeof(transition_str),
                             "geq=lum='p(X,Y)*lt(X,min(W,N*W/%g))'"
                             ":cb='128+(cb(X,Y)-128)*lt(X,min(W,N*W/%g))'"
                             ":cr='128+(cr(X,Y)-128)*lt(X,min(W,N*W/%g))'",
                             tf, tf, tf);
                    break;
                case 3: {
                    /* Zoom: the frame appears to grow from a centered 50%-size box (the
                       rest padded black) up to filling the whole frame by n>=tf.
                       Originally `scale=...:eval=frame,pad=...` — actually letting scale
                       renegotiate its OUTPUT size every frame reliably corrupted the heap
                       somewhere downstream in the filter graph (STATUS_HEAP_CORRUPTION,
                       caught by timeline_export_test.rs's
                       zoom_transition_exports_without_error — a real crash, not just a
                       parse error). Reimplemented as a geq inverse-sample instead, the same
                       technique Slide's case above uses: output frame size never changes,
                       only what each output pixel (X,Y) samples. z is the same 0.5..1.0
                       growth factor the old scale factor was; (sx,sy) is (X,Y) mapped back
                       through an inverse zoom-in around the frame center by z — at n=0
                       (z=0.5) that maps the whole canvas to a region twice the frame's
                       size, so most (sx,sy) fall outside [0,W)x[0,H) (inside=0, rendered
                       black — the pad-black-border equivalent); by n>=tf (z=1.0) every
                       (sx,sy) falls inside 1:1 (inside=1 everywhere, full frame visible).
                       Built via nested snprintf into scratch buffers, one sub-expression at
                       a time, rather than one giant format string — much easier to get the
                       %-substitution counts right, and each sub-expression only needs to
                       reference tf/z's %g-formatted text once instead of retyping the
                       argument list at every nesting level. */
                    char z[160], sx[224], sy[224], inside[1024];
                    snprintf(z, sizeof(z), "(0.5+0.5*lt(N,%g)*N/%g+0.5*gte(N,%g))", tf, tf, tf);
                    snprintf(sx, sizeof(sx), "((X-W/2)/%s+W/2)", z);
                    snprintf(sy, sizeof(sy), "((Y-H/2)/%s+H/2)", z);
                    snprintf(inside, sizeof(inside), "(1-lt(%s,0))*lt(%s,W)*(1-lt(%s,0))*lt(%s,H)",
                             sx, sx, sy, sy);
                    snprintf(transition_str, sizeof(transition_str),
                             "geq=lum='p(%s,%s)*%s'"
                             ":cb='128+(cb(%s,%s)-128)*%s'"
                             ":cr='128+(cr(%s,%s)-128)*%s'",
                             sx, sy, inside, sx, sy, inside, sx, sy, inside);
                    break;
                }
                default:
                    break;
                }
            }

            /* Build the post-fps portion: clip_filter (which already has any scale/rotation/
               opacity keyframe stages spliced onto its front — see
               ClipInstance::keyframe_video_filter_chain) and transition, optional, separated by
               a comma only where both are non-empty. clip_filter's length grows with how many
               keyframe stages are stacked on one clip, so it's the one part of this chain that
               isn't bounded by a fixed set of internal sub-expressions -- unlike
               transition_str/z/sx/sy/inside above, this is checked for truncation rather than
               silently accepting a corrupted (or, worse, still-parseable-but-wrong) filter
               chain past this point. */
            const char *post_fps = clip_filter;
            char final_chain[8192] = "";
            int fc_written;
            if (post_fps[0] && transition_str[0]) {
                fc_written =
                    snprintf(final_chain, sizeof(final_chain), "%s,%s", post_fps, transition_str);
            } else if (post_fps[0]) {
                fc_written = snprintf(final_chain, sizeof(final_chain), "%s", post_fps);
            } else if (transition_str[0]) {
                fc_written = snprintf(final_chain, sizeof(final_chain), "%s", transition_str);
            } else {
                fc_written = 0;
            }
            if (fc_written < 0 || (size_t)fc_written >= sizeof(final_chain)) {
                status = ENCODE_ERR_FILTER_GRAPH;
                goto segment_cleanup;
            }
            /* pad's x/y offsets place this segment's own anchor point (a fraction of its
               decoded content, not the padded canvas buffer) at the padded buffer's center
               (ow/2, oh/2) -- at anchor 0.5/0.5 this reduces to the original "(ow-iw)/2"
               centering exactly (verified against real ffmpeg output, byte-identical), so a
               segment with the default anchor renders identically to before this field
               existed. Any rotate=... stage later in final_chain (from scale/rotation
               keyframes) then pivots around this same buffer's own center -- i.e. around
               whatever point anchor_x/anchor_y designated -- since rotate always rotates
               around its own input frame's center. */
            char pad_x[96], pad_y[96];
            snprintf(pad_x, sizeof(pad_x), "ow/2-(%g)*iw", seg->anchor_x);
            snprintf(pad_y, sizeof(pad_y), "oh/2-(%g)*ih", seg->anchor_y);
            int vf_written = snprintf(
                vfilter_descr, sizeof(vfilter_descr),
                "%sscale=%d:%d:force_original_aspect_ratio=decrease,pad=%d:%d:%s:%s,fps=%d/%d%s%s"
                ",format=%s",
                setpts_str, canvas_width, canvas_height, canvas_width, canvas_height, pad_x, pad_y,
                canvas_fps.num, canvas_fps.den, final_chain[0] ? "," : "", final_chain,
                venc_pix_fmt_name);
            if (vf_written < 0 || (size_t)vf_written >= sizeof(vfilter_descr)) {
                status = ENCODE_ERR_FILTER_GRAPH;
                goto segment_cleanup;
            }
            if (init_video_filter_chain(vdec_ctx, vfilter_descr, &vchain) < 0) {
                status = ENCODE_ERR_FILTER_GRAPH;
                goto segment_cleanup;
            }
        }

        /* Seek to source_in_secs (container-wide — av_seek_frame with stream_index=-1 seeks
           every stream approximately together). A keyframe seek commonly lands earlier than
           requested, so frames/samples are still discarded by timestamp below until each
           stream's own decoded time actually reaches source_in_secs. */
        if (seg->source_in_secs > 0.0) {
            av_seek_frame(in_ctx, -1, (int64_t)(seg->source_in_secs * AV_TIME_BASE),
                          AVSEEK_FLAG_BACKWARD);
            avcodec_flush_buffers(vdec_ctx);
            avcodec_flush_buffers(adec_ctx);
        }

        {
            int video_done = 0;
            int audio_done = 0;
            /* Set once the frozen segment's held anchor frame has been synthesized into the
               full hold duration below — later real decoded frames within range are then just
               discarded instead of being pushed through the filter chain again. */
            int frozen_anchor_done = 0;
            while (!video_done || !audio_done) {
                if (cancel && *cancel) {
                    status = ENCODE_CANCELLED;
                    break;
                }
                if (av_read_frame(in_ctx, pkt) < 0) {
                    break;
                }

                if (pkt->stream_index == video_in_index && !video_done) {
                    int ret = avcodec_send_packet(vdec_ctx, pkt);
                    av_packet_unref(pkt);
                    if (ret < 0) {
                        status = ENCODE_ERR_PIPELINE;
                        break;
                    }
                    while (1) {
                        ret = avcodec_receive_frame(vdec_ctx, dec_frame);
                        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) break;
                        if (ret < 0) {
                            status = ENCODE_ERR_PIPELINE;
                            break;
                        }
                        double frame_secs = dec_frame->pts * av_q2d(vdec_ctx->pkt_timebase);
                        if (frame_secs < seg->source_in_secs) {
                            av_frame_unref(dec_frame);
                            continue;
                        }
                        if (frame_secs >= seg->source_out_secs) {
                            video_done = 1;
                            av_frame_unref(dec_frame);
                            continue;
                        }

                        if (seg->frozen) {
                            /* Already emitted the held frame for this segment — every later
                               real decoded frame in range is discarded, not re-pushed. */
                            if (frozen_anchor_done) {
                                av_frame_unref(dec_frame);
                                continue;
                            }
                            /* Hold this one anchor frame for the segment's whole trimmed
                               duration by synthesizing duplicate pushes spaced at canvas_fps —
                               same "one decoded frame in, one filtered frame out" contract
                               filter_encode_write_video_frame already relies on elsewhere in
                               this loop, just fed the same content repeatedly instead of newly
                               decoded frames. Driving the duplicate count off canvas_fps (not
                               the source's own frame rate) means the canvas-conform `fps=`
                               stage sees input already arriving near its target rate, so it
                               passes each duplicate through ~1:1 instead of needing to
                               extrapolate — sidesteps relying on this filter graph's EOF/flush
                               semantics, which nothing in this per-segment loop ever triggers
                               (the graph is freed, not flushed, at segment_cleanup). */
                            double hold_secs = seg->source_out_secs - seg->source_in_secs;
                            double canvas_fps_d = av_q2d(canvas_fps);
                            long long hold_frames = llround(hold_secs * canvas_fps_d);
                            if (hold_frames < 1) {
                                hold_frames = 1;
                            }
                            for (long long i = 0; i < hold_frames; i++) {
                                if (cancel && *cancel) {
                                    status = ENCODE_CANCELLED;
                                    break;
                                }
                                double target_secs = seg->source_in_secs + (double)i / canvas_fps_d;
                                AVFrame *held_frame = av_frame_clone(dec_frame);
                                if (!held_frame) {
                                    status = ENCODE_ERR_PIPELINE;
                                    break;
                                }
                                held_frame->pts =
                                    (int64_t)llround(target_secs / av_q2d(vdec_ctx->pkt_timebase));
                                int fret = filter_encode_write_video_frame(
                                    out_ctx, &vchain, venc_ctx, video_out_stream, held_frame,
                                    filt_frame, &next_video_pts, enc_pkt);
                                av_frame_free(&held_frame);
                                if (fret < 0) {
                                    status = ENCODE_ERR_PIPELINE;
                                    break;
                                }
                                if (progress_cb) {
                                    progress_cb(progress_user_data,
                                                elapsed_before_segment +
                                                    (target_secs - seg->source_in_secs));
                                }
                            }
                            frozen_anchor_done = 1;
                            video_done = 1;
                            av_frame_unref(dec_frame);
                            continue;
                        }

                        if (filter_encode_write_video_frame(out_ctx, &vchain, venc_ctx,
                                                            video_out_stream, dec_frame, filt_frame,
                                                            &next_video_pts, enc_pkt) < 0) {
                            status = ENCODE_ERR_PIPELINE;
                            break;
                        }
                        av_frame_unref(dec_frame);
                        if (progress_cb) {
                            progress_cb(progress_user_data, elapsed_before_segment +
                                                                (frame_secs - seg->source_in_secs));
                        }
                    }
                } else if (pkt->stream_index == audio_in_index && !audio_done) {
                    int ret = avcodec_send_packet(adec_ctx, pkt);
                    av_packet_unref(pkt);
                    if (ret < 0) {
                        status = ENCODE_ERR_PIPELINE;
                        break;
                    }
                    while (1) {
                        ret = avcodec_receive_frame(adec_ctx, dec_frame);
                        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) break;
                        if (ret < 0) {
                            status = ENCODE_ERR_PIPELINE;
                            break;
                        }
                        double frame_secs = dec_frame->pts * av_q2d(adec_ctx->pkt_timebase);
                        if (frame_secs < seg->source_in_secs) {
                            av_frame_unref(dec_frame);
                            continue;
                        }
                        if (frame_secs >= seg->source_out_secs) {
                            audio_done = 1;
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

            /* The decoder runs frame-threaded, so frames of this segment can still be inside it
               when the input ends: drain them, applying the same in/out range as above. */
            if (status == ENCODE_OK && !seg->frozen && !video_done && vdec_ctx) {
                avcodec_send_packet(vdec_ctx, NULL);
                while (avcodec_receive_frame(vdec_ctx, dec_frame) >= 0) {
                    double frame_secs = dec_frame->pts * av_q2d(vdec_ctx->pkt_timebase);
                    if (frame_secs < seg->source_in_secs) {
                        av_frame_unref(dec_frame);
                        continue;
                    }
                    if (frame_secs >= seg->source_out_secs) {
                        av_frame_unref(dec_frame);
                        break;
                    }
                    if (filter_encode_write_video_frame(out_ctx, &vchain, venc_ctx,
                                                        video_out_stream, dec_frame, filt_frame,
                                                        &next_video_pts, enc_pkt) < 0) {
                        status = ENCODE_ERR_PIPELINE;
                        av_frame_unref(dec_frame);
                        break;
                    }
                    av_frame_unref(dec_frame);
                    if (progress_cb) {
                        progress_cb(progress_user_data,
                                    elapsed_before_segment + (frame_secs - seg->source_in_secs));
                    }
                }
            }
        }

        /* The graph's `fps` stage holds its newest frame until it sees the next one, so the
           segment's last frame only comes out when the graph is told its input has ended. */
        if (status == ENCODE_OK && vchain.graph &&
            filter_encode_write_video_frame(out_ctx, &vchain, venc_ctx, video_out_stream, NULL,
                                            filt_frame, &next_video_pts, enc_pkt) < 0) {
            status = ENCODE_ERR_PIPELINE;
        }

    segment_cleanup:
        free_video_filter_chain(&vchain);
        avcodec_free_context(&vdec_ctx);
        avcodec_free_context(&adec_ctx);
        avformat_close_input(&in_ctx);
        elapsed_before_segment += seg->source_out_secs - seg->source_in_secs;
    }

    if (status == ENCODE_OK) {
        /* Flush: decoder(s) already drained per-segment above; only the audio worker's filter
           graph and both encoders may still be holding buffered frames. */
        if (audio_worker_finish(&audio_worker, 1) < 0) {
            status = ENCODE_ERR_PIPELINE;
        }
    }
    if (status == ENCODE_OK &&
        encode_write_packet(out_ctx, venc_ctx, video_out_stream, NULL, enc_pkt) < 0) {
        status = ENCODE_ERR_PIPELINE;
    }

    if (status == ENCODE_OK) {
        av_write_trailer(out_ctx);
    }

cleanup:
    audio_worker_finish(&audio_worker, 0);
    av_packet_free(&pkt);
    av_packet_free(&enc_pkt);
    av_frame_free(&dec_frame);
    av_frame_free(&filt_frame);
    if (out_ctx && out_ctx->pb && !(out_ctx->oformat->flags & AVFMT_NOFILE)) {
        avio_closep(&out_ctx->pb);
    }
    free_audio_filter_chain(&achain);
    av_channel_layout_uninit(&canonical_ch_layout);
    avcodec_free_context(&venc_ctx);
    avcodec_free_context(&aenc_ctx);
    if (out_ctx) {
        avformat_free_context(out_ctx);
    }
    return status;
}
