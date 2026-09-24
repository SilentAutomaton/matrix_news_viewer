# Working on matrix-news-viewer

Tier: 1

## What it is

A Wayland live wallpaper for one Arch Linux desktop. Vertical Matrix rain fills
every monitor as one continuous scene, and news headlines from a newsboat
SQLite cache appear inside the rain. A click on a headline opens its link.

## Running it

```
cargo run --release
```

## Checking it

```
cargo test && cargo clippy -- -D warnings
```

## Code

`~/.config/claude/templates/agents/codestyle.md`, the same standard at every
tier. This project has more than one module, so the modularity rules apply.

## Rules

1. The program never touches the network. News arrive only when the user runs
   `newsboat -x reload`; the renderer opens the database read-only.
2. Nothing personal is committed. No keys, no home paths, no dumps. The
   repository ships `*.example` files only.

## Commits

One step, one commit. A single line of plain English describing what the code
now does. No tool names, no attribution trailers.
