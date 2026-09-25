// Adapted from Rezmason/matrix (MIT, see assets/LICENSE-matrix): rainPass.raindrop,
// rainPass.intro, rainPass.symbol, rainPass and palettePass, merged into one stateless
// function of (world position, time) so that every output shows the same scene.

#define PI 3.14159265359
#define SQRT_2 1.4142135623730951
#define SQRT_5 2.23606797749979

#define KIND_TEXT 1
#define KIND_CATCH 2

uniform vec2 gridSize;
uniform vec2 worldSize;
uniform vec2 cellSize;
uniform float glyphAspect;
uniform float time;
// Clocks advance by fall_speed and cycle_speed per second, so a new speed never jumps the scene.
uniform float rainClock, symbolClock;
uniform float raindropLength, glyphCount;
uniform float baseBrightness, baseContrast;
uniform bool skipIntro;

uniform sampler2D rainAtlas;
uniform sampler2D textAtlas;
uniform vec2 textAtlasGrid;
uniform sampler2D textGrid;
// Row 0 per column: time offset, speed, intro offset, raining. Row 1: rain clock of the last
// on/off switch. The CPU uses the same values to time headlines.
uniform sampler2D columns;

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

vec4 column(float x) {
    return texelFetch(columns, ivec2(x, 0), 0);
}

float rainBrightness(vec2 glyphPos) {
    vec4 c = column(glyphPos.x);
    float columnTime = c.r + rainClock * c.g;
    return 1.0 - fract(wobble((glyphPos.y * 0.01 + columnTime) / raindropLength));
}

// A column that switches on or off does it behind a front falling from the top at the
// drop speed: new rain starts at the top, old drops finish their fall.
bool raining(vec2 glyphPos) {
    vec4 c = column(glyphPos.x);
    float switched = texelFetch(columns, ivec2(glyphPos.x, 1), 0).r;
    float front = (rainClock - switched) * 100.0 * c.g;
    bool belowFront = gridSize.y - glyphPos.y < front;
    return c.a > 0.5 ? belowFront : !belowFront;
}

float introProgress(vec2 glyphPos) {
    if (skipIntro) {
        return 2.0;
    }
    float offset = column(glyphPos.x).b;
    float introTime = (rainClock + offset) / gridSize.y * 100.0;
    return introTime - (1.0 - glyphPos.y / gridSize.y);
}

float symbolAt(vec2 cell) {
    float epoch = floor(symbolClock + randomFloat(cell + 0.5));
    return floor(glyphCount * randomFloat(cell + vec2(epoch * 0.618, epoch * 0.382)));
}

float median3(vec3 v) {
    return max(min(v.r, v.g), min(max(v.r, v.g), v.b));
}

// Matrix glyphs keep their shape: full cell width, height from the aspect, centred vertically.
float rainGlyph(float index, vec2 inCell) {
    float boxHeight = min(1.0, cellSize.x * glyphAspect / cellSize.y);
    inCell.y = (inCell.y - 0.5) / boxHeight + 0.5;
    if (inCell.y < 0.0 || inCell.y > 1.0) {
        return 0.0;
    }
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
    base *= float(activated) * float(raining(glyphPos));

    vec4 overlay = texelFetch(textGrid, ivec2(cell), 0);
    int kind = int(overlay.g * 255.0 + 0.5);
    float intensity = overlay.a;
    float rain = 0.0, glow = 0.0, text = 0.0;
    if (kind == KIND_TEXT) {
        text = intensity * textGlyph(overlay.r * 255.0, inCell);
    } else if (kind == KIND_CATCH) {
        glow = intensity * textGlyph(overlay.r * 255.0, inCell);
    } else {
        float symbol = rainGlyph(symbolAt(cell), inCell);
        rain = cursor ? 0.0 : base * symbol;
        glow = cursor ? base * symbol : 0.0;
    }

    vec3 textColor = textColors[int(overlay.b * 255.0 + 0.5)];
    return texture(paletteLut, vec2(clamp(rain, 0.0, 1.0), 0.5)).rgb
        + min(cursorColor * cursorIntensity * glow, vec3(1.0))
        + textColor * text
        + backgroundColor;
}
