// Each output draws its crop of one world-sized scene. Method described in
// https://github.com/HelpFreedom/matrix-rain-saver; implementation is our own.

use glow::{HasContext, NativeFramebuffer, NativeProgram, NativeTexture, NativeVertexArray};

use crate::model::{Frame, Image, LUT_SIZE, Palette, Rect, Rgb, TEXT_COLORS, TextAtlas};

const VERTEX: &str = include_str!("shaders/fullscreen.vert");
const SCENE: &str = include_str!("shaders/scene.glsl");
const BLOOM_SOURCE: &str = include_str!("shaders/bloom_source.frag");
const BLUR: &str = include_str!("shaders/blur.frag");
const PANEL: &str = include_str!("shaders/panel.frag");
const LEVELS: usize = 5;
struct Target {
    framebuffer: NativeFramebuffer,
    texture: NativeTexture,
    size: (i32, i32),
}

pub struct Renderer {
    gl: glow::Context,
    vao: NativeVertexArray,
    panel: NativeProgram,
    bloom_source: NativeProgram,
    blur: NativeProgram,
    rain_atlas: NativeTexture,
    text_atlas: NativeTexture,
    text_atlas_grid: (u32, u32),
    text_grid: NativeTexture,
    palette_lut: NativeTexture,
    black: NativeTexture,
    columns: NativeTexture,
    source: Option<Target>,
    across: Vec<Target>,
    levels: Vec<Target>,
}

impl Renderer {
    pub fn new(gl: glow::Context) -> Result<Self, String> {
        unsafe {
            let vao = gl.create_vertex_array()?;
            let scene = |body: &str| format!("#version 330 core\n{SCENE}\n{body}");
            let panel = program(&gl, &scene(PANEL))?;
            let bloom_source = program(&gl, &scene(BLOOM_SOURCE))?;
            let blur = program(&gl, BLUR)?;
            let rain = crate::atlas::rain();
            let rain_atlas = texture(&gl, glow::RGB8, glow::RGB, &rain, glow::LINEAR)?;
            let empty = Image { width: 1, height: 1, pixels: vec![0; 4] };
            let text_atlas = texture(&gl, glow::R8, glow::RED, &empty, glow::LINEAR)?;
            let text_grid = texture(&gl, glow::RGBA8, glow::RGBA, &empty, glow::NEAREST)?;
            let black = texture(&gl, glow::RGBA8, glow::RGBA, &empty, glow::NEAREST)?;
            let columns = texture(&gl, glow::RGBA8, glow::RGBA, &empty, glow::NEAREST)?;
            let palette_lut = texture(&gl, glow::RGB8, glow::RGB, &Image { width: 1, height: 1, pixels: vec![0; 3] }, glow::LINEAR)?;
            Ok(Self {
                gl, vao, panel, bloom_source, blur, rain_atlas, text_atlas,
                text_atlas_grid: (1, 1), text_grid, palette_lut, black, columns,
                source: None, across: Vec::new(), levels: Vec::new(),
            })
        }
    }

    pub fn set_palette(&mut self, palette: &Palette) {
        let image = Image { width: LUT_SIZE as u32, height: 1, pixels: palette.lut.clone() };
        unsafe { upload(&self.gl, self.palette_lut, glow::RGB8, glow::RGB, &image) };
    }

    pub fn set_text_atlas(&mut self, atlas: &TextAtlas) {
        self.text_atlas_grid = (atlas.columns, atlas.rows);
        unsafe { upload(&self.gl, self.text_atlas, glow::R8, glow::RED, &atlas.image) };
    }

    pub fn set_text_grid(&mut self, columns: i32, rows: i32, cells: &[u8]) {
        let image = Image { width: columns as u32, height: rows as u32, pixels: cells.to_vec() };
        unsafe { upload(&self.gl, self.text_grid, glow::RGBA8, glow::RGBA, &image) };
    }

    pub fn set_columns(&mut self, texels: &[f32]) {
        let gl = &self.gl;
        let bytes: Vec<u8> = texels.iter().flat_map(|v| v.to_ne_bytes()).collect();
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.columns));
            gl.tex_image_2d(
                glow::TEXTURE_2D, 0, glow::RGBA32F as i32, (texels.len() / 8) as i32, 2, 0,
                glow::RGBA, glow::FLOAT, glow::PixelUnpackData::Slice(Some(&bytes)),
            );
        }
    }

    pub fn render_bloom(&mut self, frame: &Frame) {
        let bloom = &frame.config.bloom;
        let width = ((frame.world.width as f32 * bloom.size) as i32).max(1);
        let height = ((frame.world.height as f32 * bloom.size) as i32).max(1);
        if self.source.as_ref().map(|s| s.size) != Some((width, height)) {
            self.resize_bloom(width, height);
        }
        let gl = &self.gl;
        unsafe {
            gl.bind_vertex_array(Some(self.vao));
            let source = self.source.as_ref().unwrap();
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(source.framebuffer));
            gl.viewport(0, 0, width, height);
            gl.use_program(Some(self.bloom_source));
            self.scene_uniforms(self.bloom_source, frame);
            set_f(gl, self.bloom_source, "pixelScale", width as f32 / frame.world.width as f32);
            set_f(gl, self.bloom_source, "threshold", bloom.threshold);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);

            gl.use_program(Some(self.blur));
            set_i(gl, self.blur, "source", 0);
            gl.active_texture(glow::TEXTURE0);
            for i in 0..LEVELS {
                let input = if i == 0 { source.texture } else { self.levels[i - 1].texture };
                for (target, input, direction) in [(&self.across[i], input, [1.0, 0.0]), (&self.levels[i], self.across[i].texture, [0.0, 1.0])] {
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.framebuffer));
                    gl.viewport(0, 0, target.size.0, target.size.1);
                    gl.bind_texture(glow::TEXTURE_2D, Some(input));
                    gl.uniform_2_f32(gl.get_uniform_location(self.blur, "direction").as_ref(), direction[0], direction[1]);
                    gl.uniform_2_f32(gl.get_uniform_location(self.blur, "targetSize").as_ref(), target.size.0 as f32, target.size.1 as f32);
                    gl.draw_arrays(glow::TRIANGLES, 0, 3);
                }
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
    }

    pub fn render_panel(&self, frame: &Frame, rect: Rect, scale: i32) {
        let gl = &self.gl;
        let (width, height) = (rect.width * scale, rect.height * scale);
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.viewport(0, 0, width, height);
            gl.bind_vertex_array(Some(self.vao));
            gl.use_program(Some(self.panel));
            self.scene_uniforms(self.panel, frame);
            let origin = ((rect.x - frame.world.x) as f32, (rect.y - frame.world.y) as f32);
            gl.uniform_2_f32(gl.get_uniform_location(self.panel, "origin").as_ref(), origin.0, origin.1);
            set_f(gl, self.panel, "pixelScale", scale as f32);
            set_f(gl, self.panel, "bufferHeight", height as f32);
            set_f(gl, self.panel, "bloomStrength", frame.config.bloom.strength);
            set_f(gl, self.panel, "dither", frame.palette.dither);
            for i in 0..LEVELS {
                let texture = self.levels.get(i).map_or(self.black, |level| level.texture);
                gl.active_texture(glow::TEXTURE4 + i as u32);
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                set_i(gl, self.panel, &format!("bloom{i}"), 4 + i as i32);
            }
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
    }

    unsafe fn scene_uniforms(&self, program: NativeProgram, frame: &Frame) {
        let gl = &self.gl;
        let rain = &frame.config.rain;
        let palette = frame.palette;
        unsafe {
            let vec2 = |name: &str, x: f32, y: f32| gl.uniform_2_f32(gl.get_uniform_location(program, name).as_ref(), x, y);
            let vec3 = |name: &str, c: Rgb| gl.uniform_3_f32(gl.get_uniform_location(program, name).as_ref(), c[0], c[1], c[2]);
            vec2("gridSize", frame.grid.0 as f32, frame.grid.1 as f32);
            vec2("worldSize", frame.world.width as f32, frame.world.height as f32);
            vec2("textAtlasGrid", self.text_atlas_grid.0 as f32, self.text_atlas_grid.1 as f32);
            vec2("cellSize", rain.cell_width.max(1.0), rain.cell_height.max(1.0));
            set_f(gl, program, "glyphAspect", rain.glyph_aspect);
            set_f(gl, program, "time", frame.time);
            set_f(gl, program, "rainClock", frame.rain_clock);
            set_f(gl, program, "symbolClock", frame.symbol_clock);
            set_f(gl, program, "raindropLength", rain.raindrop_length);
            set_f(gl, program, "glyphCount", rain.glyphs.min(64) as f32);
            set_f(gl, program, "baseBrightness", rain.base_brightness);
            set_f(gl, program, "baseContrast", rain.base_contrast);
            set_i(gl, program, "skipIntro", rain.skip_intro as i32);
            vec3("cursorColor", palette.cursor);
            vec3("backgroundColor", palette.background);
            set_f(gl, program, "cursorIntensity", palette.cursor_intensity);
            let mut colors = [0.0f32; TEXT_COLORS * 3];
            for (i, c) in frame.text_colors.iter().take(TEXT_COLORS).enumerate() {
                colors[i * 3..i * 3 + 3].copy_from_slice(c);
            }
            gl.uniform_3_f32_slice(gl.get_uniform_location(program, "textColors").as_ref(), &colors);
            for (unit, name, tex) in [
                (0, "rainAtlas", self.rain_atlas),
                (1, "textAtlas", self.text_atlas),
                (2, "textGrid", self.text_grid),
                (3, "paletteLut", self.palette_lut),
                (9, "columns", self.columns),
            ] {
                gl.active_texture(glow::TEXTURE0 + unit);
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                set_i(gl, program, name, unit as i32);
            }
        }
    }

    fn resize_bloom(&mut self, width: i32, height: i32) {
        let gl = &self.gl;
        for target in self.source.take().into_iter().chain(self.across.drain(..)).chain(self.levels.drain(..)) {
            unsafe {
                gl.delete_framebuffer(target.framebuffer);
                gl.delete_texture(target.texture);
            }
        }
        self.source = Some(unsafe { render_target(gl, width, height) });
        for i in 0..LEVELS {
            let size = ((width >> i).max(1), (height >> i).max(1));
            self.across.push(unsafe { render_target(gl, size.0, size.1) });
            self.levels.push(unsafe { render_target(gl, size.0, size.1) });
        }
    }
}

unsafe fn set_f(gl: &glow::Context, program: NativeProgram, name: &str, value: f32) {
    unsafe { gl.uniform_1_f32(gl.get_uniform_location(program, name).as_ref(), value) }
}

unsafe fn set_i(gl: &glow::Context, program: NativeProgram, name: &str, value: i32) {
    unsafe { gl.uniform_1_i32(gl.get_uniform_location(program, name).as_ref(), value) }
}

unsafe fn program(gl: &glow::Context, fragment: &str) -> Result<NativeProgram, String> {
    unsafe {
        let program = gl.create_program()?;
        for (kind, source) in [(glow::VERTEX_SHADER, VERTEX), (glow::FRAGMENT_SHADER, fragment)] {
            let shader = gl.create_shader(kind)?;
            gl.shader_source(shader, source);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                return Err(gl.get_shader_info_log(shader));
            }
            gl.attach_shader(program, shader);
        }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }
        Ok(program)
    }
}

unsafe fn texture(gl: &glow::Context, internal: u32, format: u32, image: &Image, filter: u32) -> Result<NativeTexture, String> {
    unsafe {
        let texture = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        upload(gl, texture, internal, format, image);
        Ok(texture)
    }
}

unsafe fn upload(gl: &glow::Context, texture: NativeTexture, internal: u32, format: u32, image: &Image) {
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        gl.tex_image_2d(
            glow::TEXTURE_2D, 0, internal as i32, image.width as i32, image.height as i32, 0,
            format, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&image.pixels)),
        );
    }
}

unsafe fn render_target(gl: &glow::Context, width: i32, height: i32) -> Target {
    unsafe {
        let texture = gl.create_texture().expect("texture");
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA16F as i32, width, height, 0, glow::RGBA, glow::HALF_FLOAT, glow::PixelUnpackData::Slice(None));
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
        let framebuffer = gl.create_framebuffer().expect("framebuffer");
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(texture), 0);
        Target { framebuffer, texture, size: (width, height) }
    }
}
