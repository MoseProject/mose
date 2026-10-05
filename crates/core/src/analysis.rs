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

//! Analysis of footage and audio: detection, tracking, speech and model inference.

pub mod auto_reframe;
pub mod background_removal;
pub mod dynamic_reframe;
pub mod gameplay_events;
pub mod highlight_detection;
pub mod mask_propagation;
pub mod motion_tracking;
pub(crate) mod onnx_models;
pub mod privacy_blur;
pub mod scene_detection;
pub mod semantic_index;
pub mod silence_detection;
pub mod text_to_speech;
pub mod transcribe;
pub mod transcript;
pub mod transcript_proposals;
pub mod transcript_search;
pub(crate) mod whisper_model;
