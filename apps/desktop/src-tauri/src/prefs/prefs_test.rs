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
