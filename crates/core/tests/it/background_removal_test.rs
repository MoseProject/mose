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

use std::path::{Path, PathBuf};

use avcore::background_removal::{mask_cache_dir_for_project, mask_path_for_clip};
use avcore::project::{Project, Sequence};
use avcore::timeline::Timeline;
use avcore::{MediaAsset, Recency};

fn test_project(file_path: Option<PathBuf>) -> Project {
    Project {
        id: 1,
        name: "Test".to_string(),
        last_edited: Recency::HoursAgo(0),
        summary: String::new(),
        media_library: Vec::<MediaAsset>::new(),
        sequences: vec![Sequence {
            id: 1,
            name: "Main".to_string(),
            timeline: Timeline {
                tracks: Vec::new(),
                playhead_secs: 0.0,
                markers: Vec::new(),
                multicam_groups: Vec::new(),
            },
            export_settings: Default::default(),
        }],
        active_sequence: 0,
        file_path,
        panel_layout: None,
        smart_bins: Vec::new(),
        recent_asset_ids: Vec::new(),
    }
}

#[test]
fn mask_cache_dir_is_a_hidden_sibling_of_a_saved_project_file() {
    let project = test_project(Some(PathBuf::from("/projects/boss_fights.json")));

    assert_eq!(
        mask_cache_dir_for_project(&project),
        Path::new("/projects/.boss_fights_mattes")
    );
}

#[test]
fn mask_cache_dir_falls_back_to_a_temp_dir_for_an_unsaved_project() {
    let project = test_project(None);

    assert_eq!(
        mask_cache_dir_for_project(&project),
        std::env::temp_dir().join("oca_unsaved_mattes")
    );
}

#[test]
fn mask_path_is_keyed_by_clip_id_inside_the_mask_dir() {
    let dir = Path::new("/cache/.myproject_mattes");

    assert_eq!(
        mask_path_for_clip(42, dir),
        Path::new("/cache/.myproject_mattes/clip_42_matte.mp4")
    );
}

#[test]
fn mask_path_differs_for_different_clip_ids_on_the_same_asset() {
    let dir = Path::new("/cache/.myproject_mattes");

    assert_ne!(mask_path_for_clip(1, dir), mask_path_for_clip(2, dir));
}
