// Adapted from Rezmason/matrix (MIT, see assets/LICENSE-matrix): rainPass.raindrop,
// rainPass.intro, rainPass.symbol, rainPass and palettePass, merged into one stateless
// function of (world position, time) so that every output shows the same scene.

#define PI 3.14159265359
#define SQRT_2 1.4142135623730951
#define SQRT_5 2.23606797749979

#define KIND_POCKET 1
#define KIND_TEXT 2
#define KIND_SCRAMBLE 3
#define KIND_FADE 4

uniform vec2 gridSize;
uniform vec2 worldSize;
uniform float cellSize;
uniform float time;
uniform float fallSpeed, raindropLength, cycleSpeed, glyphCount;
uniform float baseBrightness, baseContrast;
uniform bool skipIntro;

uniform sampler2D rainAtlas;
uniform sampler2D textAtlas;
uniform vec2 textAtlasGrid;
uniform sampler2D textGrid;

uniform sampler2D paletteLut;
uniform vec3 cursorColor, backgroundColor;
uniform float cursorIntensity;
uniform vec3 textColors[16];

float randomFloat(vec2 uv) {
    float dt = dot(uv, vec2(12.9898, 78.233));
    return fract(sin(mod(dt, PI)) * 43758.5453);
}

float wobble(float x) {
    return x + 0.3 * sin(SQRT_2 * x) + 0.2 * sin(SQRT_5 * x);
}

float rainBrightness(vec2 glyphPos) {
    float columnTimeOffset = randomFloat(vec2(glyphPos.x, 0.0)) * 1000.0;
    float columnSpeedOffset = randomFloat(vec2(glyphPos.x + 0.1, 0.0)) * 0.5 + 0.5;
    float columnTime = columnTimeOffset + time * fallSpeed * columnSpeedOffset;
    return 1.0 - fract(wobble((glyphPos.y * 0.01 + columnTime) / raindropLength));
}

float introProgress(vec2 glyphPos) {
    if (skipIntro) {
        return 2.0;
    }
    float offset;
    int column = int(glyphPos.x);
    if (column == int(gridSize.x / 2.0)) {
        offset = -1.0;
    } else if (column == int(gridSize.x * 0.75)) {
        offset = -2.0;
    } else {
        offset = randomFloat(vec2(glyphPos.x, 0.0)) * -4.0;
        offset += (sin(glyphPos.x / gridSize.x * PI) - 1.0) * 2.0 - 2.5;
    }
    float introTime = (time + offset) * fallSpeed / gridSize.y * 100.0;
    return introTime - (1.0 - glyphPos.y / gridSize.y);
}

float symbolAt(vec2 cell) {
    float epoch = floor(time * cycleSpeed + randomFloat(cell + 0.5));
    return floor(glyphCount * randomFloat(cell + vec2(epoch * 0.618, epoch * 0.382)));
}

float median3(vec3 v) {
    return max(min(v.r, v.g), min(max(v.r, v.g), v.b));
}

float rainGlyph(float index, vec2 inCell) {
    vec2 slot = vec2(mod(index, 8.0), floor(index / 8.0));
    vec2 uv = (slot + inCell) / 8.0;
    vec2 unitRange = vec2(4.0) / vec2(textureSize(rainAtlas, 0));
    vec2 screenTexSize = vec2(1.0) / fwidth(uv);
    float screenPxRange = max(0.5 * dot(unitRange, screenTexSize), 1.0);
    float distance = median3(texture(rainAtlas, uv).rgb);
    return clamp(screenPxRange * (distance - 0.5) + 0.5, 0.0, 1.0);
}

float textGlyph(float index, vec2 inCell) {
    vec2 slot = vec2(mod(index, textAtlasGrid.x), floor(index / textAtlasGrid.x));
    return texture(textAtlas, (slot + inCell) / textAtlasGrid).r;
}

// worldPos: logical pixels from the top left corner of the world, y down.
vec3 scene(vec2 worldPos) {
    vec2 cell = floor(worldPos / cellSize);
    vec2 inCell = fract(worldPos / cellSize);
    vec2 glyphPos = vec2(cell.x, gridSize.y - 1.0 - cell.y);

    float brightness = rainBrightness(glyphPos);
    float progress = introProgress(glyphPos);
    bool activated = progress > 0.0;
    bool cursor = brightness > rainBrightness(glyphPos - vec2(0.0, 1.0))
        || (activated && introProgress(glyphPos - vec2(0.0, 1.0)) <= 0.0);
    float base = (brightness + max(0.0, 1.0 - progress * 5.0)) * baseContrast + baseBrightness;
    base *= float(activated);

    vec4 overlay = texelFetch(textGrid, ivec2(cell), 0);
    int kind = int(overlay.g * 255.0 + 0.5);
    float intensity = overlay.a;
    float rain = 0.0, glow = 0.0, text = 0.0;
    if (kind == 0) {
        float symbol = rainGlyph(symbolAt(cell), inCell);
        rain = cursor ? 0.0 : base * symbol;
        glow = cursor ? base * symbol : 0.0;
    } else if (kind == KIND_TEXT) {
        text = intensity * textGlyph(overlay.r * 255.0, inCell);
    } else if (kind == KIND_SCRAMBLE) {
        glow = intensity * rainGlyph(overlay.r * 255.0, inCell);
    } else if (kind == KIND_FADE) {
        rain = intensity * rainGlyph(overlay.r * 255.0, inCell);
    }

    vec3 textColor = textColors[int(overlay.b * 255.0 + 0.5)];
    return texture(paletteLut, vec2(clamp(rain, 0.0, 1.0), 0.5)).rgb
        + min(cursorColor * cursorIntensity * glow, vec3(1.0))
        + textColor * text
        + backgroundColor;
}
