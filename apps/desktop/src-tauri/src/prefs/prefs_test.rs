// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

use super::*;

#[test]
fn round_trip_preserves_unknown_egui_preferences() {
    let mut raw = BTreeMap::new();
    raw.insert("telemetry_enabled".to_string(), Value::Boolean(true));
    let mut document = PrefsDocument {
        raw,
        view: PrefsView {
            locale: "en".to_string(),
            ..PrefsView::default()
        },
    };
    let patch = rmp_serde::to_vec_named(&DiskPrefs::from(&document.view)).unwrap();
    let Value::Map(entries) = rmp_serde::from_slice::<Value>(&patch).unwrap() else {
        panic!("serialized preferences must be a map");
    };
    for (key, value) in entries {
        document
            .raw
            .insert(key.as_str().unwrap().to_string(), value);
    }
    assert_eq!(document.raw["telemetry_enabled"], Value::Boolean(true));
    assert_eq!(document.raw["locale"], Value::from("en"));
}

fn temp_file(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("oca_desktop_prefs_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("nested").join("prefs.oc")
}

#[test]
fn defaults_match_the_documented_first_run_state() {
    let defaults = PrefsView::default();
    assert_eq!(defaults.locale, "pt_br");
    assert_eq!(defaults.autosave_minutes, 5);
    assert!(defaults.preview_hardware_decode);
    assert!(defaults.telemetry_enabled);
    assert_eq!(
        defaults.error_reporting_consent,
        ErrorReportingConsent::Disabled
    );
    assert!(default_telemetry_enabled() && default_hardware_decode());
    let document = PrefsDocument::default();
    assert!(document.raw.is_empty());
}

#[test]
fn a_missing_or_corrupt_file_loads_the_defaults() {
    let path = temp_file("missing");
    assert_eq!(PrefsDocument::load_from(&path).view.autosave_minutes, 5);

    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"not a prefs file").unwrap();
    assert_eq!(PrefsDocument::load_from(&path).view.locale, "pt_br");

    // Valid framing around a value that is not a map is also the defaults.
    let not_a_map = avcore::to_ocproj_bytes(&Value::from(7)).unwrap();
    std::fs::write(&path, not_a_map).unwrap();
    assert_eq!(PrefsDocument::load_from(&path).view.locale, "pt_br");
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn saving_creates_the_folder_and_keeps_keys_this_shell_does_not_know() {
    let path = temp_file("roundtrip");
    let mut document = PrefsDocument::default();
    document
        .raw
        .insert("egui_only_setting".to_string(), Value::from("keep me"));
    document.view.locale = "en".to_string();
    document.view.autosave_minutes = 9;
    document.view.output_folder = "D:/exports".to_string();
    document.view.recent_project_paths = vec!["a.ocproj".to_string()];
    document.view.telemetry_enabled = false;
    document.view.error_reporting_consent = ErrorReportingConsent::AlwaysSend;
    document.view.last_reviewed_crash_unix = 42;
    document.save_to(&path).unwrap();

    let loaded = PrefsDocument::load_from(&path);
    assert_eq!(loaded.view.locale, "en");
    assert_eq!(loaded.view.autosave_minutes, 9);
    assert_eq!(loaded.view.output_folder, "D:/exports");
    assert_eq!(
        loaded.view.recent_project_paths,
        vec!["a.ocproj".to_string()]
    );
    assert!(!loaded.view.telemetry_enabled);
    assert_eq!(
        loaded.view.error_reporting_consent,
        ErrorReportingConsent::AlwaysSend
    );
    assert_eq!(loaded.view.last_reviewed_crash_unix, 42);
    assert_eq!(loaded.raw["egui_only_setting"], Value::from("keep me"));
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn a_blank_locale_and_a_zero_interval_fall_back_to_the_defaults() {
    let disk = DiskPrefs {
        locale: String::new(),
        autosave_minutes: 0,
        ..DiskPrefs::default()
    };
    let view = PrefsView::from(disk);
    assert_eq!(view.locale, "pt_br");
    assert_eq!(view.autosave_minutes, 5);
}

#[test]
fn only_string_keys_of_a_map_are_kept() {
    let value = Value::Map(vec![
        (Value::from("locale"), Value::from("en")),
        (Value::from(5), Value::from("ignored")),
    ]);
    let document = PrefsDocument::from_value(value).unwrap();
    assert_eq!(document.raw.len(), 1);
    assert_eq!(document.view.locale, "en");
    assert!(PrefsDocument::from_value(Value::from(true)).is_err());
}

#[test]
fn an_unwritable_prefs_path_fails_to_save() {
    // A file where the parent directory should be makes the write impossible.
    let path = temp_file("blocked");
    std::fs::create_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
    std::fs::write(path.parent().unwrap(), b"a file, not a folder").unwrap();
    assert!(PrefsDocument::default().save_to(&path).is_err());
    let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
}

#[test]
fn the_default_loader_reads_the_preferences_file_or_the_defaults() {
    let loaded = PrefsDocument::load();
    assert!(loaded.view.autosave_minutes > 0);
}

#[test]
fn a_path_without_a_parent_folder_fails_to_save_but_does_not_panic() {
    let mut document = PrefsDocument::default();
    assert!(document.save_to(std::path::Path::new("")).is_err());
}
