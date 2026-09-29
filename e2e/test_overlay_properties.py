# Regression coverage for the text/shape overlay property fields ported this pass
# (apps/desktop/src-tauri/src/commands/overlays.rs's patch_text/patch_shape): background,
# direction, position/scale/rotation/opacity keyframes, start/duration for text; center/width/
# height keyframes, rotation, stroke thickness, start/duration for shapes.
from __future__ import annotations

from helpers import get_session, invoke
from conftest import poll_until


def _first_text_clip(session):
    sequence = session["project"]["sequences"][session["project"]["active_sequence"]]
    for track in sequence["timeline"]["tracks"]:
        if track["text_clips"]:
            return track["text_clips"][0]
    return None


def _first_shape_clip(session):
    sequence = session["project"]["sequences"][session["project"]["active_sequence"]]
    for track in sequence["timeline"]["tracks"]:
        if track["shape_clips"]:
            return track["shape_clips"][0]
    return None


def test_text_clip_patch_applies_background_direction_and_keyframes(oca_driver):
    clip_id = invoke(oca_driver, "add_text_clip", {"text": "hi", "posX": None, "posY": None})
    assert clip_id is not None

    invoke(oca_driver, "update_text_clip", {
        "clipId": clip_id,
        "patch": {
            "backgroundRgba": [10, 20, 30, 200],
            "backgroundPadding": 12,
            "backgroundCornerRadius": 6,
            "direction": "Rtl",
            "startSecs": 1.5,
            "durationSecs": 4.0,
            "posXKeyframes": [{"time_fraction": 0.0, "value": 0.2}, {"time_fraction": 1.0, "value": 0.8}],
            "rotationKeyframes": [{"time_fraction": 0.0, "value": -30.0}],
        },
    })

    def updated():
        clip = _first_text_clip(get_session(oca_driver))
        return clip if clip and clip["direction"] == "Rtl" else None

    clip = poll_until(updated, timeout_secs=3.0)
    assert clip, "text clip patch never applied"
    assert clip["background_rgba"] == [10, 20, 30, 200]
    assert clip["background_padding"] == 12
    assert clip["background_corner_radius"] == 6
    assert clip["start_secs"] == 1.5
    assert clip["duration_secs"] == 4.0
    assert len(clip["pos_x_keyframes"]) == 2
    assert len(clip["rotation_keyframes"]) == 1


def test_shape_clip_patch_applies_rotation_stroke_and_keyframes(oca_driver):
    clip_id = invoke(oca_driver, "add_shape_clip")
    assert clip_id is not None

    invoke(oca_driver, "update_shape_clip", {
        "clipId": clip_id,
        "patch": {
            "rotationDeg": 45.0,
            "strokeThicknessPx": 3.5,
            "startSecs": 2.0,
            "durationSecs": 5.0,
            "widthKeyframes": [{"time_fraction": 0.0, "value": 0.2}, {"time_fraction": 1.0, "value": 0.6}],
        },
    })

    def updated():
        clip = _first_shape_clip(get_session(oca_driver))
        return clip if clip and clip["rotation_deg"] == 45.0 else None

    clip = poll_until(updated, timeout_secs=3.0)
    assert clip, "shape clip patch never applied"
    assert clip["stroke_thickness_px"] == 3.5
    assert clip["start_secs"] == 2.0
    assert clip["duration_secs"] == 5.0
    assert len(clip["width_keyframes"]) == 2


def test_shape_clip_patch_can_edit_polygon_vertices(oca_driver):
    clip_id = invoke(oca_driver, "add_shape_clip")
    assert clip_id is not None

    new_vertices = [[0.0, -0.5], [0.5, 0.5], [-0.5, 0.5]]
    invoke(oca_driver, "update_shape_clip", {
        "clipId": clip_id,
        "patch": {"polygonVertices": new_vertices},
    })

    def updated():
        clip = _first_shape_clip(get_session(oca_driver))
        kind = clip["shape_kind"] if clip else None
        return clip if isinstance(kind, dict) and kind.get("Polygon") == new_vertices else None

    clip = poll_until(updated, timeout_secs=3.0)
    assert clip, "polygon vertex edit never applied"
