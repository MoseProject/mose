// Compute-shader port of the CPU reference compositor (cpu_renderer.rs, layer_mods.rs).
// One dispatch per layer; every invocation owns one destination pixel inside the transformed
// bounding box of the layer, reads the canvas, and writes the blended result back. Canvas and
// source frames are packed RGBA8 words (little-endian: r | g << 8 | b << 16 | a << 24), so the
// byte-exact fast paths of the CPU renderer carry over unchanged.

struct Params {
    canvas_w: u32,
    canvas_h: u32,
    box_x: u32,
    box_y: u32,
    box_w: u32,
    box_h: u32,
    src_stride_px: u32,
    src_w: u32,
    src_h: u32,
    crop_x0: i32,
    crop_y0: i32,
    crop_w: i32,
    crop_h: i32,
    blend: u32,
    pixelize: u32,
    flags: u32,
    center_x: f32,
    center_y: f32,
    scale_x: f32,
    scale_y: f32,
    sin_r: f32,
    cos_r: f32,
    opacity: f32,
    brightness: f32,
    contrast: f32,
    saturation: f32,
    vignette: f32,
    chroma_tolerance: f32,
    chroma_softness: f32,
    mask_radius: f32,
    mask_feather: f32,
    lut_size: f32,
    chroma_color: vec4<f32>,
    mask_shape: u32,
    mask_invert: u32,
    pad0: u32,
    pad1: u32,
}

const FLAG_CHROMA: u32 = 1u;
const FLAG_MASK: u32 = 2u;
const FLAG_LUT: u32 = 4u;
const FLAG_COLOUR: u32 = 8u;

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> src: array<u32>;
@group(0) @binding(2) var<storage, read_write> canvas: array<u32>;
@group(0) @binding(3) var<storage, read> lut: array<vec4<f32>>;

// Rust f32::round for non-negative values (half away from zero); WGSL round() is half-to-even.
fn rnd(x: f32) -> f32 {
    return floor(x + 0.5);
}

fn blend_channel(mode: u32, backdrop: f32, source: f32) -> f32 {
    switch mode {
        case 1u: { return backdrop * source; }
        case 2u: { return backdrop + source - backdrop * source; }
        case 3u: {
            if (backdrop <= 0.5) {
                return 2.0 * backdrop * source;
            }
            return 1.0 - 2.0 * (1.0 - backdrop) * (1.0 - source);
        }
        case 4u: { return min(backdrop, source); }
        case 5u: { return max(backdrop, source); }
        case 6u: { return min(backdrop + source, 1.0); }
        case 7u: { return abs(backdrop - source); }
        default: { return source; }
    }
}

fn lut_at(r: u32, g: u32, b: u32) -> vec3<f32> {
    let size = u32(p.lut_size);
    return lut[(b * size + g) * size + r].xyz;
}

fn lut_apply(rgb: vec3<f32>) -> vec3<f32> {
    let size = u32(p.lut_size);
    let max_index = p.lut_size - 1.0;
    let position = clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)) * max_index;
    let floor_pos = floor(position);
    let base = min(vec3<u32>(floor_pos), vec3<u32>(size - 2u));
    let fraction = position - vec3<f32>(base);
    let c000 = lut_at(base.x, base.y, base.z);
    let c100 = lut_at(base.x + 1u, base.y, base.z);
    let c010 = lut_at(base.x, base.y + 1u, base.z);
    let c110 = lut_at(base.x + 1u, base.y + 1u, base.z);
    let c001 = lut_at(base.x, base.y, base.z + 1u);
    let c101 = lut_at(base.x + 1u, base.y, base.z + 1u);
    let c011 = lut_at(base.x, base.y + 1u, base.z + 1u);
    let c111 = lut_at(base.x + 1u, base.y + 1u, base.z + 1u);
    let c00 = mix(c000, c100, fraction.x);
    let c10 = mix(c010, c110, fraction.x);
    let c01 = mix(c001, c101, fraction.x);
    let c11 = mix(c011, c111, fraction.x);
    let c0 = mix(c00, c10, fraction.y);
    let c1 = mix(c01, c11, fraction.y);
    return clamp(mix(c0, c1, fraction.z), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn mask_factor(nx: f32, ny: f32) -> f32 {
    var distance: f32;
    if (p.mask_shape == 1u) {
        distance = sqrt(nx * nx + ny * ny) - 1.0;
    } else {
        let radius = p.mask_radius;
        let qx = abs(nx) - (1.0 - radius);
        let qy = abs(ny) - (1.0 - radius);
        let ox = max(qx, 0.0);
        let oy = max(qy, 0.0);
        distance = sqrt(ox * ox + oy * oy) + min(max(qx, qy), 0.0) - radius;
    }
    var inside: f32;
    if (p.mask_feather > 0.0) {
        inside = clamp(0.5 - distance / p.mask_feather, 0.0, 1.0);
    } else if (distance <= 0.0) {
        inside = 1.0;
    } else {
        inside = 0.0;
    }
    if (p.mask_invert == 1u) {
        return 1.0 - inside;
    }
    return inside;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.box_w || gid.y >= p.box_h) {
        return;
    }
    let dest_x = p.box_x + gid.x;
    let dest_y = p.box_y + gid.y;
    let dx = f32(dest_x) + 0.5 - p.center_x;
    let dy = f32(dest_y) + 0.5 - p.center_y;
    let rx = dx * p.cos_r + dy * p.sin_r;
    let ry = -dx * p.sin_r + dy * p.cos_r;
    let u = rx / p.scale_x + f32(p.crop_w) / 2.0;
    let v = ry / p.scale_y + f32(p.crop_h) / 2.0;
    if (u < 0.0 || v < 0.0 || u >= f32(p.crop_w) || v >= f32(p.crop_h)) {
        return;
    }

    var local_x = i32(u);
    var local_y = i32(v);
    if (p.pixelize > 1u) {
        let block = i32(p.pixelize);
        local_x = min((local_x / block) * block + block / 2, p.crop_w - 1);
        local_y = min((local_y / block) * block + block / 2, p.crop_h - 1);
    }
    let source_x = u32(p.crop_x0 + local_x);
    let source_y = u32(p.crop_y0 + local_y);
    let word = src[source_y * p.src_stride_px + source_x];
    var rgb_bytes = vec3<f32>(f32(word & 255u), f32((word >> 8u) & 255u), f32((word >> 16u) & 255u));
    var alpha_byte = f32((word >> 24u) & 255u);

    let nx = (u / f32(p.crop_w) - 0.5) * 2.0;
    let ny = (v / f32(p.crop_h) - 0.5) * 2.0;

    if ((p.flags & FLAG_CHROMA) != 0u) {
        let delta = (rgb_bytes - p.chroma_color.xyz) / 255.0;
        let distance = sqrt(dot(delta, delta)) / sqrt(3.0);
        var factor = 1.0;
        if (distance <= p.chroma_tolerance) {
            factor = 0.0;
        } else if (p.chroma_softness > 0.0 && distance < p.chroma_tolerance + p.chroma_softness) {
            factor = (distance - p.chroma_tolerance) / p.chroma_softness;
        }
        alpha_byte = rnd(alpha_byte * factor);
    }
    if ((p.flags & (FLAG_LUT | FLAG_COLOUR)) != 0u) {
        var rgb = rgb_bytes / 255.0;
        if ((p.flags & FLAG_LUT) != 0u) {
            rgb = lut_apply(rgb);
        }
        if ((p.flags & FLAG_COLOUR) != 0u) {
            let radius = sqrt((nx * nx + ny * ny) / 2.0);
            let falloff = 1.0 - p.vignette * radius * radius;
            rgb = clamp((rgb - 0.5) * p.contrast + 0.5 + p.brightness, vec3<f32>(0.0), vec3<f32>(1.0));
            let luma = 0.2126 * rgb.x + 0.7152 * rgb.y + 0.0722 * rgb.z;
            rgb = clamp((vec3<f32>(luma) + (rgb - luma) * p.saturation) * falloff, vec3<f32>(0.0), vec3<f32>(1.0));
        }
        rgb_bytes = vec3<f32>(rnd(rgb.x * 255.0), rnd(rgb.y * 255.0), rnd(rgb.z * 255.0));
    }
    if ((p.flags & FLAG_MASK) != 0u) {
        alpha_byte = rnd(alpha_byte * mask_factor(nx, ny));
    }

    let index = dest_y * p.canvas_w + dest_x;
    let backdrop_word = canvas[index];
    let backdrop_bytes = vec4<f32>(
        f32(backdrop_word & 255u),
        f32((backdrop_word >> 8u) & 255u),
        f32((backdrop_word >> 16u) & 255u),
        f32((backdrop_word >> 24u) & 255u),
    );

    var out_rgb = backdrop_bytes.xyz;
    var out_alpha = backdrop_bytes.w;
    let source_alpha = (alpha_byte / 255.0) * p.opacity;
    if (p.blend == 0u && alpha_byte == 255.0 && p.opacity >= 1.0) {
        out_rgb = rgb_bytes;
        out_alpha = 255.0;
    } else if (source_alpha == 0.0) {
        // Fully transparent source: the backdrop is unchanged.
    } else if (backdrop_bytes.w == 0.0) {
        out_rgb = rgb_bytes;
        out_alpha = rnd(source_alpha * 255.0);
    } else {
        let backdrop_alpha = backdrop_bytes.w / 255.0;
        let output_alpha = source_alpha + backdrop_alpha * (1.0 - source_alpha);
        let source_colour = rgb_bytes / 255.0;
        let backdrop_colour = backdrop_bytes.xyz / 255.0;
        var blended: vec3<f32>;
        blended.x = blend_channel(p.blend, backdrop_colour.x, source_colour.x);
        blended.y = blend_channel(p.blend, backdrop_colour.y, source_colour.y);
        blended.z = blend_channel(p.blend, backdrop_colour.z, source_colour.z);
        let premultiplied = source_alpha * (1.0 - backdrop_alpha) * source_colour
            + source_alpha * backdrop_alpha * blended
            + (1.0 - source_alpha) * backdrop_alpha * backdrop_colour;
        let colour = premultiplied / output_alpha * 255.0;
        out_rgb = vec3<f32>(rnd(colour.x), rnd(colour.y), rnd(colour.z));
        out_alpha = rnd(output_alpha * 255.0);
    }

    canvas[index] = u32(out_rgb.x) | (u32(out_rgb.y) << 8u) | (u32(out_rgb.z) << 16u) | (u32(out_alpha) << 24u);
}
