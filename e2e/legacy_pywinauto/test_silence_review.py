# Regression coverage for the silence-review `egui::Modal` batch-review flow (D1,
# crates/ui/src/app/silence_review.rs -> crates/ui/src/app/modals.rs's
# show_silence_review_modal) -- accepting a detected gap and applying it should ripple-delete
# that stretch from the timeline, same behavior crates/ui/src/app/app_test.rs's
# apply_silence_review_ripple_deletes_only_accepted_gaps_and_closes_the_modal already proves at
# the App-method level. This test drives the real menu (Analyze > Detect Silence) and the real
# modal's checkbox/Apply button instead.
#
# The shared 1-second fixture (video.mp4) is too short to reliably contain a >=0.5s silence gap
# (avcore::DEFAULT_MIN_SILENCE_SECS) after real waveform extraction, so this generates its own
# tiny fixture with a deterministic silent middle: 1s tone, 1s true digital silence, 1s tone.
# Built with ffmpeg's `mpeg4` video codec (not libx264 -- this repo's LGPL FFmpeg build has no
# H.264 encoder) to match how avbridge/tests/fixtures/video.mp4 itself was encoded. The ffmpeg
# command lines themselves were run and verified manually while writing this file (confirmed a
# playable 3s mpeg4/aac mp4 with the expected stream durations).
#
# UNVERIFIED beyond that: the rest of this test (menu clicks, modal interaction, ripple-delete
# assertion) was never run end-to-end -- this session was told to stop before an iterate-until-
# green pass. A manual spot-check of the top menu bar found some of its rightmost menu_buttons
# not reliably exposing their popup items to UI Automation on the first click (see
# test_speed_ramp.py's own note on this); `_open_detect_silence_modal` below retries defensively
# for the same reason. If this test fails on a real run, check whether "🔍 Detectar/Analisar"'s
# popup items appear in the accessibility tree at all before assuming the silence-review logic
# itself regressed.
from __future__ import annotations

import shutil
import subprocess
import time
from pathlib import Path

import pytest
from pywinauto import Desktop

from conftest import poll_for_descendants
from test_layer_transform import _zoom_timeline_in, _v1_label_rect

REPO_ROOT = Path(__file__).resolve().parents[1]
OPEN_DIALOG_FILENAME_CONTROL_ID = 1148

MENU_ANALYZE_LABEL = "🔍 Detectar/Analisar"
DETECT_SILENCE_LABEL = "🔇 Detectar silêncio"
SILENCE_REVIEW_TITLE = "Revisar silêncios detectados"
SILENCE_REVIEW_EMPTY = "Nenhum trecho de silêncio encontrado."
SILENCE_REVIEW_APPLY = "Aplicar cortes selecionados"
WINDOW_CLOSE = "Fechar"


@pytest.fixture(scope="module")
def silence_fixture_video(tmp_path_factory):
    """A 3s clip: 1s tone, 1s real digital silence, 1s tone -- long and quiet enough in the
    middle to trigger avcore::clip_silence_gaps with its default 0.5s/-34dB-ish thresholds,
    unlike the other e2e tests' 1-second fixture."""
    if shutil.which("ffmpeg") is None:
        pytest.skip("ffmpeg not on PATH -- needed to synthesize the silence-gap fixture")

    out_dir = tmp_path_factory.mktemp("silence_fixture")
    audio_path = out_dir / "audio.wav"
    video_path = out_dir / "silence_gap.mp4"

    subprocess.run(
        [
            "ffmpeg", "-y", "-loglevel", "error",
            "-f", "lavfi", "-i", "sine=frequency=1000:duration=1:sample_rate=44100",
            "-f", "lavfi", "-i", "anullsrc=channel_layout=mono:sample_rate=44100:duration=1",
            "-f", "lavfi", "-i", "sine=frequency=1000:duration=1:sample_rate=44100",
            "-filter_complex", "[0:a][1:a][2:a]concat=n=3:v=0:a=1[aout]",
            "-map", "[aout]", "-c:a", "pcm_s16le", str(audio_path),
        ],
        check=True,
    )
    subprocess.run(
        [
            "ffmpeg", "-y", "-loglevel", "error",
            "-f", "lavfi", "-i", "color=c=blue:s=320x240:d=3:r=25",
            "-i", str(audio_path),
            "-c:v", "mpeg4", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest",
            str(video_path),
        ],
        check=True,
    )
    return video_path


def _import_and_select_clip(oca_window, fixture_path: Path):
    oca_window.child_window(title="Mídia", control_type="Button").click_input()

    dialog = Desktop(backend="uia").window(class_name="#32770")
    for attempt in range(3):
        oca_window.child_window(
            title="⭱ Importar arquivos", control_type="Button"
        ).click_input()
        try:
            dialog.wait("visible", timeout=5)
            break
        except Exception:
            if attempt == 2:
                raise
    filename_edit = dialog.child_window(
        control_id=OPEN_DIALOG_FILENAME_CONTROL_ID, class_name="Edit"
    )
    filename_edit.set_edit_text(str(fixture_path))
    filename_edit.type_keys("{ENTER}")
    assert poll_for_descendants(oca_window, fixture_path.name, timeout_secs=5.0)

    oca_window.child_window(title="Editor", control_type="Button").click_input()
    time.sleep(1)
    asset_label = oca_window.child_window(title=fixture_path.name, control_type="Text")
    asset_label.double_click_input()
    time.sleep(1)

    _zoom_timeline_in(oca_window, _v1_label_rect(oca_window))
    v1_rect = _v1_label_rect(oca_window)
    candidates = [
        d
        for d in oca_window.descendants(control_type="Custom")
        if abs(d.rectangle().mid_point().y - v1_rect.mid_point().y) <= 20
        and d.rectangle().left > v1_rect.right
    ]
    assert candidates, "no timeline clip block found after zooming in"
    body = max(candidates, key=lambda d: d.rectangle().width())
    body.click_input()
    time.sleep(0.5)


def _open_detect_silence_modal(oca_window):
    # Retried per label, not just once -- see this file's module docstring on the observed
    # menu-popup accessibility-tree timing issue.
    for label in (MENU_ANALYZE_LABEL, DETECT_SILENCE_LABEL):
        last_error = None
        for _attempt in range(3):
            try:
                oca_window.child_window(
                    title=label, control_type="Button"
                ).click_input()
                last_error = None
                break
            except Exception as error:
                last_error = error
                time.sleep(0.5)
        if last_error is not None:
            raise last_error
        time.sleep(0.3)


def _close_modal_if_open(oca_window):
    close_buttons = oca_window.descendants(title=WINDOW_CLOSE, control_type="Button")
    if close_buttons:
        close_buttons[0].click_input()
        time.sleep(0.3)


def test_silence_review_apply_ripple_deletes_the_accepted_gap(oca_window, silence_fixture_video):
    _import_and_select_clip(oca_window, silence_fixture_video)

    # Background enrichment (App::import.rs's ImportEvent::Enriched) computes the waveform
    # asynchronously after the asset lands in the library -- Detect Silence is a no-op (opens
    # the modal with the "no gaps found" message) until that finishes. Poll by retrying the
    # whole detect-and-check cycle rather than sleeping a fixed guess, closing the modal
    # between attempts so it doesn't stack.
    gap_rows_found = False
    deadline = time.monotonic() + 25.0
    while time.monotonic() < deadline:
        _open_detect_silence_modal(oca_window)
        assert poll_for_descendants(oca_window, SILENCE_REVIEW_TITLE, timeout_secs=3.0), (
            "silence review modal never opened after Detect Silence"
        )
        time.sleep(0.3)
        if oca_window.descendants(title=SILENCE_REVIEW_EMPTY, control_type="Text"):
            _close_modal_if_open(oca_window)
            time.sleep(0.5)
            continue
        checkboxes = oca_window.descendants(control_type="CheckBox")
        if checkboxes:
            gap_rows_found = True
            break
        _close_modal_if_open(oca_window)
        time.sleep(0.5)

    assert gap_rows_found, (
        "no silence gap was ever detected in the synthetic fixture's 1s silent middle "
        "section within 25s -- either waveform enrichment never completed, or "
        "avcore::clip_silence_gaps found nothing where a real digital-silence gap was placed"
    )

    v1_rect = _v1_label_rect(oca_window)
    blocks_before = [
        d
        for d in oca_window.descendants(control_type="Custom")
        if abs(d.rectangle().mid_point().y - v1_rect.mid_point().y) <= 20
    ]

    # Gap defaults to accepted (SilenceReviewGap::accepted starts true) -- Apply with no
    # further clicks exercises the ripple-delete path directly.
    oca_window.child_window(
        title=SILENCE_REVIEW_APPLY, control_type="Button"
    ).click_input()

    # Modal should close (App::apply_silence_review takes self.silence_review, closing it)
    # and the ripple-delete should have shortened the track -- the timeline no longer contains
    # the original single full-length block spanning the whole clip.
    deadline = time.monotonic() + 5.0
    modal_closed = False
    while time.monotonic() < deadline:
        if not oca_window.descendants(title=SILENCE_REVIEW_TITLE, control_type="Text"):
            modal_closed = True
            break
        time.sleep(0.2)
    assert modal_closed, "silence review modal still open after clicking Apply"

    time.sleep(0.5)
    blocks_after = [
        d
        for d in oca_window.descendants(control_type="Custom")
        if abs(d.rectangle().mid_point().y - v1_rect.mid_point().y) <= 20
    ]
    total_width_before = sum(b.rectangle().width() for b in blocks_before)
    total_width_after = sum(b.rectangle().width() for b in blocks_after)
    assert total_width_after < total_width_before, (
        f"ripple-delete did not shrink the V1 row's clip coverage "
        f"(before={total_width_before}px across {len(blocks_before)} blocks, "
        f"after={total_width_after}px across {len(blocks_after)} blocks)"
    )
