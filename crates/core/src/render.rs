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

//! Renders a normalized export from a source file — Fase 4's render step. Applies loudness
//! normalization (`loudnorm` + a true-peak safety limiter, per Fase 2) via `oca-avbridge`'s
//! encode FFI while copying video untouched, which is what makes the export's bitrate equal
//! the source's bitrate exactly, one of this editor's two headline differentiators.

mod errors;
mod jobs;
mod privacy;
mod segments;
mod shape;
mod text;
mod validation;

pub use crate::export::ExportAspectRatio;
pub use errors::*;
pub use jobs::*;
pub use privacy::*;
pub use segments::*;
pub use shape::*;
pub use text::*;

#[cfg(test)]
use validation::*;

#[cfg(test)]
#[path = "render/render_test.rs"]
mod tests;
