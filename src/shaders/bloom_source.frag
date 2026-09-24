uniform float pixelScale;
uniform float threshold;

out vec4 color;

void main() {
    vec2 worldPos = vec2(gl_FragCoord.x, worldSize.y * pixelScale - gl_FragCoord.y) / pixelScale;
    vec3 c = scene(worldPos);
    color = vec4(max(c - vec3(threshold), vec3(0.0)), 1.0);
}
