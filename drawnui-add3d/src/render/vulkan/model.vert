#version 450
// The WebGL2 vertex shader for Vulkan: the same inputs, the frame in a uniform buffer, the object
// in push constants. GL's depth (-1..1) is moved into Vulkan's (0..1) at the end, so the projections
// stay the same everywhere; clip space is GL's too (the image comes out bottom-up, as on GL).
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec3 a_nrm;
layout(location = 2) in vec2 a_uv;
layout(location = 3) in vec4 a_joints;
layout(location = 4) in vec4 a_weights;
layout(location = 5) in vec3 a_outline_nrm;

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

layout(set = 1, binding = 1) uniform Joints {
    mat4 joints[64];
};

layout(push_constant) uniform Object {
    mat4 model;
    vec4 color;
    float rough;
    int skinned;
    int textured;
} o;

layout(location = 0) out vec3 v_nrm;
layout(location = 1) out vec3 v_pos;
layout(location = 2) out vec2 v_uv;

void main() {
    mat4 skin = mat4(1.0);
    if (o.skinned != 0) {
        skin = a_weights.x * joints[int(a_joints.x)] + a_weights.y * joints[int(a_joints.y)]
             + a_weights.z * joints[int(a_joints.z)] + a_weights.w * joints[int(a_joints.w)];
    }
    mat4 m = o.model * skin;
    v_nrm = mat3(m) * a_nrm;
    v_uv = a_uv;
    vec4 world = m * vec4(a_pos, 1.0);
    v_pos = world.xyz;
    vec4 p = f.view_proj * world;
    if (f.outline > 0.0) {
        // The outline pass: the back faces pushed out along their smoothed normals, a fixed
        // number of pixels on screen, a little behind.
        vec4 n = f.view_proj * vec4(mat3(m) * a_outline_nrm, 0.0);
        vec2 dir = normalize(n.xy + vec2(1e-6));
        p.xy += dir * f.outline * 2.0 / f.viewport * p.w;
        p.z += 0.002 * p.w;
    }
    p.z = 0.5 * (p.z + p.w);
    gl_Position = p;
}
