//! The colour grade: the tools do what they say, a neutral grade is invisible, the grade survives
//! the project file and the clip's formatting, and the baked table the preview samples is the one
//! the export's `lut3d` reads, so both agree on a chart of colours (CM-05).

use std::process::Command;

use avcore::graphics::lut::Lut3D;
use avcore::{from_project_bytes, to_project_bytes};
use avcore::timeline::{ClipInstance, Grade};

fn luma(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

#[test]
fn a_neutral_grade_changes_nothing_and_bakes_to_the_identity() {
    let grade = Grade::default();
    assert!(grade.is_neutral());
    for rgb in [
        [0.0, 0.0, 0.0],
        [0.2, 0.5, 0.9],
        [1.0, 1.0, 1.0],
        [0.7, 0.1, 0.4],
    ] {
        let out = grade.apply(rgb);
        for i in 0..3 {
            assert!((out[i] - rgb[i]).abs() < 1e-5, "{rgb:?} -> {out:?}");
        }
    }
    let table = grade.bake(None);
    let sampled = table.sample(0.3, 0.6, 0.45);
    assert!((sampled[0] - 0.3).abs() < 1e-3 && (sampled[1] - 0.6).abs() < 1e-3);
    // Out-of-range input is clamped, never a panic.
    let out = grade.apply([f32::NAN, 2.0, -1.0]);
    assert!(out.iter().all(|c| c.is_finite() && (0.0..=1.0).contains(c)));
}

#[test]
fn each_tool_moves_the_picture_the_way_it_is_named() {
    let mid = [0.4, 0.4, 0.4];
    let with = |grade: Grade| grade.apply(mid);

    let brighter = with(Grade {
        exposure: 1.0,
        ..Default::default()
    });
    let darker = with(Grade {
        exposure: -1.0,
        ..Default::default()
    });
    assert!(brighter[0] > 0.4 && darker[0] < 0.4);
    // One stop is a doubling of linear light.
    let linear = |c: f32| c.powf(2.4);
    assert!((linear(brighter[0]) / linear(0.4) - 2.0).abs() < 0.02);

    let warm = with(Grade {
        temperature: 0.8,
        ..Default::default()
    });
    assert!(warm[0] > warm[2], "warm is redder than blue: {warm:?}");
    let cool = with(Grade {
        temperature: -0.8,
        ..Default::default()
    });
    assert!(cool[2] > cool[0]);
    let magenta = with(Grade {
        tint: 0.8,
        ..Default::default()
    });
    assert!(magenta[1] < magenta[0]);
    let green = with(Grade {
        tint: -0.8,
        ..Default::default()
    });
    assert!(green[1] > green[0]);

    let lifted = Grade {
        lift: [0.1, 0.0, 0.0],
        ..Default::default()
    };
    assert!(
        lifted.apply([0.0, 0.0, 0.0])[0] > 0.09,
        "lift raises the blacks"
    );
    assert!(
        (lifted.apply([1.0, 1.0, 1.0])[0] - 1.0).abs() < 1e-4,
        "and leaves white"
    );
    let gained = Grade {
        gain: [0.5, 1.0, 1.0],
        ..Default::default()
    };
    assert!((gained.apply([1.0, 1.0, 1.0])[0] - 0.5).abs() < 1e-4);
    let bent = Grade {
        gamma: [2.0, 1.0, 1.0],
        ..Default::default()
    };
    assert!(bent.apply(mid)[0] > 0.4 && (bent.apply([1.0; 3])[0] - 1.0).abs() < 1e-4);

    // Shadows act on the dark part more than the bright part, highlights the other way round.
    let shadows = Grade {
        shadows: 1.0,
        ..Default::default()
    };
    let dark_shift = shadows.apply([0.1; 3])[0] - 0.1;
    let bright_shift = shadows.apply([0.9; 3])[0] - 0.9;
    assert!(dark_shift > 0.1 && dark_shift > bright_shift * 5.0);
    let highlights = Grade {
        highlights: -1.0,
        ..Default::default()
    };
    assert!(highlights.apply([0.9; 3])[0] < 0.9 - 0.1);
    assert!(luma(highlights.apply([0.1; 3])) > 0.09);
}

#[test]
fn the_limits_are_enforced_and_garbage_becomes_neutral() {
    let wild = Grade {
        exposure: 99.0,
        temperature: -9.0,
        gain: [99.0, -3.0, f32::NAN],
        gamma: [0.0, 99.0, 1.0],
        ..Default::default()
    }
    .clamped();
    assert_eq!(wild.exposure, 4.0);
    assert_eq!(wild.temperature, -1.0);
    assert_eq!(wild.gain, [4.0, 0.0, 1.0]);
    assert_eq!(wild.gamma, [0.2, 5.0, 1.0]);
    let broken = Grade {
        exposure: f32::NAN,
        tint: f32::INFINITY,
        lift: [f32::NAN; 3],
        ..Default::default()
    }
    .clamped();
    assert!(broken.is_neutral());
}

#[test]
fn a_clip_carries_its_grade_through_the_project_file_and_its_formatting() {
    let mut project = avcore::Project {
        id: 1,
        name: "grade".to_string(),
        last_edited: avcore::Recency::DaysAgo(0),
        summary: String::new(),
        media_library: vec![],
        sequences: vec![avcore::Sequence {
            id: 1,
            name: "Main".to_string(),
            timeline: avcore::timeline::Timeline {
                tracks: vec![],
                playhead_secs: 0.0,
                markers: Vec::new(),
                multicam_groups: Vec::new(),
            },
            export_settings: Default::default(),
        }],
        active_sequence: 0,
        file_path: None,
        panel_layout: None,
        smart_bins: Vec::new(),
        recent_asset_ids: Vec::new(),
        frame_rate: None,
    };
    let mut clip = ClipInstance::new(1, 1, 0.0, 0.0, 2.0);
    clip.grade = Grade {
        exposure: 0.5,
        lift: [0.0, 0.02, 0.0],
        highlights: -0.3,
        ..Default::default()
    };
    // Formatting copies the grade onto another clip.
    let mut other = ClipInstance::new(2, 1, 2.0, 0.0, 2.0);
    other.apply_formatting(&clip.formatting());
    assert_eq!(other.grade, clip.grade);

    project
        .active_sequence_mut()
        .timeline
        .tracks
        .push(avcore::timeline::Track {
            id: 1,
            name: "V1".into(),
            kind: avcore::timeline::TrackKind::Video,
            clips: vec![clip.clone()],
            text_clips: vec![],
            shape_clips: vec![],
            visible: true,
            audio_role: avcore::timeline::AudioRole::Unspecified,
            muted: false,
            solo: false,
            color_label: None,
            locked: false,
            mix: Default::default(),
        });
    let bytes = to_project_bytes(&project).unwrap();
    let back = from_project_bytes(&bytes).unwrap();
    assert_eq!(
        back.active_sequence().timeline.tracks[0].clips[0].grade,
        clip.grade
    );
}

#[test]
fn the_export_stage_reads_a_baked_table_that_includes_the_clips_own_lut() {
    let mut clip = ClipInstance::new(1, 1, 0.0, 0.0, 2.0);
    assert!(
        clip.render_lut_path().is_none(),
        "no LUT and no grade: no stage"
    );

    // A neutral grade leaves the clip's LUT path as it is.
    clip.lut_path = "grade.cube".into();
    assert_eq!(clip.render_lut_path().as_deref(), Some("grade.cube"));

    // A grade alone writes a table, and the filter chain reads it.
    clip.lut_path.clear();
    clip.grade = Grade {
        exposure: 1.0,
        ..Default::default()
    };
    let path = clip.render_lut_path().expect("a baked table");
    assert!(path.ends_with(".cube") && std::path::Path::new(&path).exists());
    let read = Lut3D::load(std::path::Path::new(&path)).unwrap();
    let expected = clip.grade.apply([0.25, 0.5, 0.75]);
    let sampled = read.sample(0.25, 0.5, 0.75);
    for i in 0..3 {
        assert!(
            (sampled[i] - expected[i]).abs() < 0.01,
            "{sampled:?} {expected:?}"
        );
    }
    assert!(clip
        .video_filter_chain()
        .contains(&format!("lut3d=file='{path}'").replace('\\', "/")));

    // The same grade bakes to the same file; another grade to another.
    assert_eq!(clip.render_lut_path().unwrap(), path);
    clip.grade.exposure = 2.0;
    assert_ne!(clip.render_lut_path().unwrap(), path);

    // With a LUT of its own, the grade runs after it: an inverting LUT, then brightened.
    let mut invert = String::from("LUT_3D_SIZE 2\n");
    for b in 0..2 {
        for g in 0..2 {
            for r in 0..2 {
                invert.push_str(&format!("{} {} {}\n", 1 - r, 1 - g, 1 - b));
            }
        }
    }
    let lut_file = std::env::temp_dir().join(format!("grade_invert_{}.cube", std::process::id()));
    std::fs::write(&lut_file, invert).unwrap();
    clip.lut_path = lut_file.display().to_string();
    clip.grade = Grade {
        gain: [0.5, 0.5, 0.5],
        ..Default::default()
    };
    let table = clip.graded_lut().unwrap();
    let out = table.sample(1.0, 0.0, 0.0);
    assert!(
        (out[0] - 0.0).abs() < 0.02 && (out[1] - 0.5).abs() < 0.02,
        "{out:?}"
    );
    let _ = std::fs::remove_file(lut_file);
}

/// A chart of colours through the real `lut3d` filter against the preview's own sampling of the
/// same table: within 2 levels per channel.
#[test]
fn the_preview_sampling_matches_the_export_filter_on_a_chart() {
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        eprintln!("ffmpeg is not on the path, skipping the filter probe");
        return;
    }
    let grades = [
        Grade {
            exposure: 0.7,
            ..Default::default()
        },
        Grade {
            temperature: 0.6,
            tint: -0.3,
            ..Default::default()
        },
        Grade {
            lift: [0.03, 0.0, 0.05],
            gamma: [1.2, 1.0, 0.9],
            gain: [1.1, 1.0, 0.9],
            ..Default::default()
        },
        Grade {
            shadows: 0.7,
            highlights: -0.5,
            ..Default::default()
        },
    ];
    // 24 x 24 x 24 grid of colours, one per pixel, laid out 576 x 24.
    let steps = 24usize;
    let (width, height) = (steps * steps, steps);
    let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
    let mut inputs = Vec::new();
    for g in 0..steps {
        for b in 0..steps {
            for r in 0..steps {
                let px = [r, g, b].map(|v| (v * 255 / (steps - 1)) as u8);
                ppm.extend_from_slice(&px);
                inputs.push(px);
            }
        }
    }
    let dir = std::env::temp_dir().join(format!("grade_probe_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let chart = dir.join("chart.ppm");
    std::fs::write(&chart, &ppm).unwrap();

    for (index, grade) in grades.iter().enumerate() {
        let mut clip = ClipInstance::new(1, 1, 0.0, 0.0, 1.0);
        clip.grade = *grade;
        let table = clip.graded_lut().unwrap();
        let path = clip.render_lut_path().unwrap();
        let out = dir.join(format!("out{index}.rgb"));
        let status = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-i"])
            .arg(&chart)
            .arg("-vf")
            .arg(format!("format=rgb24,lut3d=file='{path}',format=rgb24"))
            .args(["-f", "rawvideo", "-pix_fmt", "rgb24"])
            .arg(&out)
            .status()
            .unwrap();
        assert!(status.success());
        let filtered = std::fs::read(&out).unwrap();
        assert_eq!(filtered.len(), inputs.len() * 3);
        let mut worst = 0.0f32;
        for (i, px) in inputs.iter().enumerate() {
            let expected = table.sample(
                f32::from(px[0]) / 255.0,
                f32::from(px[1]) / 255.0,
                f32::from(px[2]) / 255.0,
            );
            for c in 0..3 {
                let got = f32::from(filtered[i * 3 + c]);
                worst = worst.max((got - expected[c] * 255.0).abs());
            }
        }
        eprintln!("grade {index}: worst difference {worst:.2} levels");
        assert!(worst <= 2.0, "grade {index}: {worst} levels apart");
    }
    let _ = std::fs::remove_dir_all(dir);
}
