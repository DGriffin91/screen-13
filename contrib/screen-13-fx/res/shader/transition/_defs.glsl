#include "../inc/catmull_rom.glsl"

layout(local_size_x = 1, local_size_y = 1, local_size_z = 1) in;

layout(set = 0, binding = 0) uniform sampler2D a_image_llw;
layout(set = 0, binding = 1) uniform sampler2D b_image_llw;
layout(set = 0, binding = 2, rgba8) restrict writeonly uniform image2D dest_image;

vec4 getFromColor(vec2 uv) {
    return sample_catmull_rom(a_image_llw, uv);
}

vec4 getToColor(vec2 uv) {
    return sample_catmull_rom(b_image_llw, uv);
}
