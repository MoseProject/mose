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
#include <string.h>

/* Defensive upper bound on one segment's filter_desc length, mirroring text_overlay.c's
   TEXT_OVERLAY_SEG_MAX -- unlike that file's fixed per-segment stack buffers, this file already
   sizes filter_buf_size exactly to each segment's real strlen() (no truncation risk today), so
   this exists purely to bound the av_malloc below against unbounded growth if a future
   regression in the Rust-side generator (crate::shape_render::build_shape_filter_desc) ever let
   filter_desc grow without limit -- e.g. a pathologically large user-drawn polygon. Generous
   enough for any realistic shape (hundreds of vertices, each ray-casting term is roughly
   150-250 bytes, repeated per color channel) without being a true "unbounded" allocation. */
#define SHAPE_OVERLAY_SEG_MAX ((size_t)256 * 1024)

/* Opens `in_path`, chains each segment's pre-built geq filter node, and writes the result to
   `out_path` — structurally identical to avbridge_apply_text_overlays (see that function's
   comments for the decode/filter/encode pipeline details); only the filter-string-building
   step differs. */
TextOverlayStatus avbridge_apply_shape_overlays(const char *in_path, const char *out_path,
                                                const ShapeSegment *segments, int segment_count,
                                                int canvas_width, int canvas_height,
                                                int canvas_fps_num, int canvas_fps_den) {

    if (segment_count <= 0) return TEXT_OVERLAY_OK;

    TextOverlayStatus status = TEXT_OVERLAY_OK;
    char *filter_str = NULL;

    /* Chain each segment's pre-built geq filter node with commas — the geometry/color/rotation
       math already happened in Rust (see bridge.h's ShapeSegment doc comment).

       Wrapped in format=yuv444p ... format=yuv420p: geq's X/Y coordinate expressions are
       evaluated per-plane, but yuv420p's cb/cr planes are subsampled to half the luma plane's
       resolution — the SAME (X,Y) pixel offsets that place a shape correctly on the lum plane
       land at the wrong spot (or out of range) on cb/cr, corrupting the color there (confirmed
       empirically: a shape's cb channel alone, filtered directly against a yuv420p source,
       rendered as a stray block in the frame corner instead of the intended shape — a
       chroma-plane/luma-plane coordinate mismatch, not a geq math bug). yuv444p has no chroma
       subsampling, so X/Y mean the same full-resolution grid on every plane; converting back to
       yuv420p afterward keeps the libopenh264 encoder (which wants yuv420p) unaffected. */
    {
        static const char PREFIX[] = "format=yuv444p,";
        static const char SUFFIX[] = ",format=yuv420p";
        size_t filter_buf_size = sizeof(PREFIX) + sizeof(SUFFIX) + 8;
        for (int i = 0; i < segment_count; i++) {
            size_t seg_len = strlen(segments[i].filter_desc);
            if (seg_len > SHAPE_OVERLAY_SEG_MAX) {
                status = TEXT_OVERLAY_ERR_FILTER_GRAPH;
                goto cleanup;
            }
            filter_buf_size += seg_len + 1;
        }
        filter_str = av_malloc(filter_buf_size);
        if (!filter_str) {
            status = TEXT_OVERLAY_ERR_FILTER_GRAPH;
            goto cleanup;
        }

        size_t pos = 0;
        memcpy(filter_str + pos, PREFIX, sizeof(PREFIX) - 1);
        pos += sizeof(PREFIX) - 1;
        for (int i = 0; i < segment_count; i++) {
            if (i > 0) filter_str[pos++] = ',';
            size_t len = strlen(segments[i].filter_desc);
            memcpy(filter_str + pos, segments[i].filter_desc, len);
            pos += len;
        }
        memcpy(filter_str + pos, SUFFIX, sizeof(SUFFIX) - 1);
        pos += sizeof(SUFFIX) - 1;
        filter_str[pos] = '\0';
    }

    status = (TextOverlayStatus)avbridge_reencode_with_filter(in_path, out_path, filter_str,
                                                              canvas_width, canvas_height,
                                                              canvas_fps_num, canvas_fps_den, 0);

cleanup:
    av_free(filter_str);
    return status;
}
