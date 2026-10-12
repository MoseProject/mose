// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

#include "bridge.h"
#include "bridge_internal.h"

#include <math.h>

#include <libavcodec/avcodec.h>
#include <libavfilter/avfilter.h>
#include <libavfilter/buffersink.h>
#include <libavformat/avformat.h>
#include <libavutil/avstring.h>
#include <libavutil/channel_layout.h>
#include <libavutil/opt.h>

static int first_stream_index(AVFormatContext *ctx, enum AVMediaType type) {
    for (unsigned i = 0; i < ctx->nb_streams; i++) {
        if (ctx->streams[i]->codecpar->codec_type == type) return (int)i;
    }
    return -1;
}

static int source_has_audio(const char *path, int *open_failed) {
    AVFormatContext *ctx = NULL;
    *open_failed = 0;
    if (open_input(path, &ctx) != 0) {
        *open_failed = 1;
        return 0;
    }
    int found = first_stream_index(ctx, AVMEDIA_TYPE_AUDIO) >= 0;
    avformat_close_input(&ctx);
    return found;
}

static int create_filter(AVFilterGraph *graph, AVFilterContext **ctx, const char *filter_name,
                         const char *instance_name, const char *args) {
    const AVFilter *filter = avfilter_get_by_name(filter_name);
    if (!filter) return AVERROR_FILTER_NOT_FOUND;
    return avfilter_graph_create_filter(ctx, filter, instance_name, args, NULL, graph);
}

/* AudioSegment::duck_role values (see bridge.h) -- P2 item 6, "audio ducking". */
#define DUCK_ROLE_NORMAL 0
#define DUCK_ROLE_TRIGGER 1 /* AudioRole::Mic -- the commentary that should duck music under it */
#define DUCK_ROLE_TARGET 2  /* AudioRole::Music -- gets ducked when a trigger branch is loud */

static int build_mix_graph(const AudioSegment *segments, const uint8_t *valid, int segment_count,
                           int valid_count, double timeline_duration_secs, float target_lufs,
                           const char *sample_fmt, AVFilterGraph **out_graph,
                           AVFilterContext **out_sink) {
    int ret = 0;
    int branch = 0;
    AVFilterGraph *graph = avfilter_graph_alloc();
    AVFilterContext *final_mix = NULL, *mix_source = NULL, *noise = NULL, *loudnorm = NULL;
    AVFilterContext *limiter = NULL, *final_trim = NULL, *format = NULL, *sink = NULL;
    AVFilterContext **branch_ctx = NULL;
    int *branch_duck_role = NULL;
    if (!graph) return AVERROR(ENOMEM);

    branch_ctx = av_calloc((size_t)valid_count, sizeof(*branch_ctx));
    branch_duck_role = av_calloc((size_t)valid_count, sizeof(*branch_duck_role));
    if (!branch_ctx || !branch_duck_role) {
        ret = AVERROR(ENOMEM);
        goto fail;
    }

    char args[256];

    for (int i = 0; i < segment_count; i++) {
        if (!valid[i]) continue;
        const AudioSegment *seg = &segments[i];
        char name[64];
        AVFilterContext *movie = NULL, *trim = NULL, *pts = NULL;
        AVFilterContext *tempo = NULL, *volume = NULL, *delay = NULL;

        snprintf(name, sizeof(name), "movie_%d", branch);
        const AVFilter *movie_filter = avfilter_get_by_name("amovie");
        if (!movie_filter) {
            ret = AVERROR_FILTER_NOT_FOUND;
            goto fail;
        }
        movie = avfilter_graph_alloc_filter(graph, movie_filter, name);
        if (!movie) {
            ret = AVERROR(ENOMEM);
            goto fail;
        }
        ret = av_opt_set(movie, "filename", seg->source_path, AV_OPT_SEARCH_CHILDREN);
        if (ret < 0 || (ret = avfilter_init_str(movie, NULL)) < 0) goto fail;

        snprintf(name, sizeof(name), "trim_%d", branch);
        snprintf(args, sizeof(args), "start=%.9f:end=%.9f", seg->source_in_secs,
                 seg->source_out_secs);
        if ((ret = create_filter(graph, &trim, "atrim", name, args)) < 0) goto fail;

        snprintf(name, sizeof(name), "pts_%d", branch);
        if ((ret = create_filter(graph, &pts, "asetpts", name, "PTS-STARTPTS")) < 0) goto fail;

        double speed = seg->speed_factor > 0.0f ? seg->speed_factor : 1.0;
        if (speed < 0.5) speed = 0.5;
        if (speed > 100.0) speed = 100.0;
        snprintf(name, sizeof(name), "tempo_%d", branch);
        snprintf(args, sizeof(args), "tempo=%.9f", speed);
        if ((ret = create_filter(graph, &tempo, "atempo", name, args)) < 0) goto fail;

        snprintf(name, sizeof(name), "volume_%d", branch);
        if (seg->gain_keyframe_expr && seg->gain_keyframe_expr[0] != '\0') {
            /* Expression mode: a keyframed gain ramp built by
               avcore::keyframe::gain_filter_db_expr, already a linear multiplier (volume's
               `eval=frame` expressions are linear, not dB -- see that function's own doc
               comment) -- an arbitrary-length string, unlike the fixed-size `args` buffer used
               for every other filter here, so it's built with av_asprintf instead of snprintf. */
            char *expr_args = av_asprintf("volume=%s:eval=frame", seg->gain_keyframe_expr);
            if (!expr_args) {
                ret = AVERROR(ENOMEM);
                goto fail;
            }
            ret = create_filter(graph, &volume, "volume", name, expr_args);
            av_free(expr_args);
        } else {
            snprintf(args, sizeof(args), "volume=%.6fdB", (double)seg->gain_db);
            ret = create_filter(graph, &volume, "volume", name, args);
        }
        if (ret < 0) goto fail;

        double delay_ms = fmax(0.0, seg->timeline_start_secs * 1000.0);
        snprintf(name, sizeof(name), "delay_%d", branch);
        snprintf(args, sizeof(args), "delays=%.3f:all=1", delay_ms);
        if ((ret = create_filter(graph, &delay, "adelay", name, args)) < 0) goto fail;

        /* CF-03 "Gameplay Voice" cleanup (see bridge.h's AudioSegment doc comment): three extra
           stages spliced between this branch's own volume and delay when enabled, ahead of the
           final afftdn/loudnorm/limiter mastering pass below (which runs on the whole mix, not
           per branch) -- so a Mic branch gets cleaned up before it's mixed with everything else,
           not just at the very end. */
        /* Fade handles, right after the branch's own volume. Output seconds, which is what the
           branch's time axis is after the tempo stage. */
        AVFilterContext *fade_tail = volume;
        double branch_out_secs = (seg->source_out_secs - seg->source_in_secs) / speed;
        if (seg->fade_in_secs > 0.0f) {
            AVFilterContext *fade = NULL;
            snprintf(name, sizeof(name), "fade_in_%d", branch);
            snprintf(args, sizeof(args), "t=in:st=0:d=%.4f", (double)seg->fade_in_secs);
            if ((ret = create_filter(graph, &fade, "afade", name, args)) < 0) goto fail;
            if ((ret = avfilter_link(fade_tail, 0, fade, 0)) < 0) goto fail;
            fade_tail = fade;
        }
        if (seg->fade_out_secs > 0.0f && branch_out_secs > 0.0) {
            AVFilterContext *fade = NULL;
            double fade_out = fmin((double)seg->fade_out_secs, branch_out_secs);
            snprintf(name, sizeof(name), "fade_out_%d", branch);
            snprintf(args, sizeof(args), "t=out:st=%.4f:d=%.4f", branch_out_secs - fade_out,
                     fade_out);
            if ((ret = create_filter(graph, &fade, "afade", name, args)) < 0) goto fail;
            if ((ret = avfilter_link(fade_tail, 0, fade, 0)) < 0) goto fail;
            fade_tail = fade;
        }

        AVFilterContext *vc_highpass = NULL, *vc_denoise = NULL, *vc_compressor = NULL;
        AVFilterContext *voice_cleanup_tail = fade_tail;
        if (seg->voice_cleanup_enabled) {
            snprintf(name, sizeof(name), "vc_highpass_%d", branch);
            if ((ret = create_filter(graph, &vc_highpass, "highpass", name, "f=80")) < 0) goto fail;

            snprintf(name, sizeof(name), "vc_denoise_%d", branch);
            snprintf(args, sizeof(args), "nf=%.3f", (double)seg->voice_cleanup_noise_floor_db);
            if ((ret = create_filter(graph, &vc_denoise, "afftdn", name, args)) < 0) goto fail;

            snprintf(name, sizeof(name), "vc_compressor_%d", branch);
            snprintf(args, sizeof(args),
                     "threshold=%.3fdB:ratio=%.3f:attack=10:release=250:makeup=1.5",
                     (double)seg->voice_cleanup_compressor_threshold_db,
                     (double)seg->voice_cleanup_compressor_ratio);
            if ((ret = create_filter(graph, &vc_compressor, "acompressor", name, args)) < 0)
                goto fail;

            snprintf(name, sizeof(name), "vc_limiter_%d", branch);
            snprintf(args, sizeof(args), "limit=%.6f", (double)seg->voice_cleanup_ceiling_linear);
            if ((ret = create_filter(graph, &voice_cleanup_tail, "alimiter", name, args)) < 0)
                goto fail;

            if ((ret = avfilter_link(fade_tail, 0, vc_highpass, 0)) < 0 ||
                (ret = avfilter_link(vc_highpass, 0, vc_denoise, 0)) < 0 ||
                (ret = avfilter_link(vc_denoise, 0, vc_compressor, 0)) < 0 ||
                (ret = avfilter_link(vc_compressor, 0, voice_cleanup_tail, 0)) < 0)
                goto fail;
        }

        /* Stereo balance: the side the pan moves away from falls off as a quarter sine, the other
           side stays at unity, so a centred branch is left alone. */
        AVFilterContext *pan_tail = voice_cleanup_tail;
        if (seg->pan != 0.0f) {
            double pan = fmax(-1.0, fmin(1.0, (double)seg->pan));
            double left = pan > 0.0 ? cos(pan * M_PI / 2.0) : 1.0;
            double right = pan < 0.0 ? cos(-pan * M_PI / 2.0) : 1.0;
            AVFilterContext *to_stereo = NULL, *pan_filter = NULL;
            /* A mono source is placed in the stereo field first. */
            snprintf(name, sizeof(name), "pan_stereo_%d", branch);
            if ((ret = create_filter(graph, &to_stereo, "aformat", name,
                                     "channel_layouts=stereo")) < 0)
                goto fail;
            snprintf(name, sizeof(name), "pan_%d", branch);
            snprintf(args, sizeof(args), "stereo|c0=%.6f*c0|c1=%.6f*c1", left, right);
            if ((ret = create_filter(graph, &pan_filter, "pan", name, args)) < 0) goto fail;
            if ((ret = avfilter_link(voice_cleanup_tail, 0, to_stereo, 0)) < 0 ||
                (ret = avfilter_link(to_stereo, 0, pan_filter, 0)) < 0)
                goto fail;
            pan_tail = pan_filter;
        }

        /* The strip's tone and dynamics, in the order a mixer strip has them: high-pass, low-pass,
           low shelf, mid bell, high shelf, compressor. Only the stages that are not neutral exist,
           so a neutral strip leaves the graph as it was. */
        AVFilterContext *strip_tail = pan_tail;
        {
            struct {
                int active;
                const char *filter;
                char args[96];
            } stages[7] = {{0}};
            int count = 0;
            if (seg->highpass_hz > 0.0f) {
                stages[count].filter = "highpass";
                snprintf(stages[count].args, sizeof(stages[count].args), "f=%.3f",
                         (double)seg->highpass_hz);
                stages[count++].active = 1;
            }
            if (seg->lowpass_hz > 0.0f) {
                stages[count].filter = "lowpass";
                snprintf(stages[count].args, sizeof(stages[count].args), "f=%.3f",
                         (double)seg->lowpass_hz);
                stages[count++].active = 1;
            }
            if (seg->eq_low_db != 0.0f) {
                stages[count].filter = "bass";
                snprintf(stages[count].args, sizeof(stages[count].args), "g=%.3f:f=120",
                         (double)seg->eq_low_db);
                stages[count++].active = 1;
            }
            if (seg->eq_mid_db != 0.0f) {
                stages[count].filter = "equalizer";
                snprintf(stages[count].args, sizeof(stages[count].args), "f=1000:t=q:w=1:g=%.3f",
                         (double)seg->eq_mid_db);
                stages[count++].active = 1;
            }
            if (seg->eq_high_db != 0.0f) {
                stages[count].filter = "treble";
                snprintf(stages[count].args, sizeof(stages[count].args), "g=%.3f:f=6000",
                         (double)seg->eq_high_db);
                stages[count++].active = 1;
            }
            if (seg->comp_ratio > 1.0f) {
                stages[count].filter = "acompressor";
                snprintf(stages[count].args, sizeof(stages[count].args),
                         "threshold=%.3fdB:ratio=%.3f:attack=10:release=250",
                         (double)seg->comp_threshold_db, (double)seg->comp_ratio);
                stages[count++].active = 1;
            }
            for (int stage = 0; stage < count; stage++) {
                AVFilterContext *filter = NULL;
                snprintf(name, sizeof(name), "strip_%s_%d", stages[stage].filter, branch);
                if ((ret = create_filter(graph, &filter, stages[stage].filter, name,
                                         stages[stage].args)) < 0)
                    goto fail;
                if ((ret = avfilter_link(strip_tail, 0, filter, 0)) < 0) goto fail;
                strip_tail = filter;
            }
        }

        if ((ret = avfilter_link(movie, 0, trim, 0)) < 0 ||
            (ret = avfilter_link(trim, 0, pts, 0)) < 0 ||
            (ret = avfilter_link(pts, 0, tempo, 0)) < 0 ||
            (ret = avfilter_link(tempo, 0, volume, 0)) < 0 ||
            (ret = avfilter_link(strip_tail, 0, delay, 0)) < 0)
            goto fail;

        branch_ctx[branch] = delay;
        branch_duck_role[branch] = seg->duck_role;
        branch++;
    }

    /* Audio ducking (P2 item 6, spec/architecture/differentiators.md) -- auto-lowers music
       under commentary, Premiere/CapCut/DaVinci's "Auto Ducking". Opt-in and additive: a
       project that never tags a track's AudioRole (defaults to Unspecified) has zero DUCK_ROLE_
       TARGET/TRIGGER branches, so it falls straight to the original flat amix below, byte-
       identical to this function's behavior before ducking existed. Only routes through
       sidechaincompress when there's at least one branch of *each* role -- ducking nothing
       against nothing is meaningless. */
    int target_count = 0, trigger_count = 0;
    for (int i = 0; i < branch; i++) {
        if (branch_duck_role[i] == DUCK_ROLE_TARGET)
            target_count++;
        else if (branch_duck_role[i] == DUCK_ROLE_TRIGGER)
            trigger_count++;
    }

    if (target_count > 0 && trigger_count > 0) {
        AVFilterContext *music_mix = NULL, *trigger_mix = NULL, *duck = NULL;
        AVFilterContext *music_source = NULL, *trigger_source = NULL;

        /* Each trigger (mic) branch feeds the sidechain control input *and* still has to play
           in the final mix -- but a filter output pad can only ever be consumed once
           (avfilter_link on an already-linked source pad fails with AVERROR(EINVAL)). Give
           every trigger branch its own `asplit` so it has two independent output pads: pad 0
           goes toward `duck`'s sidechain input, pad 1 goes straight to the final mix. Target
           and normal branches each have exactly one consumer already, so they need no split. */
        AVFilterContext **trigger_split = av_calloc((size_t)branch, sizeof(*trigger_split));
        if (!trigger_split) {
            ret = AVERROR(ENOMEM);
            goto fail;
        }
        for (int i = 0; i < branch; i++) {
            if (branch_duck_role[i] != DUCK_ROLE_TRIGGER) continue;
            char split_name[64];
            snprintf(split_name, sizeof(split_name), "trigger_split_%d", i);
            if ((ret = create_filter(graph, &trigger_split[i], "asplit", split_name, NULL)) < 0) {
                av_free(trigger_split);
                goto fail;
            }
            if ((ret = avfilter_link(branch_ctx[i], 0, trigger_split[i], 0)) < 0) {
                av_free(trigger_split);
                goto fail;
            }
        }

        if (target_count == 1) {
            for (int i = 0; i < branch; i++)
                if (branch_duck_role[i] == DUCK_ROLE_TARGET) {
                    music_source = branch_ctx[i];
                    break;
                }
        } else {
            snprintf(args, sizeof(args),
                     "inputs=%d:duration=longest:dropout_transition=0:normalize=0", target_count);
            if ((ret = create_filter(graph, &music_mix, "amix", "music_mix", args)) < 0) {
                av_free(trigger_split);
                goto fail;
            }
            int pad = 0;
            for (int i = 0; i < branch; i++) {
                if (branch_duck_role[i] != DUCK_ROLE_TARGET) continue;
                if ((ret = avfilter_link(branch_ctx[i], 0, music_mix, pad++)) < 0) {
                    av_free(trigger_split);
                    goto fail;
                }
            }
            music_source = music_mix;
        }

        if (trigger_count == 1) {
            for (int i = 0; i < branch; i++)
                if (branch_duck_role[i] == DUCK_ROLE_TRIGGER) {
                    trigger_source = trigger_split[i];
                    break;
                }
        } else {
            snprintf(args, sizeof(args),
                     "inputs=%d:duration=longest:dropout_transition=0:normalize=0", trigger_count);
            if ((ret = create_filter(graph, &trigger_mix, "amix", "trigger_mix", args)) < 0) {
                av_free(trigger_split);
                goto fail;
            }
            int pad = 0;
            for (int i = 0; i < branch; i++) {
                if (branch_duck_role[i] != DUCK_ROLE_TRIGGER) continue;
                if ((ret = avfilter_link(trigger_split[i], 0, trigger_mix, pad++)) < 0) {
                    av_free(trigger_split);
                    goto fail;
                }
            }
            trigger_source = trigger_mix;
        }

        /* Pad 0 is the main signal being compressed (the music), pad 1 is the sidechain
           control signal (the mic) -- sidechaincompress's own well-established, long-stable
           2-input pad convention (confirmed against the linked libavfilter build: nb_inputs=2,
           distinct from acompressor's single-input nb_inputs=1 variant of the same DSP). */
        if ((ret = create_filter(graph, &duck, "sidechaincompress", "duck",
                                 "threshold=0.05:ratio=8:attack=5:release=250:makeup=1:"
                                 "link=average:detection=rms")) < 0) {
            av_free(trigger_split);
            goto fail;
        }
        /* The sidechain input ends with the commentary, and the compressor would end its output
           with it, cutting the music short. Padding the trigger with silence keeps the music
           going to its own end; the final trim bounds the whole mix. */
        AVFilterContext *trigger_pad = NULL;
        if ((ret = create_filter(graph, &trigger_pad, "apad", "trigger_pad", NULL)) < 0 ||
            (ret = avfilter_link(trigger_source, 0, trigger_pad, 0)) < 0) {
            av_free(trigger_split);
            goto fail;
        }
        if ((ret = avfilter_link(music_source, 0, duck, 0)) < 0 ||
            (ret = avfilter_link(trigger_pad, 0, duck, 1)) < 0) {
            av_free(trigger_split);
            goto fail;
        }

        /* Final mix: every non-music branch (Normal + Trigger, so the mic/commentary itself
           still plays in the output, not just as a control signal) plus the one ducked-music
           signal -- a music branch never appears here directly, only through `duck`. Trigger
           branches use their split's second output pad, since pad 0 already feeds `duck`. */
        int final_inputs = (branch - target_count) + 1;
        snprintf(args, sizeof(args), "inputs=%d:duration=longest:dropout_transition=0:normalize=0",
                 final_inputs);
        if ((ret = create_filter(graph, &final_mix, "amix", "mix", args)) < 0) {
            av_free(trigger_split);
            goto fail;
        }
        int pad = 0;
        for (int i = 0; i < branch; i++) {
            if (branch_duck_role[i] == DUCK_ROLE_TARGET) continue;
            AVFilterContext *src =
                branch_duck_role[i] == DUCK_ROLE_TRIGGER ? trigger_split[i] : branch_ctx[i];
            int src_pad = branch_duck_role[i] == DUCK_ROLE_TRIGGER ? 1 : 0;
            if ((ret = avfilter_link(src, src_pad, final_mix, pad++)) < 0) {
                av_free(trigger_split);
                goto fail;
            }
        }
        av_free(trigger_split);
        if ((ret = avfilter_link(duck, 0, final_mix, pad)) < 0) goto fail;
        mix_source = final_mix;
    } else {
        snprintf(args, sizeof(args), "inputs=%d:duration=longest:dropout_transition=0:normalize=0",
                 branch);
        if ((ret = create_filter(graph, &final_mix, "amix", "mix", args)) < 0) goto fail;
        for (int i = 0; i < branch; i++) {
            if ((ret = avfilter_link(branch_ctx[i], 0, final_mix, i)) < 0) goto fail;
        }
        mix_source = final_mix;
    }

    av_free(branch_ctx);
    av_free(branch_duck_role);
    branch_ctx = NULL;
    branch_duck_role = NULL;

    const int normalize = !isnan(target_lufs);
    if (normalize) {
        if ((ret = create_filter(graph, &noise, "afftdn", "noise", NULL)) < 0) goto fail;
        snprintf(args, sizeof(args), MASTERING_LOUDNORM_ARGS_FORMAT, (double)target_lufs);
        if ((ret = create_filter(graph, &loudnorm, "loudnorm", "loudnorm", args)) < 0) goto fail;
    }
    if ((ret = create_filter(graph, &limiter, "alimiter", "limiter", MASTERING_LIMITER_ARGS)) < 0)
        goto fail;
    snprintf(args, sizeof(args), "end=%.9f", timeline_duration_secs);
    if ((ret = create_filter(graph, &final_trim, "atrim", "timeline_trim", args)) < 0) goto fail;
    snprintf(args, sizeof(args), "sample_fmts=%s:sample_rates=48000:channel_layouts=stereo",
             sample_fmt);
    if ((ret = create_filter(graph, &format, "aformat", "format", args)) < 0) goto fail;
    if ((ret = create_filter(graph, &sink, "abuffersink", "out", NULL)) < 0) goto fail;

    if (normalize) {
        if ((ret = avfilter_link(mix_source, 0, noise, 0)) < 0 ||
            (ret = avfilter_link(noise, 0, loudnorm, 0)) < 0 ||
            (ret = avfilter_link(loudnorm, 0, limiter, 0)) < 0)
            goto fail;
    } else if ((ret = avfilter_link(mix_source, 0, limiter, 0)) < 0) {
        goto fail;
    }
    if ((ret = avfilter_link(limiter, 0, final_trim, 0)) < 0 ||
        (ret = avfilter_link(final_trim, 0, format, 0)) < 0 ||
        (ret = avfilter_link(format, 0, sink, 0)) < 0 ||
        (ret = avfilter_graph_config(graph, NULL)) < 0)
        goto fail;

    *out_graph = graph;
    *out_sink = sink;
    return 0;

fail:
    av_free(branch_ctx);
    av_free(branch_duck_role);
    avfilter_graph_free(&graph);
    return ret;
}

/* What each output format needs from the shared mix: the encoder, the sample format the graph
   ends in, and the encoder's own settings. Lossless and PCM formats take 24-bit integers. */
typedef struct {
    enum AVCodecID codec_id;
    const char *encoder_name; /* preferred encoder when the codec id has several, or NULL */
    enum AVSampleFormat sample_fmt;
    const char *filter_fmt; /* the same format as the aformat filter spells it */
    int64_t bit_rate;       /* 0 for lossless */
    int bits_per_raw_sample;
} AudioMixFormatSpec;

static int audio_mix_format_spec(int format, AudioMixFormatSpec *spec) {
    switch (format) {
    case AUDIO_MIX_FORMAT_AAC:
        *spec = (AudioMixFormatSpec){AV_CODEC_ID_AAC,          NULL, AV_SAMPLE_FMT_FLTP, "fltp",
                                     EXPORT_AUDIO_BITRATE_BPS, 0};
        return 0;
    case AUDIO_MIX_FORMAT_WAV24:
        *spec = (AudioMixFormatSpec){AV_CODEC_ID_PCM_S24LE, NULL, AV_SAMPLE_FMT_S32, "s32", 0, 24};
        return 0;
    case AUDIO_MIX_FORMAT_FLAC:
        *spec = (AudioMixFormatSpec){AV_CODEC_ID_FLAC, NULL, AV_SAMPLE_FMT_S32, "s32", 0, 24};
        return 0;
    case AUDIO_MIX_FORMAT_MP3:
        *spec = (AudioMixFormatSpec){AV_CODEC_ID_MP3, "libmp3lame", AV_SAMPLE_FMT_FLTP,
                                     "fltp",          256000,       0};
        return 0;
    case AUDIO_MIX_FORMAT_OPUS:
        *spec =
            (AudioMixFormatSpec){AV_CODEC_ID_OPUS, "libopus", AV_SAMPLE_FMT_FLT, "flt", 160000, 0};
        return 0;
    default:
        return -1;
    }
}

AudioMixStatus avbridge_mix_audio_timeline(const AudioSegment *segments, int segment_count,
                                           double timeline_duration_secs, const char *out_path,
                                           float target_lufs, int format, const uint8_t *cancel) {
    if (!segments || segment_count <= 0) return AUDIO_MIX_ERR_NO_AUDIO;
    AudioMixFormatSpec spec;
    if (audio_mix_format_spec(format, &spec) < 0) return AUDIO_MIX_ERR_ENCODER;

    AudioMixStatus status = AUDIO_MIX_OK;
    uint8_t *valid = av_calloc((size_t)segment_count, sizeof(*valid));
    AVFilterGraph *graph = NULL;
    AVFilterContext *sink = NULL;
    AVFormatContext *out_ctx = NULL;
    AVCodecContext *enc_ctx = NULL;
    AVStream *out_stream = NULL;
    AVFrame *frame = NULL;
    AVPacket *pkt = NULL;
    int valid_count = 0;
    if (!valid) return AUDIO_MIX_ERR_FILTER_GRAPH;

    for (int i = 0; i < segment_count; i++) {
        int open_failed = 0;
        if (source_has_audio(segments[i].source_path, &open_failed)) {
            valid[i] = 1;
            valid_count++;
        } else if (open_failed) {
            status = AUDIO_MIX_ERR_OPEN_INPUT;
            goto cleanup;
        }
    }
    if (valid_count == 0) {
        status = AUDIO_MIX_ERR_NO_AUDIO;
        goto cleanup;
    }

    if (build_mix_graph(segments, valid, segment_count, valid_count, timeline_duration_secs,
                        target_lufs, spec.filter_fmt, &graph, &sink) < 0) {
        status = AUDIO_MIX_ERR_FILTER_GRAPH;
        goto cleanup;
    }

    avformat_alloc_output_context2(&out_ctx, NULL, NULL, out_path);
    if (!out_ctx) {
        status = AUDIO_MIX_ERR_ALLOC_OUTPUT;
        goto cleanup;
    }
    const AVCodec *encoder = NULL;
    if (spec.encoder_name) encoder = avcodec_find_encoder_by_name(spec.encoder_name);
    if (!encoder) encoder = avcodec_find_encoder(spec.codec_id);
    if (!encoder) {
        status = AUDIO_MIX_ERR_ENCODER;
        goto cleanup;
    }
    enc_ctx = avcodec_alloc_context3(encoder);
    if (!enc_ctx) {
        status = AUDIO_MIX_ERR_ENCODER;
        goto cleanup;
    }
    enc_ctx->sample_rate = EXPORT_AUDIO_SAMPLE_RATE;
    enc_ctx->sample_fmt = spec.sample_fmt;
    av_channel_layout_default(&enc_ctx->ch_layout, 2);
    if (spec.bit_rate > 0) enc_ctx->bit_rate = spec.bit_rate;
    if (spec.bits_per_raw_sample > 0) enc_ctx->bits_per_raw_sample = spec.bits_per_raw_sample;
    enc_ctx->time_base = (AVRational){1, EXPORT_AUDIO_SAMPLE_RATE};
    if (out_ctx->oformat->flags & AVFMT_GLOBALHEADER) enc_ctx->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
    if (avcodec_open2(enc_ctx, encoder, NULL) < 0) {
        status = AUDIO_MIX_ERR_ENCODER;
        goto cleanup;
    }
    if (enc_ctx->frame_size > 0) av_buffersink_set_frame_size(sink, (unsigned)enc_ctx->frame_size);

    out_stream = avformat_new_stream(out_ctx, NULL);
    if (!out_stream || avcodec_parameters_from_context(out_stream->codecpar, enc_ctx) < 0) {
        status = AUDIO_MIX_ERR_ALLOC_OUTPUT;
        goto cleanup;
    }
    out_stream->time_base = enc_ctx->time_base;
    if (!(out_ctx->oformat->flags & AVFMT_NOFILE) &&
        avio_open(&out_ctx->pb, out_path, AVIO_FLAG_WRITE) < 0) {
        status = AUDIO_MIX_ERR_OPEN_OUTPUT;
        goto cleanup;
    }
    if (avformat_write_header(out_ctx, NULL) < 0) {
        status = AUDIO_MIX_ERR_WRITE_HEADER;
        goto cleanup;
    }

    frame = av_frame_alloc();
    pkt = av_packet_alloc();
    if (!frame || !pkt) {
        status = AUDIO_MIX_ERR_PIPELINE;
        goto cleanup;
    }
    AVRational sink_tb = av_buffersink_get_time_base(sink);
    while (1) {
        if (cancel && *cancel) {
            status = AUDIO_MIX_CANCELLED;
            goto cleanup;
        }
        int ret = av_buffersink_get_frame(sink, frame);
        if (ret == AVERROR_EOF) break;
        if (ret == AVERROR(EAGAIN)) continue;
        if (ret < 0) {
            status = AUDIO_MIX_ERR_PIPELINE;
            goto cleanup;
        }
        frame->pts = av_rescale_q(frame->pts, sink_tb, enc_ctx->time_base);
        if (encode_write_packet(out_ctx, enc_ctx, out_stream, frame, pkt) < 0) {
            av_frame_unref(frame);
            status = AUDIO_MIX_ERR_PIPELINE;
            goto cleanup;
        }
        av_frame_unref(frame);
    }
    if (encode_write_packet(out_ctx, enc_ctx, out_stream, NULL, pkt) < 0) {
        status = AUDIO_MIX_ERR_PIPELINE;
        goto cleanup;
    }
    if (av_write_trailer(out_ctx) < 0) status = AUDIO_MIX_ERR_PIPELINE;

cleanup:
    av_packet_free(&pkt);
    av_frame_free(&frame);
    avcodec_free_context(&enc_ctx);
    avfilter_graph_free(&graph);
    if (out_ctx && !(out_ctx->oformat->flags & AVFMT_NOFILE) && out_ctx->pb)
        avio_closep(&out_ctx->pb);
    if (out_ctx) avformat_free_context(out_ctx);
    av_free(valid);
    return status;
}

static int read_next_stream_packet(AVFormatContext *ctx, int stream_index, AVPacket *pkt) {
    while (av_read_frame(ctx, pkt) >= 0) {
        if (pkt->stream_index == stream_index) return 1;
        av_packet_unref(pkt);
    }
    return 0;
}

static int64_t packet_ts(const AVPacket *pkt) {
    return pkt->dts != AV_NOPTS_VALUE ? pkt->dts : pkt->pts;
}

MediaMuxStatus avbridge_mux_video_audio(const char *video_path, const char *audio_path,
                                        const char *out_path) {
    AVFormatContext *video_ctx = NULL, *audio_ctx = NULL, *out_ctx = NULL;
    AVPacket *video_pkt = NULL, *audio_pkt = NULL;
    MediaMuxStatus status = MEDIA_MUX_OK;

    if (open_input(video_path, &video_ctx) != 0 || open_input(audio_path, &audio_ctx) != 0) {
        status = MEDIA_MUX_ERR_OPEN_INPUT;
        goto cleanup;
    }
    int video_idx = first_stream_index(video_ctx, AVMEDIA_TYPE_VIDEO);
    int audio_idx = first_stream_index(audio_ctx, AVMEDIA_TYPE_AUDIO);
    if (video_idx < 0 || audio_idx < 0) {
        status = MEDIA_MUX_ERR_MISSING_STREAM;
        goto cleanup;
    }

    avformat_alloc_output_context2(&out_ctx, NULL, NULL, out_path);
    if (!out_ctx) {
        status = MEDIA_MUX_ERR_ALLOC_OUTPUT;
        goto cleanup;
    }
    AVStream *video_out = avformat_new_stream(out_ctx, NULL);
    AVStream *audio_out = avformat_new_stream(out_ctx, NULL);
    if (!video_out || !audio_out ||
        avcodec_parameters_copy(video_out->codecpar, video_ctx->streams[video_idx]->codecpar) < 0 ||
        avcodec_parameters_copy(audio_out->codecpar, audio_ctx->streams[audio_idx]->codecpar) < 0) {
        status = MEDIA_MUX_ERR_NEW_STREAM;
        goto cleanup;
    }
    video_out->codecpar->codec_tag = 0;
    audio_out->codecpar->codec_tag = 0;
    video_out->time_base = video_ctx->streams[video_idx]->time_base;
    audio_out->time_base = audio_ctx->streams[audio_idx]->time_base;
    if (!(out_ctx->oformat->flags & AVFMT_NOFILE) &&
        avio_open(&out_ctx->pb, out_path, AVIO_FLAG_WRITE) < 0) {
        status = MEDIA_MUX_ERR_OPEN_OUTPUT;
        goto cleanup;
    }
    if (avformat_write_header(out_ctx, NULL) < 0) {
        status = MEDIA_MUX_ERR_WRITE_HEADER;
        goto cleanup;
    }

    video_pkt = av_packet_alloc();
    audio_pkt = av_packet_alloc();
    if (!video_pkt || !audio_pkt) {
        status = MEDIA_MUX_ERR_WRITE_FRAME;
        goto cleanup;
    }
    int have_video = read_next_stream_packet(video_ctx, video_idx, video_pkt);
    int have_audio = read_next_stream_packet(audio_ctx, audio_idx, audio_pkt);
    while (have_video || have_audio) {
        int write_video =
            have_video &&
            (!have_audio ||
             av_compare_ts(packet_ts(video_pkt), video_ctx->streams[video_idx]->time_base,
                           packet_ts(audio_pkt), audio_ctx->streams[audio_idx]->time_base) <= 0);
        AVPacket *pkt = write_video ? video_pkt : audio_pkt;
        AVStream *in_stream =
            write_video ? video_ctx->streams[video_idx] : audio_ctx->streams[audio_idx];
        AVStream *out_stream = write_video ? video_out : audio_out;
        pkt->stream_index = out_stream->index;
        av_packet_rescale_ts(pkt, in_stream->time_base, out_stream->time_base);
        pkt->pos = -1;
        if (av_interleaved_write_frame(out_ctx, pkt) < 0) {
            status = MEDIA_MUX_ERR_WRITE_FRAME;
            goto cleanup;
        }
        if (write_video)
            have_video = read_next_stream_packet(video_ctx, video_idx, video_pkt);
        else
            have_audio = read_next_stream_packet(audio_ctx, audio_idx, audio_pkt);
    }
    if (av_write_trailer(out_ctx) < 0) status = MEDIA_MUX_ERR_WRITE_FRAME;

cleanup:
    av_packet_free(&video_pkt);
    av_packet_free(&audio_pkt);
    if (out_ctx && !(out_ctx->oformat->flags & AVFMT_NOFILE) && out_ctx->pb)
        avio_closep(&out_ctx->pb);
    if (out_ctx) avformat_free_context(out_ctx);
    avformat_close_input(&video_ctx);
    avformat_close_input(&audio_ctx);
    return status;
}
