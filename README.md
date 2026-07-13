# fp-artificiency
Flarepoint | Artificial Efficiency

Local-first AI usage analytics. Every usage tracker tells you how much you
spent; this one tells you **why**, and whether the thing you changed actually
helped. See [DESIGN.md](./DESIGN.md).

## Layout

- `crates/artificiency-core` — store (SQLite), collectors, stats engine. No GUI
  dependencies; compiles and tests standalone.
- `src-tauri` — Tauri v2 desktop shell exposing core commands.
- `src` — Svelte 5 dashboard.

## Development

Prerequisites: `node`, Rust (`rustup`), and on Linux the Tauri webview deps
(Arch: `sudo pacman -S webkit2gtk-4.1`).

```sh
npm install
npm run tauri dev          # desktop app (dashboard + collectors)
cargo test -p artificiency-core
cargo run --example backfill /tmp/smoke.db   # headless collector smoke run
```
