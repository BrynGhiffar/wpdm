
struct U {
	resolution: vec2<f32>,
	origin: vec2<f32>, // circle origin, screen pixel coords (top-left, y-down)
	angle: f32, // wipe angle in degrees
	progress: f32, // 0..1
	effect: u32,  // 0 none | 1 circle | 2 wipe
	has_from: u32 // 0 → render `to` alone
}

@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var to_tex: texture_2d<f32>;
@group(0) @binding(2) var from_tex: texture_2d<f32>;
@group(0) @binding(3) var tex_sampler: sampler;

struct VertexOutput {
	@builtin(position) pos: vec4<f32>,
	@location(0) uv: vec2<f32>
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
	// One oversized triangle covering the whole NDC square.
	var positions = array<vec2<f32>, 3>(
		vec2<f32>(-1.0, -1.0),
		vec2<f32>( 3.0, -1.0),
		vec2<f32>(-1.0,  3.0),
	);

	var out: VertexOutput;
	out.pos = vec4<f32>(positions[vertex_index], 0.0, 1.0);
	// NDC (y-up, [-1,1]) → UV (y-down, [0,1]): the flip baked in, so
	// screen top-left samples texture top-left.
	out.uv = positions[vertex_index] * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
	return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
	let to_color = textureSample(to_tex, tex_sampler, in.uv);

	// Hard cut: no effect, or nothing to blend with.
	if (u.effect == 0u || u.has_from == 0u) {
		return to_color;
	}

	let from_color = textureSample(from_tex, tex_sampler, in.uv);

	// Framebuffer pixel coordinates: top-left origin, y-down — the same
	// convention the transition origins use.
	let pixel = in.pos.xy;
	let t = clamp(u.progress, 0.0, 1.0);

	var mask = 0.0;
	if (u.effect == 1u) {
		// Circle: reveal `to` inside a circle growing from the origin
		// to the farthest corner.
		let max_radius = max(
			max(
				distance(vec2<f32>(0.0, 0.0), u.origin),
				distance(vec2<f32>(u.resolution.x, 0.0), u.origin)
			),
			max(
				distance(vec2<f32>(0.0, u.resolution.y), u.origin),
				distance(u.resolution, u.origin)
			)
		);
		let radius = t * max_radius;
		mask = select(0.0, 1.0, distance(pixel, u.origin) <= radius);
	} else {
		// Wipe: reveal `to` on the low-projection side of the direction
		// (cos, sin), sweeping from the top-left corner.
		let radians = u.angle * 3.14159265 / 180.0;
		let ca = cos(radians);
		let sa = sin(radians);
		let max_projected = u.resolution.x * ca + u.resolution.y * sa;
		let projected = (pixel.x * ca + pixel.y * sa) / max_projected;
		mask = select(0.0, 1.0, projected <= t);
	}

	// mask 1 → `to` (revealed), 0 → `from`.
	return mix(from_color, to_color, mask);
}
