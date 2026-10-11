// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

#include "bridge_internal.h"

#include <libavutil/error.h>

static int receive_audio_frames(AVCodecContext *decoder, AVFrame *frame, AudioFrameConsumer consume,
                                void *user_data) {
    while (1) {
        int ret = avcodec_receive_frame(decoder, frame);
        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
            return 0;
        }
        if (ret < 0) {
            return ret;
        }
        ret = consume(frame, user_data);
        if (ret < 0) {
            return ret;
        }
    }
}

int decode_audio_stream(AVFormatContext *input, AVCodecContext *decoder, int stream_index,
                        AVPacket *packet, AVFrame *frame, AudioFrameConsumer consume,
                        void *user_data) {
    if (!input || !decoder || stream_index < 0 || !packet || !frame || !consume) {
        return AVERROR(EINVAL);
    }

    int ret;
    while ((ret = av_read_frame(input, packet)) >= 0) {
        if (packet->stream_index == stream_index) {
            ret = avcodec_send_packet(decoder, packet);
            av_packet_unref(packet);
            if (ret < 0) {
                return ret;
            }
            ret = receive_audio_frames(decoder, frame, consume, user_data);
            if (ret < 0) {
                return ret;
            }
        } else {
            av_packet_unref(packet);
        }
    }

    ret = avcodec_send_packet(decoder, NULL);
    if (ret < 0 && ret != AVERROR_EOF) {
        return ret;
    }
    ret = receive_audio_frames(decoder, frame, consume, user_data);
    if (ret < 0) {
        return ret;
    }
    return consume(NULL, user_data);
}
