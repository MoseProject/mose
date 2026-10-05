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
        double tf = (double)seg->transition_duration_secs * (double)fps_num / (double)fps_den;
        if (tf < 1.0) tf = 1.0;
        switch (seg->transition_in) {
        case 1: /* Fade: fade in from black over transition_duration_secs. */
            snprintf(transition_str, sizeof(transition_str), "fade=t=in:st=0:d=%.4f",
                     (double)seg->transition_duration_secs);
            break;
        case 2:
            /* Both geq transitions below are per-pixel expressions, so they are enabled only
               for the first ceil(tf) frames (`enable` bypasses the filter afterwards, where
               its expression is the identity anyway): left on, a 60 s Slide export took 221 s
               against 11 s for the same clip without it. Slide: reveal the clip from left to right. Originally an animated
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
                     ":cr='128+(cr(X,Y)-128)*lt(X,min(W,N*W/%g))'"
                     ":enable='lt(n,%g)'",
                     tf, tf, tf, ceil(tf));
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
            snprintf(inside, sizeof(inside), "(1-lt(%s,0))*lt(%s,W)*(1-lt(%s,0))*lt(%s,H)", sx, sx,
                     sy, sy);
            snprintf(transition_str, sizeof(transition_str),
                     "geq=lum='p(%s,%s)*%s'"
                     ":cb='128+(cb(%s,%s)-128)*%s'"
                     ":cr='128+(cr(%s,%s)-128)*%s'"
                     ":enable='lt(n,%g)'",
                     sx, sy, inside, sx, sy, inside, sx, sy, inside, ceil(tf));
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
        fc_written = snprintf(final_chain, sizeof(final_chain), "%s,%s", post_fps, transition_str);
    } else if (post_fps[0]) {
        fc_written = snprintf(final_chain, sizeof(final_chain), "%s", post_fps);
    } else if (transition_str[0]) {
        fc_written = snprintf(final_chain, sizeof(final_chain), "%s", transition_str);
    } else {
        fc_written = 0;
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
