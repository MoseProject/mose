//! Shared `prefs.oc` persistence. The desktop shell only exposes the settings it implements,
//! but retains unknown MessagePack map entries written by the egui shell.

use std::collections::BTreeMap;

use avcore::LayerTemplate;
use rmpv::Value;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrefsView {
    pub locale: String,
    pub lufs_profile: usize,
    pub output_folder: String,
    pub autosave_minutes: u8,
    pub preview_hardware_decode: bool,
    pub recent_project_paths: Vec<String>,
    /// Saved layer-group templates -- app-wide, shared with the egui shell's
    /// `PrefsState.saved_layer_templates` under the same `prefs.oc` key.
    #[serde(default)]
    pub saved_layer_templates: Vec<LayerTemplate>,
    /// Path to a local GGML Whisper model file, or empty to use the bundled Whisper Base model
    /// -- shared with the egui shell's `PrefsState.whisper_model_path` under the same key.
    #[serde(default)]
    pub whisper_model_path: String,
    /// Path to a local UltraFace ONNX model file for auto-reframe, or empty to use the bundled
    /// model -- shared with the egui shell's `PrefsState.reframe_model_path` key.
    #[serde(default)]
    pub reframe_model_path: String,
    /// Path to a local MODNet ONNX model file for AI background removal, or empty to use the
    /// bundled model -- shared with the egui shell's `PrefsState.background_removal_model_path`
    /// key.
    #[serde(default)]
    pub background_removal_model_path: String,
    /// Path to a local Piper voice `.onnx` model file for text-to-speech, or empty to use the
    /// bundled pt-BR voice -- shared with the egui shell's `PrefsState.tts_model_path` key.
    #[serde(default)]
    pub tts_model_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
struct DiskPrefs {
    #[serde(default)]
    locale: String,
    #[serde(default)]
    lufs_profile: usize,
    #[serde(default)]
    output_folder: String,
    #[serde(default)]
    autosave_minutes: u8,
    #[serde(default = "default_hardware_decode")]
    preview_hardware_decode: bool,
    #[serde(default)]
    recent_project_paths: Vec<String>,
    #[serde(default)]
    saved_layer_templates: Vec<LayerTemplate>,
    #[serde(default)]
    whisper_model_path: String,
    #[serde(default)]
    reframe_model_path: String,
    #[serde(default)]
    background_removal_model_path: String,
    #[serde(default)]
    tts_model_path: String,
}

fn default_hardware_decode() -> bool {
    true
}

impl From<DiskPrefs> for PrefsView {
    fn from(value: DiskPrefs) -> Self {
        let defaults = Self::default();
        Self {
            locale: if value.locale.is_empty() {
                defaults.locale
            } else {
                value.locale
            },
            lufs_profile: value.lufs_profile,
            output_folder: value.output_folder,
            autosave_minutes: if value.autosave_minutes == 0 {
                defaults.autosave_minutes
            } else {
                value.autosave_minutes
            },
            preview_hardware_decode: value.preview_hardware_decode,
            recent_project_paths: value.recent_project_paths,
            saved_layer_templates: value.saved_layer_templates,
            whisper_model_path: value.whisper_model_path,
            reframe_model_path: value.reframe_model_path,
            background_removal_model_path: value.background_removal_model_path,
            tts_model_path: value.tts_model_path,
        }
    }
}

impl From<&PrefsView> for DiskPrefs {
    fn from(value: &PrefsView) -> Self {
        Self {
            locale: value.locale.clone(),
            lufs_profile: value.lufs_profile,
            output_folder: value.output_folder.clone(),
            autosave_minutes: value.autosave_minutes,
            preview_hardware_decode: value.preview_hardware_decode,
            recent_project_paths: value.recent_project_paths.clone(),
            saved_layer_templates: value.saved_layer_templates.clone(),
            whisper_model_path: value.whisper_model_path.clone(),
            reframe_model_path: value.reframe_model_path.clone(),
            background_removal_model_path: value.background_removal_model_path.clone(),
            tts_model_path: value.tts_model_path.clone(),
        }
    }
}

impl Default for PrefsView {
    fn default() -> Self {
        Self {
            locale: "pt_br".to_string(),
            lufs_profile: 0,
            output_folder: String::new(),
            autosave_minutes: 5,
            preview_hardware_decode: true,
            recent_project_paths: Vec::new(),
            saved_layer_templates: Vec::new(),
            whisper_model_path: String::new(),
            reframe_model_path: String::new(),
            background_removal_model_path: String::new(),
            tts_model_path: String::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PrefsDocument {
    raw: BTreeMap<String, Value>,
    pub view: PrefsView,
}

impl PrefsDocument {
    pub fn load() -> Self {
        let Ok(bytes) = std::fs::read(prefs_path()) else {
            return Self::default();
        };
        let Ok(value) = avcore::from_ocproj_bytes::<Value>(&bytes) else {
            return Self::default();
        };
        Self::from_value(value).unwrap_or_default()
    }

    fn from_value(value: Value) -> Result<Self, String> {
        let Value::Map(entries) = value else {
            return Err("preferences must be a MessagePack map".to_string());
        };
        let raw = entries
            .into_iter()
            .filter_map(|(key, value)| key.as_str().map(|key| (key.to_string(), value)))
            .collect::<BTreeMap<_, _>>();
        let encoded = rmp_serde::to_vec(&Value::Map(
            raw.iter()
                .map(|(key, value)| (Value::from(key.as_str()), value.clone()))
                .collect(),
        ))
        .map_err(|error| error.to_string())?;
        let view = rmp_serde::from_slice::<DiskPrefs>(&encoded)
            .unwrap_or_default()
            .into();
        Ok(Self { raw, view })
    }

    pub fn save(&mut self) -> Result<(), String> {
        // Struct-map (named) encoding: plain `to_vec` writes structs as arrays, which neither
        // this merge nor the egui shell's `PrefsState` can read back.
        let patch = rmp_serde::to_vec_named(&DiskPrefs::from(&self.view))
            .map_err(|error| error.to_string())?;
        let Value::Map(entries) =
            rmp_serde::from_slice::<Value>(&patch).map_err(|error| error.to_string())?
        else {
            return Err("preferences patch must be a MessagePack map".to_string());
        };
        for (key, value) in entries {
            if let Some(key) = key.as_str() {
                self.raw.insert(key.to_string(), value);
            }
        }
        let value = Value::Map(
            self.raw
                .iter()
                .map(|(key, value)| (Value::from(key.as_str()), value.clone()))
                .collect(),
        );
        let bytes = avcore::to_ocproj_bytes(&value).map_err(|error| error.to_string())?;
        let path = prefs_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(path, bytes).map_err(|error| error.to_string())
    }
}

impl Default for PrefsDocument {
    fn default() -> Self {
        Self {
            raw: BTreeMap::new(),
            view: PrefsView::default(),
        }
    }
}

pub use avcore::app_paths::{autosave_path, prefs_path};

#[cfg(test)]
mod tests {
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
}
