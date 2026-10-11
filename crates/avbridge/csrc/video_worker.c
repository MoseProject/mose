/* Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>

   This program is free software: you can redistribute it and/or modify
   it under the terms of the GNU General Public License as published by
   the Free Software Foundation, either version 3 of the License, or
   (at your option) any later version. */

/* Runs an export's video encoder on a thread of its own, the way audio_worker.c does for audio.

   The export loop decoded, filtered and encoded each frame before it went back for the next, so
   at any moment only one of those stages was working: a software encoder at 4K left the decoder
   idle and the other way round. Here the loop hands the finished frame over and carries on with
   the next one while the encoder works on this one. The one thing both this thread and the
   audio worker write to is the output muxer, which `out_ctx->opaque`'s mutex already guards. */

#include "bridge_internal.h"
#include "bridge_thread.h"

/* A few frames of slack: enough to ride out a slow stretch of encoding without the decoder
   waiting, few enough that a 4K export does not hold much decoded video. */
#define QUEUE_CAPACITY 6

struct VideoEncodeWorker {
    AVFormatContext *out_ctx;
    AVCodecContext *encoder;
    AVStream *stream;
    AVPacket *packet;

    BridgeMutex lock;
    BridgeCond changed;
    BridgeThread thread;
    AVFrame *queue[QUEUE_CAPACITY];
    int head;
    int count;
    int closed;
    int error;
};

static void run(void *param) {
    VideoEncodeWorker *w = (VideoEncodeWorker *)param;
    for (;;) {
        bridge_mutex_lock(&w->lock);
        while (w->count == 0 && !w->closed) {
            bridge_cond_wait(&w->changed, &w->lock);
        }
        if (w->count == 0) {
            bridge_mutex_unlock(&w->lock);
            return;
        }
        AVFrame *frame = w->queue[w->head];
        w->head = (w->head + 1) % QUEUE_CAPACITY;
        w->count -= 1;
        int failed = w->error;
        bridge_cond_broadcast(&w->changed);
        bridge_mutex_unlock(&w->lock);

        /* After a failure the rest is only drained, so a producer blocked on a full queue wakes. */
        if (!failed &&
            encode_write_packet(w->out_ctx, w->encoder, w->stream, frame, w->packet) < 0) {
            bridge_mutex_lock(&w->lock);
            w->error = 1;
            bridge_mutex_unlock(&w->lock);
        }
        av_frame_free(&frame);
    }
}

VideoEncodeWorker *video_worker_start(AVFormatContext *out_ctx, AVCodecContext *encoder,
                                      AVStream *stream) {
    VideoEncodeWorker *w = av_mallocz(sizeof(*w));
    if (!w) {
        return NULL;
    }
    w->out_ctx = out_ctx;
    w->encoder = encoder;
    w->stream = stream;
    w->packet = av_packet_alloc();
    if (!w->packet) {
        av_free(w);
        return NULL;
    }
    bridge_mutex_init(&w->lock);
    bridge_cond_init(&w->changed);
    if (bridge_thread_start(&w->thread, run, w) < 0) {
        bridge_mutex_destroy(&w->lock);
        bridge_cond_destroy(&w->changed);
        av_packet_free(&w->packet);
        av_free(w);
        return NULL;
    }
    return w;
}

int video_worker_push(VideoEncodeWorker *w, const AVFrame *frame) {
    AVFrame *held = av_frame_clone(frame);
    if (!held) {
        return -1;
    }
    bridge_mutex_lock(&w->lock);
    while (w->count == QUEUE_CAPACITY && !w->error) {
        bridge_cond_wait(&w->changed, &w->lock);
    }
    if (w->error) {
        bridge_mutex_unlock(&w->lock);
        av_frame_free(&held);
        return -1;
    }
    w->queue[(w->head + w->count) % QUEUE_CAPACITY] = held;
    w->count += 1;
    bridge_cond_broadcast(&w->changed);
    bridge_mutex_unlock(&w->lock);
    return 0;
}

int video_worker_finish(VideoEncodeWorker **worker, int flush) {
    VideoEncodeWorker *w = *worker;
    if (!w) {
        return 0;
    }
    *worker = NULL;
    bridge_mutex_lock(&w->lock);
    w->closed = 1;
    if (!flush) {
        /* Aborting: what is still queued is not going to be encoded. */
        while (w->count > 0) {
            av_frame_free(&w->queue[w->head]);
            w->head = (w->head + 1) % QUEUE_CAPACITY;
            w->count -= 1;
        }
    }
    bridge_cond_broadcast(&w->changed);
    bridge_mutex_unlock(&w->lock);
    bridge_thread_join(&w->thread);

    int error = w->error;
    bridge_mutex_destroy(&w->lock);
    bridge_cond_destroy(&w->changed);
    av_packet_free(&w->packet);
    av_free(w);
    return error ? -1 : 0;
}
