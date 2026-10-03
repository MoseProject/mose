// GPU port of blur.rs: premultiply, three horizontal then three vertical box passes, unpremultiply
// and unsharp mask, all in the same 8-bit integer arithmetic as the CPU path so results are
// bit-identical (except the float unsharp blend, which may differ by one level).

struct Params {
    width: u32,
    height: u32,
    radius: u32,
    reciprocal: u32,
    horizontal: u32,
    amount: f32,
    noise_floor: f32,
    pad: u32,
}

@group(0) @binding(0) var<uniform> q: Params;
@group(0) @binding(1) var<storage, read> input_pixels: array<u32>;
@group(0) @binding(2) var<storage, read_write> output_pixels: array<u32>;
@group(0) @binding(3) var<storage, read> original_pixels: array<u32>;

fn unpack(word: u32) -> vec4<u32> {
    return vec4<u32>(word & 255u, (word >> 8u) & 255u, (word >> 16u) & 255u, (word >> 24u) & 255u);
}

fn pack(value: vec4<u32>) -> u32 {
    return value.x | (value.y << 8u) | (value.z << 16u) | (value.w << 24u);
}

// Flattened pixel index for 1D dispatches that may need more than 65535 workgroups.
fn pixel_index(gid: vec3<u32>, groups: vec3<u32>) -> u32 {
    return gid.x + gid.y * groups.x * 256u;
}

@compute @workgroup_size(256)
fn premultiply(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let index = pixel_index(gid, groups);
    if (index >= q.width * q.height) {
        return;
    }
    var pixel = unpack(input_pixels[index]);
    if (pixel.w != 255u) {
        pixel = vec4<u32>((pixel.x * pixel.w + 127u) / 255u, (pixel.y * pixel.w + 127u) / 255u, (pixel.z * pixel.w + 127u) / 255u, pixel.w);
    }
    output_pixels[index] = pack(pixel);
}

@compute @workgroup_size(256)
fn unpremultiply(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let index = pixel_index(gid, groups);
    if (index >= q.width * q.height) {
        return;
    }
    var pixel = unpack(input_pixels[index]);
    if (pixel.w != 255u && pixel.w != 0u) {
        let half_alpha = pixel.w / 2u;
        pixel = vec4<u32>(
            min((pixel.x * 255u + half_alpha) / pixel.w, 255u),
            min((pixel.y * 255u + half_alpha) / pixel.w, 255u),
            min((pixel.z * 255u + half_alpha) / pixel.w, 255u),
            pixel.w,
        );
    }
    output_pixels[index] = pack(pixel);
}

// round(sum / window) as (sum * reciprocal + 2^23) >> 24, evaluated without 64-bit integers by
// splitting the reciprocal into 12-bit halves (sum < 2^16, reciprocal < 2^24).
fn scaled(sum: u32) -> u32 {
    let high = q.reciprocal >> 12u;
    let low = q.reciprocal & 4095u;
    let a = sum * high;
    let b = sum * low;
    return (a + ((b + 8388608u) >> 12u)) >> 12u;
}

// One box blur of radius q.radius along every row (horizontal) or column, edges clamped.
@compute @workgroup_size(64)
fn box_pass(@builtin(global_invocation_id) gid: vec3<u32>) {
    let line = gid.x;
    var line_count = q.width;
    var length = q.height;
    if (q.horizontal == 1u) {
        line_count = q.height;
        length = q.width;
    }
    if (line >= line_count) {
        return;
    }
    let radius = q.radius;
    var sums = vec4<u32>(0u);
    for (var offset = 0u; offset <= 2u * radius; offset = offset + 1u) {
        var at = 0u;
        if (offset > radius) {
            at = offset - radius;
        }
        at = min(at, length - 1u);
        var index = at * q.width + line;
        if (q.horizontal == 1u) {
            index = line * q.width + at;
        }
        sums = sums + unpack(input_pixels[index]);
    }
    for (var pixel = 0u; pixel < length; pixel = pixel + 1u) {
        var out_index = pixel * q.width + line;
        if (q.horizontal == 1u) {
            out_index = line * q.width + pixel;
        }
        output_pixels[out_index] = pack(vec4<u32>(scaled(sums.x), scaled(sums.y), scaled(sums.z), scaled(sums.w)));
        var leaving = 0u;
        if (pixel > radius) {
            leaving = pixel - radius;
        }
        leaving = min(leaving, length - 1u);
        let entering = min(pixel + radius + 1u, length - 1u);
        var enter_index = entering * q.width + line;
        var leave_index = leaving * q.width + line;
        if (q.horizontal == 1u) {
            enter_index = line * q.width + entering;
            leave_index = line * q.width + leaving;
        }
        sums = sums + unpack(input_pixels[enter_index]) - unpack(input_pixels[leave_index]);
    }
}

// Unsharp mask: original + amount * (original - blurred), skipping rounding-noise differences.
@compute @workgroup_size(256)
fn sharpen(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
    let index = pixel_index(gid, groups);
    if (index >= q.width * q.height) {
        return;
    }
    let original = unpack(original_pixels[index]);
    let soft = unpack(input_pixels[index]);
    var result = original;
    for (var channel = 0u; channel < 3u; channel = channel + 1u) {
        let value = f32(original[channel]);
        let difference = value - f32(soft[channel]);
        if (abs(difference) > q.noise_floor) {
            let sharpened = floor(value + q.amount * difference + 0.5);
            result[channel] = u32(clamp(sharpened, 0.0, 255.0));
        }
    }
    output_pixels[index] = pack(result);
}