uniform vec2 origin;
uniform float pixelScale;
uniform float bufferHeight;
uniform float bloomStrength;
uniform float dither;
uniform sampler2D bloom0, bloom1, bloom2, bloom3, bloom4;

out vec4 color;

float noise(vec2 uv) {
    return fract(sin(mod(dot(uv, vec2(12.9898, 78.233)), PI)) * 43758.5453 + time);
}

void main() {
    vec2 worldPos = origin + vec2(gl_FragCoord.x, bufferHeight - gl_FragCoord.y) / pixelScale;
    vec2 bloomUv = vec2(worldPos.x, worldSize.y - worldPos.y) / worldSize;
    vec3 glow = texture(bloom0, bloomUv).rgb * 0.96549
        + texture(bloom1, bloomUv).rgb * 0.92832
        + texture(bloom2, bloomUv).rgb * 0.88790
        + texture(bloom3, bloomUv).rgb * 0.84343
        + texture(bloom4, bloomUv).rgb * 0.79370;
    vec3 c = scene(worldPos) + glow * bloomStrength;
    color = vec4(c - noise(gl_FragCoord.xy) * dither / 3.0, 1.0);
}
