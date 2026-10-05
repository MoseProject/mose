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
#include <libavfilter/avfilter.h>
#include <libavfilter/buffersink.h>
#include <libavfilter/buffersrc.h>
#include <libavformat/avformat.h>
#include <libavutil/opt.h>
#include <stdarg.h>

/* Maximum byte length of one movie+overlay filter fragment (per segment) -- sized to fit the
   escaped path plus the escaped opacity expr and the escaped scale/rotation sample exprs (each
   up to 2048 bytes) with room to spare; the snprintf truncation check below is the real safety
   net regardless. */
#define TEXT_OVERLAY_SEG_MAX 16384

/* Appends one snprintf-formatted fragment to `buf` at `*pos` (buffer size `buf_size`),
   advancing `*pos` past it. Returns 0 on success, -1 on truncation. */
static int append_stage(char *buf, size_t buf_size, size_t *pos, const char *fmt, ...) {
    va_list args;
    va_start(args, fmt);
    int written = vsnprintf(buf + *pos, buf_size - *pos, fmt, args);
    va_end(args);
    if (written < 0 || *pos + (size_t)written >= buf_size) return -1;
    *pos += (size_t)written;
    return 0;
}

/* Escapes `in` for use as a single-quoted avfilter movie filename. Backslash, single-quote,
   colon, comma, semicolon, and graph-label brackets each get a backslash prefix.
   Writes into `out` (size `out_size`); returns bytes written excluding NUL, or -1 on
   buffer overflow. */
static int escape_filter_path(char *out, size_t out_size, const char *in) {
    size_t pos = 0;
    for (size_t i = 0; in[i] != '\0'; i++) {
        char c = in[i];
        int needs_escape =
            (c == '\\' || c == '\'' || c == ':' || c == ',' || c == ';' || c == '[' || c == ']');
        if (pos + (size_t)(needs_escape ? 2 : 1) + 1 > out_size) return -1;
        if (needs_escape) out[pos++] = '\\';
        out[pos++] = c;
    }
    out[pos] = '\0';
    return (int)pos;
}

/* Escapes `in` (a geq-expression-language fragment from
   avcore::keyframe::text_opacity_alpha_expr) for embedding as one avfilter option value inside
   the whole-graph chain description this file builds via avfilter_graph_parse_ptr -- only
   commas need escaping (the expression is built purely from digits/letters/operators/parens/
   dots/minus signs plus commas as `if`/`between`'s argument separators), since a bare comma at
   that level would otherwise end the current filter and start a new one in the chain. Writes
   into `out` (size `out_size`); returns bytes written excluding NUL, or -1 on buffer overflow. */
static int escape_filter_expr(char *out, size_t out_size, const char *in) {
    size_t pos = 0;
    for (size_t i = 0; in[i] != '\0'; i++) {
        char c = in[i];
        if (pos + (size_t)(c == ',' ? 2 : 1) + 1 > out_size) return -1;
        if (c == ',') out[pos++] = '\\';
        out[pos++] = c;
    }
    out[pos] = '\0';
    return (int)pos;
}

/* Opens `in_path` (an already-rendered H.264/AAC mp4 produced by
   avbridge_encode_timeline_export), composites pre-rasterized RGBA PNG overlays, and
   writes the result to `out_path`. Video is decoded, filtered, and re-encoded via
   libopenh264; audio is stream-copied unchanged to avoid a lossy second AAC encode.
   canvas_width/height and canvas_fps_num/den must match the rendered video so the output
   encoder is configured correctly.
   Returns TEXT_OVERLAY_OK immediately without touching any files when segment_count <= 0. */
TextOverlayStatus avbridge_apply_text_overlays(const char *in_path, const char *out_path,
                                               const TextSegment *segments, int segment_count,
                                               int canvas_width, int canvas_height,
                                               int canvas_fps_num, int canvas_fps_den) {

    if (segment_count <= 0) return TEXT_OVERLAY_OK;

    TextOverlayStatus status = TEXT_OVERLAY_OK;
    char *filter_str = NULL;

    /* Open input */
    /* Build one movie source and one timeline-enabled overlay node per pre-rasterized PNG, with
       an optional geq scale-remap stage and/or an optional geq alpha-multiply (fade) stage
       chained in between, for segments that have a scale_keyframe_expr_x/_y and/or an
       opacity_keyframe_expr. */
    {
        size_t filter_buf_size = (size_t)segment_count * TEXT_OVERLAY_SEG_MAX + 8;
        filter_str = av_malloc(filter_buf_size);
        if (!filter_str) {
            status = TEXT_OVERLAY_ERR_FILTER_GRAPH;
            goto cleanup;
        }

        size_t pos = 0;
        for (int i = 0; i < segment_count; i++) {
            const TextSegment *seg = &segments[i];
            char escaped[1024];
            if (escape_filter_path(escaped, sizeof(escaped), seg->overlay_path) < 0) {
                status = TEXT_OVERLAY_ERR_FILTER_GRAPH;
                goto cleanup;
            }
            int has_fade = seg->opacity_keyframe_expr && seg->opacity_keyframe_expr[0] != '\0';
            char escaped_fade[2048];
            if (has_fade && escape_filter_expr(escaped_fade, sizeof(escaped_fade),
                                               seg->opacity_keyframe_expr) < 0) {
                status = TEXT_OVERLAY_ERR_FILTER_GRAPH;
                goto cleanup;
            }
            int has_scale = seg->scale_keyframe_expr_x && seg->scale_keyframe_expr_x[0] != '\0' &&
                            seg->scale_keyframe_expr_y && seg->scale_keyframe_expr_y[0] != '\0';
            char escaped_scale_x[2048];
            char escaped_scale_y[2048];
            if (has_scale && (escape_filter_expr(escaped_scale_x, sizeof(escaped_scale_x),
                                                 seg->scale_keyframe_expr_x) < 0 ||
                              escape_filter_expr(escaped_scale_y, sizeof(escaped_scale_y),
                                                 seg->scale_keyframe_expr_y) < 0)) {
                status = TEXT_OVERLAY_ERR_FILTER_GRAPH;
                goto cleanup;
            }
            int has_rotation =
                seg->rotation_keyframe_expr_x && seg->rotation_keyframe_expr_x[0] != '\0' &&
                seg->rotation_keyframe_expr_y && seg->rotation_keyframe_expr_y[0] != '\0';
            char escaped_rotation_x[2048];
            char escaped_rotation_y[2048];
            if (has_rotation && (escape_filter_expr(escaped_rotation_x, sizeof(escaped_rotation_x),
                                                    seg->rotation_keyframe_expr_x) < 0 ||
                                 escape_filter_expr(escaped_rotation_y, sizeof(escaped_rotation_y),
                                                    seg->rotation_keyframe_expr_y) < 0)) {
                status = TEXT_OVERLAY_ERR_FILTER_GRAPH;
                goto cleanup;
            }
            /* Unescaped and single-quoted, same convention as timeline_export_multi.c's
               build_overlay_vfilter -- an overlay x=/y= expression sits directly in its own
               quoted option value, not nested inside another quoted expression the way the
               fade/scale geq stages below are, so it needs no comma-escaping of its own. */
            const char *pos_expr_x =
                (seg->position_keyframe_expr_x && seg->position_keyframe_expr_x[0] != '\0')
                    ? seg->position_keyframe_expr_x
                    : "0";
            const char *pos_expr_y =
                (seg->position_keyframe_expr_y && seg->position_keyframe_expr_y[0] != '\0')
                    ? seg->position_keyframe_expr_y
                    : "0";
            double end_secs = seg->start_secs + seg->duration_secs;
            char main_label[32];
            char out_label[32];
            char cur_label[32];
            if (i == 0)
                snprintf(main_label, sizeof(main_label), "in");
            else
                snprintf(main_label, sizeof(main_label), "v%d", i - 1);
            if (i == segment_count - 1)
                snprintf(out_label, sizeof(out_label), "out");
            else
                snprintf(out_label, sizeof(out_label), "v%d", i);
            snprintf(cur_label, sizeof(cur_label), "text%d_src", i);

            /* `repeatlast=1` holds the PNG's only frame for the full main-video timeline;
               enable='between(...)' controls the actual clip visibility window. Confirmed for
               real against this project's linked FFmpeg build (a dedicated avbridge test, not
               just the docs) that geq's alpha read-back function is spelled `alpha(X,Y)`, not
               `a(X,Y)` as FFmpeg's own docs otherwise imply -- `a(X,Y)` parses as "Unknown
               function" here; two other candidates were ruled out the same way
               (colorchannelmixer has no `eval` option at all in this build; its `t`/`n`
               per-frame variables were never reached). See
               keyframe::text_opacity_alpha_expr's doc comment for the full story. */
            if (append_stage(filter_str, filter_buf_size, &pos,
                             "movie=filename='%s',format=rgba[%s];", escaped, cur_label) < 0) {
                status = TEXT_OVERLAY_ERR_FILTER_GRAPH;
                goto cleanup;
            }

            if (has_scale) {
                char next_label[32];
                snprintf(next_label, sizeof(next_label), "text%d_scaled", i);
                /* geq inverse-sample remap around the raster's own anchor point: each output
                   pixel (X,Y) samples r/g/b/a from the pre-scale image at the position that
                   would land on (X,Y) after scaling by the (possibly time-varying) factor
                   avcore::keyframe::text_scale_sample_exprs already folded into
                   escaped_scale_x/y. Output frame size never changes, only what each pixel
                   samples -- deliberately not `scale=...:eval=frame`, which reliably corrupted
                   the heap in a real export elsewhere in this codebase (see
                   timeline_export.c's zoom-transition case, CLAUDE.md). */
                if (append_stage(filter_str, filter_buf_size, &pos,
                                 "[%s]geq=r='r(%s\\,%s)':g='g(%s\\,%s)':b='b(%s\\,%s)':"
                                 "a='alpha(%s\\,%s)'[%s];",
                                 cur_label, escaped_scale_x, escaped_scale_y, escaped_scale_x,
                                 escaped_scale_y, escaped_scale_x, escaped_scale_y, escaped_scale_x,
                                 escaped_scale_y, next_label) < 0) {
                    status = TEXT_OVERLAY_ERR_FILTER_GRAPH;
                    goto cleanup;
                }
                snprintf(cur_label, sizeof(cur_label), "%s", next_label);
            }

            if (has_rotation) {
                char next_label[32];
                snprintf(next_label, sizeof(next_label), "text%d_rotated", i);
                /* Same inverse-sample contract as the scale stage above, around the same
                   anchor -- composes independently of it (an isotropic scale and a rotation
                   around the same center commute, see keyframe::text_rotation_sample_exprs'
                   doc comment), so stage order relative to the scale stage doesn't matter. */
                if (append_stage(filter_str, filter_buf_size, &pos,
                                 "[%s]geq=r='r(%s\\,%s)':g='g(%s\\,%s)':b='b(%s\\,%s)':"
                                 "a='alpha(%s\\,%s)'[%s];",
                                 cur_label, escaped_rotation_x, escaped_rotation_y,
                                 escaped_rotation_x, escaped_rotation_y, escaped_rotation_x,
                                 escaped_rotation_y, escaped_rotation_x, escaped_rotation_y,
                                 next_label) < 0) {
                    status = TEXT_OVERLAY_ERR_FILTER_GRAPH;
                    goto cleanup;
                }
                snprintf(cur_label, sizeof(cur_label), "%s", next_label);
            }

            if (has_fade) {
                char next_label[32];
                snprintf(next_label, sizeof(next_label), "text%d_faded", i);
                /* alpha(X,Y) here reads whatever the previous stage (the plain raster, or the
                   scale remap above) left in the alpha channel at each pixel; r/g/b pass
                   through unchanged. */
                if (append_stage(filter_str, filter_buf_size, &pos,
                                 "[%s]geq=r='r(X\\,Y)':g='g(X\\,Y)':b='b(X\\,Y)':"
                                 "a='alpha(X\\,Y)*(%s)'[%s];",
                                 cur_label, escaped_fade, next_label) < 0) {
                    status = TEXT_OVERLAY_ERR_FILTER_GRAPH;
                    goto cleanup;
                }
                snprintf(cur_label, sizeof(cur_label), "%s", next_label);
            }

            if (append_stage(filter_str, filter_buf_size, &pos,
                             "[%s][%s]overlay=x='%s':y='%s':format=auto:alpha=straight:"
                             "repeatlast=1:eof_action=repeat:enable='between(t\\,%.4f\\,%.4f)'"
                             "[%s]%s",
                             main_label, cur_label, pos_expr_x, pos_expr_y, seg->start_secs,
                             end_secs, out_label, i == segment_count - 1 ? "" : ";") < 0) {
                status = TEXT_OVERLAY_ERR_FILTER_GRAPH;
                goto cleanup;
            }
        }
        filter_str[pos] = '\0';
    }

    status = (TextOverlayStatus)avbridge_reencode_with_filter(in_path, out_path, filter_str, canvas_width,
                                                       canvas_height, canvas_fps_num, canvas_fps_den,
                                                       0);

cleanup:
    av_free(filter_str);
    return status;
}
