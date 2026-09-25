mod atlas;
mod cli;
mod config;
mod gl;
mod headline;
mod model;
mod news;
mod notices;
mod palette;
mod rain;
mod render;
mod rng;
mod stage;
mod wayland;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use smithay_client_toolkit::reexports::{
    calloop::EventLoop,
    calloop_wayland_source::WaylandSource,
    client::{Connection, globals::registry_queue_init},
};
use smithay_client_toolkit::shell::WaylandSurface;

use config::Config;
use model::{CellRect, Frame, Palette, Rect};
use render::Renderer;
use stage::Stage;
use wayland::Desktop;

const POLL: Duration = Duration::from_secs(2);
const PALETTE_FADE: f32 = 1.2;

struct Watched {
    path: PathBuf,
    modified: Option<SystemTime>,
}

impl Watched {
    fn new(path: PathBuf) -> Self {
        Self { path, modified: None }
    }

    fn changed(&mut self) -> bool {
        let modified = std::fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        let changed = modified != self.modified;
        self.modified = modified;
        changed
    }
}

enum Change {
    Layout,
    Settings,
}

// Only these rebuild the grid; everything else applies to the running scene.
fn layout_key(config: &Config) -> ((f32, f32), String) {
    (config.rain.cell(), config.text.font.clone())
}

struct Clocks {
    rain: f64,
    symbol: f64,
    last: Instant,
}

impl Clocks {
    fn advance(&mut self, rain: &config::Rain) {
        let now = Instant::now();
        let seconds = (now - self.last).as_secs_f64() * rain.animation_speed as f64;
        self.last = now;
        self.rain += seconds * rain.fall_speed as f64;
        self.symbol += seconds * rain.cycle_speed as f64;
    }

    // f32 loses precision as the rain clock grows, so past the intro it jumps back by
    // 720 units, an hour at the default speed.
    fn rain_for_shader(&self) -> f32 {
        (if self.rain < 1440.0 { self.rain } else { 720.0 + (self.rain - 720.0) % 720.0 }) as f32
    }

    // Glyph epochs only need whole numbers; the jump comes once in about ten days.
    fn symbol_for_shader(&self) -> f32 {
        (self.symbol % 1.0e6) as f32
    }
}

struct Fade {
    from: Palette,
    started: Instant,
}

struct Settings {
    config: Config,
    palette: Palette,
    config_file: Watched,
    theme_file: Watched,
}

impl Settings {
    fn load() -> Self {
        let dir = config::dir();
        let mut settings = Self {
            config: Config::default(),
            palette: palette::build(&Config::default().palette, &HashMap::new()),
            config_file: Watched::new(dir.join("config.toml")),
            theme_file: Watched::new(dir.join("theme.toml")),
        };
        settings.reload();
        settings
    }

    fn reload(&mut self) -> Option<Change> {
        let config_changed = self.config_file.changed();
        let theme_changed = self.theme_file.changed();
        if !config_changed && !theme_changed {
            return None;
        }
        let before = layout_key(&self.config);
        if config_changed
            && let Some(config) = read_config(&self.config_file.path) {
                self.config = config;
            }
        let theme = std::fs::read_to_string(&self.theme_file.path).unwrap_or_default();
        self.palette = palette::build(&self.config.palette, &palette::parse_theme(&theme));
        Some(if layout_key(&self.config) == before { Change::Settings } else { Change::Layout })
    }

    fn fps(&self) -> f32 {
        let fps = if on_battery() { self.config.display.fps_on_battery } else { self.config.display.fps };
        fps.max(1.0)
    }
}

fn read_config(path: &Path) -> Option<Config> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => return Some(Config::default()),
    };
    config::parse(&text).map_err(|reason| eprintln!("{}", notices::bad_config(path, &reason))).ok()
}

fn on_battery() -> bool {
    let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") else { return false };
    let mains: Vec<String> = entries
        .flatten()
        .filter(|e| std::fs::read_to_string(e.path().join("type")).is_ok_and(|t| t.trim() == "Mains"))
        .filter_map(|e| std::fs::read_to_string(e.path().join("online")).ok())
        .collect();
    !mains.is_empty() && mains.iter().all(|online| online.trim() == "0")
}

fn grid(world: Rect, cell: (f32, f32)) -> (i32, i32) {
    ((world.width as f32 / cell.0).ceil() as i32, (world.height as f32 / cell.1).ceil() as i32)
}

// Only whole cells inside one output, so a headline never crosses a monitor edge.
fn cell_rect(rect: Rect, world: Rect, cell: (f32, f32)) -> CellRect {
    let left = ((rect.x - world.x) as f32 / cell.0).ceil() as i32;
    let top = ((rect.y - world.y) as f32 / cell.1).ceil() as i32;
    let right = ((rect.x + rect.width - world.x) as f32 / cell.0).floor() as i32;
    let bottom = ((rect.y + rect.height - world.y) as f32 / cell.1).floor() as i32;
    CellRect { col: left, row: top, cols: right - left, rows: bottom - top }
}

fn fail(message: String) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli::run(&args) {
        Some(Ok(output)) => {
            if !output.is_empty() {
                println!("{output}");
            }
            return;
        }
        Some(Err(message)) => fail(message),
        None => {}
    }
    let mut settings = Settings::load();
    let conn = Connection::connect_to_env().unwrap_or_else(|_| fail(notices::no_wayland()));
    let (globals, queue) = registry_queue_init::<Desktop>(&conn).unwrap_or_else(|_| fail(notices::no_wayland()));
    let qh = queue.handle();
    let mut desktop = Desktop::new(&globals, &qh, settings.config.display.outputs.clone())
        .unwrap_or_else(|name| fail(notices::missing_protocol(name)));
    let mut event_loop: EventLoop<Desktop> = EventLoop::try_new().expect("event loop");
    WaylandSource::new(conn.clone(), queue).insert(event_loop.handle()).expect("wayland source");
    let egl = gl::Egl::new(&conn).unwrap_or_else(|e| fail(notices::no_gpu(&e.to_string())));
    let mut renderer = Renderer::new(egl.glow()).unwrap_or_else(|e| fail(notices::no_gpu(&e)));
    renderer.set_palette(&settings.palette);
    let mut shown = settings.palette.clone();
    let mut fade: Option<Fade> = None;
    let mut clocks = Clocks { rain: 0.0, symbol: 0.0, last: Instant::now() };

    let start = Instant::now();
    let mut next_frame = Instant::now();
    let mut next_poll = Instant::now();
    let mut world = desktop.world();
    let mut cells = grid(world, settings.config.rain.cell());
    let mut stage = Stage::new(&settings.config);

    loop {
        let timeout = next_frame.saturating_duration_since(Instant::now());
        event_loop.dispatch(Some(timeout), &mut desktop).expect("dispatch");
        for panel in desktop.removed.drain(..) {
            if let Some(target) = panel.target {
                egl.destroy(target);
            }
        }
        let seconds = start.elapsed().as_secs_f32();
        clocks.advance(&settings.config.rain);
        let rain_clock = clocks.rain_for_shader();
        let now = Instant::now();
        if now >= next_poll {
            next_poll = now + POLL;
            match settings.reload() {
                Some(Change::Layout) => desktop.layout_changed = true,
                Some(Change::Settings) => {
                    stage.set_density(settings.config.rain.density, rain_clock);
                    stage.follow_feed(&settings.config);
                    renderer.set_columns(&rain::texels(&stage.columns));
                }
                None => {}
            }
            if fade.is_none() && (settings.palette.lut != shown.lut || settings.palette.text != shown.text) {
                fade = Some(Fade { from: shown.clone(), started: now });
            }
            stage.poll(&settings.config);
        }
        if let Some(active) = &fade {
            let k = active.started.elapsed().as_secs_f32() / PALETTE_FADE;
            shown = palette::blend(&active.from, &settings.palette, k);
            renderer.set_palette(&shown);
            if k >= 1.0 {
                fade = None;
            }
        }
        if desktop.layout_changed {
            desktop.layout_changed = false;
            world = desktop.world();
            cells = grid(world, settings.config.rain.cell());
            stage.reset(&settings.config, cells.0, cells.1, seconds);
            stage.poll(&settings.config);
            renderer.set_columns(&rain::texels(&stage.columns));
            let scale = desktop.panels.iter().map(|p| p.scale).max().unwrap_or(1);
            let (width, height) = settings.config.rain.cell();
            let slot = ((width * scale as f32).round() as u32, (height * scale as f32).round() as u32);
            if let Some(atlas) = stage.text_atlas(&settings.config, slot) {
                renderer.set_text_atlas(&atlas);
            }
        }
        let cell = settings.config.rain.cell();
        for (x, y) in desktop.clicks.drain(..) {
            stage.click(((x - world.x as f64) / cell.0 as f64) as i32, ((y - world.y as f64) / cell.1 as f64) as i32);
        }
        if now < next_frame {
            continue;
        }
        next_frame = (next_frame + Duration::from_secs_f32(1.0 / settings.fps())).max(now);

        let ready: Vec<usize> = (0..desktop.panels.len())
            .filter(|&i| desktop.panels[i].configured && desktop.panels[i].frame_ready)
            .collect();
        if ready.is_empty() {
            continue;
        }
        let regions: Vec<CellRect> = desktop.panels.iter().filter(|p| p.configured).map(|p| cell_rect(p.rect, world, cell)).collect();
        stage.frame(&settings.config, seconds, rain_clock, &regions);
        renderer.set_text_grid(stage.grid.cols, stage.grid.rows, &stage.grid.cells);
        let text_colors = stage::text_colors(&settings.config, shown.text);
        let frame = Frame {
            time: seconds % 3600.0,
            rain_clock,
            symbol_clock: clocks.symbol_for_shader(),
            world,
            grid: cells,
            config: &settings.config,
            palette: &shown,
            text_colors: &text_colors,
        };
        for (n, &index) in ready.iter().enumerate() {
            let panel = &mut desktop.panels[index];
            let (width, height) = panel.pixel_size();
            match &panel.target {
                Some(target) => egl.resize(target, width, height),
                None => match egl.target(panel.layer.wl_surface(), width, height) {
                    Ok(target) => panel.target = Some(target),
                    Err(e) => fail(notices::no_gpu(&e.to_string())),
                },
            }
            let Some(target) = panel.target.as_ref() else { continue };
            egl.make_current(target).unwrap_or_else(|e| fail(notices::no_gpu(&e.to_string())));
            if n == 0 && frame.config.bloom.strength > 0.0 {
                renderer.render_bloom(&frame);
            }
            renderer.render_panel(&frame, panel.rect, panel.scale);
            desktop.request_frame(index, &qh);
            let panel = &desktop.panels[index];
            egl.swap(panel.target.as_ref().unwrap()).unwrap_or_else(|e| fail(notices::no_gpu(&e.to_string())));
        }
    }
}
