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

#include <string.h>

#include <libavformat/avformat.h>
#include <libavutil/dict.h>
#include <libavutil/pixdesc.h>

uint32_t avbridge_version(void) {
    return avformat_version();
}

ProbeStatus avbridge_probe(const char *path, ProbeInfo *out) {
    AVFormatContext *fmt_ctx = NULL;

    switch (open_input(path, &fmt_ctx)) {
    case -1:
        return PROBE_ERR_OPEN;
    case -2:
        return PROBE_ERR_STREAM_INFO;
    }

    AVStream *video = NULL;
    AVStream *audio = NULL;
    int audio_count = 0;
    for (unsigned int i = 0; i < fmt_ctx->nb_streams; i++) {
        AVStream *s = fmt_ctx->streams[i];
        if (!video && s->codecpar->codec_type == AVMEDIA_TYPE_VIDEO) {
            video = s;
        } else if (s->codecpar->codec_type == AVMEDIA_TYPE_AUDIO) {
            if (!audio) audio = s;
            audio_count++;
        }
    }

    AVStream *stream = video ? video : audio;
    if (!stream) {
        avformat_close_input(&fmt_ctx);
        return PROBE_ERR_NO_MEDIA_STREAM;
    }

    memset(out, 0, sizeof(*out));
    out->has_video = video ? 1 : 0;
    out->has_audio = audio ? 1 : 0;

    double duration_secs = 0.0;
    if (fmt_ctx->duration != (int64_t)AV_NOPTS_VALUE) {
        duration_secs = (double)fmt_ctx->duration / AV_TIME_BASE;
    } else if (stream->duration != (int64_t)AV_NOPTS_VALUE) {
        duration_secs = stream->duration * av_q2d(stream->time_base);
    }
    out->duration_secs = duration_secs;

    const char *codec_name = avcodec_get_name(stream->codecpar->codec_id);
    strncpy(out->codec_name, codec_name, sizeof(out->codec_name) - 1);

    int64_t bit_rate = fmt_ctx->bit_rate;
    if (bit_rate <= 0) {
        bit_rate = stream->codecpar->bit_rate;
    }
    out->bit_rate = bit_rate > 0 ? bit_rate : 0;

    out->pixel_format = -1;
    {
        int described = 0;
        for (unsigned int i = 0; i < fmt_ctx->nb_streams && described < PROBE_MAX_AUDIO_STREAMS;
             i++) {
            AVStream *s = fmt_ctx->streams[i];
            if (s->codecpar->codec_type != AVMEDIA_TYPE_AUDIO) continue;
            out->audio_streams[described].stream_index = (int)i;
            out->audio_streams[described].channels = s->codecpar->ch_layout.nb_channels;
            out->audio_streams[described].sample_rate_hz = s->codecpar->sample_rate;
            strncpy(out->audio_streams[described].codec_name,
                    avcodec_get_name(s->codecpar->codec_id),
                    sizeof(out->audio_streams[described].codec_name) - 1);
            AVDictionaryEntry *title = av_dict_get(s->metadata, "title", NULL, 0);
            if (title) {
                strncpy(out->audio_streams[described].title, title->value,
                        sizeof(out->audio_streams[described].title) - 1);
            }
            AVDictionaryEntry *language = av_dict_get(s->metadata, "language", NULL, 0);
            if (language) {
                strncpy(out->audio_streams[described].language, language->value,
                        sizeof(out->audio_streams[described].language) - 1);
            }
            described++;
        }
        out->audio_stream_count = audio_count;
    }

    if (video) {
        out->pixel_format = stream->codecpar->format;
        const AVPixFmtDescriptor *descriptor = av_pix_fmt_desc_get(stream->codecpar->format);
        out->bit_depth = descriptor ? descriptor->comp[0].depth : 0;
        out->color_space = stream->codecpar->color_space;
        out->color_primaries = stream->codecpar->color_primaries;
        out->color_transfer = stream->codecpar->color_trc;
        out->color_full_range = stream->codecpar->color_range == AVCOL_RANGE_JPEG;
        out->width = stream->codecpar->width;
        out->height = stream->codecpar->height;
        AVRational fps = stream->avg_frame_rate.num ? stream->avg_frame_rate : stream->r_frame_rate;
        out->fps_num = fps.num;
        out->fps_den = fps.den;
    } else {
        out->sample_rate_hz = stream->codecpar->sample_rate;
    }

    avformat_close_input(&fmt_ctx);
    return PROBE_OK;
}

RemuxStatus avbridge_remux_copy(const char *in_path, const char *out_path) {
    AVFormatContext *in_ctx = NULL;
    AVFormatContext *out_ctx = NULL;
    int *stream_mapping = NULL;
    RemuxStatus status = REMUX_OK;

    switch (open_input(in_path, &in_ctx)) {
    case -1:
        return REMUX_ERR_OPEN_INPUT;
    case -2:
        return REMUX_ERR_STREAM_INFO;
    }

    avformat_alloc_output_context2(&out_ctx, NULL, NULL, out_path);
    if (!out_ctx) {
        avformat_close_input(&in_ctx);
        return REMUX_ERR_ALLOC_OUTPUT;
    }

    stream_mapping = av_calloc(in_ctx->nb_streams, sizeof(*stream_mapping));
    if (!stream_mapping) {
        avformat_free_context(out_ctx);
        avformat_close_input(&in_ctx);
        return REMUX_ERR_ALLOC_OUTPUT;
    }

    int out_stream_count = 0;
    for (unsigned int i = 0; i < in_ctx->nb_streams; i++) {
        AVStream *in_stream = in_ctx->streams[i];
        AVCodecParameters *in_codecpar = in_stream->codecpar;

        if (in_codecpar->codec_type != AVMEDIA_TYPE_VIDEO &&
            in_codecpar->codec_type != AVMEDIA_TYPE_AUDIO) {
            stream_mapping[i] = -1;
            continue;
        }

        AVStream *out_stream = avformat_new_stream(out_ctx, NULL);
        if (!out_stream || avcodec_parameters_copy(out_stream->codecpar, in_codecpar) < 0) {
            status = REMUX_ERR_NEW_STREAM;
            goto cleanup;
        }
        out_stream->codecpar->codec_tag = 0;
        out_stream->time_base = in_stream->time_base;
        stream_mapping[i] = out_stream_count++;
    }

    if (!(out_ctx->oformat->flags & AVFMT_NOFILE)) {
        if (avio_open(&out_ctx->pb, out_path, AVIO_FLAG_WRITE) < 0) {
            status = REMUX_ERR_OPEN_OUTPUT;
            goto cleanup;
        }
    }

    if (avformat_write_header(out_ctx, NULL) < 0) {
        status = REMUX_ERR_WRITE_HEADER;
        goto cleanup_output_io;
    }

    {
        AVPacket *pkt = av_packet_alloc();
        if (!pkt) {
            status = REMUX_ERR_WRITE_FRAME;
            goto cleanup_output_io;
        }

        while (av_read_frame(in_ctx, pkt) >= 0) {
            AVStream *in_stream = in_ctx->streams[pkt->stream_index];
            int mapped_index = stream_mapping[pkt->stream_index];
            if (mapped_index < 0) {
                av_packet_unref(pkt);
                continue;
            }

            AVStream *out_stream = out_ctx->streams[mapped_index];
            pkt->stream_index = mapped_index;
            av_packet_rescale_ts(pkt, in_stream->time_base, out_stream->time_base);
            pkt->pos = -1;

            if (av_interleaved_write_frame(out_ctx, pkt) < 0) {
                status = REMUX_ERR_WRITE_FRAME;
                break;
            }
        }
        av_packet_free(&pkt);

        if (status == REMUX_OK) {
            av_write_trailer(out_ctx);
        }
    }

cleanup_output_io:
    if (out_ctx && !(out_ctx->oformat->flags & AVFMT_NOFILE)) {
        avio_closep(&out_ctx->pb);
    }
cleanup:
    av_freep(&stream_mapping);
    if (out_ctx) {
        avformat_free_context(out_ctx);
    }
    avformat_close_input(&in_ctx);
    return status;
}
