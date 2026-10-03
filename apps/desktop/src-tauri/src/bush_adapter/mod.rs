// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Bush preview backend (feature `bush-preview`), the second engine behind `PreviewEngine`.
//!
//! This is the only code that knows both `avcore` types and Bush's scene model; `core` never
//! imports Bush. See `spec/architecture/bush-adapter-rfc.md` for the design, rollout and rollback.
//!
//! The engine is chosen at startup: `OCA_PREVIEW_ENGINE=bush` selects Bush, anything else keeps
//! GStreamer. `OCA_BUSH_RENDERER=gpu` lets Bush composite on the GPU (CPU otherwise).

mod backend;
mod mapper;

pub use backend::run;

/// Whether this process was asked to preview through Bush.
pub fn selected() -> bool {
    std::env::var("OCA_PREVIEW_ENGINE").is_ok_and(|value| value.eq_ignore_ascii_case("bush"))
}
