# matrix-news-viewer

A live wallpaper for Wayland compositors with the layer-shell protocol
(Hyprland, Sway, river, niri). Vertical Matrix rain covers all monitors as one
scene. Headlines from a newsboat cache appear inside the rain; a click on a
headline opens its link.

The program never goes to the network. News arrive only when you run
`newsboat -x reload`.

## Install

```
pacman -S newsboat
cargo install --path . --root ~/.local
mkdir -p ~/.config/newsboat ~/.config/matrix-news-viewer
cp newsboat-urls.example ~/.config/newsboat/urls
cp config.example.toml ~/.config/matrix-news-viewer/config.toml
newsboat -x reload
matrix-news-viewer
```

## Config

`~/.config/matrix-news-viewer/config.toml`. Every key is optional; see
`config.example.toml`. The program reloads the file when it changes.

The palette preset `theme` reads `~/.config/matrix-news-viewer/theme.toml`
(`rain`, `cursor`, `text`, `background` as `"#rrggbb"`), so a theme switcher
can recolour the rain live.

## Credits

Glyph atlas and rain shaders: [Rezmason/matrix](https://github.com/Rezmason/matrix),
MIT, see `assets/LICENSE-matrix`. The multi-monitor world crop and the headline
animation follow the design of
[HelpFreedom/matrix-rain-saver](https://github.com/HelpFreedom/matrix-rain-saver),
reimplemented from its description.
