mod gl;
mod wayland;

use std::time::{Duration, Instant};

use glow::HasContext;
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::reexports::{
    calloop::EventLoop,
    calloop_wayland_source::WaylandSource,
    client::{Connection, globals::registry_queue_init},
};

use wayland::Desktop;

fn main() {
    let conn = Connection::connect_to_env().expect("wayland");
    let (globals, queue) = registry_queue_init::<Desktop>(&conn).expect("registry");
    let qh = queue.handle();
    let mut desktop = Desktop::new(&globals, &qh, Vec::new()).expect("globals");
    let mut event_loop: EventLoop<Desktop> = EventLoop::try_new().expect("event loop");
    WaylandSource::new(conn.clone(), queue).insert(event_loop.handle()).expect("wayland source");
    let egl = gl::Egl::new(&conn).expect("egl");
    let glow = egl.glow();
    let start = Instant::now();

    loop {
        event_loop.dispatch(Some(Duration::from_millis(33)), &mut desktop).expect("dispatch");
        for panel in desktop.removed.drain(..) {
            if let Some(target) = panel.target {
                egl.destroy(target);
            }
        }
        let t = start.elapsed().as_secs_f32();
        for index in 0..desktop.panels.len() {
            let panel = &mut desktop.panels[index];
            if !panel.configured || !panel.frame_ready {
                continue;
            }
            let (width, height) = panel.pixel_size();
            match &panel.target {
                Some(target) => egl.resize(target, width, height),
                None => panel.target = Some(egl.target(panel.layer.wl_surface(), width, height).expect("surface")),
            }
            let target = panel.target.as_ref().unwrap();
            egl.make_current(target).expect("current");
            unsafe {
                glow.viewport(0, 0, width, height);
                glow.clear_color(0.0, 0.3 + 0.2 * t.sin(), 0.0, 1.0);
                glow.clear(glow::COLOR_BUFFER_BIT);
            }
            desktop.request_frame(index, &qh);
            egl.swap(desktop.panels[index].target.as_ref().unwrap()).expect("swap");
        }
    }
}
