/* Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>

   This program is free software: you can redistribute it and/or modify
   it under the terms of the GNU General Public License as published by
   the Free Software Foundation, either version 3 of the License, or
   (at your option) any later version. */

/* A frame-accurate cut of one H.264 clip without encoding all of it.

   A cut almost never starts or ends on a keyframe, so a plain stream copy would be off by up to a
   GOP. Instead the pictures between the cut and the neighbouring keyframes are decoded and encoded
   again (a second or two at each end), and everything between the two keyframes is copied packet
   for packet: the same bits, the same bitrate, no second generation of compression, and no time
   spent on it. Only video is written; the caller mixes the audio in and remuxes.

   The two kinds of picture come from different encoders, so they carry different SPS/PPS. The
   output stream is written with Annex B packets and the source's parameter sets in its extradata
   (what `h264_mp4toannexb` produces), the copied keyframes get their parameter sets in front of
   them and the new pictures get theirs from `dump_extra`: a decoder switches parameter sets at
   each IDR picture.

   Whatever does not fit (another codec, a variable frame rate, an open GOP, a GOP too short to
   hold a copy) returns SMART_CUT_NOT_APPLICABLE before anything is written, and the caller
   exports the usual way. */

#include "bridge.h"
#include "bridge_internal.h"

#include <libavcodec/avcodec.h>
#include <libavcodec/bsf.h>
#include <libavformat/avformat.h>
#include <libavutil/opt.h>

typedef struct {
    int64_t last_le_pts;  /* pts of the last keyframe at or before the target */
    int64_t first_gt_pts; /* pts of the first keyframe after the target; NOPTS when there is none */
    int last_le_idr;
    int first_gt_idr;
} KeyScan;

/* Whether the AVCC packet holds an IDR slice. */
static int packet_is_idr(const AVPacket *pkt, int nal_length_size) {
    size_t pos = 0;
    while (pos + (size_t)nal_length_size + 1 <= (size_t)pkt->size) {
        uint32_t len = 0;
        for (int i = 0; i < nal_length_size; i++)
            len = (len << 8) | pkt->data[pos + (size_t)i];
        pos += (size_t)nal_length_size;
        if (len == 0 || pos + len > (size_t)pkt->size) return 0;
        if ((pkt->data[pos] & 0x1f) == 5) return 1;
        pos += len;
    }
    return 0;
}

/* Reads video packets from a seek to `target`, reporting the keyframes on either side of it.
   Nothing is decoded. Returns 0, or -1 when the stream has no usable timestamps. */
static int scan_keyframes(AVFormatContext *ctx, int vidx, int64_t target, int nal_length_size,
                          AVPacket *pkt, KeyScan *scan) {
    scan->last_le_pts = AV_NOPTS_VALUE;
    scan->first_gt_pts = AV_NOPTS_VALUE;
    scan->last_le_idr = scan->first_gt_idr = 0;
    if (av_seek_frame(ctx, vidx, target, AVSEEK_FLAG_BACKWARD) < 0) return -1;
    while (av_read_frame(ctx, pkt) >= 0) {
        if (pkt->stream_index == vidx && (pkt->flags & AV_PKT_FLAG_KEY)) {
            if (pkt->pts == AV_NOPTS_VALUE || pkt->dts == AV_NOPTS_VALUE) {
                av_packet_unref(pkt);
                return -1;
            }
            int idr = packet_is_idr(pkt, nal_length_size);
            if (pkt->pts <= target) {
                scan->last_le_pts = pkt->pts;
                scan->last_le_idr = idr;
            } else {
                scan->first_gt_pts = pkt->pts;
                scan->first_gt_idr = idr;
                av_packet_unref(pkt);
                break;
            }
        }
        av_packet_unref(pkt);
    }
    return scan->last_le_pts == AV_NOPTS_VALUE ? -1 : 0;
}

typedef struct {
    AVFormatContext *out_ctx;
    AVStream *out_stream;
    AVBSFContext *annexb; /* AVCC packets of the source -> Annex B with parameter sets */
    AVPacket *scratch;
    int64_t first_pts; /* source pts that becomes output pts 0 */
    int64_t delay;     /* pts - dts of the first copied keyframe: the source's reorder delay */
} Writer;

/* Writes `pkt` (Annex B) as it is. */
static int write_annexb(Writer *w, AVPacket *pkt) {
    pkt->stream_index = w->out_stream->index;
    pkt->pos = -1;
    return av_interleaved_write_frame(w->out_ctx, pkt);
}

/* Copies one source packet through the AVCC -> Annex B filter. */
static int copy_packet(Writer *w, AVPacket *pkt) {
    pkt->pts -= w->first_pts;
    pkt->dts -= w->first_pts;
    int ret = av_bsf_send_packet(w->annexb, pkt);
    if (ret < 0) return ret;
    while ((ret = av_bsf_receive_packet(w->annexb, w->scratch)) >= 0) {
        int wr = write_annexb(w, w->scratch);
        av_packet_unref(w->scratch);
        if (wr < 0) return wr;
    }
    return ret == AVERROR(EAGAIN) || ret == AVERROR_EOF ? 0 : ret;
}

/* Encodes frames and writes them with timestamps in the output stream's time base. */
typedef struct {
    AVCodecContext *enc;
    AVBSFContext *dump_extra; /* puts the encoder's parameter sets in front of each keyframe */
    AVRational frame_tb;      /* one tick per frame */
    int64_t ticks_per_frame;  /* in the output stream's time base */
    int64_t base_pts;         /* source pts of output pts 0 */
    int64_t delay;
    AVPacket *packet;
    AVPacket *filtered;
} FrameSink;

static int sink_open(FrameSink *s, int width, int height, AVRational fps, int64_t bit_rate,
                     int64_t ticks_per_frame, int64_t base_pts, int64_t delay) {
    memset(s, 0, sizeof(*s));
    request_high_profile_encoders(1);
    s->enc = open_video_encoder(GPU_ENCODER_CPU, width, height, fps, bit_rate, 1, NULL, NULL);
    request_high_profile_encoders(0);
    if (!s->enc) return -1;
    s->frame_tb = av_inv_q(fps);
    s->ticks_per_frame = ticks_per_frame;
    s->base_pts = base_pts;
    s->delay = delay;
    s->packet = av_packet_alloc();
    s->filtered = av_packet_alloc();
    const AVBitStreamFilter *dump = av_bsf_get_by_name("dump_extra");
    if (!s->packet || !s->filtered || !dump || av_bsf_alloc(dump, &s->dump_extra) < 0) return -1;
    if (avcodec_parameters_from_context(s->dump_extra->par_in, s->enc) < 0) return -1;
    s->dump_extra->time_base_in = s->enc->time_base;
    av_opt_set(s->dump_extra->priv_data, "freq", "keyframe", 0);
    return av_bsf_init(s->dump_extra);
}

static void sink_close(FrameSink *s) {
    av_packet_free(&s->packet);
    av_packet_free(&s->filtered);
    av_bsf_free(&s->dump_extra);
    avcodec_free_context(&s->enc);
}

/* Drains the encoder's packets (all of them with a null `frame`). `stream_tb` is the output
   stream's time base. */
static int sink_encode(FrameSink *s, Writer *w, AVFrame *frame) {
    int ret = avcodec_send_frame(s->enc, frame);
    if (ret < 0 && ret != AVERROR_EOF) return ret;
    while ((ret = avcodec_receive_packet(s->enc, s->packet)) >= 0) {
        /* No B-frames and no skipped frames: a packet's pts in frames is what the frame had. */
        int64_t frames = av_rescale_q(s->packet->pts, s->enc->time_base, s->frame_tb);
        s->packet->pts = frames * s->ticks_per_frame;
        s->packet->dts = s->packet->pts - s->delay;
        s->packet->duration = s->ticks_per_frame;
        int sret = av_bsf_send_packet(s->dump_extra, s->packet);
        if (sret < 0) return sret;
        while ((sret = av_bsf_receive_packet(s->dump_extra, s->filtered)) >= 0) {
            int wr = write_annexb(w, s->filtered);
            av_packet_unref(s->filtered);
            if (wr < 0) return wr;
        }
        if (sret != AVERROR(EAGAIN) && sret != AVERROR_EOF) return sret;
    }
    return ret == AVERROR(EAGAIN) || ret == AVERROR_EOF ? 0 : ret;
}

/* Decodes from the keyframe a seek to `seek_pts` lands on and hands every picture with
   `from_pts <= pts < to_pts` to `sink`, stopping at `stop_packet_pts` (a keyframe the copy takes
   over from, or NOPTS to run until `to_pts`). Returns the number of pictures written, or -1. */
static int64_t reencode_range(AVFormatContext *ctx, int vidx, AVStream *vs, int64_t seek_pts,
                              int64_t from_pts, int64_t to_pts, int64_t stop_packet_pts,
                              FrameSink *sink, Writer *w, int64_t fdur, const uint8_t *cancel,
                              int *cancelled) {
    int64_t written = 0;
    int result = 0;
    AVCodecContext *dec = NULL;
    AVPacket *pkt = av_packet_alloc();
    AVFrame *frame = av_frame_alloc();
    AVFrame *out = av_frame_alloc();
    const AVCodec *codec = avcodec_find_decoder(AV_CODEC_ID_H264);
    if (!pkt || !frame || !out || !codec) goto fail;
    dec = avcodec_alloc_context3(codec);
    if (!dec || avcodec_parameters_to_context(dec, vs->codecpar) < 0) goto fail;
    dec->pkt_timebase = vs->time_base;
    bridge_enable_decoder_threads(dec);
    if (avcodec_open2(dec, codec, NULL) < 0) goto fail;
    if (av_seek_frame(ctx, vidx, seek_pts, AVSEEK_FLAG_BACKWARD) < 0) goto fail;

    int draining = 0;
    int stop = 0;
    while (!stop) {
        if (cancel && *cancel) {
            *cancelled = 1;
            break;
        }
        if (!draining) {
            int rd = av_read_frame(ctx, pkt);
            if (rd < 0 || (pkt->stream_index == vidx && stop_packet_pts != AV_NOPTS_VALUE &&
                           (pkt->flags & AV_PKT_FLAG_KEY) && pkt->pts == stop_packet_pts)) {
                if (rd >= 0) av_packet_unref(pkt);
                avcodec_send_packet(dec, NULL);
                draining = 1;
            } else if (pkt->stream_index == vidx) {
                if (avcodec_send_packet(dec, pkt) < 0) {
                    av_packet_unref(pkt);
                    goto fail;
                }
                av_packet_unref(pkt);
            } else {
                av_packet_unref(pkt);
                continue;
            }
        }
        int got;
        while ((got = avcodec_receive_frame(dec, frame)) >= 0) {
            int64_t pts = frame->pts;
            if (pts == AV_NOPTS_VALUE) goto fail;
            if (pts >= to_pts) {
                stop = 1;
                av_frame_unref(frame);
                break;
            }
            if (pts >= from_pts) {
                if (av_frame_ref(out, frame) < 0) goto fail;
                out->pts = (pts - w->first_pts) / fdur;
                out->pict_type = AV_PICTURE_TYPE_NONE;
                int er = sink_encode(sink, w, out);
                av_frame_unref(out);
                if (er < 0) goto fail;
                written++;
            }
            av_frame_unref(frame);
        }
        if (!stop) {
            if (got == AVERROR_EOF) {
                stop = 1;
            } else if (got != AVERROR(EAGAIN)) {
                goto fail;
            }
        }
    }
    if (sink_encode(sink, w, NULL) < 0) goto fail;
    goto done;
fail:
    result = -1;
done:
    avcodec_free_context(&dec);
    av_packet_free(&pkt);
    av_frame_free(&frame);
    av_frame_free(&out);
    return result < 0 ? -1 : written;
}

/* What the cut of one range will do, worked out before anything is written. */
typedef struct {
    int64_t in_ts, out_ts;
    int64_t seek_pts;  /* keyframe the head (or the whole range) is decoded from */
    int64_t first_pts; /* source pts of the range's first picture */
    int64_t key_in;    /* copy starts here (when `copy`) */
    int64_t key_out;   /* copy ends here (when `copy`) */
    int64_t frames;    /* pictures the range holds */
    int copy;
} RangePlan;

/* Plans one range. Returns 0 (planned), 1 (not applicable) or -1 (unreadable timestamps). */
static int plan_range(AVFormatContext *ctx, int vidx, int nal_length_size, AVPacket *pkt,
                      int64_t fdur, int64_t in_ts, int64_t out_ts, RangePlan *plan) {
    KeyScan at_in, at_out;
    memset(plan, 0, sizeof(*plan));
    plan->in_ts = in_ts;
    plan->out_ts = out_ts;
    if (scan_keyframes(ctx, vidx, in_ts, nal_length_size, pkt, &at_in) < 0 ||
        scan_keyframes(ctx, vidx, out_ts, nal_length_size, pkt, &at_out) < 0) {
        return -1;
    }
    /* The copy starts at the first keyframe at or after the cut-in and ends at the last one at or
       before the cut-out; both must be IDR pictures so that nothing before them is needed. */
    int64_t key_in = at_in.last_le_pts == in_ts ? at_in.last_le_pts : at_in.first_gt_pts;
    int key_in_idr = at_in.last_le_pts == in_ts ? at_in.last_le_idr : at_in.first_gt_idr;
    int64_t key_out = at_out.last_le_pts;
    plan->copy = key_in != AV_NOPTS_VALUE && key_in_idr && at_out.last_le_idr && key_in < key_out;
    if (plan->copy) {
        plan->key_in = key_in;
        plan->key_out = key_out;
        /* Pictures sit at whole frames from the keyframe. */
        plan->first_pts = key_in - ((key_in - in_ts) / fdur) * fdur;
        plan->seek_pts = in_ts;
        /* A head decodes from the keyframe at or before the cut-in. */
        if (key_in > plan->first_pts && !at_in.last_le_idr) return 1;
        plan->frames = (out_ts - plan->first_pts + fdur - 1) / fdur;
    } else {
        /* Too short to copy anything: the whole range is decoded and encoded. */
        if (!at_in.last_le_idr) return 1;
        plan->first_pts =
            at_in.last_le_pts + ((in_ts - at_in.last_le_pts + fdur - 1) / fdur) * fdur;
        plan->seek_pts = in_ts;
        plan->frames = (out_ts - plan->first_pts + fdur - 1) / fdur;
    }
    return plan->frames > 0 ? 0 : 1;
}

/* pts - dts of the keyframe at `key`: the source's reorder delay. -1 when it can't be read. */
static int64_t keyframe_delay(AVFormatContext *ctx, int vidx, AVPacket *pkt, int64_t key) {
    if (av_seek_frame(ctx, vidx, key, AVSEEK_FLAG_BACKWARD) < 0) return -1;
    int64_t delay = -1;
    while (av_read_frame(ctx, pkt) >= 0) {
        if (pkt->stream_index == vidx) {
            if ((pkt->flags & AV_PKT_FLAG_KEY) && pkt->pts == key) delay = pkt->pts - pkt->dts;
            av_packet_unref(pkt);
            break;
        }
        av_packet_unref(pkt);
    }
    return delay;
}

SmartCutStatus avbridge_smart_cut_video(const char *in_path, const double *in_secs,
                                        const double *out_secs, int range_count, int canvas_width,
                                        int canvas_height, int canvas_fps_num, int canvas_fps_den,
                                        int64_t canvas_bit_rate_bps, const char *out_path,
                                        ProgressCallback progress_cb, void *progress_user_data,
                                        const uint8_t *cancel) {
    SmartCutStatus status = SMART_CUT_NOT_APPLICABLE;
    AVFormatContext *in_ctx = NULL;
    AVFormatContext *out_ctx = NULL;
    AVPacket *pkt = NULL;
    Writer w = {0};
    FrameSink head = {0}, tail = {0};
    RangePlan *plans = NULL;
    int opened_output = 0;
    int cancelled = 0;

    if (range_count < 1) return status;
    for (int i = 0; i < range_count; i++) {
        if (!(out_secs[i] > in_secs[i]) || in_secs[i] < 0.0) return status;
    }
    if (open_input(in_path, &in_ctx) != 0) return status;
    int vidx = -1;
    for (unsigned i = 0; i < in_ctx->nb_streams; i++) {
        if (in_ctx->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_VIDEO) {
            vidx = (int)i;
            break;
        }
    }
    if (vidx < 0) goto done;
    AVStream *vs = in_ctx->streams[vidx];
    const AVCodecParameters *par = vs->codecpar;
    if (par->codec_id != AV_CODEC_ID_H264 || par->width != canvas_width ||
        par->height != canvas_height || par->format != AV_PIX_FMT_YUV420P ||
        par->extradata_size < 7 || par->extradata[0] != 1) {
        goto done;
    }
    for (unsigned i = 0; i < in_ctx->nb_streams; i++) {
        if ((int)i != vidx) in_ctx->streams[i]->discard = AVDISCARD_ALL;
    }
    AVRational tb = vs->time_base;
    AVRational fps = {canvas_fps_num, canvas_fps_den};
    int64_t fdur = av_rescale_q(1, av_inv_q(fps), tb);
    /* A constant frame rate that is a whole number of ticks per frame, or the timestamps below
       would not land on frames. */
    if (fdur <= 0 || (int64_t)fdur * tb.num * canvas_fps_num != (int64_t)tb.den * canvas_fps_den) {
        goto done;
    }
    int nal_length_size = (par->extradata[4] & 3) + 1;

    pkt = av_packet_alloc();
    plans = av_calloc((size_t)range_count, sizeof(*plans));
    if (!pkt || !plans) goto done;
    int64_t delay = -1;
    int copies = 0;
    for (int i = 0; i < range_count; i++) {
        int64_t in_ts = av_rescale_q((int64_t)(in_secs[i] * AV_TIME_BASE), AV_TIME_BASE_Q, tb);
        int64_t out_ts = av_rescale_q((int64_t)(out_secs[i] * AV_TIME_BASE), AV_TIME_BASE_Q, tb);
        if (plan_range(in_ctx, vidx, nal_length_size, pkt, fdur, in_ts, out_ts, &plans[i]) != 0) {
            goto done;
        }
        if (plans[i].copy) {
            int64_t d = keyframe_delay(in_ctx, vidx, pkt, plans[i].key_in);
            /* One reorder delay for the whole output. */
            if (d < 0 || (delay >= 0 && d != delay)) goto done;
            delay = d;
            copies++;
        }
    }
    /* Nothing to copy means nothing saved. */
    if (copies == 0) goto done;
    w.delay = delay;

    /* From here on something is written: a failure is an error, not a reason to fall back
       silently with a half-written file. */
    status = SMART_CUT_ERROR;

    /* Output container: the source's parameters as Annex B, so packets of both origins agree. */
    w.annexb = NULL;
    const AVBitStreamFilter *toannexb = av_bsf_get_by_name("h264_mp4toannexb");
    if (!toannexb || av_bsf_alloc(toannexb, &w.annexb) < 0) goto done;
    if (avcodec_parameters_copy(w.annexb->par_in, par) < 0) goto done;
    w.annexb->time_base_in = tb;
    if (av_bsf_init(w.annexb) < 0) goto done;
    w.scratch = av_packet_alloc();
    if (!w.scratch) goto done;

    avformat_alloc_output_context2(&out_ctx, NULL, NULL, out_path);
    if (!out_ctx) goto done;
    AVStream *os = avformat_new_stream(out_ctx, NULL);
    if (!os || avcodec_parameters_copy(os->codecpar, w.annexb->par_out) < 0) goto done;
    os->codecpar->codec_tag = 0;
    os->time_base = tb;
    os->avg_frame_rate = fps;
    if (!(out_ctx->oformat->flags & AVFMT_NOFILE) &&
        avio_open(&out_ctx->pb, out_path, AVIO_FLAG_WRITE) < 0) {
        goto done;
    }
    opened_output = 1;
    if (avformat_write_header(out_ctx, NULL) < 0) goto done;
    w.out_ctx = out_ctx;
    w.out_stream = os;

    int64_t offset = 0; /* output ticks written so far */
    for (int r = 0; r < range_count; r++) {
        const RangePlan *p = &plans[r];
        /* A source pts becomes (pts - first_pts) here, so this range starts at `offset`. */
        w.first_pts = p->first_pts - offset;

        if (!p->copy) {
            if (sink_open(&head, canvas_width, canvas_height, fps, canvas_bit_rate_bps, fdur,
                          p->first_pts, w.delay) < 0) {
                goto done;
            }
            if (reencode_range(in_ctx, vidx, vs, p->seek_pts, p->first_pts, p->out_ts,
                               AV_NOPTS_VALUE, &head, &w, fdur, cancel, &cancelled) < 0) {
                goto done;
            }
            sink_close(&head);
            memset(&head, 0, sizeof(head));
        } else {
            /* Head: from the cut-in to the keyframe the copy starts at. */
            if (p->key_in > p->first_pts) {
                if (sink_open(&head, canvas_width, canvas_height, fps, canvas_bit_rate_bps, fdur,
                              p->first_pts, w.delay) < 0) {
                    goto done;
                }
                if (reencode_range(in_ctx, vidx, vs, p->in_ts, p->first_pts, p->key_in, p->key_in,
                                   &head, &w, fdur, cancel, &cancelled) < 0) {
                    goto done;
                }
                sink_close(&head);
                memset(&head, 0, sizeof(head));
            }
            if (cancelled) {
                status = SMART_CUT_CANCELLED;
                goto done;
            }

            /* Middle: copied. */
            if (av_seek_frame(in_ctx, vidx, p->key_in, AVSEEK_FLAG_BACKWARD) < 0) goto done;
            while (av_read_frame(in_ctx, pkt) >= 0) {
                if (pkt->stream_index != vidx) {
                    av_packet_unref(pkt);
                    continue;
                }
                if ((pkt->flags & AV_PKT_FLAG_KEY) && pkt->pts == p->key_out) {
                    av_packet_unref(pkt);
                    break;
                }
                if (cancel && *cancel) {
                    av_packet_unref(pkt);
                    status = SMART_CUT_CANCELLED;
                    goto done;
                }
                if (progress_cb && pkt->pts != AV_NOPTS_VALUE) {
                    progress_cb(progress_user_data, (double)(pkt->pts - w.first_pts) * av_q2d(tb));
                }
                if (copy_packet(&w, pkt) < 0) {
                    av_packet_unref(pkt);
                    goto done;
                }
                av_packet_unref(pkt);
            }
            /* Flush the filter so nothing it holds is lost, and make it usable for the next range.
             */
            av_bsf_send_packet(w.annexb, NULL);
            while (av_bsf_receive_packet(w.annexb, w.scratch) >= 0) {
                write_annexb(&w, w.scratch);
                av_packet_unref(w.scratch);
            }
            av_bsf_flush(w.annexb);

            /* Tail: from the last keyframe to the cut-out. */
            if (p->out_ts > p->key_out) {
                if (sink_open(&tail, canvas_width, canvas_height, fps, canvas_bit_rate_bps, fdur,
                              p->first_pts, w.delay) < 0) {
                    goto done;
                }
                if (reencode_range(in_ctx, vidx, vs, p->key_out, p->key_out, p->out_ts,
                                   AV_NOPTS_VALUE, &tail, &w, fdur, cancel, &cancelled) < 0) {
                    goto done;
                }
                sink_close(&tail);
                memset(&tail, 0, sizeof(tail));
            }
        }
        if (cancelled) {
            status = SMART_CUT_CANCELLED;
            goto done;
        }
        offset += p->frames * fdur;
    }
    if (av_write_trailer(out_ctx) < 0) goto done;
    record_video_encoder_name("copy");
    status = SMART_CUT_OK;

done:
    sink_close(&head);
    sink_close(&tail);
    av_free(plans);
    av_packet_free(&pkt);
    av_packet_free(&w.scratch);
    av_bsf_free(&w.annexb);
    if (out_ctx) {
        if (opened_output && !(out_ctx->oformat->flags & AVFMT_NOFILE) && out_ctx->pb) {
            avio_closep(&out_ctx->pb);
        }
        avformat_free_context(out_ctx);
    }
    avformat_close_input(&in_ctx);
    return status;
}
