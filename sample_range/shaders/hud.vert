#version 450

layout(location = 0) in vec2 inPos;
layout(location = 1) in vec2 inUV;
layout(location = 2) in vec4 inColor;
layout(location = 3) in float inUseTex;

layout(set = 0, binding = 0) uniform UBOHUD {
    vec2 screen_size;
    vec2 _pad;
} ubo;

layout(location = 0) out vec2 fragUV;
layout(location = 1) out vec4 fragColor;
layout(location = 2) out float fragUseTex;

void main() {
    // pixel coords [0,W]x[0,H] → NDC [-1,1]x[-1,1]
    vec2 ndc = (inPos / ubo.screen_size) * 2.0 - 1.0;
    gl_Position = vec4(ndc, 0.0, 1.0);
    fragUV = inUV;
    fragColor = inColor;
    fragUseTex = inUseTex;
}
