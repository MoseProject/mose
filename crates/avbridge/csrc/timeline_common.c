/* Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>

   This program is free software: you can redistribute it and/or modify
   it under the terms of the GNU General Public License as published by
   the Free Software Foundation, either version 3 of the License, or
   (at your option) any later version. */

/* What the single-track and multi-track timeline exports have in common: how a segment's speed
   turns into filter settings, and how the export's one audio output is set up. */

#include <math.h>
#include <stdio.h>

#include <libavfilter/buffersink.h>

#include "bridge_internal.h"

/* The audio graph the whole timeline goes through. `vol` and `tempo` are instance names the
   per-segment commands target without rebuilding the graph; loudnorm has to see the whole
   concatenated stream, so the graph is built once, for the first segment. */
#define EXPORT_AUDIO_FILTER_FORMAT                                                                 \
    "atempo@tempo=1.0,volume@vol=0dB,afftdn,loudnorm=I=%.1f:TP=-1.0:LRA=11,"                       \
    "alimiter=limit=0.95:attack=5:release=50"

/* Audio's atempo filter takes one fixed parameter, not a `t`-keyed expression, so a smooth
   per-sample speed ramp isn't achievable on the audio side in this FFmpeg build — the ramp's
   *average* speed stands in instead (see ClipSegment::smooth_speed_ramp_end_factor's own doc
   comment in bridge.h for why this is a deliberate, documented approximation, not a bug). */
float smooth_speed_ramp_average_speed(const ClipSegment *seg) {
    if (seg->smooth_speed_ramp_end_factor > 0.0f) {
        return (seg->speed_factor + seg->smooth_speed_ramp_end_factor) / 2.0f;
    }
    return seg->speed_factor;
}

double segment_output_secs(const ClipSegment *seg) {
    double source = seg->source_out_secs - seg->source_in_secs;
    double v0 = (double)seg->speed_factor;
    double v1 = (double)seg->smooth_speed_ramp_end_factor;
    if (source <= 0.0 || v0 <= 0.0) return source > 0.0 ? source : 0.0;
    if (seg->smooth_speed_ramp_end_factor > 0.0f && fabs(v1 - v0) > 1e-4) {
        return source / (v1 - v0) * log(v1 / v0);
    }
    return source / v0;
}

/* Gain of a fade at `t_secs` seconds into a clip of `total_secs`, both in output seconds. */
static double fade_gain(double t_secs, double total_secs, double fade_in, double fade_out) {
    double gain = 1.0;
    if (fade_in > 0.0 && t_secs < fade_in) gain *= t_secs < 0.0 ? 0.0 : t_secs / fade_in;
    double left = total_secs - t_secs;
    if (fade_out > 0.0 && left < fade_out) gain *= left < 0.0 ? 0.0 : left / fade_out;
    return gain;
}

void timeline_apply_audio_fade(AVFrame *frame, const ClipSegment *seg, double frame_start_secs) {
    if (!frame || frame->sample_rate <= 0 || frame->nb_samples <= 0) return;
    if (seg->fade_in_secs <= 0.0f && seg->fade_out_secs <= 0.0f) return;
    double speed = (double)smooth_speed_ramp_average_speed(seg);
    if (speed <= 0.0) speed = 1.0;
    double total = segment_output_secs(seg);
    int channels = frame->ch_layout.nb_channels;
    enum AVSampleFormat fmt = (enum AVSampleFormat)frame->format;
    int planar = av_sample_fmt_is_planar(fmt);
    for (int i = 0; i < frame->nb_samples; i++) {
        /* Output seconds into the clip: source offset divided by the (average) speed. */
        double t =
            (frame_start_secs + (double)i / frame->sample_rate - seg->source_in_secs) / speed;
        double g = fade_gain(t, total, (double)seg->fade_in_secs, (double)seg->fade_out_secs);
        if (g >= 1.0) continue;
        for (int c = 0; c < (planar ? channels : 1); c++) {
            uint8_t *base = planar ? frame->extended_data[c] : frame->extended_data[0];
            int stride_channels = planar ? 1 : channels;
            for (int k = 0; k < stride_channels; k++) {
                size_t idx = (size_t)i * (size_t)stride_channels + (size_t)k;
                switch (fmt) {
                case AV_SAMPLE_FMT_FLT:
                case AV_SAMPLE_FMT_FLTP:
                    ((float *)base)[idx] *= (float)g;
                    break;
                case AV_SAMPLE_FMT_DBL:
                case AV_SAMPLE_FMT_DBLP:
                    ((double *)base)[idx] *= g;
                    break;
                case AV_SAMPLE_FMT_S16:
                case AV_SAMPLE_FMT_S16P:
                    ((int16_t *)base)[idx] = (int16_t)((double)((int16_t *)base)[idx] * g);
                    break;
                case AV_SAMPLE_FMT_S32:
                case AV_SAMPLE_FMT_S32P:
                    ((int32_t *)base)[idx] = (int32_t)((double)((int32_t *)base)[idx] * g);
                    break;
                default:
                    return;
                }
            }
        }
    }
}

/* Builds this segment's `setpts=...,` video filter fragment (trailing comma included, ready to
   prepend to the rest of the per-segment chain), or an empty string when speed is neutral
   (1.0, no ramp). Plain `setpts=PTS/<speed>` for a constant speed_factor, unchanged from before
   this field existed. When smooth_speed_ramp_end_factor is set, video speed varies linearly in
   source time from speed_factor to it, so output PTS is the *integral* of 1/speed(t) instead of
   a constant divisor — works out to a natural-log term (see bridge.h's own derivation notes).
   Expressed directly in PTS units via the filter's own runtime `TB`/`T` variables, so it's valid
   regardless of the actual stream timebase rather than needing it baked in at build time. */
void build_setpts_str(const ClipSegment *seg, double source_duration_secs, char *out,
                      size_t out_size) {
    out[0] = '\0';
    double v0 = (double)seg->speed_factor;
    double v1 = (double)seg->smooth_speed_ramp_end_factor;
    if (seg->smooth_speed_ramp_end_factor > 0.0f && fabs(v1 - v0) > 1e-4 && v0 > 0.0 &&
        source_duration_secs > 1e-9) {
        double k = source_duration_secs / (v1 - v0);
        double b = (v1 - v0) / source_duration_secs;
        snprintf(out, out_size, "setpts=(%.10f/TB)*log((%.10f+(%.10f)*T)/%.10f),", k, v0, b, v0);
    } else if (fabsf(seg->speed_factor - 1.0f) > 1e-4f && seg->speed_factor > 0.0f) {
        snprintf(out, out_size, "setpts=PTS/%.6f,", (double)seg->speed_factor);
    }
}

/* Builds the filter string of the segment's entry transition (empty for none / hard cut).
   `frame_offset` is how many frames of the clip had already played when the graph this chain
   lives in started counting (avfilter's `n` and `N` restart with each graph): 0 for a segment of
   its own graph, positive for an overlay layer that was already running when the overlay graph
   was rebuilt. With `alpha` set (overlay layers) the part of the frame not yet revealed is made
   transparent instead of black, so the transition wipes the layer onto the footage below.
   Returns 0, or -1 if the result was truncated. */
int build_entry_transition(const ClipSegment *seg, int fps_num, int fps_den, double frame_offset,
                           int alpha, char *transition_str, size_t cap) {
    transition_str[0] = '\0';
    char nx[64] = "N", nl[64] = "n";
    if (frame_offset != 0.0) {
        snprintf(nx, sizeof(nx), "(N+%.6f)", frame_offset);
        snprintf(nl, sizeof(nl), "(n+%.6f)", frame_offset);
    }
    /* Build the transition filter string for this segment's entry effect. A comma is
       only a filter-chain separator OUTSIDE quotes — every comma below sits inside a
       single-quoted option value (lum='...', w='...', etc.), so lt()/gte()'s own
       comma-separated arguments are safe (verified against a real ffmpeg build, not
       just read off the docs). n/N reset to 0 at the start of each segment's filter
       graph, so they count frames from this clip's first frame. */
    /* A Zoom transition expands `inside` three times, alongside six coordinate
       expressions. Keep this comfortably above their bounded worst-case expansion. */
    if (seg->transition_in != 0) {
        double tf = (double)seg->transition_duration_secs * (double)fps_num / (double)fps_den;
        if (tf < 1.0) tf = 1.0;
        switch (seg->transition_in) {
        case 1: /* Fade: fade in from black over transition_duration_secs. */
            snprintf(transition_str, cap, "fade=t=in:st=0:d=%.4f",
                     (double)seg->transition_duration_secs);
            break;
        case 2:
            /* Both geq transitions below are per-pixel expressions, so they are enabled only
               for the first ceil(tf) frames (`enable` bypasses the filter afterwards, where
               its expression is the identity anyway): left on, a 60 s Slide export took 221 s
               against 11 s for the same clip without it. Slide: reveal the clip from left to right.
               Originally an animated drawbox (covers the frame with black, retreats rightward each
               frame) — switched to a geq per-pixel expression because drawbox's x/y don't expose a
               frame-count variable in the FFmpeg build this project links against ("n" evaluates as
               an undefined constant there), unlike crop/scale's n or geq's own N. inside is 1 while
               X sits left of the reveal edge (min(W,N*W/tf), clamped so it never exceeds the
               frame), else 0; luma is zeroed and chroma pinned to neutral (128) outside it,
               matching drawbox's opaque black rectangle. */
            if (alpha) {
                snprintf(transition_str, cap,
                         "format=yuva420p,geq=lum='p(X,Y)':cb='cb(X,Y)':cr='cr(X,Y)'"
                         ":a='255*lt(X,min(W,%s*W/%g))':enable='lt(%s,%g)'",
                         nx, tf, nl, ceil(tf));
            } else {
                snprintf(transition_str, cap,
                         "geq=lum='p(X,Y)*lt(X,min(W,%s*W/%g))'"
                         ":cb='128+(cb(X,Y)-128)*lt(X,min(W,%s*W/%g))'"
                         ":cr='128+(cr(X,Y)-128)*lt(X,min(W,%s*W/%g))'"
                         ":enable='lt(%s,%g)'",
                         nx, tf, nx, tf, nx, tf, nl, ceil(tf));
            }
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
            snprintf(z, sizeof(z), "(0.5+0.5*lt(%s,%g)*%s/%g+0.5*gte(%s,%g))", nx, tf, nx, tf, nx,
                     tf);
            snprintf(sx, sizeof(sx), "((X-W/2)/%s+W/2)", z);
            snprintf(sy, sizeof(sy), "((Y-H/2)/%s+H/2)", z);
            snprintf(inside, sizeof(inside), "(1-lt(%s,0))*lt(%s,W)*(1-lt(%s,0))*lt(%s,H)", sx, sx,
                     sy, sy);
            if (alpha) {
                snprintf(transition_str, cap,
                         "format=yuva420p,geq=lum='p(%s,%s)':cb='cb(%s,%s)':cr='cr(%s,%s)'"
                         ":a='255*%s':enable='lt(%s,%g)'",
                         sx, sy, sx, sy, sx, sy, inside, nl, ceil(tf));
            } else {
                snprintf(transition_str, cap,
                         "geq=lum='p(%s,%s)*%s'"
                         ":cb='128+(cb(%s,%s)-128)*%s'"
                         ":cr='128+(cr(%s,%s)-128)*%s'"
                         ":enable='lt(%s,%g)'",
                         sx, sy, inside, sx, sy, inside, sx, sy, inside, nl, ceil(tf));
            }
            break;
        }
        default:
            break;
        }
    }
    return 0;
}

/* The whole video filter graph of one segment: canvas conform (scale, pad around the segment's
   anchor, fps), the clip's own effect filters, an optional entry transition and a final format
   lock for the encoder. Rebuilt for every segment since filters and transitions differ; avfilter's
   `n` counter restarts with each graph, which is what drives a transition's animation. Returns 0,
   or -1 if the result would not fit in `buf`. */
int build_segment_vfilter(const ClipSegment *seg, int canvas_width, int canvas_height, int fps_num,
                          int fps_den, const char *pix_fmt_name, char *buf, size_t cap) {
    /* Generous margins throughout this block: clip_filter (video_filter_chain()'s
       output, e.g. a RoundedRect mask's geq expression) and the Zoom transition's own
       geq expression (see case 3 below) can each independently run past a thousand
       bytes. */
    const char *clip_filter = (seg->video_filter && seg->video_filter[0]) ? seg->video_filter : "";
    char setpts_str[192];
    build_setpts_str(seg, seg->source_out_secs - seg->source_in_secs, setpts_str,
                     sizeof(setpts_str));
    /* The entry transition (fade, slide, zoom): see build_entry_transition. */
    char transition_str[8192] = "";
    build_entry_transition(seg, fps_num, fps_den, 0.0, 0, transition_str, sizeof(transition_str));

    /* Clip fade handles: from / to black over the first / last fade_*_secs of the output. */
    char fade_str[160] = "";
    {
        double total = segment_output_secs(seg);
        double fi = (double)seg->fade_in_secs;
        double fo = (double)seg->fade_out_secs;
        size_t used = 0;
        if (fi > 0.0) {
            used += (size_t)snprintf(fade_str + used, sizeof(fade_str) - used,
                                     "fade=t=in:st=0:d=%.4f", fi);
        }
        if (fo > 0.0 && total > 0.0) {
            if (fo > total) fo = total;
            snprintf(fade_str + used, sizeof(fade_str) - used, "%sfade=t=out:st=%.4f:d=%.4f",
                     used ? "," : "", total - fo, fo);
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
    const char *stages[3] = {clip_filter, transition_str, fade_str};
    char final_chain[8192] = "";
    int fc_written = 0;
    for (int i = 0; i < 3 && fc_written >= 0 && (size_t)fc_written < sizeof(final_chain); i++) {
        if (!stages[i][0]) continue;
        int n = snprintf(final_chain + fc_written, sizeof(final_chain) - (size_t)fc_written, "%s%s",
                         fc_written ? "," : "", stages[i]);
        fc_written = n < 0 ? -1 : fc_written + n;
    }
    if (fc_written < 0 || (size_t)fc_written >= sizeof(final_chain)) {
        return -1;
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
    int vf_written =
        snprintf(buf, cap,
                 "%sscale=%d:%d:force_original_aspect_ratio=decrease,pad=%d:%d:%s:%s,fps=%d/%d%s%s"
                 ",format=%s",
                 setpts_str, canvas_width, canvas_height, canvas_width, canvas_height, pad_x, pad_y,
                 fps_num, fps_den, final_chain[0] ? "," : "", final_chain, pix_fmt_name);
    if (vf_written < 0 || (size_t)vf_written >= cap) {
        return -1;
    }
    return 0;
}

EncodeStatus timeline_open_audio_output(AVFormatContext *out_ctx, AVCodecContext *adec_ctx,
                                        float target_lufs, const char *out_path,
                                        AudioFilterChain *achain, AVCodecContext **aenc_ctx,
                                        AVStream **audio_stream, AudioWorker **worker) {
    const AVCodec *aencoder = avcodec_find_encoder(AV_CODEC_ID_AAC);
    if (!aencoder) {
        return ENCODE_ERR_ENCODER;
    }

    char afilter_descr[256];
    snprintf(afilter_descr, sizeof(afilter_descr), EXPORT_AUDIO_FILTER_FORMAT, (double)target_lufs);
    if (init_audio_filter_chain(adec_ctx, aencoder, afilter_descr, achain) < 0) {
        return ENCODE_ERR_FILTER_GRAPH;
    }

    AVCodecContext *encoder = avcodec_alloc_context3(aencoder);
    *aenc_ctx = encoder;
    if (!encoder) {
        return ENCODE_ERR_ENCODER;
    }
    encoder->sample_rate = av_buffersink_get_sample_rate(achain->buffersink_ctx);
    av_buffersink_get_ch_layout(achain->buffersink_ctx, &encoder->ch_layout);
    encoder->sample_fmt = (enum AVSampleFormat)av_buffersink_get_format(achain->buffersink_ctx);
    encoder->bit_rate = EXPORT_AUDIO_BITRATE_BPS;
    encoder->time_base = av_buffersink_get_time_base(achain->buffersink_ctx);
    if (out_ctx->oformat->flags & AVFMT_GLOBALHEADER) {
        encoder->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
    }
    if (avcodec_open2(encoder, aencoder, NULL) < 0) {
        return ENCODE_ERR_ENCODER;
    }
    if (encoder->frame_size > 0) {
        av_buffersink_set_frame_size(achain->buffersink_ctx, (unsigned)encoder->frame_size);
    }

    AVStream *stream = avformat_new_stream(out_ctx, NULL);
    *audio_stream = stream;
    if (!stream || avcodec_parameters_from_context(stream->codecpar, encoder) < 0) {
        return ENCODE_ERR_NEW_STREAM;
    }
    stream->time_base = encoder->time_base;

    if (!(out_ctx->oformat->flags & AVFMT_NOFILE) &&
        avio_open(&out_ctx->pb, out_path, AVIO_FLAG_WRITE) < 0) {
        return ENCODE_ERR_OPEN_OUTPUT;
    }
    if (avformat_write_header(out_ctx, NULL) < 0) {
        return ENCODE_ERR_WRITE_HEADER;
    }
    /* From here on the audio graph and encoder belong to the worker's thread. */
    *worker = audio_worker_start(out_ctx, achain, encoder, stream);
    return *worker ? ENCODE_OK : ENCODE_ERR_PIPELINE;
}

void timeline_queue_segment_audio_commands(AudioWorker *worker, const ClipSegment *seg) {
    char gain[32];
    snprintf(gain, sizeof(gain), "%.4fdB", (double)seg->gain_db);
    audio_worker_command(worker, "vol", "volume", gain);

    float speed = smooth_speed_ramp_average_speed(seg);
    speed = speed > 0.0f ? speed : 1.0f;
    if (speed < 0.5f) speed = 0.5f;
    if (speed > 100.0f) speed = 100.0f;
    char tempo[32];
    snprintf(tempo, sizeof(tempo), "%.6f", (double)speed);
    audio_worker_command(worker, "tempo", "tempo", tempo);
}
