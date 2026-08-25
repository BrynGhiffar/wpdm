
struct U {
	resolution: vec2<f32>,
	origin: vec2<f32>, // circle/wipe origin
	angle: f32, // wipe angle
	progress: f32, // 0..1
	effect: u32,  // 0 none | 1 circle | 2 wipe
	has_from: u32 // 
}

@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var to_tex: texture_2d<f32>;
@group(0) @binding(2) var from_tex: texture_2d<f32>;
@group(0) @binding(3) var tex_sampler: sampler;

struct FragmentInput {
	@builtin(position) frag_coord: vec4<f32>
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> FragmentInput {
	var frag: FragmentInput;

	var positions = array<vec2<f32>, 4>(
        vec2<f32>(-1.0,  1.0), // 0
        vec2<f32>(-1.0, -1.0), // 1
        vec2<f32>( 1.0, -1.0), // 2
        vec2<f32>( 1.0,  1.0)  // 3
    );

	frag.frag_coord = vec4<f32>(positions[vertex_index], 0.0, 1.0);
	return frag;
}

@fragment
fn fs_main(in: FragmentInput) -> @location(0) vec4<f32> {
	return vec4<f32>(1.0, 0.0, 0.0, 1.0);
}
