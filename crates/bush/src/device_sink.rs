//! Real audio output through `cpal`, behind the `cpal` feature.
//!
//! [`DeviceSink`] implements [`AudioSink`] with a bounded queue the device callback drains. The
//! callback never allocates or waits on anything but the queue lock; underflows and the number of
//! frames the device actually consumed are counted so playback health and clock drift can be
//! measured against real hardware.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use crate::{AudioBlock, AudioSink, SinkError};

/// Largest channel count the engine mixes; wider devices get the extra channels silent.
const ENGINE_MAX_CHANNELS: u16 = 2;

#[derive(Debug)]
pub enum DeviceSinkError {
    /// The host has no default output device.
    NoDevice,
    UnsupportedFormat(SampleFormat),
    Device(String),
    /// The new default device differs in rate or channels; open a fresh sink.
    FormatChanged {
        sample_rate: u32,
        channels: u16,
    },
}

struct Shared {
    queue: Mutex<VecDeque<f32>>,
    capacity_samples: usize,
    engine_channels: usize,
    consumed_frames: AtomicU64,
    underflow_callbacks: AtomicU64,
    failed: AtomicBool,
    recoveries: AtomicU64,
}

/// A running output stream fed through [`AudioSink`]. Dropping it stops the stream.
pub struct DeviceSink {
    shared: Arc<Shared>,
    stream: cpal::Stream,
    sample_rate: u32,
    engine_channels: u16,
}

impl std::fmt::Debug for DeviceSink {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DeviceSink")
            .field("sample_rate", &self.sample_rate)
            .field("engine_channels", &self.engine_channels)
            .finish_non_exhaustive()
    }
}

impl DeviceSink {
    /// Opens the default output device at its native rate. `capacity_frames` bounds the queue.
    pub fn open_default(capacity_frames: usize) -> Result<Self, DeviceSinkError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(DeviceSinkError::NoDevice)?;
        let supported = device
            .default_output_config()
            .map_err(|error| DeviceSinkError::Device(error.to_string()))?;
        let engine_channels = supported.channels().clamp(1, ENGINE_MAX_CHANNELS);
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::with_capacity(
                capacity_frames * usize::from(engine_channels),
            )),
            capacity_samples: capacity_frames * usize::from(engine_channels),
            engine_channels: usize::from(engine_channels),
            consumed_frames: AtomicU64::new(0),
            underflow_callbacks: AtomicU64::new(0),
            failed: AtomicBool::new(false),
            recoveries: AtomicU64::new(0),
        });
        let stream = open_stream(&device, &supported, &shared)?;
        Ok(Self {
            shared,
            stream,
            sample_rate: supported.sample_rate(),
            engine_channels,
        })
    }

    /// True once the stream reported an error (device unplugged, driver reset). Writes fail
    /// with [`SinkError::Closed`] until [`DeviceSink::recover`] succeeds.
    pub fn is_failed(&self) -> bool {
        self.shared.failed.load(Ordering::Acquire)
    }

    /// Times [`DeviceSink::recover`] brought the stream back.
    pub fn recoveries(&self) -> u64 {
        self.shared.recoveries.load(Ordering::Relaxed)
    }

    /// Reopens the stream on the current default output device after a failure (or on demand,
    /// for example after the user changes the default device). The queue is emptied, so the
    /// caller must re-seek its [`crate::AudioPlayer`] to refill from the playhead;
    /// `consumed_frames` keeps counting across the recovery. If the new device runs at a
    /// different rate or channel count the audio renderer would be wrong, so that case fails
    /// with [`DeviceSinkError::FormatChanged`] and the caller opens a fresh sink instead.
    pub fn recover(&mut self) -> Result<(), DeviceSinkError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(DeviceSinkError::NoDevice)?;
        let supported = device
            .default_output_config()
            .map_err(|error| DeviceSinkError::Device(error.to_string()))?;
        let channels = supported.channels().clamp(1, ENGINE_MAX_CHANNELS);
        if supported.sample_rate() != self.sample_rate || channels != self.engine_channels {
            return Err(DeviceSinkError::FormatChanged {
                sample_rate: supported.sample_rate(),
                channels,
            });
        }
        self.shared
            .queue
            .lock()
            .expect("device queue poisoned")
            .clear();
        match open_stream(&device, &supported, &self.shared) {
            Ok(stream) => {
                self.stream = stream;
                self.shared.failed.store(false, Ordering::Release);
                self.shared.recoveries.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(error) => {
                self.shared.failed.store(true, Ordering::Release);
                Err(error)
            }
        }
    }
    /// The device sample rate; build the audio renderer at this rate.
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Channels the engine should mix for this device (1 or 2).
    pub const fn channels(&self) -> u16 {
        self.engine_channels
    }

    /// Frames of real queued audio the device callback has pulled so far.
    pub fn consumed_frames(&self) -> u64 {
        self.shared.consumed_frames.load(Ordering::Relaxed)
    }

    /// Callbacks that found the queue short and padded the output with silence.
    pub fn underflow_callbacks(&self) -> u64 {
        self.shared.underflow_callbacks.load(Ordering::Relaxed)
    }
}

fn open_stream(
    device: &cpal::Device,
    supported: &cpal::SupportedStreamConfig,
    shared: &Arc<Shared>,
) -> Result<cpal::Stream, DeviceSinkError> {
    let config = supported.config();
    let device_channels = usize::from(config.channels);
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(device, config, shared, device_channels),
        SampleFormat::I16 => build::<i16>(device, config, shared, device_channels),
        SampleFormat::I32 => build::<i32>(device, config, shared, device_channels),
        other => return Err(DeviceSinkError::UnsupportedFormat(other)),
    }?;
    stream
        .play()
        .map_err(|error| DeviceSinkError::Device(error.to_string()))?;
    Ok(stream)
}
fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    shared: &Arc<Shared>,
    device_channels: usize,
) -> Result<cpal::Stream, DeviceSinkError>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let callback_shared = Arc::clone(shared);
    let error_shared = Arc::clone(shared);
    device
        .build_output_stream(
            config,
            move |output: &mut [T], _| fill(output, &callback_shared, device_channels),
            move |_error| error_shared.failed.store(true, Ordering::Release),
            None,
        )
        .map_err(|error| DeviceSinkError::Device(error.to_string()))
}

/// Device callback: drains whole frames from the queue, padding with silence on underflow and
/// spreading the engine's 1-2 channels across the device's channel layout.
fn fill<T>(output: &mut [T], shared: &Shared, device_channels: usize)
where
    T: SizedSample + FromSample<f32>,
{
    let frames = output.len() / device_channels;
    let mut queue = shared.queue.lock().expect("device queue poisoned");
    let available = queue.len() / shared.engine_channels;
    let served = available.min(frames);
    for frame in 0..frames {
        for channel in 0..device_channels {
            let sample = if frame < served && channel < shared.engine_channels {
                queue.pop_front().unwrap_or(0.0)
            } else {
                0.0
            };
            output[frame * device_channels + channel] = T::from_sample(sample);
        }
    }
    drop(queue);
    shared
        .consumed_frames
        .fetch_add(served as u64, Ordering::Relaxed);
    if served < frames {
        shared.underflow_callbacks.fetch_add(1, Ordering::Relaxed);
    }
}

impl AudioSink for DeviceSink {
    fn write(&mut self, block: &AudioBlock) -> Result<(), SinkError> {
        if self.shared.failed.load(Ordering::Acquire) {
            return Err(SinkError::Closed);
        }
        let mut queue = self.shared.queue.lock().expect("device queue poisoned");
        if queue.len() + block.samples().len() > self.shared.capacity_samples {
            return Err(SinkError::Overrun {
                capacity_frames: self.shared.capacity_samples / self.shared.engine_channels,
                requested_frames: block.frame_count(),
            });
        }
        queue.extend(block.samples().iter().copied());
        Ok(())
    }

    fn buffered_frames(&self) -> usize {
        self.shared
            .queue
            .lock()
            .expect("device queue poisoned")
            .len()
            / self.shared.engine_channels
    }

    fn clear(&mut self) {
        self.shared
            .queue
            .lock()
            .expect("device queue poisoned")
            .clear();
    }
}
