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

use avbridge::{apply_text_overlays, probe, StreamKind, TextOverlaySegment};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// A 4x4 fully-opaque red RGBA PNG -- content doesn't matter for this test, only that the file
/// exists and decodes, same as any other pre-rasterized text overlay `avbridge_apply_text_overlays`
/// consumes.
fn write_test_overlay_png(path: &Path) {
    let width = 4u32;
    let height = 4u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..(width * height) {
        rgba.extend_from_slice(&[255, 0, 0, 255]);
    }
    image::save_buffer_with_format(
        path,
        &rgba,
        width,
        height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .unwrap();
}

#[test]
fn applies_a_keyframed_opacity_fade_to_a_text_overlay() {
    let out = std::env::temp_dir().join("avbridge_text_overlay_opacity_test.mp4");
    let overlay_png = std::env::temp_dir().join("avbridge_text_overlay_opacity_test.png");
    let _ = std::fs::remove_file(&out);
    write_test_overlay_png(&overlay_png);

    let source = fixture("video.mp4");
    let probed = probe(&source).unwrap();
    let (width, height) = probed.resolution.unwrap();
    let fps = probed.fps.unwrap();

    let segments = vec![TextOverlaySegment {
        start_secs: 0.0,
        duration_secs: probed.duration_secs.max(0.5),
        overlay_path: overlay_png.clone(),
        // Same shape keyframe::text_opacity_alpha_expr would build for a 0->1 fade.
        opacity_keyframe_expr: "if(lt((T-0.000000),0.000000),0.0000000,\
                                 if(between((T-0.000000),0.000000,0.500000),\
                                 (0.0000000+2.0000000*((T-0.000000)-0.000000)),1.0000000))"
            .to_string(),
        position_keyframe_expr_x: String::new(),
        position_keyframe_expr_y: String::new(),
        scale_keyframe_expr_x: String::new(),
        scale_keyframe_expr_y: String::new(),
        rotation_keyframe_expr_x: String::new(),
        rotation_keyframe_expr_y: String::new(),
    }];

    let result = apply_text_overlays(
        &source,
        &out,
        &segments,
        width,
        height,
        fps.round() as u32,
        1,
    );
    assert!(result.is_ok(), "apply_text_overlays failed: {result:?}");
    assert!(out.exists());
    let out_info = probe(&out).unwrap();
    assert_eq!(out_info.kind, StreamKind::Video);

    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&overlay_png);
}

#[test]
fn applies_a_text_overlay_with_no_opacity_expr_unchanged() {
    // Confirms the has_fade == false branch (the unanimated common case, same filter graph as
    // before opacity keyframes existed) still works.
    let out = std::env::temp_dir().join("avbridge_text_overlay_static_test.mp4");
    let overlay_png = std::env::temp_dir().join("avbridge_text_overlay_static_test.png");
    let _ = std::fs::remove_file(&out);
    write_test_overlay_png(&overlay_png);

    let source = fixture("video.mp4");
    let probed = probe(&source).unwrap();
    let (width, height) = probed.resolution.unwrap();
    let fps = probed.fps.unwrap();

    let segments = vec![TextOverlaySegment {
        start_secs: 0.0,
        duration_secs: probed.duration_secs.max(0.5),
        overlay_path: overlay_png.clone(),
        opacity_keyframe_expr: String::new(),
        position_keyframe_expr_x: String::new(),
        position_keyframe_expr_y: String::new(),
        scale_keyframe_expr_x: String::new(),
        scale_keyframe_expr_y: String::new(),
        rotation_keyframe_expr_x: String::new(),
        rotation_keyframe_expr_y: String::new(),
    }];

    let result = apply_text_overlays(
        &source,
        &out,
        &segments,
        width,
        height,
        fps.round() as u32,
        1,
    );
    assert!(result.is_ok(), "apply_text_overlays failed: {result:?}");

    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&overlay_png);
}

#[test]
fn applies_a_keyframed_position_offset_to_a_text_overlay() {
    // Real linked-FFmpeg-build check for keyframe::text_position_offset_expr's output, same
    // motivation as applies_a_keyframed_opacity_fade_to_a_text_overlay's own doc comment on why
    // that one exists (a syntax check alone wouldn't catch a real filtergraph rejecting the
    // overlay=x='...':y='...' expression syntax).
    let out = std::env::temp_dir().join("avbridge_text_overlay_position_test.mp4");
    let overlay_png = std::env::temp_dir().join("avbridge_text_overlay_position_test.png");
    let _ = std::fs::remove_file(&out);
    write_test_overlay_png(&overlay_png);

    let source = fixture("video.mp4");
    let probed = probe(&source).unwrap();
    let (width, height) = probed.resolution.unwrap();
    let fps = probed.fps.unwrap();

    let segments = vec![TextOverlaySegment {
        start_secs: 0.0,
        duration_secs: probed.duration_secs.max(0.5),
        overlay_path: overlay_png.clone(),
        opacity_keyframe_expr: String::new(),
        // Same shape keyframe::text_position_offset_expr would build for a two-keyframe move
        // from the raster's own baked anchor (delta 0) to 100px to the right/down.
        position_keyframe_expr_x: "if(lt((t-0.000000),0.000000),0.0000000,\
                                     if(between((t-0.000000),0.000000,0.500000),\
                                     (0.0000000+200.0000000*((t-0.000000)-0.000000)),100.0000000))"
            .to_string(),
        position_keyframe_expr_y: "if(lt((t-0.000000),0.000000),0.0000000,\
                                     if(between((t-0.000000),0.000000,0.500000),\
                                     (0.0000000+200.0000000*((t-0.000000)-0.000000)),100.0000000))"
            .to_string(),
        scale_keyframe_expr_x: String::new(),
        scale_keyframe_expr_y: String::new(),
        rotation_keyframe_expr_x: String::new(),
        rotation_keyframe_expr_y: String::new(),
    }];

    let result = apply_text_overlays(
        &source,
        &out,
        &segments,
        width,
        height,
        fps.round() as u32,
        1,
    );
    assert!(result.is_ok(), "apply_text_overlays failed: {result:?}");
    assert!(out.exists());
    let out_info = probe(&out).unwrap();
    assert_eq!(out_info.kind, StreamKind::Video);

    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&overlay_png);
}

#[test]
fn applies_a_keyframed_scale_to_a_text_overlay() {
    // Real linked-FFmpeg-build check for keyframe::text_scale_sample_exprs' output -- confirms
    // the geq inverse-sample remap syntax (movie source -> scale geq -> overlay) parses against
    // a real filtergraph, same motivation as the opacity/position tests above.
    let out = std::env::temp_dir().join("avbridge_text_overlay_scale_test.mp4");
    let overlay_png = std::env::temp_dir().join("avbridge_text_overlay_scale_test.png");
    let _ = std::fs::remove_file(&out);
    write_test_overlay_png(&overlay_png);

    let source = fixture("video.mp4");
    let probed = probe(&source).unwrap();
    let (width, height) = probed.resolution.unwrap();
    let fps = probed.fps.unwrap();

    let segments = vec![TextOverlaySegment {
        start_secs: 0.0,
        duration_secs: probed.duration_secs.max(0.5),
        overlay_path: overlay_png.clone(),
        opacity_keyframe_expr: String::new(),
        position_keyframe_expr_x: String::new(),
        position_keyframe_expr_y: String::new(),
        // Same shape keyframe::text_scale_sample_exprs would build for a two-keyframe zoom
        // from 1x to 2x around a (100, 50) anchor.
        scale_keyframe_expr_x: "(100.0000+(X-(100.0000))/(if(lt((T-0.000000),0.000000),\
                                 1.0000000,if(between((T-0.000000),0.000000,0.500000),\
                                 (1.0000000+2.0000000*((T-0.000000)-0.000000)),2.0000000))))"
            .to_string(),
        scale_keyframe_expr_y: "(50.0000+(Y-(50.0000))/(if(lt((T-0.000000),0.000000),\
                                 1.0000000,if(between((T-0.000000),0.000000,0.500000),\
                                 (1.0000000+2.0000000*((T-0.000000)-0.000000)),2.0000000))))"
            .to_string(),
        rotation_keyframe_expr_x: String::new(),
        rotation_keyframe_expr_y: String::new(),
    }];

    let result = apply_text_overlays(
        &source,
        &out,
        &segments,
        width,
        height,
        fps.round() as u32,
        1,
    );
    assert!(result.is_ok(), "apply_text_overlays failed: {result:?}");
    assert!(out.exists());
    let out_info = probe(&out).unwrap();
    assert_eq!(out_info.kind, StreamKind::Video);

    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&overlay_png);
}

#[test]
fn applies_a_keyframed_rotation_to_a_text_overlay() {
    // Real linked-FFmpeg-build check for keyframe::text_rotation_sample_exprs' output -- confirms
    // the sin()/cos()-based geq inverse-sample remap syntax parses against a real filtergraph,
    // same motivation as the scale/opacity/position tests above.
    let out = std::env::temp_dir().join("avbridge_text_overlay_rotation_test.mp4");
    let overlay_png = std::env::temp_dir().join("avbridge_text_overlay_rotation_test.png");
    let _ = std::fs::remove_file(&out);
    write_test_overlay_png(&overlay_png);

    let source = fixture("video.mp4");
    let probed = probe(&source).unwrap();
    let (width, height) = probed.resolution.unwrap();
    let fps = probed.fps.unwrap();

    let segments = vec![TextOverlaySegment {
        start_secs: 0.0,
        duration_secs: probed.duration_secs.max(0.5),
        overlay_path: overlay_png.clone(),
        opacity_keyframe_expr: String::new(),
        position_keyframe_expr_x: String::new(),
        position_keyframe_expr_y: String::new(),
        scale_keyframe_expr_x: String::new(),
        scale_keyframe_expr_y: String::new(),
        // Same shape keyframe::text_rotation_sample_exprs would build for a two-keyframe spin
        // from 0 to 90 degrees around a (100, 50) anchor.
        rotation_keyframe_expr_x: "(100.0000+(X-(100.0000))*cos((if(lt((T-0.000000),0.000000),\
                                    0.0000000,if(between((T-0.000000),0.000000,0.500000),\
                                    (0.0000000+3.1415927*((T-0.000000)-0.000000)),1.5707963))))+\
                                    (Y-(50.0000))*sin((if(lt((T-0.000000),0.000000),0.0000000,\
                                    if(between((T-0.000000),0.000000,0.500000),\
                                    (0.0000000+3.1415927*((T-0.000000)-0.000000)),1.5707963)))))"
            .to_string(),
        rotation_keyframe_expr_y: "(50.0000-(X-(100.0000))*sin((if(lt((T-0.000000),0.000000),\
                                    0.0000000,if(between((T-0.000000),0.000000,0.500000),\
                                    (0.0000000+3.1415927*((T-0.000000)-0.000000)),1.5707963))))+\
                                    (Y-(50.0000))*cos((if(lt((T-0.000000),0.000000),0.0000000,\
                                    if(between((T-0.000000),0.000000,0.500000),\
                                    (0.0000000+3.1415927*((T-0.000000)-0.000000)),1.5707963)))))"
            .to_string(),
    }];

    let result = apply_text_overlays(
        &source,
        &out,
        &segments,
        width,
        height,
        fps.round() as u32,
        1,
    );
    assert!(result.is_ok(), "apply_text_overlays failed: {result:?}");
    assert!(out.exists());
    let out_info = probe(&out).unwrap();
    assert_eq!(out_info.kind, StreamKind::Video);

    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&overlay_png);
}

/// Number of video frames in `path`, by decoding it.
fn frame_count(path: &Path) -> usize {
    let (mut session, _) = avbridge::DecodeSession::open(path).unwrap();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let mut frames = 0;
    while let avbridge::Advance::Frame(_) = session.advance(&cancel).unwrap() {
        frames += 1;
    }
    frames
}

#[test]
fn an_overlay_pass_re_encodes_every_frame_of_its_input() {
    // The pass decodes on several threads and encodes with whatever hardware encoder opens:
    // neither may hold back or drop frames at the end of the file.
    let out = std::env::temp_dir().join("avbridge_text_overlay_frames_test.mp4");
    let overlay_png = std::env::temp_dir().join("avbridge_text_overlay_frames_test.png");
    let _ = std::fs::remove_file(&out);
    write_test_overlay_png(&overlay_png);

    let source = fixture("video.mp4");
    let probed = probe(&source).unwrap();
    let (width, height) = probed.resolution.unwrap();
    let fps = probed.fps.unwrap();
    let segments = vec![TextOverlaySegment {
        start_secs: 0.0,
        duration_secs: probed.duration_secs.max(0.5),
        overlay_path: overlay_png.clone(),
        opacity_keyframe_expr: String::new(),
        position_keyframe_expr_x: String::new(),
        position_keyframe_expr_y: String::new(),
        scale_keyframe_expr_x: String::new(),
        scale_keyframe_expr_y: String::new(),
        rotation_keyframe_expr_x: String::new(),
        rotation_keyframe_expr_y: String::new(),
    }];
    apply_text_overlays(
        &source,
        &out,
        &segments,
        width,
        height,
        fps.round() as u32,
        1,
    )
    .unwrap();

    assert_eq!(frame_count(&out), frame_count(&source));

    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&overlay_png);
}
