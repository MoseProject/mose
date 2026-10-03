//! GPU blur and sharpen (feature `gpu`): the compute-shader twin of `blur.rs`.
//!
//! One chain per affected layer is recorded into the renderer command encoder: premultiply, three
//! horizontal and three vertical box passes, unpremultiply, and for sharpen a second small blur
//! plus the unsharp mask. Scratch buffers are pooled per layer slot, and every pass has its own
//! 256-byte slice of one uniform buffer (dynamic offsets) so all parameters coexist when the
//! encoder is submitted.

use wgpu::util::DeviceExt;

use crate::blur::{box_radii, MAX_BLUR_SIGMA, SHARPEN_SIGMA};

const SHADER: &str = include_str!("gpu_blur.wgsl");
const UNIFORM_STRIDE: u64 = 256;
const PARAMS_BYTES: usize = 32;
/// Unsharp differences at or below this many levels are rounding noise (matches `blur.rs`).
const SHARPEN_NOISE_FLOOR: f32 = 2.0;

#[derive(Clone, Copy)]
enum Slot {
    Source,
    Scratch(usize),
}

#[derive(Clone, Copy)]
enum Kind {
    Premultiply,
    Unpremultiply,
    Box,
    Sharpen,
}

struct Pass {
    kind: Kind,
    input: Slot,
    output: Slot,
    original: Slot,
    radius: u32,
    horizontal: bool,
    amount: f32,
}

struct Scratch {
    bytes: u64,
    buffers: [wgpu::Buffer; 4],
}

pub(crate) struct GpuBlur {
    layout: wgpu::BindGroupLayout,
    premultiply: wgpu::ComputePipeline,
    unpremultiply: wgpu::ComputePipeline,
    box_pass: wgpu::ComputePipeline,
    sharpen: wgpu::ComputePipeline,
    scratch: Vec<Option<Scratch>>,
}

/// Where the processed frame ended up.
#[derive(Clone, Copy)]
pub(crate) enum BlurResult {
    Scratch(usize),
}

impl GpuBlur {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bush-blur"),
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
            label: Some("bush-blur-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(PARAMS_BYTES as u64),
                    },
                    count: None,
                },
                storage(1, true),
                storage(2, false),
                storage(3, true),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bush-blur-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &'static str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Self {
            premultiply: pipeline("premultiply"),
            unpremultiply: pipeline("unpremultiply"),
            box_pass: pipeline("box_pass"),
            sharpen: pipeline("sharpen"),
            layout,
            scratch: Vec::new(),
        }
    }

    pub(crate) fn buffer(&self, layer_slot: usize, result: BlurResult) -> &wgpu::Buffer {
        let BlurResult::Scratch(index) = result;
        &self.scratch[layer_slot]
            .as_ref()
            .expect("blur scratch exists for a processed layer")
            .buffers[index]
    }

    fn ensure_scratch(&mut self, device: &wgpu::Device, layer_slot: usize, bytes: u64) {
        while self.scratch.len() <= layer_slot {
            self.scratch.push(None);
        }
        if self.scratch[layer_slot]
            .as_ref()
            .is_some_and(|s| s.bytes == bytes)
        {
            return;
        }
        let make = |label: &'static str| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: bytes,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            })
        };
        self.scratch[layer_slot] = Some(Scratch {
            bytes,
            buffers: [
                make("blur-a"),
                make("blur-b"),
                make("blur-c"),
                make("blur-d"),
            ],
        });
    }

    /// Records the blur/sharpen chain for one layer into `encoder`, reading `source` (tight RGBA
    /// words of `width` x `height`). Returns the scratch buffer holding the processed frame.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn encode(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        layer_slot: usize,
        source: &wgpu::Buffer,
        width: u32,
        height: u32,
        blur_sigma: f32,
        sharpen_amount: f32,
    ) -> BlurResult {
        self.ensure_scratch(device, layer_slot, u64::from(width) * u64::from(height) * 4);
        let mut passes = Vec::new();
        let mut current = Slot::Source;
        if blur_sigma > 0.0 {
            blur_chain(&mut passes, current, Slot::Scratch(2), blur_sigma);
            current = Slot::Scratch(2);
        }
        if sharpen_amount > 0.0 {
            blur_chain(&mut passes, current, Slot::Scratch(3), SHARPEN_SIGMA);
            passes.push(Pass {
                kind: Kind::Sharpen,
                input: Slot::Scratch(3),
                output: Slot::Scratch(0),
                original: current,
                radius: 0,
                horizontal: false,
                amount: sharpen_amount,
            });
            current = Slot::Scratch(0);
        }

        let mut params = vec![0u8; passes.len() * UNIFORM_STRIDE as usize];
        for (index, pass) in passes.iter().enumerate() {
            let window = u64::from(2 * pass.radius + 1);
            let reciprocal = ((1_u64 << 24).div_ceil(window)) as u32;
            let at = index * UNIFORM_STRIDE as usize;
            let mut write = |offset: usize, bytes: [u8; 4]| {
                params[at + offset..at + offset + 4].copy_from_slice(&bytes)
            };
            write(0, width.to_le_bytes());
            write(4, height.to_le_bytes());
            write(8, pass.radius.to_le_bytes());
            write(12, reciprocal.to_le_bytes());
            write(16, u32::from(pass.horizontal).to_le_bytes());
            write(20, pass.amount.to_le_bytes());
            write(24, SHARPEN_NOISE_FLOOR.to_le_bytes());
        }
        let uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("bush-blur-params"),
            contents: &params,
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let scratch = self.scratch[layer_slot].as_ref().expect("scratch ensured");
        let resolve = |slot: Slot| match slot {
            Slot::Source => source,
            Slot::Scratch(index) => &scratch.buffers[index],
        };
        for (index, pass) in passes.iter().enumerate() {
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bush-blur-bind-group"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &uniforms,
                            offset: 0,
                            size: wgpu::BufferSize::new(PARAMS_BYTES as u64),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: resolve(pass.input).as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: resolve(pass.output).as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: resolve(pass.original).as_entire_binding(),
                    },
                ],
            });
            let mut compute = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("bush-blur-pass"),
                timestamp_writes: None,
            });
            let (pipeline, items, per_group) = match pass.kind {
                Kind::Premultiply => (&self.premultiply, width * height, 256),
                Kind::Unpremultiply => (&self.unpremultiply, width * height, 256),
                Kind::Sharpen => (&self.sharpen, width * height, 256),
                Kind::Box => (
                    &self.box_pass,
                    if pass.horizontal { height } else { width },
                    64,
                ),
            };
            compute.set_pipeline(pipeline);
            compute.set_bind_group(0, &bind_group, &[(index as u64 * UNIFORM_STRIDE) as u32]);
            let groups = items.div_ceil(per_group);
            // Spill into the second dimension past the 65535 workgroup limit per axis.
            let x = groups.min(32_768);
            compute.dispatch_workgroups(x, groups.div_ceil(x), 1);
        }
        match current {
            Slot::Scratch(index) => BlurResult::Scratch(index),
            Slot::Source => unreachable!("encode is only called for layers with an effect"),
        }
    }
}

/// premultiply -> 3 horizontal -> 3 vertical box passes -> unpremultiply, `input` to `output`,
/// using scratch buffers 0 and 1 in between.
fn blur_chain(passes: &mut Vec<Pass>, input: Slot, output: Slot, sigma: f32) {
    let radii = box_radii(sigma.min(MAX_BLUR_SIGMA));
    let (a, b) = (Slot::Scratch(0), Slot::Scratch(1));
    passes.push(Pass {
        kind: Kind::Premultiply,
        input,
        output: a,
        original: input,
        radius: 0,
        horizontal: false,
        amount: 0.0,
    });
    let mut from = a;
    let mut to = b;
    for horizontal in [true, false] {
        for radius in radii {
            if radius == 0 {
                continue;
            }
            passes.push(Pass {
                kind: Kind::Box,
                input: from,
                output: to,
                original: from,
                radius: radius as u32,
                horizontal,
                amount: 0.0,
            });
            std::mem::swap(&mut from, &mut to);
        }
    }
    passes.push(Pass {
        kind: Kind::Unpremultiply,
        input: from,
        output,
        original: from,
        radius: 0,
        horizontal: false,
        amount: 0.0,
    });
}
