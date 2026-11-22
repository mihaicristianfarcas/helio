// The benchmark scenes, drawn entirely here: no textures, no fonts. Every
// value comes from integer arithmetic on the frame number, so a frame looks
// the same on every GPU and every stack, and a reference picture of it can be
// rendered later at any size (`--dump`).
//
// Every scene but `flash` carries a frame counter in its top tenth: sixteen
// cells holding the frame number in binary (white is 1), and below them the
// same bits inverted. A reader trusts a frame only when each cell and the one
// under it disagree, which rejects frames torn between two counters.

struct Params {
    frame: u32,
    scene: u32,
    flash: u32,
    heavy: u32,
    width: f32,
    height: f32,
    clock_ms: u32,
    _pad: u32,
};

@group(0) @binding(0) var<uniform> p: Params;

@vertex
fn v_full(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    // One triangle covering the screen.
    let x = f32((i << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(i & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

// PCG's output permutation: a fast hash with no visible pattern.
fn pcg(v: u32) -> u32 {
    let s = v * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}

fn counter(px: vec2<f32>) -> vec4<f32> {
    let cell = u32(px.x / p.width * 16.0);
    let bit = (p.frame >> (15u - min(cell, 15u))) & 1u;
    let lower = px.y >= p.height / 20.0;
    let on = select(bit, 1u - bit, lower);
    return vec4<f32>(vec3<f32>(f32(on)), 1.0);
}

// Seven-segment digits for the clock: segments a-g as bits 0-6.
fn segments(d: u32) -> u32 {
    switch d {
        case 0u: { return 0x3fu; }
        case 1u: { return 0x06u; }
        case 2u: { return 0x5bu; }
        case 3u: { return 0x4fu; }
        case 4u: { return 0x66u; }
        case 5u: { return 0x6du; }
        case 6u: { return 0x7du; }
        case 7u: { return 0x07u; }
        case 8u: { return 0x7fu; }
        default: { return 0x6fu; }
    }
}

// Whether `q` (0..1 in a digit's box, y down) falls on a lit segment of `d`.
fn digit(d: u32, q: vec2<f32>) -> bool {
    let s = segments(d);
    let t = 0.14;
    let h = abs(q.x - 0.5) < 0.5 - t * 0.5;
    let left = q.x < t;
    let right = q.x > 1.0 - t;
    let top = q.y < 0.5;
    if (h && q.y < t && (s & 1u) != 0u) { return true; }
    if (right && top && (s & 2u) != 0u) { return true; }
    if (right && !top && (s & 4u) != 0u) { return true; }
    if (h && q.y > 1.0 - t && (s & 8u) != 0u) { return true; }
    if (left && !top && (s & 16u) != 0u) { return true; }
    if (left && top && (s & 32u) != 0u) { return true; }
    if (h && abs(q.y - 0.5) < t * 0.5 && (s & 64u) != 0u) { return true; }
    return false;
}

// Five digits, seconds and milliseconds (SS mmm), across the middle.
fn clock(px: vec2<f32>) -> f32 {
    let box_w = p.width / 6.5;
    let box_h = box_w * 1.6;
    let top = (p.height - box_h) * 0.5;
    let y = (px.y - top) / box_h;
    if (y < 0.0 || y > 1.0) { return 0.0; }
    let x = px.x / box_w - 0.5;
    if (x < 0.0) { return 0.0; }
    var slot = u32(x);
    var fx = fract(x);
    // A gap after the seconds.
    if (slot == 2u) { return 0.0; }
    if (slot > 2u) { slot = slot - 1u; }
    if (slot > 4u) { return 0.0; }
    let ms = p.clock_ms % 100000u;
    var div = 10000u;
    for (var i = 0u; i < slot; i = i + 1u) { div = div / 10u; }
    let d = (ms / div) % 10u;
    let q = vec2<f32>((fx - 0.12) / 0.76, y);
    if (q.x < 0.0 || q.x > 1.0) { return 0.0; }
    return select(0.0, 1.0, digit(d, q));
}

// Moving discs on a slow gradient: motion an encoder must follow.
fn shapes(px: vec2<f32>) -> vec3<f32> {
    let uv = px / vec2<f32>(p.width, p.height);
    var c = vec3<f32>(uv.x * 0.6, uv.y * 0.6, 0.3 + 0.3 * uv.x * uv.y);
    let t = f32(p.frame);
    for (var i = 0u; i < 8u; i = i + 1u) {
        let h = pcg(i + 17u);
        let speed = vec2<f32>(f32(h & 255u) / 255.0 + 0.3, f32((h >> 8u) & 255u) / 255.0 + 0.3) * 6.0;
        let r = p.height * (0.05 + f32((h >> 16u) & 63u) / 63.0 * 0.08);
        let span = vec2<f32>(p.width, p.height) - 2.0 * r;
        // A triangle wave keeps each disc bouncing inside the frame.
        let travel = abs(fract((t * speed + f32(h & 4095u)) / (2.0 * span)) * 2.0 - 1.0);
        let centre = vec2<f32>(r, r) + travel * span;
        if (distance(px, centre) < r) {
            let k = pcg(h);
            c = vec3<f32>(f32(k & 255u), f32((k >> 8u) & 255u), f32((k >> 16u) & 255u)) / 255.0;
        }
    }
    return c;
}

fn noise(px: vec2<f32>) -> vec3<f32> {
    let x = u32(px.x);
    let y = u32(px.y);
    let h = pcg(x ^ pcg(y ^ pcg(p.frame)));
    return vec3<f32>(f32(h & 255u), f32((h >> 8u) & 255u), f32((h >> 16u) & 255u)) / 255.0;
}

@fragment
fn f_scene(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let px = pos.xy;
    // Work a game would do, which the stream then competes with. The result
    // feeds the colour, so no compiler can drop the loop.
    var acc = u32(px.x) ^ (u32(px.y) << 16u);
    for (var i = 0u; i < p.heavy; i = i + 1u) { acc = pcg(acc); }
    let busy = f32(acc >> 31u) * (1.0 / 4096.0);

    if (p.scene == 0u) {
        return vec4<f32>(vec3<f32>(f32(p.flash)) + busy, 1.0);
    }
    if (px.y < p.height / 10.0) {
        return counter(px);
    }
    var c: vec3<f32>;
    switch p.scene {
        case 1u: {
            // Counter alone over a gray that steps each frame, so every
            // frame differs from the one before it.
            c = vec3<f32>(f32(64u + (p.frame % 8u) * 16u) / 255.0);
        }
        case 2u: { c = shapes(px); }
        case 3u: { c = noise(px); }
        default: { c = vec3<f32>(clock(px)); }
    }
    return vec4<f32>(c + busy, 1.0);
}
