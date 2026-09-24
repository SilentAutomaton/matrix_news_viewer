use std::ffi::c_void;

use khronos_egl as egl;
use smithay_client_toolkit::reexports::client::{Connection, Proxy, protocol::wl_surface::WlSurface};
use wayland_egl::WlEglSurface;

pub struct Egl {
    egl: egl::Instance<egl::Static>,
    display: egl::Display,
    config: egl::Config,
    context: egl::Context,
}

pub struct Target {
    window: WlEglSurface,
    surface: egl::Surface,
}

impl Egl {
    pub fn new(conn: &Connection) -> Result<Self, egl::Error> {
        let egl = egl::Instance::new(egl::Static);
        let native = conn.backend().display_ptr() as egl::NativeDisplayType;
        let display = unsafe { egl.get_display(native) }.ok_or(egl::Error::BadDisplay)?;
        egl.initialize(display)?;
        egl.bind_api(egl::OPENGL_API)?;
        let attributes = [
            egl::SURFACE_TYPE, egl::WINDOW_BIT,
            egl::RENDERABLE_TYPE, egl::OPENGL_BIT,
            egl::RED_SIZE, 8, egl::GREEN_SIZE, 8, egl::BLUE_SIZE, 8,
            egl::NONE,
        ];
        let config = egl.choose_first_config(display, &attributes)?.ok_or(egl::Error::BadConfig)?;
        let context_attributes = [
            egl::CONTEXT_MAJOR_VERSION, 3,
            egl::CONTEXT_MINOR_VERSION, 3,
            egl::CONTEXT_OPENGL_PROFILE_MASK, egl::CONTEXT_OPENGL_CORE_PROFILE_BIT,
            egl::NONE,
        ];
        let context = egl.create_context(display, config, None, &context_attributes)?;
        egl.make_current(display, None, None, Some(context))?;
        Ok(Self { egl, display, config, context })
    }

    pub fn glow(&self) -> glow::Context {
        unsafe {
            glow::Context::from_loader_function(|name| {
                self.egl.get_proc_address(name).map_or(std::ptr::null(), |f| f as *const c_void)
            })
        }
    }

    pub fn target(&self, surface: &WlSurface, width: i32, height: i32) -> Result<Target, egl::Error> {
        let window = WlEglSurface::new(surface.id(), width, height).map_err(|_| egl::Error::BadNativeWindow)?;
        let surface = unsafe {
            self.egl.create_window_surface(self.display, self.config, window.ptr() as egl::NativeWindowType, None)
        }?;
        Ok(Target { window, surface })
    }

    pub fn resize(&self, target: &Target, width: i32, height: i32) {
        target.window.resize(width, height, 0, 0);
    }

    pub fn make_current(&self, target: &Target) -> Result<(), egl::Error> {
        self.egl.make_current(self.display, Some(target.surface), Some(target.surface), Some(self.context))?;
        // The compositor frame callback paces each output; a blocking swap would stall on hidden ones.
        self.egl.swap_interval(self.display, 0)
    }

    pub fn swap(&self, target: &Target) -> Result<(), egl::Error> {
        self.egl.swap_buffers(self.display, target.surface)
    }

    pub fn destroy(&self, target: Target) {
        let _ = self.egl.make_current(self.display, None, None, Some(self.context));
        let _ = self.egl.destroy_surface(self.display, target.surface);
    }
}
