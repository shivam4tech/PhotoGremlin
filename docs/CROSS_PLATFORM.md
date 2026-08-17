# Cross-Platform

One codebase: **Tauri 2 + React/TypeScript + Vite (webview UI)** and **Rust
(core)**. Targets: Windows, macOS, Linux.

## What's shared vs platform-specific

- Shared: all of the product logic. Rust owns files/pixels/DB; one React UI.
- Platform-specific (handled by Tauri/ecosystem, not by us): window chrome,
  native dialogs (folder picker via `tauri-plugin-dialog`), OS trash target,
  bundle format, app data/cache/log locations (via Tauri path resolver).
- No `if OS == X` branches in domain logic. If one ever becomes unavoidable,
  it goes behind a small trait and gets documented here.

## Build requirements per OS

- **Common:** Rust stable, Node 20+.
- **Linux (dev/test box):** `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`,
  `libglib2.0-dev`, `libjavascriptcoregtk-4.1-dev`, `libxdo-dev`,
  `libssl-dev` (see DEVELOPMENT.md for the user-space sysroot recipe when
  root is unavailable).
- **Windows:** webview2 (preinstalled on Win10/11), NSIS for installer.
- **macOS:** Xcode command-line tools.

## Bundle targets (as configured)

`bundle.targets = ["deb", "app", "dmg", "nsis"]` — Linux ships `.deb`
(the `.rpm` bundler needs `rpmbuild`, unavailable on our CI-less dev box;
add the rpm target where the toolchain exists), macOS `.app`/`.dmg`,
Windows NSIS installer.

## Testing matrix on the dev box (Linux)

- `cargo test` — pure logic (analysis math, filter translation, rename
  templates, statistics, file-op planning) is OS-agnostic and runs here.
- GUI smoke: the built binary runs under `xvfb-run` (virtual X server) to
  verify launch, DB init and command registration without a display.
- Windows/macOS builds are produced on their respective toolchains at release
  time; the code itself is fully platform-neutral Rust + web.
