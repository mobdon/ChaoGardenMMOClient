struct Uniforms {
    view_proj: mat4x4<f32>,
    light: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>,
      @location(2) uv: vec2<f32>, @location(3) color: vec4<f32>) -> VsOut {
    var o: VsOut;
    o.clip = u.view_proj * vec4<f32>(pos, 1.0);
    o.normal = normal;
    o.color = color;
    return o;
}

@fragment
fn fs(i: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(i.normal);
    let l = normalize(u.light.xyz);
    let lit = 0.35 + 0.65 * abs(dot(n, l));
    return vec4<f32>(i.color.rgb * lit, 1.0);
}
