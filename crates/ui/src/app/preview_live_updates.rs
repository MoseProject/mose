// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Dispatches core-owned live property updates to the loaded preview pipeline.

use super::App;

impl App {
    pub(super) fn push_live_clip_updates(&mut self, clip: &avcore::timeline::ClipInstance) {
        let video_loaded = self.preview_state.preview_clip_id == Some(clip.id)
            || self
                .preview_state
                .preview_overlay_clip_ids
                .contains(&clip.id);
        if !video_loaded {
            return;
        }
        if let Some(worker) = &self.preview_state.worker {
            for update in avcore::LivePreviewUpdate::for_clip(clip) {
                worker.live_update(update);
            }
        }
    }
}
