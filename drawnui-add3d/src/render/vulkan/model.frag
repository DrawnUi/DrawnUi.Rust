#version 450
// The WebGL2 fragment shader for Vulkan.
layout(location = 0) in vec3 v_nrm;
layout(location = 1) in vec3 v_pos;
layout(location = 2) in vec2 v_uv;

layout(set = 0, binding = 0) uniform sampler2D u_texture;

layout(set = 1, binding = 0) uniform Frame {
    mat4 view_proj;
    vec4 eye;
    vec4 light;
    vec4 outline_color;
    vec2 viewport;
    float outline;
    int look;
    int outline_pass;
} f;

layout(push_constant) uniform Object {
    mat4 model;
    vec4 color;
    float rough;
    int skinned;
    int textured;
} o;

layout(location = 0) out vec4 o_color;

void main() {
    if (f.outline_pass != 0) { o_color = vec4(f.outline_color.rgb * f.outline_color.a, f.outline_color.a); return; }
    vec4 c = o.color;
    // glTF colors are linear; textures are sRGB: light in linear, show in sRGB.
    if (o.textured != 0) { vec4 t = texture(u_texture, v_uv); c *= vec4(pow(t.rgb, vec3(2.2)), t.a); }
    if (c.a < 0.02) discard;
    vec3 n = normalize(v_nrm);
    if (!gl_FrontFacing) n = -n;
    vec3 light = f.light.xyz;
    float sun = max(dot(n, light), 0.0);
    float sky = 0.5 + 0.5 * n.y;
    vec3 lit;
    if (f.look == 2) lit = c.rgb;
    else if (f.look == 1) lit = c.rgb * (sun > 0.35 ? 1.0 : 0.62);
    else {
        float fill = max(dot(n, normalize(vec3(-light.x, 0.3, -light.z))), 0.0);
        lit = c.rgb * (0.24 + 0.20 * sky + 0.68 * sun + 0.22 * fill);
        float gloss = 1.0 - clamp(o.rough, 0.0, 1.0);
        if (gloss > 0.01 && sun > 0.0) {
            vec3 v = f.eye.w > 0.5 ? normalize(f.eye.xyz - v_pos) : normalize(f.eye.xyz);
            float s = pow(max(dot(n, normalize(light + v)), 0.0), mix(6.0, 90.0, gloss * gloss));
            lit += vec3(0.55 * gloss * gloss * s);
        }
    }
    o_color = vec4(pow(lit, vec3(1.0 / 2.2)) * c.a, c.a);
}
