mod atlas;
mod config;
mod gl;
mod notices;
mod palette;
mod render;
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
use palette::Palette;
use render::{Frame, Renderer};
use wayland::{Desktop, Rect};

const POLL: Duration = Duration::from_secs(2);

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

    fn reload(&mut self) -> bool {
        let config_changed = self.config_file.changed();
        let theme_changed = self.theme_file.changed();
        if config_changed {
            if let Some(config) = read_config(&self.config_file.path) {
                self.config = config;
            }
        }
        if config_changed || theme_changed {
            let theme = std::fs::read_to_string(&self.theme_file.path).unwrap_or_default();
            self.palette = palette::build(&self.config.palette, &palette::parse_theme(&theme));
        }
        config_changed || theme_changed
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

fn grid(world: Rect, cell: f32) -> (i32, i32) {
    ((world.width as f32 / cell).ceil() as i32, (world.height as f32 / cell).ceil() as i32)
}

// ponytail: float time loses precision after hours, so it jumps back once an hour past the intro.
fn shader_time(seconds: f32) -> f32 {
    if seconds < 7200.0 { seconds } else { 3600.0 + (seconds - 3600.0) % 3600.0 }
}

fn fail(message: String) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}

fn main() {
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

    let start = Instant::now();
    let mut next_frame = Instant::now();
    let mut next_poll = Instant::now() + POLL;
    let mut world = desktop.world();
    let mut cells = grid(world, settings.config.rain.cell_size);
    let mut empty_grid = vec![0u8; (cells.0 * cells.1 * 4) as usize];
    renderer.set_text_grid(cells.0, cells.1, &empty_grid);

    loop {
        let timeout = next_frame.saturating_duration_since(Instant::now());
        event_loop.dispatch(Some(timeout), &mut desktop).expect("dispatch");
        for panel in desktop.removed.drain(..) {
            if let Some(target) = panel.target {
                egl.destroy(target);
            }
        }
        let now = Instant::now();
        if now >= next_poll {
            next_poll = now + POLL;
            if settings.reload() {
                renderer.set_palette(&settings.palette);
                desktop.layout_changed = true;
            }
        }
        if desktop.layout_changed {
            desktop.layout_changed = false;
            world = desktop.world();
            cells = grid(world, settings.config.rain.cell_size);
            empty_grid = vec![0u8; (cells.0 * cells.1 * 4) as usize];
            renderer.set_text_grid(cells.0, cells.1, &empty_grid);
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
        let seconds = start.elapsed().as_secs_f32() * settings.config.rain.animation_speed;
        let frame = Frame {
            time: shader_time(seconds),
            world,
            grid: cells,
            config: &settings.config,
            palette: &settings.palette,
            text_colors: &[settings.palette.text],
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
