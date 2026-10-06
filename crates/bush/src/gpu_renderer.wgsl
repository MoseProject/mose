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
    sepia: f32,
    pad1: u32,
}

const FLAG_CHROMA: u32 = 1u;
const FLAG_MASK: u32 = 2u;
const FLAG_LUT: u32 = 4u;
const FLAG_COLOUR: u32 = 8u;
const FLAG_FLIP_X: u32 = 16u;
const FLAG_FLIP_Y: u32 = 32u;

@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> src: array<u32>;
@group(0) @binding(2) var<storage, read_write> canvas: array<u32>;
@group(0) @binding(3) var<storage, read> lut: array<vec4<f32>>;

// Rust f32::round for non-negative values (half away from zero); WGSL round() is half-to-even.
// Comparing the fraction also avoids rounding an already integral value when x + 0.5 loses precision.
fn rnd(x: f32) -> f32 {
    let integral = floor(x);
    return integral + select(0.0, 1.0, x - integral >= 0.5);
}

@group(0) @binding(4) var<storage, read> softlight: array<u32>;

fn clip255(value: i32) -> u32 {
    return u32(clamp(value, 0, 255));
}

fn burn_i(a: i32, b: i32) -> i32 {
    if (a == 0) {
        return a;
    }
    return max(0, 255 - ((255 - b) << 8u) / a);
}

fn dodge_i(a: i32, b: i32) -> i32 {
    if (a == 255) {
        return a;
    }
    return min(255, (b << 8u) / (255 - a));
}

fn mul_i(scale: i32, a: i32, b: i32) -> i32 {
    return scale * (a * b / 255);
}

fn scr_i(scale: i32, a: i32, b: i32) -> i32 {
    return 255 - scale * ((255 - a) * (255 - b) / 255);
}

// FFmpeg 8-bit blend functions (see blend8.rs): a is the blended layer, b the backdrop.
fn blend_int(mode: u32, au: u32, bu: u32) -> u32 {
    let a = i32(au);
    let b = i32(bu);
    switch mode {
        case 1u: { return u32(a * b / 255); }
        case 2u: { return u32(255 - (255 - a) * (255 - b) / 255); }
        case 4u: { return min(au, bu); }
        case 5u: { return max(au, bu); }
        case 6u: { return clip255(a + b); }
        case 7u: { return u32(abs(a - b)); }
        case 3u: {
            if (a < 128) { return clip255(mul_i(2, b, a)); }
            return clip255(scr_i(2, b, a));
        }
        case 8u: { return au & bu; }
        case 9u: { return u32((a + b) / 2); }
        case 10u: { return clip255(burn_i(a, b)); }
        case 11u: { return clip255(a - b + 128); }
        case 12u: {
            if (b == 0) { return 255u; }
            return clip255(255 * a / b);
        }
        case 13u: { return clip255(dodge_i(a, b)); }
        case 14u: { return clip255(a + b - 2 * a * b / 255); }
        case 15u: {
            if (b < 128) { return clip255(mul_i(2, a, b)); }
            return clip255(scr_i(2, a, b));
        }
        case 16u: { return u32(255 - abs(255 - a - b)); }
        case 17u: { return au | bu; }
        case 18u: { return u32(min(a, b) - max(a, b) + 255); }
        case 19u: {
            if (b < 128) { return clip255(min(a, 2 * b)); }
            return clip255(max(a, 2 * (b - 128)));
        }
        case 20u: {
            if (b == 255) { return 255u; }
            return clip255(min(a * a / (255 - b), 255));
        }
        case 21u: { return (softlight[(bu * 256u + au) / 4u] >> (((bu * 256u + au) % 4u) * 8u)) & 255u; }
        case 22u: { return clip255(a - b); }
        case 23u: {
            if (a < 128) { return clip255(burn_i(2 * a, b)); }
            return clip255(dodge_i(2 * (a - 128), b));
        }
        case 24u: { return au ^ bu; }
        case 25u: {
            if (a >= 255 - b) { return 255u; }
            return 0u;
        }
        case 26u: {
            if (b < 128) { return clip255(b + 2 * a - 255); }
            return clip255(b + 2 * (a - 128));
        }
        case 27u: {
            if (a == 255) { return 255u; }
            return clip255(min(b * b / (255 - a), 255));
        }
        case 28u: { return clip255(a + b - 128); }
        case 29u: {
            let value = (a - 128) * b;
            // Floor division by 32 (WGSL division truncates toward zero).
            let remainder = ((value % 32) + 32) % 32;
            return clip255((value - remainder) / 32 + 128);
        }
        case 30u: {
            if (a == 0) { return 0u; }
            return clip255(255 - min((255 - b) * (255 - b) / a, 255));
        }
        case 31u: {
            if (b == 0) { return 0u; }
            return clip255(255 - min((255 - a) * (255 - a) / b, 255));
        }
        case 32u: { return u32(abs(255 - a - b)); }
        case 33u: {
            if (a > b) {
                if (b == 255) { return 0u; }
                return clip255(min((a - b) * 255 / (255 - b), 255));
            }
            if (b == 0) { return 0u; }
            return clip255(min((b - a) * 255 / b, 255));
        }
        case 34u: { return u32(floor(sqrt(f32(a * b)) + 0.5)); }
        case 35u: {
            if (a == 0 && b == 0) { return 0u; }
            return u32(2 * a * b / (a + b));
        }
        case 36u: { return u32((255 - a - b) & 255); }
        case 37u: { return u32((254 - a - b) & 255); }
        case 38u: {
            let pi = 3.14159265358979;
            let value = 255.0 * (0.5 - 0.25 * cos(pi * f32(a) / 255.0) - 0.25 * cos(pi * f32(b) / 255.0));
            return clip255(i32(floor(value + 0.5)));
        }
        case 39u: {
            if (a < 128) { return clip255(2 * a * b / 255); }
            if (a == 255) { return 255u; }
            return clip255(min(b * 255 / (2 * (255 - a)), 255));
        }
        default: { return au; }
    }
}

fn blend_any(mode: u32, backdrop: f32, source: f32, backdrop_byte: f32, source_byte: f32) -> f32 {
    if (mode == 0u) {
        return source;
    }
    return f32(blend_int(mode, u32(source_byte), u32(backdrop_byte))) / 255.0;
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
    if ((p.flags & FLAG_FLIP_X) != 0u) {
        local_x = p.crop_w - 1 - local_x;
    }
    if ((p.flags & FLAG_FLIP_Y) != 0u) {
        local_y = p.crop_h - 1 - local_y;
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
            rgb = clamp(vec3<f32>(luma) + (rgb - luma) * p.saturation, vec3<f32>(0.0), vec3<f32>(1.0));
            if (p.sepia > 0.0) {
                let toned = vec3<f32>(
                    0.393 * rgb.x + 0.769 * rgb.y + 0.189 * rgb.z,
                    0.349 * rgb.x + 0.686 * rgb.y + 0.168 * rgb.z,
                    0.272 * rgb.x + 0.534 * rgb.y + 0.131 * rgb.z,
                );
                rgb = clamp(rgb + (min(toned, vec3<f32>(1.0)) - rgb) * p.sepia, vec3<f32>(0.0), vec3<f32>(1.0));
            }
            rgb = clamp(rgb * falloff, vec3<f32>(0.0), vec3<f32>(1.0));
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
    } else if (backdrop_bytes.w == 255.0) {
        // Share the CPU's 24-bit fixed-point coverage for an opaque backdrop. Integer
        // interpolation prevents backend FMA/division rounding from changing half-way bytes.
        let scale = 16777216u;
        let opacity = u32(rnd(p.opacity * f32(scale)));
        let alpha = (u32(alpha_byte) * opacity + 127u) / 255u;
        let keep = scale - alpha;
        let source_bytes = vec3<u32>(rgb_bytes);
        let backdrop = vec3<u32>(backdrop_bytes.xyz);
        let blended = vec3<u32>(
            blend_int(p.blend, source_bytes.x, backdrop.x),
            blend_int(p.blend, source_bytes.y, backdrop.y),
            blend_int(p.blend, source_bytes.z, backdrop.z),
        );
        out_rgb = vec3<f32>((alpha * blended + keep * backdrop + vec3<u32>(scale / 2u)) / scale);
        out_alpha = 255.0;
    } else {
        let backdrop_alpha = backdrop_bytes.w / 255.0;
        let output_alpha = source_alpha + backdrop_alpha * (1.0 - source_alpha);
        let source_colour = rgb_bytes / 255.0;
        let backdrop_colour = backdrop_bytes.xyz / 255.0;
        var blended: vec3<f32>;
        blended.x = blend_any(p.blend, backdrop_colour.x, source_colour.x, backdrop_bytes.x, rgb_bytes.x);
        blended.y = blend_any(p.blend, backdrop_colour.y, source_colour.y, backdrop_bytes.y, rgb_bytes.y);
        blended.z = blend_any(p.blend, backdrop_colour.z, source_colour.z, backdrop_bytes.z, rgb_bytes.z);
        let premultiplied = source_alpha * (1.0 - backdrop_alpha) * source_colour
            + source_alpha * backdrop_alpha * blended
            + (1.0 - source_alpha) * backdrop_alpha * backdrop_colour;
        let colour = premultiplied / output_alpha * 255.0;
        out_rgb = vec3<f32>(rnd(colour.x), rnd(colour.y), rnd(colour.z));
        out_alpha = rnd(output_alpha * 255.0);
    }

    canvas[index] = u32(out_rgb.x) | (u32(out_rgb.y) << 8u) | (u32(out_rgb.z) << 16u) | (u32(out_alpha) << 24u);
}
