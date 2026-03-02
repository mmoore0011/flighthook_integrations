#version 450

layout(location = 0) in vec3 inPos;
layout(location = 1) in vec3 inNormal;

layout(set = 0, binding = 0) uniform UBO3D {
    mat4 view;
    mat4 proj;
    vec4 light_dir;
} ubo;

layout(push_constant) uniform Push3D {
    mat4 model;
    vec4 color;
    vec4 emission;
} push;

layout(location = 0) out vec3 fragNormal;
layout(location = 1) out vec4 fragColor;
layout(location = 2) out vec4 fragEmission;

void main() {
    vec4 worldPos = push.model * vec4(inPos, 1.0);
    gl_Position = ubo.proj * ubo.view * worldPos;
    fragNormal = normalize(mat3(transpose(inverse(push.model))) * inNormal);
    fragColor = push.color;
    fragEmission = push.emission;
}
