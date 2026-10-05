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

use serde::{Deserialize, Serialize};

/// A review/comment marker's category — Final Cut Pro's typed-marker model (per `ROADMAP.md`
/// P2 item 9), not just a plain unstyled note: `ToDo` tracks a `completed` state a searchable
/// Timeline Index panel can filter on, `Chapter` marks a navigable section boundary, `Standard`
/// is a plain annotation. `Highlight` (D2, `spec/architecture/differentiators.md`) marks an
/// auto-detected candidate moment — same non-destructive "add a marker, let the existing
/// Timeline Index panel's rename/delete be the review step" shape D4's Chapter markers already
/// established, rather than a separate accept/reject modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MarkerKind {
    #[default]
    Standard,
    ToDo,
    Chapter,
    Highlight,
}

impl MarkerKind {
    pub const ALL: &'static [MarkerKind] = &[
        MarkerKind::Standard,
        MarkerKind::ToDo,
        MarkerKind::Chapter,
        MarkerKind::Highlight,
    ];
}

/// One review/comment marker on the timeline — a point in time (not a clip, not tied to any
/// particular track) with a short label and a [`MarkerKind`]. `id`s are unique within a
/// [`super::Timeline`], same convention [`super::ClipInstance::id`]/[`super::TextClip::id`]
/// already use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    pub id: u64,
    pub position_secs: f64,
    pub label: String,
    pub kind: MarkerKind,
    /// Only meaningful for [`MarkerKind::ToDo`] — a searchable Timeline Index panel can filter
    /// these out once resolved without deleting the marker (the review history stays visible).
    #[serde(default)]
    pub completed: bool,
}
