# Regression coverage for the silence-review flow (avcore::clip_silence_gaps,
# apps/desktop/src-tauri/src/commands/silence.rs) -- detecting a gap and applying it should
# ripple-delete that stretch from the timeline, same behavior
# apply_silence_review_ripple_deletes_only_accepted_gaps_and_closes_the_modal already proves at
# the core level. This drives the real detect_silence_gaps_for_clip/apply_silence_gaps
# commands instead.
#
# The shared 1-second fixture (video.mp4) is too short to reliably contain a >=0.5s silence gap
# (avcore::DEFAULT_MIN_SILENCE_SECS) after real waveform extraction, so this generates its own
# tiny fixture with a deterministic silent middle: 1s tone, 1s true digital silence, 1s tone.
# Built with ffmpeg's `mpeg4` video codec (not libx264 -- this repo's LGPL FFmpeg build has no
# H.264 encoder) to match how avbridge/tests/fixtures/video.mp4 itself was encoded.
from __future__ import annotations

import shutil
import subprocess

import pytest

from helpers import get_session, import_and_add_to_timeline, invoke
from conftest import poll_until


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


def _track(session, track_id):
    project = session["project"]
    sequence = project["sequences"][project["active_sequence"]]
    return next(t for t in sequence["timeline"]["tracks"] if t["id"] == track_id)


def test_silence_review_apply_ripple_deletes_the_accepted_gap(oca_driver, silence_fixture_video):
    clip_id = import_and_add_to_timeline(oca_driver, video_path=silence_fixture_video)
    session = get_session(oca_driver)
    sequence = session["project"]["sequences"][session["project"]["active_sequence"]]
    track_id = sequence["timeline"]["tracks"][0]["id"]

    # Background enrichment computes the waveform asynchronously after the asset lands in the
    # library -- detect_silence_gaps_for_clip is a no-op (returns null/no gaps) until that
    # finishes, so poll the whole detect cycle rather than sleeping a fixed guess.
    def detect():
        review = invoke(oca_driver, "detect_silence_gaps_for_clip", {"clipId": clip_id})
        return review if review and review["gaps"] else None

    review = poll_until(detect, timeout_secs=25.0)
    assert review, (
        "no silence gap was ever detected in the synthetic fixture's 1s silent middle section "
        "within 25s -- either waveform enrichment never completed, or avcore::clip_silence_gaps "
        "found nothing where a real digital-silence gap was placed"
    )

    before = _track(get_session(oca_driver), track_id)
    total_duration_before = sum(c["source_out_secs"] - c["source_in_secs"] for c in before["clips"])

    # Every gap defaults to accepted -- applying all of them directly exercises the
    # ripple-delete path.
    invoke(oca_driver, "apply_silence_gaps", {"trackId": track_id, "gaps": review["gaps"]})

    after = _track(get_session(oca_driver), track_id)
    total_duration_after = sum(c["source_out_secs"] - c["source_in_secs"] for c in after["clips"])
    assert total_duration_after < total_duration_before, (
        f"ripple-delete did not shrink track {track_id}'s clip coverage "
        f"(before={total_duration_before}s, after={total_duration_after}s)"
    )
