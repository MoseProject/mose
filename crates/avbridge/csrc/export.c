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
#include <libavfilter/buffersink.h>
#include <libavformat/avformat.h>

EncodeStatus avbridge_encode_export(const char *in_path, const char *out_path, float target_lufs,
                                    ProgressCallback progress_cb, void *progress_user_data,
                                    const uint8_t *cancel) {
    AVFormatContext *in_ctx = NULL;
    AVFormatContext *out_ctx = NULL;
    AVCodecContext *dec_ctx = NULL;
    AVCodecContext *enc_ctx = NULL;
    int *stream_mapping = NULL;
    AudioFilterChain chain = {0};
    AVPacket *pkt = NULL;
    AVFrame *dec_frame = NULL;
    AVFrame *filt_frame = NULL;
    AVPacket *enc_pkt = NULL;
    EncodeStatus status = ENCODE_OK;
    int audio_in_index = -1;
    int audio_out_index = -1;

    switch (open_input(in_path, &in_ctx)) {
    case -1:
        return ENCODE_ERR_OPEN_INPUT;
    case -2:
        return ENCODE_ERR_STREAM_INFO;
    }

    for (unsigned int i = 0; i < in_ctx->nb_streams; i++) {
        if (in_ctx->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_AUDIO) {
            audio_in_index = (int)i;
            break;
        }
    }
    if (audio_in_index < 0) {
        avformat_close_input(&in_ctx);
        return ENCODE_ERR_NO_AUDIO_STREAM;
    }

    avformat_alloc_output_context2(&out_ctx, NULL, NULL, out_path);
    if (!out_ctx) {
        status = ENCODE_ERR_ALLOC_OUTPUT;
        goto cleanup;
    }

    stream_mapping = av_calloc(in_ctx->nb_streams, sizeof(*stream_mapping));
    if (!stream_mapping) {
        status = ENCODE_ERR_ALLOC_OUTPUT;
        goto cleanup;
    }

    {
        const AVCodec *decoder =
            avcodec_find_decoder(in_ctx->streams[audio_in_index]->codecpar->codec_id);
        if (!decoder) {
            status = ENCODE_ERR_DECODER;
            goto cleanup;
        }
        dec_ctx = avcodec_alloc_context3(decoder);
        if (!dec_ctx ||
            avcodec_parameters_to_context(dec_ctx, in_ctx->streams[audio_in_index]->codecpar) < 0) {
            status = ENCODE_ERR_DECODER;
            goto cleanup;
        }
        dec_ctx->pkt_timebase = in_ctx->streams[audio_in_index]->time_base;
        if (avcodec_open2(dec_ctx, decoder, NULL) < 0) {
            status = ENCODE_ERR_DECODER;
            goto cleanup;
        }
    }

    const AVCodec *encoder = avcodec_find_encoder(AV_CODEC_ID_AAC);
    if (!encoder) {
        status = ENCODE_ERR_ENCODER;
        goto cleanup;
    }

    {
        char filter_descr[256];
        snprintf(filter_descr, sizeof(filter_descr), MASTERING_FILTER_FORMAT, (double)target_lufs);
        if (init_audio_filter_chain(dec_ctx, encoder, filter_descr, &chain) < 0) {
            status = ENCODE_ERR_FILTER_GRAPH;
            goto cleanup;
        }
    }

    enc_ctx = avcodec_alloc_context3(encoder);
    if (!enc_ctx) {
        status = ENCODE_ERR_ENCODER;
        goto cleanup;
    }
    enc_ctx->sample_rate = av_buffersink_get_sample_rate(chain.buffersink_ctx);
    av_buffersink_get_ch_layout(chain.buffersink_ctx, &enc_ctx->ch_layout);
    enc_ctx->sample_fmt = (enum AVSampleFormat)av_buffersink_get_format(chain.buffersink_ctx);
    enc_ctx->bit_rate = EXPORT_AUDIO_BITRATE_BPS;
    enc_ctx->time_base = av_buffersink_get_time_base(chain.buffersink_ctx);
    if (out_ctx->oformat->flags & AVFMT_GLOBALHEADER) {
        enc_ctx->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
    }
    if (avcodec_open2(enc_ctx, encoder, NULL) < 0) {
        status = ENCODE_ERR_ENCODER;
        goto cleanup;
    }
    if (enc_ctx->frame_size > 0) {
        /* AAC (and most audio codecs) need exactly frame_size samples per input frame; the
           filter chain doesn't chunk to that on its own — without this, avcodec_send_frame
           on the encoder fails with "nb_samples > frame_size" on anything but a lucky length. */
        av_buffersink_set_frame_size(chain.buffersink_ctx, (unsigned)enc_ctx->frame_size);
    }

    {
        int out_stream_count = 0;
        for (unsigned int i = 0; i < in_ctx->nb_streams; i++) {
            AVStream *in_stream = in_ctx->streams[i];
            enum AVMediaType type = in_stream->codecpar->codec_type;
            AVStream *out_stream;

            if ((int)i == audio_in_index) {
                out_stream = avformat_new_stream(out_ctx, NULL);
                if (!out_stream ||
                    avcodec_parameters_from_context(out_stream->codecpar, enc_ctx) < 0) {
                    status = ENCODE_ERR_NEW_STREAM;
                    goto cleanup;
                }
                out_stream->time_base = enc_ctx->time_base;
                stream_mapping[i] = out_stream_count++;
                audio_out_index = out_stream->index;
                continue;
            }

            if (type != AVMEDIA_TYPE_VIDEO && type != AVMEDIA_TYPE_AUDIO) {
                stream_mapping[i] = -1;
                continue;
            }

            out_stream = avformat_new_stream(out_ctx, NULL);
            if (!out_stream ||
                avcodec_parameters_copy(out_stream->codecpar, in_stream->codecpar) < 0) {
                status = ENCODE_ERR_NEW_STREAM;
                goto cleanup;
            }
            out_stream->codecpar->codec_tag = 0;
            out_stream->time_base = in_stream->time_base;
            stream_mapping[i] = out_stream_count++;
        }
    }

    if (!(out_ctx->oformat->flags & AVFMT_NOFILE)) {
        if (avio_open(&out_ctx->pb, out_path, AVIO_FLAG_WRITE) < 0) {
            status = ENCODE_ERR_OPEN_OUTPUT;
            goto cleanup_output_io;
        }
    }

    if (avformat_write_header(out_ctx, NULL) < 0) {
        status = ENCODE_ERR_WRITE_HEADER;
        goto cleanup_output_io;
    }

    pkt = av_packet_alloc();
    dec_frame = av_frame_alloc();
    filt_frame = av_frame_alloc();
    enc_pkt = av_packet_alloc();
    if (!pkt || !dec_frame || !filt_frame || !enc_pkt) {
        status = ENCODE_ERR_PIPELINE;
        goto cleanup_output_io;
    }

    while (av_read_frame(in_ctx, pkt) >= 0) {
        int mapped = stream_mapping[pkt->stream_index];
        if (mapped < 0) {
            av_packet_unref(pkt);
            continue;
        }

        if (cancel && *cancel) {
            av_packet_unref(pkt);
            status = ENCODE_CANCELLED;
            break;
        }
        if (progress_cb && pkt->pts != AV_NOPTS_VALUE) {
            progress_cb(progress_user_data,
                        pkt->pts * av_q2d(in_ctx->streams[pkt->stream_index]->time_base));
        }

        if (pkt->stream_index == audio_in_index) {
            AVStream *out_stream = out_ctx->streams[audio_out_index];
            int ret = avcodec_send_packet(dec_ctx, pkt);
            av_packet_unref(pkt);
            if (ret < 0) {
                status = ENCODE_ERR_PIPELINE;
                break;
            }
            while (1) {
                ret = avcodec_receive_frame(dec_ctx, dec_frame);
                if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
                    break;
                } else if (ret < 0) {
                    status = ENCODE_ERR_PIPELINE;
                    break;
                }
                if (filter_encode_write_frame(out_ctx, &chain, enc_ctx, out_stream, dec_frame,
                                              filt_frame, enc_pkt) < 0) {
                    status = ENCODE_ERR_PIPELINE;
                    break;
                }
            }
            if (status != ENCODE_OK) break;
            continue;
        }

        /* Video (or other passthrough) packet — copy through unchanged. */
        {
            AVStream *in_stream = in_ctx->streams[pkt->stream_index];
            AVStream *out_stream = out_ctx->streams[mapped];
            pkt->stream_index = mapped;
            av_packet_rescale_ts(pkt, in_stream->time_base, out_stream->time_base);
            pkt->pos = -1;
            if (av_interleaved_write_frame(out_ctx, pkt) < 0) {
                status = ENCODE_ERR_WRITE_FRAME;
                break;
            }
        }
    }

    if (status == ENCODE_OK) {
        /* Flush: decoder -> filter -> encoder, in that order — each stage may be holding
           buffered frames the next stage hasn't seen yet. */
        AVStream *out_stream = out_ctx->streams[audio_out_index];
        avcodec_send_packet(dec_ctx, NULL);
        while (avcodec_receive_frame(dec_ctx, dec_frame) >= 0) {
            if (filter_encode_write_frame(out_ctx, &chain, enc_ctx, out_stream, dec_frame,
                                          filt_frame, enc_pkt) < 0) {
                status = ENCODE_ERR_PIPELINE;
                break;
            }
        }
        if (status == ENCODE_OK && filter_encode_write_frame(out_ctx, &chain, enc_ctx, out_stream,
                                                             NULL, filt_frame, enc_pkt) < 0) {
            status = ENCODE_ERR_PIPELINE;
        }
        if (status == ENCODE_OK &&
            encode_write_packet(out_ctx, enc_ctx, out_stream, NULL, enc_pkt) < 0) {
            status = ENCODE_ERR_PIPELINE;
        }
    }

    if (status == ENCODE_OK) {
        av_write_trailer(out_ctx);
    }

cleanup_output_io:
    av_packet_free(&pkt);
    av_packet_free(&enc_pkt);
    av_frame_free(&dec_frame);
    av_frame_free(&filt_frame);
    if (out_ctx && !(out_ctx->oformat->flags & AVFMT_NOFILE)) {
        avio_closep(&out_ctx->pb);
    }
cleanup:
    free_audio_filter_chain(&chain);
    avcodec_free_context(&dec_ctx);
    avcodec_free_context(&enc_ctx);
    av_freep(&stream_mapping);
    if (out_ctx) {
        avformat_free_context(out_ctx);
    }
    avformat_close_input(&in_ctx);
    return status;
}
