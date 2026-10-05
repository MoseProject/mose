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

#include <string.h>

#include "bridge.h"
#include "bridge_internal.h"

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>

/* Encodes `frame_count` grayscale-as-luma frames (see avbridge_encode_matte_video's doc
   comment in bridge.h) to `out_path` via libopenh264, forced (no GPU attempt — this is an
   internal-only artifact never shown to the user, unlike the main export path's
   open_video_encoder ladder). Mirrors proxy.c's video-encoder-opening block closely, minus the
   decode/scale side (frames are already decoded and already the right size, supplied whole by
   the caller) and minus any audio handling (a matte video has none). */
MatteStatus avbridge_encode_matte_video(const uint8_t *luma_frames, int frame_count, int width,
                                         int height, int fps_num, int fps_den,
                                         const char *out_path) {
    if (frame_count <= 0 || width <= 0 || height <= 0 || fps_num <= 0 || fps_den <= 0) {
        return MATTE_ERR_EMPTY;
    }
    /* yuv420p's chroma planes are subsampled 2x in both directions — odd dimensions would
       leave a chroma row/column undefined. Every real decoded video frame this is fed from
       already has even width/height (a hard requirement most encoders/containers already
       impose upstream), so this is a defensive check, not an expected path. */
    if (width % 2 != 0 || height % 2 != 0) {
        return MATTE_ERR_ODD_DIMENSIONS;
    }

    AVFormatContext *out_ctx = NULL;
    AVCodecContext *venc_ctx = NULL;
    AVStream *out_stream = NULL;
    AVFrame *frame = NULL;
    AVPacket *enc_pkt = NULL;
    MatteStatus status = MATTE_OK;

    avformat_alloc_output_context2(&out_ctx, NULL, NULL, out_path);
    if (!out_ctx) {
        return MATTE_ERR_ALLOC_OUTPUT;
    }

    const AVCodec *venc = find_cpu_h264_encoder();
    if (!venc) {
        status = MATTE_ERR_ENCODER;
        goto cleanup;
    }
    venc_ctx = avcodec_alloc_context3(venc);
    if (!venc_ctx) {
        status = MATTE_ERR_ENCODER;
        goto cleanup;
    }
    venc_ctx->width = width;
    venc_ctx->height = height;
    venc_ctx->pix_fmt = AV_PIX_FMT_YUV420P;
    AVRational frame_rate = (AVRational){fps_num, fps_den};
    venc_ctx->time_base = av_inv_q(frame_rate);
    venc_ctx->framerate = frame_rate;
    venc_ctx->gop_size = (fps_num / fps_den) * KEYFRAME_INTERVAL_SECS;
    venc_ctx->max_b_frames = 0; /* keeps internal matte decoding lightweight */
    /* Internal-only artifact re-decoded frame-by-frame at export time, not archival quality —
       sized the same way proxy.c's bit_rate is, off the (small, luma-dominated) pixel count. */
    venc_ctx->bit_rate = (int64_t)width * height * 2;
    if (out_ctx->oformat->flags & AVFMT_GLOBALHEADER) {
        venc_ctx->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
    }
    if (avcodec_open2(venc_ctx, venc, NULL) < 0) {
        status = MATTE_ERR_ENCODER;
        goto cleanup;
    }

    out_stream = avformat_new_stream(out_ctx, NULL);
    if (!out_stream || avcodec_parameters_from_context(out_stream->codecpar, venc_ctx) < 0) {
        status = MATTE_ERR_NEW_STREAM;
        goto cleanup;
    }
    out_stream->time_base = venc_ctx->time_base;

    if (!(out_ctx->oformat->flags & AVFMT_NOFILE)) {
        if (avio_open(&out_ctx->pb, out_path, AVIO_FLAG_WRITE) < 0) {
            status = MATTE_ERR_OPEN_OUTPUT;
            goto cleanup_output_io;
        }
    }
    if (avformat_write_header(out_ctx, NULL) < 0) {
        status = MATTE_ERR_WRITE_HEADER;
        goto cleanup_output_io;
    }

    frame = av_frame_alloc();
    enc_pkt = av_packet_alloc();
    if (!frame || !enc_pkt) {
        status = MATTE_ERR_PIPELINE;
        goto cleanup_output_io;
    }
    frame->format = AV_PIX_FMT_YUV420P;
    frame->width = width;
    frame->height = height;
    if (av_frame_get_buffer(frame, 0) < 0) {
        status = MATTE_ERR_PIPELINE;
        goto cleanup_output_io;
    }

    {
        int chroma_w = width / 2;
        int chroma_h = height / 2;
        for (int i = 0; i < frame_count; i++) {
            if (av_frame_make_writable(frame) < 0) {
                status = MATTE_ERR_PIPELINE;
                goto cleanup_output_io;
            }
            const uint8_t *src = luma_frames + (size_t)i * (size_t)width * (size_t)height;
            for (int y = 0; y < height; y++) {
                memcpy(frame->data[0] + (size_t)y * frame->linesize[0], src + (size_t)y * width,
                       (size_t)width);
            }
            /* Neutral chroma (128 = "no color") — only the luma plane carries the matte. */
            for (int y = 0; y < chroma_h; y++) {
                memset(frame->data[1] + (size_t)y * frame->linesize[1], 128, (size_t)chroma_w);
                memset(frame->data[2] + (size_t)y * frame->linesize[2], 128, (size_t)chroma_w);
            }
            frame->pts = i;
            if (encode_write_packet(out_ctx, venc_ctx, out_stream, frame, enc_pkt) < 0) {
                status = MATTE_ERR_PIPELINE;
                goto cleanup_output_io;
            }
        }
    }

    if (encode_write_packet(out_ctx, venc_ctx, out_stream, NULL, enc_pkt) < 0) {
        status = MATTE_ERR_PIPELINE;
        goto cleanup_output_io;
    }

    av_write_trailer(out_ctx);

cleanup_output_io:
    av_frame_free(&frame);
    av_packet_free(&enc_pkt);
    if (out_ctx && !(out_ctx->oformat->flags & AVFMT_NOFILE)) {
        avio_closep(&out_ctx->pb);
    }
cleanup:
    avcodec_free_context(&venc_ctx);
    if (out_ctx) {
        avformat_free_context(out_ctx);
    }
    return status;
}
