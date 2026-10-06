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

//! The picture the webview shows while editing: the playback thread and its Bush adapter, preview frames, the `oca://` protocol and the tile decoders.

pub(crate) mod bush_adapter;
pub(crate) mod frames;
pub(crate) mod preview_engine;
pub(crate) mod protocol;
pub(crate) mod tile_workers;

pub(crate) mod mailbox;

pub(crate) mod media_work;
