#version 450

layout(location = 0) in vec3 fragNormal;
layout(location = 1) in vec4 fragColor;
layout(location = 2) in vec4 fragEmission;

layout(set = 0, binding = 0) uniform UBO3D {
    mat4 view;
    mat4 proj;
    vec4 light_dir;
} ubo;

layout(location = 0) out vec4 outColor;

void main() {
    vec3 N = normalize(fragNormal);
    vec3 L = normalize(-ubo.light_dir.xyz);
    float diff = max(dot(N, L), 0.0) * 1.2;
    vec3 ambient = vec3(0.55, 0.75, 0.95) * 0.8;
    vec3 lit = fragColor.rgb * (ambient + vec3(diff));
    vec3 emission = fragEmission.rgb * fragEmission.a;
    outColor = vec4(lit + emission, fragColor.a);
}
