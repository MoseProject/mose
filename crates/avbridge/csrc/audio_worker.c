/* Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>

   This program is free software: you can redistribute it and/or modify
   it under the terms of the GNU General Public License as published by
   the Free Software Foundation, either version 3 of the License, or
   (at your option) any later version. */

/* Runs an export's audio filter graph and AAC encoder on a thread of their own.

   The export loops decode video and audio packets in file order and used to push every audio
   frame through the audio graph (afftdn, loudnorm, alimiter) and the encoder before going back to
   the video: on a 60 s 1080p60 clip that was 4.4 s of a 10 s export spent with the video encoder
   idle. Here the loop only hands the decoded frame over; the graph and encoder, which nothing
   else touches, run beside the video work.

   The one thing both threads write to is the output muxer, so while a worker exists
   `out_ctx->opaque` points at a mutex that `encode_write_packet` takes around the interleaved
   write. */

#include "bridge_internal.h"
#include "bridge_thread.h"

/* About 1.3 s of 1024-sample AAC frames: enough to ride out a slow stretch of audio without the
   decoder thread waiting, small enough not to hold much decoded audio. */
#define QUEUE_CAPACITY 64

typedef enum { ITEM_FRAME, ITEM_COMMAND } ItemKind;

typedef struct {
    ItemKind kind;
    AVFrame *frame;
    char target[16];
    char command[16];
    char argument[32];
} Item;

struct AudioWorker {
    AVFormatContext *out_ctx;
    AudioFilterChain *chain;
    AVCodecContext *encoder;
    AVStream *stream;
    AVFrame *filtered;
    AVPacket *packet;

    BridgeMutex lock;
    BridgeCond changed;
    BridgeMutex mux_lock;
    BridgeThread thread;
    Item queue[QUEUE_CAPACITY];
    int head;
    int count;
    int closed;
    int flush;
    int error;
};

static void free_item(Item *item) {
    av_frame_free(&item->frame);
}

static void run(void *param) {
    AudioWorker *w = (AudioWorker *)param;
    for (;;) {
        bridge_mutex_lock(&w->lock);
        while (w->count == 0 && !w->closed) {
            bridge_cond_wait(&w->changed, &w->lock);
        }
        if (w->count == 0) {
            int flush = w->flush && !w->error;
            bridge_mutex_unlock(&w->lock);
            if (flush &&
                (filter_encode_write_frame(w->out_ctx, w->chain, w->encoder, w->stream, NULL,
                                           w->filtered, w->packet) < 0 ||
                 encode_write_packet(w->out_ctx, w->encoder, w->stream, NULL, w->packet) < 0)) {
                bridge_mutex_lock(&w->lock);
                w->error = 1;
                bridge_mutex_unlock(&w->lock);
            }
            return;
        }
        Item item = w->queue[w->head];
        w->head = (w->head + 1) % QUEUE_CAPACITY;
        w->count -= 1;
        int failed = w->error;
        bridge_cond_broadcast(&w->changed);
        bridge_mutex_unlock(&w->lock);

        /* After a failure the rest is only drained, so a producer blocked on a full queue wakes. */
        if (!failed) {
            int ret = 0;
            if (item.kind == ITEM_FRAME) {
                ret = filter_encode_write_frame(w->out_ctx, w->chain, w->encoder, w->stream,
                                                item.frame, w->filtered, w->packet);
            } else {
                /* Same as the old inline call: an unknown target or bad value is ignored. */
                avfilter_graph_send_command(w->chain->graph, item.target, item.command,
                                            item.argument, NULL, 0, 0);
            }
            if (ret < 0) {
                bridge_mutex_lock(&w->lock);
                w->error = 1;
                bridge_mutex_unlock(&w->lock);
            }
        }
        free_item(&item);
    }
}

AudioWorker *audio_worker_start(AVFormatContext *out_ctx, AudioFilterChain *chain,
                                AVCodecContext *encoder, AVStream *stream) {
    AudioWorker *w = av_mallocz(sizeof(*w));
    if (!w) {
        return NULL;
    }
    w->out_ctx = out_ctx;
    w->chain = chain;
    w->encoder = encoder;
    w->stream = stream;
    w->filtered = av_frame_alloc();
    w->packet = av_packet_alloc();
    if (!w->filtered || !w->packet) {
        av_frame_free(&w->filtered);
        av_packet_free(&w->packet);
        av_free(w);
        return NULL;
    }
    bridge_mutex_init(&w->lock);
    bridge_mutex_init(&w->mux_lock);
    bridge_cond_init(&w->changed);
    out_ctx->opaque = &w->mux_lock;
    if (bridge_thread_start(&w->thread, run, w) < 0) {
        out_ctx->opaque = NULL;
        bridge_mutex_destroy(&w->lock);
        bridge_mutex_destroy(&w->mux_lock);
        bridge_cond_destroy(&w->changed);
        av_frame_free(&w->filtered);
        av_packet_free(&w->packet);
        av_free(w);
        return NULL;
    }
    return w;
}

/* Appends `item`, waiting while the queue is full. Takes ownership of its frame either way. */
static int push(AudioWorker *w, Item *item) {
    bridge_mutex_lock(&w->lock);
    while (w->count == QUEUE_CAPACITY && !w->error) {
        bridge_cond_wait(&w->changed, &w->lock);
    }
    if (w->error) {
        bridge_mutex_unlock(&w->lock);
        free_item(item);
        return -1;
    }
    w->queue[(w->head + w->count) % QUEUE_CAPACITY] = *item;
    w->count += 1;
    bridge_cond_broadcast(&w->changed);
    bridge_mutex_unlock(&w->lock);
    return 0;
}

int audio_worker_push_frame(AudioWorker *w, const AVFrame *frame) {
    Item item = {ITEM_FRAME, av_frame_clone(frame), "", "", ""};
    if (!item.frame) {
        return -1;
    }
    return push(w, &item);
}

int audio_worker_command(AudioWorker *w, const char *target, const char *command,
                         const char *argument) {
    Item item = {ITEM_COMMAND, NULL, "", "", ""};
    snprintf(item.target, sizeof(item.target), "%s", target);
    snprintf(item.command, sizeof(item.command), "%s", command);
    snprintf(item.argument, sizeof(item.argument), "%s", argument);
    return push(w, &item);
}

int audio_worker_finish(AudioWorker **worker, int flush) {
    AudioWorker *w = *worker;
    if (!w) {
        return 0;
    }
    *worker = NULL;
    bridge_mutex_lock(&w->lock);
    w->closed = 1;
    w->flush = flush;
    if (!flush) {
        /* Aborting: what is still queued is not going to be encoded. */
        while (w->count > 0) {
            free_item(&w->queue[w->head]);
            w->head = (w->head + 1) % QUEUE_CAPACITY;
            w->count -= 1;
        }
    }
    bridge_cond_broadcast(&w->changed);
    bridge_mutex_unlock(&w->lock);
    bridge_thread_join(&w->thread);

    int error = w->error;
    w->out_ctx->opaque = NULL;
    bridge_mutex_destroy(&w->lock);
    bridge_mutex_destroy(&w->mux_lock);
    bridge_cond_destroy(&w->changed);
    av_frame_free(&w->filtered);
    av_packet_free(&w->packet);
    av_free(w);
    return error ? -1 : 0;
}
