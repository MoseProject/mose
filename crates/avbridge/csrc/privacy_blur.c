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

/* CF-09 slice 4 ("Support privacy blur as the first end-to-end effect before general selective
   effects", spec/architecture/competitive-feature-plan.md) — structurally identical to
   avbridge_apply_text_overlays/_shape_overlays (see those functions' comments for the full
   decode/filter/encode pipeline shape); only the filter-string-building step differs, and unlike
   those two this graph has two sources (the main video via buffersrc, the matte video via a
   `movie=` filter source) feeding one `maskedmerge` node instead of chaining N overlay/geq
   segments onto one source. */

/* Escapes `in` for use as a single-quoted avfilter movie filename — identical in behavior to
   text_overlay.c's own static helper of the same name (each csrc file that needs this keeps its
   own copy rather than sharing one across translation units, matching this codebase's existing
   convention: see shape_overlay.c's own escape-free but otherwise self-contained structure).
   Writes into `out` (size `out_size`); returns bytes written excluding NUL, or -1 on overflow. */
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

/* The graph for one privacy-blur segment, [in] to [out], in a newly allocated string (free it with
   av_free). NULL when the path or sigma are unusable. See avbridge_apply_privacy_blur for what
   each stage is for. */
static char *build_privacy_blur_filter(const char *matte_path, double blur_sigma, int canvas_width,
                                       int canvas_height) {
    if (blur_sigma <= 0.0) return NULL;
    char escaped_matte[1024];
    if (escape_filter_path(escaped_matte, sizeof(escaped_matte), matte_path) < 0) return NULL;
    size_t size = sizeof(escaped_matte) + 512;
    char *filter_str = av_malloc(size);
    if (!filter_str) return NULL;
    int written = snprintf(filter_str, size,
                           "movie=filename='%s':loop=1,format=yuv420p,scale=%d:%d[mask];"
                           "[in]format=yuv420p,split=2[base][to_blur];"
                           "[to_blur]gblur=sigma=%f[blurred];"
                           "[base][blurred][mask]maskedmerge[out]",
                           escaped_matte, canvas_width, canvas_height, blur_sigma);
    if (written < 0 || (size_t)written >= size) {
        av_free(filter_str);
        return NULL;
    }
    return filter_str;
}

char *avbridge_privacy_blur_filter(const char *matte_path, double blur_sigma, int canvas_width,
                                   int canvas_height) {
    /* Released with avbridge_free_string, like the overlay filters. */
    return build_privacy_blur_filter(matte_path, blur_sigma, canvas_width, canvas_height);
}

PrivacyBlurStatus avbridge_apply_privacy_blur(const char *in_path, const char *out_path,
                                              const char *matte_path, double blur_sigma,
                                              int canvas_width, int canvas_height,
                                              int canvas_fps_num, int canvas_fps_den) {

    /* No meaningful "zero blur" — a caller wanting the frame untouched should skip calling this
       function entirely, same convention avbridge_apply_text_overlays/_shape_overlays use for
       segment_count <= 0. gblur itself also rejects a non-positive sigma at graph-config time,
       but checking here gives a specific, actionable status instead of a generic filter-graph
       failure. */
    if (blur_sigma <= 0.0) return PRIVACY_BLUR_ERR_FILTER_GRAPH;

    PrivacyBlurStatus status = PRIVACY_BLUR_OK;
    char *filter_str = NULL;

    /* Builds:
         movie=filename='<matte>':loop=1,format=yuv420p,scale=W:H[mask];
         [in]format=yuv420p,split=2[base][to_blur];
         [to_blur]gblur=sigma=<sigma>[blurred];
         [base][blurred][mask]maskedmerge[out]

       `format=yuv420p` on both the main branch and the matte movie source keeps every input to
       maskedmerge in the same planar format (maskedmerge's own docs require base/overlay/mask to
       share pixel format) — the matte is already produced as yuv420p by
       avbridge_encode_matte_video, but a caller-supplied matte of a different format is
       normalized here rather than trusted. `scale=W:H` on the matte guards against a matte
       encoded at a different resolution than the main video (e.g. built from downscaled sample
       frames) silently failing maskedmerge's own equal-dimensions requirement. `loop=1` plays it
       once and `maskedmerge` holds its last frame if it runs shorter than the main video, rather
       than the movie source hitting EOF and stalling the graph early (a looping source never ends,
       so the graph could not finish when the video does) -- a caller building the matte from fewer
       sample points than the export's own frame count (a coarser sampling cadence, not every frame)
       is expected and shouldn't truncate the output. */
    filter_str = build_privacy_blur_filter(matte_path, blur_sigma, canvas_width, canvas_height);
    if (!filter_str) {
        status = PRIVACY_BLUR_ERR_FILTER_GRAPH;
        goto cleanup;
    }

    status = (PrivacyBlurStatus)avbridge_reencode_with_filter(in_path, out_path, filter_str,
                                                              canvas_width, canvas_height,
                                                              canvas_fps_num, canvas_fps_den, 1);

cleanup:
    av_free(filter_str);
    return status;
}
