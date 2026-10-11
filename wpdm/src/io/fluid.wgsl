
// Port of the sim core of Paveldogreat's WebGL-Fluid-Simulation (MIT,
// © Pavel Dobryakov), driving an offset-distortion field (design B)
// instead of dye colors: the composite pass resamples the wallpaper
// through the offset field, so the field heals to the pristine wallpaper
// as it dissipates.

struct SimU {
	texel: vec2<f32>,       // field texel size — sim texel for sim passes
	resolution: vec2<f32>,  // monitor logical size
	dt: f32,                // frame delta, clamped ≤ 1/60
	dissipation: f32,
	curl: f32,
	pressure: f32,          // pressure warm-start factor
	splat_pos: vec2<f32>,   // splat center, uv
	splat_duv: vec2<f32>,   // splat delta (velocity force / offset inject)
	splat_aspect: f32,      // resolution.x / resolution.y
	splat_radius: f32,
	pad: vec2<f32>
}

@group(0) @binding(0) var<uniform> u: SimU;
@group(0) @binding(1) var samp: sampler;
// tex0 is the primary source; tex1 the secondary (or the dummy when unused).
@group(0) @binding(2) var tex0: texture_2d<f32>;
@group(0) @binding(3) var tex1: texture_2d<f32>;

struct VertexOutput {
	@builtin(position) pos: vec4<f32>,
	@location(0) uv: vec2<f32>
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
	var positions = array<vec2<f32>, 3>(
		vec2<f32>(-1.0, -1.0),
		vec2<f32>( 3.0, -1.0),
		vec2<f32>(-1.0,  3.0),
	);

	var out: VertexOutput;
	out.pos = vec4<f32>(positions[vertex_index], 0.0, 1.0);
	out.uv = positions[vertex_index] * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
	return out;
}

// ─── pressure warm start ───────────────────────────────────────────────────
@fragment
fn fs_pressure_bias(in: VertexOutput) -> @location(0) vec4<f32> {
	return textureSample(tex0, samp, in.uv) * u.pressure;
}

// ─── curl ──────────────────────────────────────────────────────────────────
@fragment
fn fs_curl(in: VertexOutput) -> @location(0) vec4<f32> {
	let uv = in.uv;
	let dx = vec2<f32>(u.texel.x, 0.0);
	let dy = vec2<f32>(0.0, u.texel.y);
	let l = textureSample(tex0, samp, uv - dx).y;
	let r = textureSample(tex0, samp, uv + dx).y;
	let b = textureSample(tex0, samp, uv - dy).x;
	let t = textureSample(tex0, samp, uv + dy).x;
	return vec4<f32>(0.5 * (r - l - (t - b)), 0.0, 0.0, 1.0);
}

// ─── vorticity confinement ─────────────────────────────────────────────────
@fragment
fn fs_vorticity(in: VertexOutput) -> @location(0) vec4<f32> {
	let uv = in.uv;
	let dx = vec2<f32>(u.texel.x, 0.0);
	let dy = vec2<f32>(0.0, u.texel.y);
	let l = abs(textureSample(tex1, samp, uv - dx).x);
	let r = abs(textureSample(tex1, samp, uv + dx).x);
	let b = abs(textureSample(tex1, samp, uv - dy).x);
	let t = abs(textureSample(tex1, samp, uv + dy).x);
	let c = textureSample(tex1, samp, uv).x;

	var force = 0.5 * vec2<f32>(t - b, r - l);
	force = force / (length(force) + 0.0001);
	force = force * u.curl * c;
	force.y = -force.y;

	var velocity = textureSample(tex0, samp, uv).xy;
	velocity = velocity + force * u.dt;
	velocity = clamp(velocity, vec2<f32>(-1000.0), vec2<f32>(1000.0));
	return vec4<f32>(velocity, 0.0, 1.0);
}

// ─── divergence, with the reference's edge-wall fixups ─────────────────────
@fragment
fn fs_divergence(in: VertexOutput) -> @location(0) vec4<f32> {
	let uv = in.uv;
	let dx = vec2<f32>(u.texel.x, 0.0);
	let dy = vec2<f32>(0.0, u.texel.y);
	var l = textureSample(tex0, samp, uv - dx).x;
	var r = textureSample(tex0, samp, uv + dx).x;
	var b = textureSample(tex0, samp, uv - dy).y;
	var t = textureSample(tex0, samp, uv + dy).y;
	let c = textureSample(tex0, samp, uv).xy;
	if (uv.x - u.texel.x <= 0.0) { l = -c.x; }
	if (uv.x + u.texel.x >= 1.0) { r = -c.x; }
	if (uv.y + u.texel.y >= 1.0) { t = -c.y; }
	if (uv.y - u.texel.y <= 0.0) { b = -c.y; }
	return vec4<f32>(0.5 * (r - l + t - b), 0.0, 0.0, 1.0);
}

// ─── pressure Jacobi iteration ─────────────────────────────────────────────
@fragment
fn fs_jacobi(in: VertexOutput) -> @location(0) vec4<f32> {
	let uv = in.uv;
	let dx = vec2<f32>(u.texel.x, 0.0);
	let dy = vec2<f32>(0.0, u.texel.y);
	let l = textureSample(tex0, samp, uv - dx).x;
	let r = textureSample(tex0, samp, uv + dx).x;
	let b = textureSample(tex0, samp, uv - dy).x;
	let t = textureSample(tex0, samp, uv + dy).x;
	let divergence = textureSample(tex1, samp, uv).x;
	return vec4<f32>((l + r + b + t - divergence) * 0.25, 0.0, 0.0, 1.0);
}

// ─── make velocity divergence-free ─────────────────────────────────────────
@fragment
fn fs_gradient_subtract(in: VertexOutput) -> @location(0) vec4<f32> {
	let uv = in.uv;
	let dx = vec2<f32>(u.texel.x, 0.0);
	let dy = vec2<f32>(0.0, u.texel.y);
	let l = textureSample(tex1, samp, uv - dx).x;
	let r = textureSample(tex1, samp, uv + dx).x;
	let b = textureSample(tex1, samp, uv - dy).x;
	let t = textureSample(tex1, samp, uv + dy).x;
	var velocity = textureSample(tex0, samp, uv).xy;
	velocity = velocity - vec2<f32>(r - l, t - b);
	return vec4<f32>(velocity, 0.0, 1.0);
}

// ─── semi-Lagrangian advection (texel is the sim texel for both passes) ───
@fragment
fn fs_advect(in: VertexOutput) -> @location(0) vec4<f32> {
	let velocity = textureSample(tex0, samp, in.uv).xy;
	let coord = in.uv - u.dt * velocity * u.texel;
	let field = textureSample(tex1, samp, coord);
	return field / (1.0 + u.dissipation * u.dt);
}

// ─── splat: read-modify-write gaussian injection ───────────────────────────
@fragment
fn fs_splat(in: VertexOutput) -> @location(0) vec4<f32> {
	let prev = textureSample(tex0, samp, in.uv);
	var d = in.uv - u.splat_pos;
	d.x *= u.splat_aspect;
	let splat = exp(-dot(d, d) / u.splat_radius) * u.splat_duv;
	// Both fields (velocity, offset) hold their data in xy; the reference
	// carries a vec3 color, ours is 2-channel by construction.
	return vec4<f32>(prev.xy + splat, prev.z, 1.0);
}

// ─── composite: wallpaper resampled through the offset field ───────────────
@fragment
fn fs_composite(in: VertexOutput) -> @location(0) vec4<f32> {
	let off = textureSample(tex0, samp, in.uv).xy;
	let wallpaper_uv = cover_uv(in.uv, tex1);
	return textureSample(tex1, samp, wallpaper_uv + off);
}

/// Map a screen uv in [0,1] into the center crop window of `tex` such that
/// the texture covers the resolution without distortion.
fn cover_uv(uv: vec2<f32>, tex: texture_2d<f32>) -> vec2<f32> {
	let size = vec2<f32>(textureDimensions(tex));
	if (size.x * u.resolution.y >= u.resolution.x * size.y) {
		let tw = u.resolution.x * size.y / (u.resolution.y * size.x);
		return (uv - vec2<f32>(0.5)) * vec2<f32>(tw, 1.0) + vec2<f32>(0.5);
	} else {
		let th = u.resolution.y * size.x / (u.resolution.x * size.y);
		return (uv - vec2<f32>(0.5)) * vec2<f32>(1.0, th) + vec2<f32>(0.5);
	}
}
