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

use super::*;

#[test]
fn decode_open_status_maps_every_code() {
    assert!(matches!(decode_open_status(0), Ok(())));
    assert!(matches!(decode_open_status(3), Err(DecodeError::OpenInput)));
    assert!(matches!(
        decode_open_status(4),
        Err(DecodeError::StreamInfo)
    ));
    assert!(matches!(
        decode_open_status(5),
        Err(DecodeError::NoVideoStream)
    ));
    assert!(matches!(decode_open_status(6), Err(DecodeError::Decoder)));
    assert!(matches!(decode_open_status(9), Err(DecodeError::FrameSize)));
    assert!(matches!(
        decode_open_status(99),
        Err(DecodeError::Unknown(99))
    ));
}

#[test]
fn remux_copy_status_maps_every_code() {
    assert!(matches!(remux_copy_status(0), Ok(())));
    assert!(matches!(remux_copy_status(1), Err(RemuxError::OpenInput)));
    assert!(matches!(remux_copy_status(2), Err(RemuxError::StreamInfo)));
    assert!(matches!(remux_copy_status(3), Err(RemuxError::AllocOutput)));
    assert!(matches!(remux_copy_status(4), Err(RemuxError::NewStream)));
    assert!(matches!(remux_copy_status(5), Err(RemuxError::OpenOutput)));
    assert!(matches!(remux_copy_status(6), Err(RemuxError::WriteHeader)));
    assert!(matches!(remux_copy_status(7), Err(RemuxError::WriteFrame)));
    assert!(matches!(
        remux_copy_status(99),
        Err(RemuxError::Unknown(99))
    ));
}

#[test]
fn encode_export_status_maps_every_code() {
    assert!(matches!(
        encode_export_status(0),
        Ok(EncodeOutcome::Completed)
    ));
    assert!(matches!(
        encode_export_status(1),
        Err(EncodeError::OpenInput)
    ));
    assert!(matches!(
        encode_export_status(2),
        Err(EncodeError::StreamInfo)
    ));
    assert!(matches!(
        encode_export_status(3),
        Err(EncodeError::AllocOutput)
    ));
    assert!(matches!(
        encode_export_status(4),
        Err(EncodeError::NewStream)
    ));
    assert!(matches!(
        encode_export_status(5),
        Err(EncodeError::OpenOutput)
    ));
    assert!(matches!(
        encode_export_status(6),
        Err(EncodeError::WriteHeader)
    ));
    assert!(matches!(
        encode_export_status(7),
        Err(EncodeError::WriteFrame)
    ));
    assert!(matches!(
        encode_export_status(8),
        Err(EncodeError::NoAudioStream)
    ));
    assert!(matches!(encode_export_status(9), Err(EncodeError::Decoder)));
    assert!(matches!(
        encode_export_status(10),
        Err(EncodeError::FilterGraph)
    ));
    assert!(matches!(
        encode_export_status(11),
        Err(EncodeError::Encoder)
    ));
    assert!(matches!(
        encode_export_status(12),
        Err(EncodeError::Pipeline)
    ));
    assert!(matches!(
        encode_export_status(13),
        Ok(EncodeOutcome::Cancelled)
    ));
    assert!(matches!(
        encode_export_status(99),
        Err(EncodeError::Unknown(99))
    ));
}

#[test]
fn encode_timeline_export_status_maps_every_code() {
    assert!(matches!(
        encode_timeline_export_status(0),
        Ok(EncodeOutcome::Completed)
    ));
    assert!(matches!(
        encode_timeline_export_status(1),
        Err(EncodeError::OpenInput)
    ));
    assert!(matches!(
        encode_timeline_export_status(2),
        Err(EncodeError::StreamInfo)
    ));
    assert!(matches!(
        encode_timeline_export_status(3),
        Err(EncodeError::AllocOutput)
    ));
    assert!(matches!(
        encode_timeline_export_status(4),
        Err(EncodeError::NewStream)
    ));
    assert!(matches!(
        encode_timeline_export_status(5),
        Err(EncodeError::OpenOutput)
    ));
    assert!(matches!(
        encode_timeline_export_status(6),
        Err(EncodeError::WriteHeader)
    ));
    assert!(matches!(
        encode_timeline_export_status(7),
        Err(EncodeError::WriteFrame)
    ));
    assert!(matches!(
        encode_timeline_export_status(8),
        Err(EncodeError::NoAudioStream)
    ));
    assert!(matches!(
        encode_timeline_export_status(9),
        Err(EncodeError::Decoder)
    ));
    assert!(matches!(
        encode_timeline_export_status(10),
        Err(EncodeError::FilterGraph)
    ));
    assert!(matches!(
        encode_timeline_export_status(11),
        Err(EncodeError::Encoder)
    ));
    assert!(matches!(
        encode_timeline_export_status(12),
        Err(EncodeError::Pipeline)
    ));
    assert!(matches!(
        encode_timeline_export_status(13),
        Ok(EncodeOutcome::Cancelled)
    ));
    assert!(matches!(
        encode_timeline_export_status(14),
        Err(EncodeError::NoVideoStream)
    ));
    assert!(matches!(
        encode_timeline_export_status(15),
        Err(EncodeError::AudioFormatMismatch)
    ));
    assert!(matches!(
        encode_timeline_export_status(16),
        Err(EncodeError::EmptyTimeline)
    ));
    assert!(matches!(
        encode_timeline_export_status(99),
        Err(EncodeError::Unknown(99))
    ));
}

#[test]
fn encode_timeline_export_multi_status_maps_every_code() {
    assert!(matches!(
        encode_timeline_export_multi_status(0),
        Ok(EncodeOutcome::Completed)
    ));
    assert!(matches!(
        encode_timeline_export_multi_status(1),
        Err(EncodeError::OpenInput)
    ));
    assert!(matches!(
        encode_timeline_export_multi_status(2),
        Err(EncodeError::StreamInfo)
    ));
    assert!(matches!(
        encode_timeline_export_multi_status(3),
        Err(EncodeError::AllocOutput)
    ));
    assert!(matches!(
        encode_timeline_export_multi_status(4),
        Err(EncodeError::NewStream)
    ));
    assert!(matches!(
        encode_timeline_export_multi_status(5),
        Err(EncodeError::OpenOutput)
    ));
    assert!(matches!(
        encode_timeline_export_multi_status(6),
        Err(EncodeError::WriteHeader)
    ));
    assert!(matches!(
        encode_timeline_export_multi_status(7),
        Err(EncodeError::WriteFrame)
    ));
    assert!(matches!(
        encode_timeline_export_multi_status(8),
        Err(EncodeError::NoAudioStream)
    ));
    assert!(matches!(
        encode_timeline_export_multi_status(9),
        Err(EncodeError::Decoder)
    ));
    assert!(matches!(
        encode_timeline_export_multi_status(10),
        Err(EncodeError::FilterGraph)
    ));
    assert!(matches!(
        encode_timeline_export_multi_status(11),
        Err(EncodeError::Encoder)
    ));
    assert!(matches!(
        encode_timeline_export_multi_status(12),
        Err(EncodeError::Pipeline)
    ));
    assert!(matches!(
        encode_timeline_export_multi_status(13),
        Ok(EncodeOutcome::Cancelled)
    ));
    assert!(matches!(
        encode_timeline_export_multi_status(14),
        Err(EncodeError::NoVideoStream)
    ));
    assert!(matches!(
        encode_timeline_export_multi_status(15),
        Err(EncodeError::AudioFormatMismatch)
    ));
    assert!(matches!(
        encode_timeline_export_multi_status(16),
        Err(EncodeError::EmptyTimeline)
    ));
    assert!(matches!(
        encode_timeline_export_multi_status(99),
        Err(EncodeError::Unknown(99))
    ));
}

#[test]
fn mix_audio_timeline_status_maps_every_code() {
    assert!(matches!(
        mix_audio_timeline_status(0),
        Ok(AudioMixOutcome::Completed)
    ));
    assert!(matches!(
        mix_audio_timeline_status(1),
        Err(AudioMixError::OpenInput)
    ));
    assert!(matches!(
        mix_audio_timeline_status(2),
        Err(AudioMixError::AllocOutput)
    ));
    assert!(matches!(
        mix_audio_timeline_status(3),
        Err(AudioMixError::NoAudio)
    ));
    assert!(matches!(
        mix_audio_timeline_status(4),
        Err(AudioMixError::FilterGraph)
    ));
    assert!(matches!(
        mix_audio_timeline_status(5),
        Err(AudioMixError::Encoder)
    ));
    assert!(matches!(
        mix_audio_timeline_status(6),
        Err(AudioMixError::OpenOutput)
    ));
    assert!(matches!(
        mix_audio_timeline_status(7),
        Err(AudioMixError::WriteHeader)
    ));
    assert!(matches!(
        mix_audio_timeline_status(8),
        Err(AudioMixError::Pipeline)
    ));
    assert!(matches!(
        mix_audio_timeline_status(9),
        Ok(AudioMixOutcome::Cancelled)
    ));
    assert!(matches!(
        mix_audio_timeline_status(99),
        Err(AudioMixError::Unknown(99))
    ));
}

#[test]
fn mux_video_audio_status_maps_every_code() {
    assert!(matches!(mux_video_audio_status(0), Ok(())));
    assert!(matches!(
        mux_video_audio_status(1),
        Err(MediaMuxError::OpenInput)
    ));
    assert!(matches!(
        mux_video_audio_status(2),
        Err(MediaMuxError::AllocOutput)
    ));
    assert!(matches!(
        mux_video_audio_status(3),
        Err(MediaMuxError::MissingStream)
    ));
    assert!(matches!(
        mux_video_audio_status(4),
        Err(MediaMuxError::NewStream)
    ));
    assert!(matches!(
        mux_video_audio_status(5),
        Err(MediaMuxError::OpenOutput)
    ));
    assert!(matches!(
        mux_video_audio_status(6),
        Err(MediaMuxError::WriteHeader)
    ));
    assert!(matches!(
        mux_video_audio_status(7),
        Err(MediaMuxError::WriteFrame)
    ));
    assert!(matches!(
        mux_video_audio_status(99),
        Err(MediaMuxError::Unknown(99))
    ));
}

#[test]
fn generate_proxy_status_maps_every_code() {
    assert!(matches!(generate_proxy_status(0), Ok(())));
    assert!(matches!(
        generate_proxy_status(1),
        Err(ProxyError::OpenInput)
    ));
    assert!(matches!(
        generate_proxy_status(2),
        Err(ProxyError::StreamInfo)
    ));
    assert!(matches!(
        generate_proxy_status(3),
        Err(ProxyError::NoVideoStream)
    ));
    assert!(matches!(
        generate_proxy_status(4),
        Err(ProxyError::AllocOutput)
    ));
    assert!(matches!(
        generate_proxy_status(5),
        Err(ProxyError::NewStream)
    ));
    assert!(matches!(
        generate_proxy_status(6),
        Err(ProxyError::OpenOutput)
    ));
    assert!(matches!(
        generate_proxy_status(7),
        Err(ProxyError::WriteHeader)
    ));
    assert!(matches!(
        generate_proxy_status(8),
        Err(ProxyError::WriteFrame)
    ));
    assert!(matches!(generate_proxy_status(9), Err(ProxyError::Decoder)));
    assert!(matches!(
        generate_proxy_status(10),
        Err(ProxyError::Encoder)
    ));
    assert!(matches!(generate_proxy_status(11), Err(ProxyError::Scaler)));
    assert!(matches!(
        generate_proxy_status(12),
        Err(ProxyError::FilterGraph)
    ));
    assert!(matches!(
        generate_proxy_status(13),
        Err(ProxyError::Pipeline)
    ));
    assert!(matches!(
        generate_proxy_status(99),
        Err(ProxyError::Unknown(99))
    ));
}

#[test]
fn encode_matte_video_status_maps_every_code() {
    assert!(matches!(encode_matte_video_status(0), Ok(())));
    assert!(matches!(
        encode_matte_video_status(1),
        Err(MatteError::AllocOutput)
    ));
    assert!(matches!(
        encode_matte_video_status(2),
        Err(MatteError::Encoder)
    ));
    assert!(matches!(
        encode_matte_video_status(3),
        Err(MatteError::NewStream)
    ));
    assert!(matches!(
        encode_matte_video_status(4),
        Err(MatteError::OpenOutput)
    ));
    assert!(matches!(
        encode_matte_video_status(5),
        Err(MatteError::WriteHeader)
    ));
    assert!(matches!(
        encode_matte_video_status(6),
        Err(MatteError::Pipeline)
    ));
    assert!(matches!(
        encode_matte_video_status(7),
        Err(MatteError::Empty)
    ));
    assert!(matches!(
        encode_matte_video_status(8),
        Err(MatteError::OddDimensions)
    ));
    assert!(matches!(
        encode_matte_video_status(99),
        Err(MatteError::Unknown(99))
    ));
}

#[test]
fn apply_text_overlays_status_maps_every_code() {
    assert!(matches!(apply_text_overlays_status(0), Ok(())));
    assert!(matches!(
        apply_text_overlays_status(1),
        Err(TextOverlayError::OpenInput)
    ));
    assert!(matches!(
        apply_text_overlays_status(2),
        Err(TextOverlayError::AllocOutput)
    ));
    assert!(matches!(
        apply_text_overlays_status(3),
        Err(TextOverlayError::FilterGraph)
    ));
    assert!(matches!(
        apply_text_overlays_status(4),
        Err(TextOverlayError::Pipeline)
    ));
    assert!(matches!(
        apply_text_overlays_status(99),
        Err(TextOverlayError::Unknown(99))
    ));
}

#[test]
fn apply_shape_overlays_status_maps_every_code() {
    assert!(matches!(apply_shape_overlays_status(0), Ok(())));
    assert!(matches!(
        apply_shape_overlays_status(1),
        Err(TextOverlayError::OpenInput)
    ));
    assert!(matches!(
        apply_shape_overlays_status(2),
        Err(TextOverlayError::AllocOutput)
    ));
    assert!(matches!(
        apply_shape_overlays_status(3),
        Err(TextOverlayError::FilterGraph)
    ));
    assert!(matches!(
        apply_shape_overlays_status(4),
        Err(TextOverlayError::Pipeline)
    ));
    assert!(matches!(
        apply_shape_overlays_status(99),
        Err(TextOverlayError::Unknown(99))
    ));
}

#[test]
fn apply_privacy_blur_status_maps_every_code() {
    assert!(matches!(apply_privacy_blur_status(0), Ok(())));
    assert!(matches!(
        apply_privacy_blur_status(1),
        Err(PrivacyBlurError::OpenInput)
    ));
    assert!(matches!(
        apply_privacy_blur_status(2),
        Err(PrivacyBlurError::AllocOutput)
    ));
    assert!(matches!(
        apply_privacy_blur_status(3),
        Err(PrivacyBlurError::FilterGraph)
    ));
    assert!(matches!(
        apply_privacy_blur_status(4),
        Err(PrivacyBlurError::Pipeline)
    ));
    assert!(matches!(
        apply_privacy_blur_status(99),
        Err(PrivacyBlurError::Unknown(99))
    ));
}

#[test]
fn extract_pcm_16k_mono_status_maps_every_code() {
    assert!(matches!(extract_pcm_16k_mono_status(0), Ok(())));
    assert!(matches!(
        extract_pcm_16k_mono_status(1),
        Err(PcmError::OpenInput)
    ));
    assert!(matches!(
        extract_pcm_16k_mono_status(2),
        Err(PcmError::StreamInfo)
    ));
    assert!(matches!(
        extract_pcm_16k_mono_status(3),
        Err(PcmError::NoAudioStream)
    ));
    assert!(matches!(
        extract_pcm_16k_mono_status(4),
        Err(PcmError::Decoder)
    ));
    assert!(matches!(
        extract_pcm_16k_mono_status(5),
        Err(PcmError::FilterGraph)
    ));
    assert!(matches!(
        extract_pcm_16k_mono_status(6),
        Err(PcmError::Pipeline)
    ));
    assert!(matches!(
        extract_pcm_16k_mono_status(99),
        Err(PcmError::Unknown(99))
    ));
}

#[test]
fn probe_status_maps_every_code() {
    assert!(matches!(probe_status(0), Ok(())));
    assert!(matches!(probe_status(1), Err(ProbeError::Open)));
    assert!(matches!(probe_status(2), Err(ProbeError::StreamInfo)));
    assert!(matches!(probe_status(3), Err(ProbeError::NoMediaStream)));
    assert!(matches!(probe_status(99), Err(ProbeError::Unknown(99))));
}

#[test]
fn loudness_status_maps_every_code() {
    assert!(matches!(loudness_status(0), Ok(())));
    assert!(matches!(loudness_status(1), Err(LoudnessError::OpenInput)));
    assert!(matches!(loudness_status(2), Err(LoudnessError::StreamInfo)));
    assert!(matches!(
        loudness_status(3),
        Err(LoudnessError::NoAudioStream)
    ));
    assert!(matches!(loudness_status(4), Err(LoudnessError::Decoder)));
    assert!(matches!(
        loudness_status(5),
        Err(LoudnessError::FilterGraph)
    ));
    assert!(matches!(loudness_status(6), Err(LoudnessError::Pipeline)));
    assert!(matches!(loudness_status(7), Err(LoudnessError::NoReport)));
    assert!(matches!(
        loudness_status(99),
        Err(LoudnessError::Unknown(99))
    ));
}

#[test]
fn waveform_status_maps_every_code() {
    assert!(matches!(waveform_status(0), Ok(())));
    assert!(matches!(waveform_status(1), Err(WaveformError::OpenInput)));
    assert!(matches!(waveform_status(2), Err(WaveformError::StreamInfo)));
    assert!(matches!(
        waveform_status(3),
        Err(WaveformError::NoAudioStream)
    ));
    assert!(matches!(waveform_status(4), Err(WaveformError::Decoder)));
    assert!(matches!(
        waveform_status(5),
        Err(WaveformError::FilterGraph)
    ));
    assert!(matches!(waveform_status(6), Err(WaveformError::Pipeline)));
    assert!(matches!(
        waveform_status(99),
        Err(WaveformError::Unknown(99))
    ));
}

#[test]
fn decode_seek_status_maps_every_code() {
    assert!(matches!(decode_seek_status(0), Ok(())));
    assert!(matches!(decode_seek_status(7), Err(DecodeError::Seek)));
    assert!(matches!(
        decode_seek_status(99),
        Err(DecodeError::Unknown(99))
    ));
}

#[test]
fn failure_and_seek_and_advance_statuses_are_mapped() {
    assert!(matches!(decode_seek_status(0), Ok(())));
    assert!(matches!(decode_seek_status(7), Err(DecodeError::Seek)));
    assert!(matches!(
        decode_seek_status(99),
        Err(DecodeError::Unknown(99))
    ));
    assert!(matches!(decode_frame_failure(8), DecodeError::Pipeline));
    assert!(matches!(decode_frame_failure(9), DecodeError::FrameSize));
    assert!(matches!(decode_frame_failure(99), DecodeError::Unknown(99)));
    assert!(matches!(decode_advance_status(0, 1.5), Ok(Advance::Frame(t)) if t == 1.5));
    assert!(matches!(
        decode_advance_status(0, f64::NAN),
        Err(DecodeError::Pipeline)
    ));
    assert!(matches!(
        decode_advance_status(1, 0.0),
        Ok(Advance::EndOfStream)
    ));
    assert!(matches!(
        decode_advance_status(2, 0.0),
        Ok(Advance::Cancelled)
    ));
    assert!(matches!(
        decode_advance_status(8, 0.0),
        Err(DecodeError::Pipeline)
    ));
    assert!(matches!(
        decode_advance_status(99, 0.0),
        Err(DecodeError::Unknown(99))
    ));
}

#[test]
fn frame_dimensions_and_statuses_are_validated() {
    assert_eq!(decode_dimensions(640, 360).unwrap(), (640, 360));
    for (width, height) in [(0, 360), (640, 0), (-1, 360), (640, -1)] {
        assert!(matches!(
            decode_dimensions(width, height),
            Err(DecodeError::FrameSize)
        ));
    }
    assert!(next_frame_status(0).unwrap());
    assert!(!next_frame_status(1).unwrap());
    assert!(!next_frame_status(2).unwrap());
    assert!(matches!(next_frame_status(8), Err(DecodeError::Pipeline)));
    assert!(take_frame_status(0).unwrap());
    assert!(!take_frame_status(1).unwrap());
    assert!(matches!(take_frame_status(2), Err(DecodeError::Unknown(2))));
    assert!(matches!(take_frame_status(9), Err(DecodeError::FrameSize)));
}

#[test]
fn a_native_frame_without_pixels_or_a_valid_size_is_rejected() {
    let mut empty = RawDecodedRgbaFrame {
        pixels: std::ptr::null_mut(),
        pixel_len: 0,
        width: 2,
        height: 2,
        stride_bytes: 8,
        timestamp_secs: 0.0,
        borrowed: 0,
    };
    assert!(matches!(
        copy_out_frame(&mut empty),
        Err(DecodeError::FrameSize)
    ));
    let mut negative = RawDecodedRgbaFrame { width: -1, ..empty };
    assert!(matches!(
        copy_out_frame(&mut negative),
        Err(DecodeError::FrameSize)
    ));
}

#[test]
fn copied_samples_follow_the_count() {
    let source = [0.25_f32, -0.5, 1.0];
    assert_eq!(unsafe { copy_samples(source.as_ptr(), 3) }, source.to_vec());
    assert!(unsafe { copy_samples(std::ptr::null(), 0) }.is_empty());
}

#[test]
fn serde_defaults_and_the_encoder_preference_codes() {
    assert_eq!(default_anchor(), 0.5);
    assert_eq!(default_voice_cleanup_noise_floor_db(), -30.0);
    assert_eq!(default_voice_cleanup_compressor_threshold_db(), -18.0);
    assert_eq!(default_voice_cleanup_compressor_ratio(), 3.0);
    assert_eq!(default_voice_cleanup_ceiling_linear(), 0.95);
    assert_eq!(GpuEncoderPreference::VideoToolbox.as_c_int(), 6);
    assert_eq!(version(), version());
}
