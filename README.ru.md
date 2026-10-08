[English version](README.md)

# matrix-news-viewer

Живые обои для композиторов Wayland с протоколом layer-shell (Hyprland, Sway,
river, niri). Вертикальный дождь из «Матрицы» покрывает все мониторы как одна
сцена. Внутри дождя появляются заголовки из кэша newsboat. Клик по заголовку
проливает дождём первый абзац новости, двойной клик открывает ссылку. Идея
взята у [HelpFreedom/matrix-rain-saver](https://github.com/HelpFreedom/matrix-rain-saver);
это отдельная программа, а не форк.

Программа сама в сеть не ходит. Новости появляются, только когда вы запускаете
`newsboat -x reload`.

## Установка

```
pacman -S newsboat
cargo install --path . --root ~/.local
mkdir -p ~/.config/newsboat ~/.config/matrix-news-viewer
cp newsboat-urls.example ~/.config/newsboat/urls
cp config.example.toml ~/.config/matrix-news-viewer/config.toml
newsboat -x reload
matrix-news-viewer
```

## Настройка

`~/.config/matrix-news-viewer/config.toml`. Все ключи необязательные, пример в
`config.example.toml`. Программа перечитывает файл, когда он меняется.

Пресет палитры `theme` читает `~/.config/matrix-news-viewer/theme.toml`
(`rain`, `cursor`, `text`, `background` в виде `"#rrggbb"`), поэтому
переключатель тем может перекрашивать дождь на лету.

Отдельные ключи можно читать и менять из скриптов, комментарии в файле при
этом сохраняются:

```
matrix-news-viewer get rain.fall_speed
matrix-news-viewer set rain.density 0.3
```

## Благодарности

Атлас глифов и шейдеры дождя взяты из
[Rezmason/matrix](https://github.com/Rezmason/matrix), лицензия MIT, см.
`assets/LICENSE-matrix`. Общая картинка на несколько мониторов и анимация
заголовков повторяют замысел
[HelpFreedom/matrix-rain-saver](https://github.com/HelpFreedom/matrix-rain-saver)
и написаны заново по его описанию.

## Лицензия

GPL-3.0. Файлы из Rezmason/matrix остаются под лицензией MIT.
