#version 450

layout(location = 0) in vec3 inPos;
layout(location = 1) in vec4 inColor;

layout(set = 0, binding = 0) uniform UBO3D {
    mat4 view;
    mat4 proj;
    vec4 light_dir;
} ubo;

layout(push_constant) uniform PushTrail {
    mat4 model;
} push;

layout(location = 0) out vec4 fragColor;

void main() {
    gl_Position = ubo.proj * ubo.view * push.model * vec4(inPos, 1.0);
    fragColor = inColor;
}
