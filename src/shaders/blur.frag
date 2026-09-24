#version 330 core
// Adapted from Rezmason/matrix bloomPass.blur (MIT, see assets/LICENSE-matrix).

uniform sampler2D source;
uniform vec2 direction;
uniform vec2 targetSize;

out vec4 color;

void main() {
    vec2 size = targetSize;
    vec2 uv = gl_FragCoord.xy / targetSize;
    vec2 step = direction / max(size.x, size.y) * (size.x > size.y ? vec2(size.x / size.y, 1.0) : vec2(1.0, size.y / size.x));
    color = texture(source, uv) * 0.442 + (texture(source, uv + step) + texture(source, uv - step)) * 0.279;
}
