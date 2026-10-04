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

//! The ONNX Runtime calls of the Piper voice, face detection and person segmentation models.
//! The models are separate downloads that are not in the repository, so this file is excluded
//! from coverage; everything around the model calls (sentence assembly, tensor selection and
//! decoding, matte resizing) is tested by passing the model run in as a function.

use std::path::Path;

use ort::session::Session;
use ort::value::Value;

use crate::auto_reframe::ReframeError;
use crate::background_removal::SegmentError;
use crate::text_to_speech::{PiperVoiceConfig, TtsError};

/// Opens the Piper model once and returns a function that speaks one sentence's phoneme ids.
pub(crate) fn piper_runner(
    model_path: &Path,
    config: &PiperVoiceConfig,
) -> Result<impl FnMut(Vec<i64>) -> Result<Vec<f32>, TtsError>, TtsError> {
    let mut session = Session::builder()
        .map_err(|e| TtsError::Onnx(e.to_string()))?
        .commit_from_file(model_path)
        .map_err(|e| TtsError::Onnx(e.to_string()))?;
    let scales = [
        config.inference.noise_scale,
        config.inference.length_scale,
        config.inference.noise_w,
    ];
    let multi_speaker = config.num_speakers > 1;

    Ok(move |ids: Vec<i64>| {
        let ids_len = ids.len();
        let input =
            Value::from_array(([1, ids_len], ids)).map_err(|e| TtsError::Onnx(e.to_string()))?;
        let input_lengths = Value::from_array(([1], vec![ids_len as i64]))
            .map_err(|e| TtsError::Onnx(e.to_string()))?;
        let scales =
            Value::from_array(([3], scales.to_vec())).map_err(|e| TtsError::Onnx(e.to_string()))?;

        let mut inputs = vec![
            ("input", input.into_dyn()),
            ("input_lengths", input_lengths.into_dyn()),
            ("scales", scales.into_dyn()),
        ];
        if multi_speaker {
            let sid =
                Value::from_array(([1], vec![0i64])).map_err(|e| TtsError::Onnx(e.to_string()))?;
            inputs.push(("sid", sid.into_dyn()));
        }

        let outputs = session
            .run(inputs)
            .map_err(|e| TtsError::Onnx(e.to_string()))?;
        let (_name, value) = outputs
            .iter()
            .next()
            .ok_or_else(|| TtsError::Onnx("model produced no output tensors".into()))?;
        let (_shape, data) = value
            .try_extract_tensor::<f32>()
            .map_err(|e| TtsError::Onnx(e.to_string()))?;
        Ok(data.to_vec())
    })
}

/// Runs the face-detection model on a preprocessed `1x3xHxW` input and returns every output
/// tensor as `(dims, data)`.
pub(crate) fn run_face_model(
    model_path: &Path,
    input_data: Vec<f32>,
    input_width: u32,
    input_height: u32,
) -> Result<Vec<(Vec<i64>, Vec<f32>)>, ReframeError> {
    let mut session = Session::builder()
        .map_err(|e| ReframeError::Onnx(e.to_string()))?
        .commit_from_file(model_path)
        .map_err(|e| ReframeError::Onnx(e.to_string()))?;
    let input = Value::from_array((
        [1, 3, input_height as usize, input_width as usize],
        input_data,
    ))
    .map_err(|e| ReframeError::Onnx(e.to_string()))?;
    let outputs = session
        .run(ort::inputs![input])
        .map_err(|e| ReframeError::Onnx(e.to_string()))?;
    let mut tensors = Vec::new();
    for (_name, value) in outputs.iter() {
        let (shape, data) = value
            .try_extract_tensor::<f32>()
            .map_err(|e| ReframeError::Onnx(e.to_string()))?;
        tensors.push((shape.iter().copied().collect::<Vec<i64>>(), data.to_vec()));
    }
    Ok(tensors)
}

/// Runs the person-segmentation model on a preprocessed `1x3xHxW` input and returns its first
/// output as the model-resolution matte.
pub(crate) fn run_segmentation_model(
    model_path: &Path,
    input_data: Vec<f32>,
    model_width: u32,
    model_height: u32,
) -> Result<Vec<f32>, SegmentError> {
    let mut session = Session::builder()
        .map_err(|e| SegmentError::Onnx(e.to_string()))?
        .commit_from_file(model_path)
        .map_err(|e| SegmentError::Onnx(e.to_string()))?;
    let input = Value::from_array((
        [1, 3, model_height as usize, model_width as usize],
        input_data,
    ))
    .map_err(|e| SegmentError::Onnx(e.to_string()))?;
    let outputs = session
        .run(ort::inputs![input])
        .map_err(|e| SegmentError::Onnx(e.to_string()))?;
    let (_name, value) = outputs
        .iter()
        .next()
        .ok_or_else(|| SegmentError::Onnx("model produced no output tensors".into()))?;
    let (_shape, data) = value
        .try_extract_tensor::<f32>()
        .map_err(|e| SegmentError::Onnx(e.to_string()))?;
    Ok(data.to_vec())
}
