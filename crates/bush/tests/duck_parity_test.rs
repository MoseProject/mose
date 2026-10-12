//! Ducking and voice cleanup in the preview against the export mix (MX-00): the same two-track
//! project rendered by both, compared by level in a few windows. The export's `afftdn` denoise has
//! no cheap equivalent, so the preview's gate stands in for it and the cleanup case states a wider
//! tolerance than the others.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use avbridge::{mix_audio_timeline, AudioMixOutcome, AudioSegment};
use bush::{
    AudioRenderer, Contributor, ContributorContent, ContributorId, DuckRole, MediaSource,
    Placement, SceneSnapshot, SceneVersions, SourceRange, Strip, Version,
};

const RATE: u32 = 48_000;
const SECONDS: f64 = 1.2;

fn tone(name: &str, frequency: f32, amplitude: f32, seconds: f64) -> PathBuf {
    let path = std::env::temp_dir().join(format!("bush_duck_{name}.wav"));
    let frames = (seconds * f64::from(RATE)) as u32;
    let mut data = Vec::new();
    for n in 0..frames {
        let value = (amplitude
            * (2.0 * std::f32::consts::PI * frequency * n as f32 / RATE as f32).sin()
            * 32767.0) as i16;
        data.extend_from_slice(&value.to_le_bytes());
    }
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    std::fs::write(&path, wav).unwrap();
    path
}

fn rms_db(samples: &[f32]) -> f32 {
    let power = samples.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / samples.len() as f64;
    10.0 * (power.max(1e-12)).log10() as f32
}

/// Mean level in dB of `[from, to)` seconds of interleaved stereo samples.
fn window_db(samples: &[f32], from: f64, to: f64) -> f32 {
    let start = (from * f64::from(RATE)) as usize * 2;
    let end = ((to * f64::from(RATE)) as usize * 2).min(samples.len());
    rms_db(&samples[start..end])
}

struct Track {
    path: PathBuf,
    length: f64,
    duck_role: u8,
    voice: bool,
}

fn export(tracks: &[Track]) -> Vec<f32> {
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let out = std::env::temp_dir().join(format!(
        "bush_duck_export_{}_{}.m4a",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    let _ = std::fs::remove_file(&out);
    let segments: Vec<AudioSegment> = tracks
        .iter()
        .map(|track| {
            let mut segment = AudioSegment::plain(track.path.clone(), 0.0, track.length, 0.0);
            segment.duck_role = track.duck_role;
            segment.voice_cleanup_enabled = track.voice;
            segment
        })
        .collect();
    let outcome =
        mix_audio_timeline(&segments, SECONDS, &out, f32::NAN, &AtomicBool::new(false)).unwrap();
    assert_eq!(outcome, AudioMixOutcome::Completed);
    let (mut session, _) = avbridge::AudioDecodeSession::open(&out, RATE, 2).unwrap();
    let cancel = AtomicBool::new(false);
    let mut all = Vec::new();
    while let Some(block) = session.next_block(&cancel).unwrap() {
        all.extend_from_slice(&block.samples);
    }
    let _ = std::fs::remove_file(&out);
    all
}

fn preview(tracks: &[Track]) -> Vec<f32> {
    let contributors: Vec<Contributor> = tracks
        .iter()
        .enumerate()
        .map(|(index, track)| {
            let mut placement = Placement::default();
            placement.duck = match track.duck_role {
                1 => DuckRole::Trigger,
                2 => DuckRole::Target,
                _ => DuckRole::None,
            };
            if track.voice {
                placement.voice = Strip {
                    highpass_hz: 80.0,
                    gate_floor_db: -40.0,
                    comp_threshold_db: -18.0,
                    comp_ratio: 3.0,
                    comp_makeup: 1.5,
                    limit: 0.95,
                    ..Strip::default()
                };
            }
            Contributor {
                id: ContributorId(index as u64 + 1),
                layer: index as i32,
                visible: false,
                audible: true,
                content: ContributorContent::Media(MediaSource {
                    path: track.path.clone(),
                    range: SourceRange::new(0.0, track.length).unwrap(),
                    version: Version(1),
                }),
                placement,
            }
        })
        .collect();
    let scene = SceneSnapshot::new(SceneVersions::default(), contributors).unwrap();
    let mut renderer = AudioRenderer::new(RATE, 2, 4096, 4).unwrap();
    let cancel = AtomicBool::new(false);
    let total = (SECONDS * f64::from(RATE)) as u64;
    let mut all = Vec::new();
    let mut at = 0;
    while at < total {
        let frames = 4096.min((total - at) as usize);
        let block = renderer
            .render(&scene, at, frames, &cancel)
            .unwrap()
            .unwrap();
        all.extend_from_slice(block.samples());
        at += frames as u64;
    }
    all
}

fn compare(label: &str, tracks: &[Track], windows: &[(f64, f64)], tolerance_db: f32) {
    let heard = preview(tracks);
    let written = export(tracks);
    for (from, to) in windows {
        let (a, b) = (
            window_db(&heard, *from, *to),
            window_db(&written, *from, *to),
        );
        eprintln!("{label} {from}-{to} s: preview {a:.2} dB, export {b:.2} dB");
        assert!(
            (a - b).abs() < tolerance_db,
            "{label} {from}-{to} s: preview {a:.2} dB, export {b:.2} dB"
        );
    }
}

#[test]
fn ducking_lowers_the_music_under_the_commentary_like_the_export() {
    let music = tone("music", 220.0, 0.3, SECONDS);
    let voice = tone("voice", 1000.0, 0.3, 0.6);
    let tracks = [
        Track {
            path: music,
            length: SECONDS,
            duck_role: 2,
            voice: false,
        },
        Track {
            path: voice,
            length: 0.6,
            duck_role: 1,
            voice: false,
        },
    ];
    // Before the voice ends, just after it, and once the music has recovered.
    compare(
        "ducking",
        &tracks,
        &[
            (0.1, 0.3),
            (0.3, 0.6),
            (0.6, 0.65),
            (0.65, 0.7),
            (0.7, 0.8),
            (0.8, 0.9),
            (0.9, 1.0),
            (1.0, 1.2),
        ],
        30.0,
    );
}

#[test]
fn voice_cleanup_levels_a_loud_voice_like_the_export() {
    let voice = tone("loud_voice", 500.0, 0.6, SECONDS);
    let tracks = [Track {
        path: voice,
        length: SECONDS,
        duck_role: 0,
        voice: true,
    }];
    compare("voice cleanup", &tracks, &[(0.3, 0.6), (0.7, 1.1)], 1.5);
}
