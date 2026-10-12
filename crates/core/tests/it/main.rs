// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Every integration test of `core` in one binary. Cargo links each file under `tests/` as its
//! own executable, and with this crate's native dependencies (FFmpeg, ONNX Runtime, whisper.cpp)
//! that made linking ~30 binaries dominate every rebuild. Add a new test file here and declare it
//! below; run one with `cargo test -p core --test it <module>::`.

mod background_removal_test;
mod bundle_runtime_test;
mod bundle_test;
mod clip_gaps_test;
mod collab_bundle_test;
mod error_reporting_test;
mod export_perf_test;
mod frame_sampler_test;
mod import_perf_test;
mod loudness_test;
mod lut_test;
mod media_test;
mod oracle_test;
mod persistence_test;
mod probe_test;
mod project_test;
mod proxy_test;
mod qa_files_and_text_test;
mod qa_track_invariants_test;
mod recovery_test;
mod release_check_test;
mod render_overlay_passes_test;
mod render_test;
mod sound_library_test;
mod telemetry_test;
mod timeline_export_multi_test;
mod timeline_export_test;
mod timeline_groups_test;
mod timeline_test;
mod transcribe_test;
mod undo_test;
mod voice_cleanup_preview_test;
mod waveform_test;
