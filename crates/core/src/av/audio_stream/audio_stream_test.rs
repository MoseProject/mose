use std::path::PathBuf;

use super::*;

fn multitrack() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audio_codecs/obs_multitrack.mkv")
}

fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("oca-audio-stream-{name}-{}", std::process::id()))
}

#[test]
fn each_track_of_a_recording_gets_its_own_file_name() {
    let dir = Path::new("cache");
    let a = audio_stream_file_path(Path::new("/clips/run.mkv"), 2, dir);
    let b = audio_stream_file_path(Path::new("/clips/run.mkv"), 3, dir);
    let other_folder = audio_stream_file_path(Path::new("/other/run.mkv"), 2, dir);
    assert_ne!(a, b);
    assert_ne!(
        a, other_folder,
        "same name in another folder is another file"
    );
    assert_eq!(
        a,
        audio_stream_file_path(Path::new("/clips/run.mkv"), 2, dir)
    );
    assert!(a.to_string_lossy().ends_with(".mka"));
}

#[test]
fn a_track_is_copied_out_once_and_probes_as_audio_only() {
    let dir = scratch("copy");
    let path = ensure_audio_stream_file(&multitrack(), 2, &dir).expect("copies the Mic track");
    let first = fs::metadata(&path).unwrap().modified().unwrap();
    let asset = crate::probe_media(&path).expect("the copy probes");
    assert_eq!(asset.kind, crate::MediaKind::Audio);
    assert!(asset.has_audio);
    // A second call reuses the finished copy instead of writing it again.
    let again = ensure_audio_stream_file(&multitrack(), 2, &dir).unwrap();
    assert_eq!(again, path);
    assert_eq!(fs::metadata(&again).unwrap().modified().unwrap(), first);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_track_that_is_not_there_fails_and_leaves_nothing_behind() {
    let dir = scratch("missing");
    assert!(ensure_audio_stream_file(&multitrack(), 9, &dir).is_err());
    assert!(
        ensure_audio_stream_file(&multitrack(), 0, &dir).is_err(),
        "stream 0 is video"
    );
    let leftovers: Vec<_> = fs::read_dir(&dir)
        .map(|entries| entries.flatten().map(|e| e.file_name()).collect())
        .unwrap_or_default();
    let _ = fs::remove_dir_all(&dir);
    assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
}
