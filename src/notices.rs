use std::path::Path;

pub fn no_wayland() -> String {
    "Cannot connect to the Wayland display. Run the program inside a Wayland session.".into()
}

pub fn missing_protocol(name: &str) -> String {
    format!("The compositor does not offer {name}. Use a compositor with the layer-shell protocol (Hyprland, Sway, river, niri).")
}

pub fn no_gpu(detail: &str) -> String {
    format!("Cannot start OpenGL 3.3 through EGL ({detail}). Check the graphics driver.")
}

pub fn bad_config(path: &Path, reason: &str) -> String {
    format!("{} is not valid: {reason}. The previous settings stay in use until the file is fixed.", path.display())
}

pub fn no_font(font: &str) -> String {
    format!("Font \"{font}\" is not found. Set [text] font to a font file or a fontconfig name.")
}

pub fn bad_font(font: &str, reason: &str) -> String {
    format!("Font \"{font}\" cannot be read: {reason}. Set [text] font to another font.")
}

pub fn no_news(path: &Path, reason: &str) -> String {
    format!("Cannot read news from {}: {reason}. Run `newsboat -x reload` or fix [feed] in the config. Fallback phrases are shown.", path.display())
}

pub fn link_failed(url: &str) -> String {
    format!("Cannot open {url}: xdg-open is not available.")
}

pub fn usage() -> String {
    "Usage: matrix-news-viewer [get <section.key> | set <section.key> <value>]. Without arguments it runs the wallpaper.".into()
}

pub fn unknown_key(key: &str) -> String {
    format!("\"{key}\" is not a setting. Use a section and a key from config.example.toml, such as rain.fall_speed.")
}

pub fn bad_value(key: &str, raw: &str) -> String {
    format!("\"{raw}\" does not fit {key}. Give a value of the same kind as in config.example.toml.")
}

pub fn bad_value_reason(key: &str, raw: &str, reason: &str) -> String {
    format!("\"{raw}\" cannot be stored in {key}: {reason}. The config file is unchanged.")
}
