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

int open_input(const char *path, AVFormatContext **fmt_ctx_out) {
    if (avformat_open_input(fmt_ctx_out, path, NULL, NULL) < 0) return -1;
    if (avformat_find_stream_info(*fmt_ctx_out, NULL) < 0) {
        avformat_close_input(fmt_ctx_out);
        return -2;
    }
    return 0;
}
