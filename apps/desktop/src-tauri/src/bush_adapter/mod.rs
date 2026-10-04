// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! The preview backend: the playback loop behind `PreviewEngine` and the mapper from the session
//! timeline to a Bush scene. It is the only preview engine of the app (GStreamer was removed).
//!
//! This is where `avcore` types meet Bush's scene model. See
//! `spec/architecture/bush-adapter-rfc.md` for the design.
//!
//! Developer switches (environment): `OCA_BUSH_RENDERER=gpu` composites on the GPU (CPU
//! otherwise) and `OCA_BUSH_DECODE=hardware` opts into hardware decode (software otherwise).

mod audio;
mod backend;
mod mapper;

pub use backend::run;
