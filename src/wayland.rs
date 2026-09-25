use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData},
    delegate_registry,
    output::{OutputHandler, OutputState},
    reexports::client::{
        Connection, QueueHandle,
        globals::GlobalList,
        protocol::{wl_output::{Transform, WlOutput}, wl_pointer::WlPointer, wl_seat::WlSeat, wl_surface::WlSurface},
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        pointer::{CursorIcon, PointerEvent, PointerEventKind, PointerHandler, ThemeSpec, ThemedPointer},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
    },
    shm::{Shm, ShmHandler},
};

use crate::model::{Rect, Target};

const BUTTON_LEFT: u32 = 0x110;

pub struct Panel {
    pub output: WlOutput,
    pub layer: LayerSurface,
    pub rect: Rect,
    pub scale: i32,
    pub configured: bool,
    pub frame_ready: bool,
    pub target: Option<Target>,
}

impl Panel {
    pub fn pixel_size(&self) -> (i32, i32) {
        (self.rect.width * self.scale, self.rect.height * self.scale)
    }
}

pub struct Desktop {
    registry: RegistryState,
    outputs: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    seats: SeatState,
    shm: Shm,
    pointer: Option<ThemedPointer>,
    allowed_outputs: Vec<String>,
    pub panels: Vec<Panel>,
    pub removed: Vec<Panel>,
    pub clicks: Vec<(f64, f64)>,
    pub layout_changed: bool,
}

impl Desktop {
    pub fn new(globals: &GlobalList, qh: &QueueHandle<Self>, allowed_outputs: Vec<String>) -> Result<Self, &'static str> {
        Ok(Self {
            registry: RegistryState::new(globals),
            outputs: OutputState::new(globals, qh),
            compositor: CompositorState::bind(globals, qh).map_err(|_| "wl_compositor")?,
            layer_shell: LayerShell::bind(globals, qh).map_err(|_| "zwlr_layer_shell_v1")?,
            seats: SeatState::new(globals, qh),
            shm: Shm::bind(globals, qh).map_err(|_| "wl_shm")?,
            pointer: None,
            allowed_outputs,
            panels: Vec::new(),
            removed: Vec::new(),
            clicks: Vec::new(),
            layout_changed: false,
        })
    }

    pub fn world(&self) -> Rect {
        let live = || self.panels.iter().filter(|p| p.configured);
        let left = live().map(|p| p.rect.x).min().unwrap_or(0);
        let top = live().map(|p| p.rect.y).min().unwrap_or(0);
        let right = live().map(|p| p.rect.x + p.rect.width).max().unwrap_or(1);
        let bottom = live().map(|p| p.rect.y + p.rect.height).max().unwrap_or(1);
        Rect { x: left, y: top, width: right - left, height: bottom - top }
    }

    pub fn request_frame(&mut self, index: usize, qh: &QueueHandle<Self>) {
        let panel = &mut self.panels[index];
        panel.frame_ready = false;
        let surface = panel.layer.wl_surface();
        surface.frame(qh, FrameCallbackData(surface.clone()));
    }

    fn read_output(&self, output: &WlOutput) -> Option<(String, Rect, i32)> {
        let info = self.outputs.info(output)?;
        let (x, y) = info.logical_position.unwrap_or(info.location);
        let (width, height) = info.logical_size?;
        Some((info.name.clone().unwrap_or_default(), Rect { x, y, width, height }, info.scale_factor))
    }

    fn add_panel(&mut self, qh: &QueueHandle<Self>, output: WlOutput) {
        let Some((name, rect, scale)) = self.read_output(&output) else { return };
        if !self.allowed_outputs.is_empty() && !self.allowed_outputs.contains(&name) {
            return;
        }
        let surface = self.compositor.create_surface(qh);
        surface.set_buffer_scale(scale);
        let layer = self.layer_shell.create_layer_surface(qh, surface, Layer::Background, Some("matrix-news-viewer"), Some(&output));
        layer.set_anchor(Anchor::all());
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(0, 0);
        layer.commit();
        self.panels.push(Panel { output, layer, rect, scale, configured: false, frame_ready: false, target: None });
    }

    fn panel_of(&mut self, surface: &WlSurface) -> Option<&mut Panel> {
        self.panels.iter_mut().find(|p| p.layer.wl_surface() == surface)
    }
}

impl CompositorHandler for Desktop {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &WlSurface, factor: i32) {
        if let Some(panel) = self.panel_of(surface) {
            panel.scale = factor;
            surface.set_buffer_scale(factor);
            self.layout_changed = true;
        }
    }

    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlSurface, _: Transform) {}

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &WlSurface, _: u32) {
        if let Some(panel) = self.panel_of(surface) {
            panel.frame_ready = true;
        }
    }

    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlSurface, _: &WlOutput) {}

    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlSurface, _: &WlOutput) {}
}

impl OutputHandler for Desktop {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, qh: &QueueHandle<Self>, output: WlOutput) {
        self.add_panel(qh, output);
    }

    fn update_output(&mut self, _: &Connection, qh: &QueueHandle<Self>, output: WlOutput) {
        let Some((_, rect, _)) = self.read_output(&output) else { return };
        match self.panels.iter_mut().find(|p| p.output == output) {
            Some(panel) => panel.rect = Rect { width: panel.rect.width, height: panel.rect.height, ..rect },
            None => self.add_panel(qh, output),
        }
        self.layout_changed = true;
    }

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, output: WlOutput) {
        if let Some(index) = self.panels.iter().position(|p| p.output == output) {
            let panel = self.panels.remove(index);
            self.removed.push(panel);
            self.layout_changed = true;
        }
    }
}

impl LayerShellHandler for Desktop {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        if let Some(index) = self.panels.iter().position(|p| &p.layer == layer) {
            let panel = self.panels.remove(index);
            self.removed.push(panel);
            self.layout_changed = true;
        }
    }

    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface, configure: LayerSurfaceConfigure, _: u32) {
        let Some(panel) = self.panels.iter_mut().find(|p| &p.layer == layer) else { return };
        let (width, height) = configure.new_size;
        if width > 0 && height > 0 {
            panel.rect.width = width as i32;
            panel.rect.height = height as i32;
        }
        panel.configured = true;
        panel.frame_ready = true;
        self.layout_changed = true;
    }
}

impl SeatHandler for Desktop {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}

    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: WlSeat, capability: Capability) {
        if capability != Capability::Pointer || self.pointer.is_some() {
            return;
        }
        let surface = self.compositor.create_surface(qh);
        self.pointer = self.seats.get_pointer_with_theme::<_, ()>(qh, &seat, self.shm.wl_shm(), surface, ThemeSpec::default()).ok();
    }

    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat, capability: Capability) {
        if capability == Capability::Pointer
            && let Some(pointer) = self.pointer.take() {
                pointer.pointer().release();
            }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}
}

impl PointerHandler for Desktop {
    fn pointer_frame(&mut self, conn: &Connection, _: &QueueHandle<Self>, _: &WlPointer, events: &[PointerEvent]) {
        for event in events {
            match event.kind {
                PointerEventKind::Enter { .. } => {
                    if let Some(pointer) = &self.pointer {
                        let _ = pointer.set_cursor(conn, CursorIcon::Default);
                    }
                }
                PointerEventKind::Press { button: BUTTON_LEFT, .. } => {
                    if let Some(panel) = self.panel_of(&event.surface) {
                        let point = (panel.rect.x as f64 + event.position.0, panel.rect.y as f64 + event.position.1);
                        self.clicks.push(point);
                    }
                }
                _ => {}
            }
        }
    }
}

impl ShmHandler for Desktop {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_registry!(Desktop);

impl ProvidesRegistryState for Desktop {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

smithay_client_toolkit::delegate_dispatch2!(Desktop);
