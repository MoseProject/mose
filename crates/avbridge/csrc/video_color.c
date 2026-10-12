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

#include "bridge_internal.h"

#include <libavutil/pixdesc.h>

enum AVColorSpace avbridge_resolve_colorspace_for(enum AVColorSpace tagged, int width, int height) {
    switch (tagged) {
    case AVCOL_SPC_BT709:
    case AVCOL_SPC_BT470BG:
    case AVCOL_SPC_SMPTE170M:
    case AVCOL_SPC_SMPTE240M:
    case AVCOL_SPC_BT2020_NCL:
    case AVCOL_SPC_BT2020_CL:
        return tagged;
    default:
        break;
    }
    return (width >= 1280 || height >= 720) ? AVCOL_SPC_BT709 : AVCOL_SPC_SMPTE170M;
}

enum AVColorSpace avbridge_resolve_colorspace(const AVFrame *frame) {
    return avbridge_resolve_colorspace_for(frame->colorspace, frame->width, frame->height);
}

int avbridge_frame_is_full_range(const AVFrame *frame) {
    if (frame->color_range == AVCOL_RANGE_JPEG) return 1;
    switch (frame->format) {
    case AV_PIX_FMT_YUVJ420P:
    case AV_PIX_FMT_YUVJ422P:
    case AV_PIX_FMT_YUVJ444P:
    case AV_PIX_FMT_YUVJ440P:
        return 1;
    default:
        return 0;
    }
}
