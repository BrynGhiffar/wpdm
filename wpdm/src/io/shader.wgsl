
struct U {
	resolution: vec2<f32>,
	origin: vec2<f32>, // circle/wipe origin
	angle: f32, // wipe angle
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
	// Hard cut until transitions land (checkpoint 3).
	return textureSample(to_tex, tex_sampler, in.uv);
}
