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
#define EXPORT_AUDIO_FILTER_FORMAT                                                       \
    "atempo@tempo=1.0,volume@vol=0dB,afftdn,loudnorm=I=%.1f:TP=-1.0:LRA=11,"             \
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

EncodeStatus timeline_open_audio_output(AVFormatContext *out_ctx, AVCodecContext *adec_ctx,
                                        float target_lufs, const char *out_path,
                                        AudioFilterChain *achain, AVCodecContext **aenc_ctx,
                                        AVStream **audio_stream, AudioWorker **worker) {
    const AVCodec *aencoder = avcodec_find_encoder(AV_CODEC_ID_AAC);
    if (!aencoder) {
        return ENCODE_ERR_ENCODER;
    }

    char afilter_descr[256];
    snprintf(afilter_descr, sizeof(afilter_descr), EXPORT_AUDIO_FILTER_FORMAT,
             (double)target_lufs);
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
