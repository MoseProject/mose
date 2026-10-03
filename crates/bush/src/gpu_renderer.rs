//! GPU compositor (feature `gpu`): the CPU reference compositor ported to a wgpu compute shader.
//!
//! Layers are uploaded as packed RGBA words into storage buffers and blended one dispatch per
//! layer into a canvas storage buffer, which is read back at the end. The shader reproduces the
//! reference arithmetic (including the byte-exact fast paths), so parity with [`CpuRenderer`]
//! is checked pixel by pixel in tests and any difference is a bug, not a quality setting.

use wgpu::util::DeviceExt;

use crate::cpu_renderer::LayerGeometry;
use crate::{BlendMode, LayerFrame, MaskShape, PixelFormat, RenderError, Renderer, VideoFrame};

const SHADER: &str = include_str!("gpu_renderer.wgsl");

/// Size in bytes of the shader `Params` uniform (see `gpu_renderer.wgsl`).
const PARAMS_BYTES: usize = 160;

const FLAG_CHROMA: u32 = 1;
const FLAG_MASK: u32 = 2;
const FLAG_LUT: u32 = 4;
const FLAG_COLOUR: u32 = 8;
const FLAG_FLIP_X: u32 = 16;
const FLAG_FLIP_Y: u32 = 32;

#[derive(Debug)]
pub enum GpuError {
    /// No adapter could be created (no GPU, or drivers unavailable).
    NoAdapter(String),
    Device(String),
}

struct Canvas {
    width: u32,
    height: u32,
    storage: wgpu::Buffer,
    readback: wgpu::Buffer,
}

struct LayerBuffers {
    params: wgpu::Buffer,
    source: wgpu::Buffer,
    source_bytes: u64,
}

pub struct GpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    adapter_name: String,
    max_output_bytes: usize,
    canvas: Option<Canvas>,
    layer_buffers: Vec<LayerBuffers>,
    empty_lut: wgpu::Buffer,
    softlight: wgpu::Buffer,
}

impl GpuRenderer {
    /// Creates a renderer on the best available adapter. `max_output_bytes` bounds the canvas.
    pub fn new(max_output_bytes: usize) -> Result<Self, GpuError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|error| GpuError::NoAdapter(error.to_string()))?;
        let adapter_name = adapter.get_info().name;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("bush-gpu"),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .map_err(|error| GpuError::Device(error.to_string()))?;

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bush-composite"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let storage = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bush-composite-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                storage(1, true),
                storage(2, false),
                storage(3, true),
                storage(4, true),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bush-composite-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("bush-composite-pipeline"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let empty_lut = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("bush-empty-lut"),
            contents: &[0u8; 16],
            usage: wgpu::BufferUsages::STORAGE,
        });
        let softlight = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("bush-softlight"),
            contents: crate::blend8::softlight_table(),
            usage: wgpu::BufferUsages::STORAGE,
        });
        Ok(Self {
            device,
            queue,
            pipeline,
            layout,
            adapter_name,
            max_output_bytes,
            canvas: None,
            layer_buffers: Vec::new(),
            empty_lut,
            softlight,
        })
    }

    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    fn ensure_canvas(&mut self, width: u32, height: u32) {
        let reuse = self
            .canvas
            .as_ref()
            .is_some_and(|canvas| canvas.width == width && canvas.height == height);
        if reuse {
            return;
        }
        let bytes = u64::from(width) * u64::from(height) * 4;
        self.canvas = Some(Canvas {
            width,
            height,
            storage: self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("bush-canvas"),
                size: bytes,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            readback: self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("bush-readback"),
                size: bytes,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        });
    }

    /// Grows the per-layer buffer pool and makes sure slot `index` holds `source_bytes`.
    fn ensure_layer_buffers(&mut self, index: usize, source_bytes: u64) {
        while self.layer_buffers.len() <= index {
            self.layer_buffers.push(LayerBuffers {
                params: self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("bush-layer-params"),
                    size: PARAMS_BYTES as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                source: self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("bush-layer-source"),
                    size: 4,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                source_bytes: 4,
            });
        }
        let slot = &mut self.layer_buffers[index];
        if slot.source_bytes != source_bytes {
            slot.source = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("bush-layer-source"),
                size: source_bytes,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            slot.source_bytes = source_bytes;
        }
    }
}

/// Little-endian writer for the 160-byte `Params` uniform.
struct ParamsWriter(Vec<u8>);

impl ParamsWriter {
    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn i32(&mut self, value: i32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn f32(&mut self, value: f32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
}

fn blend_code(mode: BlendMode) -> u32 {
    match mode {
        BlendMode::Normal => 0,
        BlendMode::Multiply => 1,
        BlendMode::Screen => 2,
        BlendMode::Overlay => 3,
        BlendMode::Darken => 4,
        BlendMode::Lighten => 5,
        BlendMode::Add => 6,
        BlendMode::Difference => 7,
        BlendMode::And => 8,
        BlendMode::Average => 9,
        BlendMode::Burn => 10,
        BlendMode::GrainExtract => 11,
        BlendMode::Divide => 12,
        BlendMode::Dodge => 13,
        BlendMode::Exclusion => 14,
        BlendMode::HardLight => 15,
        BlendMode::Negation => 16,
        BlendMode::Or => 17,
        BlendMode::Phoenix => 18,
        BlendMode::PinLight => 19,
        BlendMode::Reflect => 20,
        BlendMode::SoftLight => 21,
        BlendMode::Subtract => 22,
        BlendMode::VividLight => 23,
        BlendMode::Xor => 24,
        BlendMode::HardMix => 25,
        BlendMode::LinearLight => 26,
        BlendMode::Glow => 27,
        BlendMode::GrainMerge => 28,
        BlendMode::Multiply128 => 29,
        BlendMode::Heat => 30,
        BlendMode::Freeze => 31,
        BlendMode::Extremity => 32,
        BlendMode::SoftDifference => 33,
        BlendMode::Geometric => 34,
        BlendMode::Harmonic => 35,
        BlendMode::Bleach => 36,
        BlendMode::Stain => 37,
        BlendMode::Interpolate => 38,
        BlendMode::HardOverlay => 39,
    }
}

fn layer_params(
    canvas_width: u32,
    canvas_height: u32,
    layer: &LayerFrame<'_>,
    geometry: &LayerGeometry,
    source_stride_px: u32,
) -> Vec<u8> {
    let effects = layer.effects;
    let mut flags = 0;
    if effects.chroma_key.is_some() {
        flags |= FLAG_CHROMA;
    }
    if effects.mask.is_some() {
        flags |= FLAG_MASK;
    }
    if layer.lut.is_some() {
        flags |= FLAG_LUT;
    }
    let neutral = crate::LayerEffects::default();
    if effects.brightness != neutral.brightness
        || effects.contrast != neutral.contrast
        || effects.saturation != neutral.saturation
        || effects.vignette != neutral.vignette
        || effects.sepia != neutral.sepia
    {
        flags |= FLAG_COLOUR;
    }
    if layer.transform.flip_x {
        flags |= FLAG_FLIP_X;
    }
    if layer.transform.flip_y {
        flags |= FLAG_FLIP_Y;
    }
    let key = effects.chroma_key;
    let mask = effects.mask;

    let mut writer = ParamsWriter(Vec::with_capacity(PARAMS_BYTES));
    writer.u32(canvas_width);
    writer.u32(canvas_height);
    writer.u32(geometry.start_x as u32);
    writer.u32(geometry.start_y as u32);
    writer.u32((geometry.end_x - geometry.start_x) as u32);
    writer.u32((geometry.end_y - geometry.start_y) as u32);
    writer.u32(source_stride_px);
    writer.u32(layer.frame.width());
    writer.u32(layer.frame.height());
    writer.i32(geometry.crop_x0 as i32);
    writer.i32(geometry.crop_y0 as i32);
    writer.i32(geometry.crop_w as i32);
    writer.i32(geometry.crop_h as i32);
    writer.u32(blend_code(layer.blend));
    writer.u32(effects.pixelize_block);
    writer.u32(flags);
    writer.f32(geometry.center_x);
    writer.f32(geometry.center_y);
    writer.f32(layer.transform.scale_x);
    writer.f32(layer.transform.scale_y);
    writer.f32(geometry.sin);
    writer.f32(geometry.cos);
    writer.f32(layer.opacity);
    writer.f32(effects.brightness);
    writer.f32(effects.contrast);
    writer.f32(effects.saturation);
    writer.f32(effects.vignette);
    writer.f32(key.map_or(0.0, |key| key.tolerance));
    writer.f32(key.map_or(0.0, |key| key.softness));
    writer.f32(mask.map_or(0.0, |mask| mask.corner_radius));
    writer.f32(mask.map_or(0.0, |mask| mask.feather));
    writer.f32(layer.lut.map_or(0.0, |lut| lut.size() as f32));
    let color = key.map_or([0, 0, 0], |key| key.color);
    writer.f32(f32::from(color[0]));
    writer.f32(f32::from(color[1]));
    writer.f32(f32::from(color[2]));
    writer.f32(0.0);
    writer.u32(mask.map_or(0, |mask| u32::from(mask.shape == MaskShape::Ellipse)));
    writer.u32(mask.map_or(0, |mask| u32::from(mask.invert)));
    writer.f32(effects.sepia);
    writer.u32(0);
    debug_assert_eq!(writer.0.len(), PARAMS_BYTES);
    writer.0
}

impl Renderer for GpuRenderer {
    fn render(
        &mut self,
        width: u32,
        height: u32,
        timestamp_secs: f64,
        layers: &[LayerFrame<'_>],
    ) -> Result<VideoFrame, RenderError> {
        // Validation is shared with the reference so both renderers reject the same input.
        crate::cpu_renderer::validate_render_input(
            width,
            height,
            timestamp_secs,
            layers,
            self.max_output_bytes,
        )?;
        let canvas_bytes = u64::from(width) * u64::from(height) * 4;

        // Prepare per-layer uploads.
        let mut plans = Vec::new();
        for (index, layer) in layers.iter().enumerate() {
            let Some(geometry) = LayerGeometry::new(width, height, layer) else {
                continue;
            };
            let frame = layer.frame;
            let row_bytes = frame.width() as usize * 4;
            let tight: std::borrow::Cow<'_, [u8]> = if frame.stride_bytes() == row_bytes {
                std::borrow::Cow::Borrowed(&frame.pixels()[..row_bytes * frame.height() as usize])
            } else {
                let mut packed = Vec::with_capacity(row_bytes * frame.height() as usize);
                for row in 0..frame.height() as usize {
                    let start = row * frame.stride_bytes();
                    packed.extend_from_slice(&frame.pixels()[start..start + row_bytes]);
                }
                std::borrow::Cow::Owned(packed)
            };
            self.ensure_layer_buffers(index, tight.len() as u64);
            let params = layer_params(width, height, layer, &geometry, frame.width());
            let buffers = &self.layer_buffers[index];
            self.queue.write_buffer(&buffers.source, 0, &tight);
            self.queue.write_buffer(&buffers.params, 0, &params);
            plans.push((index, geometry, layer.lut));
        }

        // LUT tables are uploaded per layer that needs one (padded to vec4 for storage layout).
        let luts: Vec<Option<wgpu::Buffer>> = plans
            .iter()
            .map(|(_, _, lut)| {
                lut.map(|lut| {
                    let mut data = Vec::with_capacity(lut.size().pow(3) * 16);
                    for entry in lut.entries() {
                        for value in entry {
                            data.extend_from_slice(&value.to_le_bytes());
                        }
                        data.extend_from_slice(&0.0_f32.to_le_bytes());
                    }
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("bush-lut"),
                            contents: &data,
                            usage: wgpu::BufferUsages::STORAGE,
                        })
                })
            })
            .collect();

        self.ensure_canvas(width, height);
        let canvas = self.canvas.as_ref().expect("canvas exists");
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("bush-composite"),
            });
        encoder.clear_buffer(&canvas.storage, 0, None);
        for ((index, geometry, _), lut) in plans.iter().zip(&luts) {
            let buffers = &self.layer_buffers[*index];
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bush-composite-bind-group"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffers.params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: buffers.source.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: canvas.storage.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: lut.as_ref().unwrap_or(&self.empty_lut).as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: self.softlight.as_entire_binding(),
                    },
                ],
            });
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("bush-layer"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let box_w = (geometry.end_x - geometry.start_x) as u32;
            let box_h = (geometry.end_y - geometry.start_y) as u32;
            pass.dispatch_workgroups(box_w.div_ceil(8), box_h.div_ceil(8), 1);
        }
        encoder.copy_buffer_to_buffer(&canvas.storage, 0, &canvas.readback, 0, canvas_bytes);
        self.queue.submit([encoder.finish()]);

        let slice = canvas.readback.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| RenderError::Gpu(format!("poll failed: {error}")))?;
        receiver
            .recv()
            .map_err(|_| RenderError::Gpu("readback callback dropped".into()))?
            .map_err(|error| RenderError::Gpu(format!("readback map failed: {error}")))?;
        let view = slice
            .get_mapped_range()
            .map_err(|error| RenderError::Gpu(format!("mapped range unavailable: {error}")))?;
        let pixels = view.to_vec();
        drop(view);
        canvas.readback.unmap();

        VideoFrame::new(
            PixelFormat::Rgba8,
            width,
            height,
            width as usize * 4,
            timestamp_secs,
            pixels,
        )
        .map_err(Into::into)
    }

    fn name(&self) -> String {
        format!("gpu:{}", self.adapter_name)
    }
}
