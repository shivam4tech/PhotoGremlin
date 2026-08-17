# Development

## Toolchain

- Rust stable (1.77+; developed on 1.97)
- Node 20+ (developed on 22) — npm
- Linux only: Tauri system libraries (see below)

## Environment setup (this machine, no root)

This dev box has **no usable root** (`sudo` demands an interactive password),
so Tauri's Linux `-dev` libraries were installed into a **user-space
sysroot** instead of the system:

```
~/pgsysroot/          extracted .deb tree (libwebkit2gtk-4.1, gtk3, glib,
                      javascriptcore, xdo, ssl, appindicator, rsvg + deps)
~/pg-env.sh           REQUIRED before any cargo / tauri / npm-tauri command
```

`pg-env.sh` exports: `$HOME/.cargo/bin` on PATH, `PKG_CONFIG_PATH` and
`LD_LIBRARY_PATH` into the sysroot, and C/C++ include paths.

Reconstruction recipe (if the sysroot is ever wiped):

1. `apt-get install -s <pkgs> | grep '^Inst'` to list the closure
   (pkgs: libwebkit2gtk-4.1-dev libgtk-3-dev libglib2.0-dev
   libjavascriptcoregtk-4.1-dev libgdk-pixbuf-2.0-dev libxdo-dev
   libssl-dev libayatana-appindicator3-dev librsvg2-dev),
   `apt-get download` them, `dpkg -x` each into the prefix.
2. `find prefix -name '*.pc' -exec sed -i 's|/usr/|prefix/usr/|g' {} +`
3. Relink dangling dev symlinks: for each `*.so` symlink whose target is
   missing in the sysroot, point it at the matching file in
   `/usr/lib/x86_64-linux-gnu` (runtime libs are already installed system-wide).

On a root-ful machine the normal path works: `apt install <same pkg list>`
and skip the sysroot entirely.

## ONNX Runtime (local intelligence, optional)

The Sprint 9 face detector runs the embedded YuNet model through the system
ONNX Runtime, dlopened at runtime (nothing bundled, nothing downloaded). It
is **optional**: without it the app works fully and face detection reports
itself unavailable from Settings.

- Dev box install here: the runtime is present at
  `/usr/lib/x86_64-linux-gnu/libonnxruntime.so.1.23` — note there is **no
  unversioned `.so.1` symlink** on this machine, which is why the resolver
  in `ml/mod.rs` also scans standard lib dirs for `libonnxruntime.so.1.*`
  (see LOCAL_AI.md §Runtime). If the library is missing, the integration
  pass tests skip (the queue/storage invariants still run).
- The `ort`/`ort-sys` pinning is deliberate and documented in LOCAL_AI.md —
  do not "upgrade" it: ort 1.x is yanked, `load-dynamic` died after
  `2.0.0-rc.9`, and ort rc.9 does not compile against ort-sys rc.10+.
- Rust `cargo test` needs nothing extra (the `ml` unit tests are pure; the
  integration tests detect the runtime at runtime).

## Day-to-day commands

| task | command (from repo root; source ~/pg-env.sh first on Linux here) |
|---|---|
| dev UI (hot reload) | `npm run dev` |
| dev full app | `npm run tauri dev` |
| Rust unit/integration tests | `cd src-tauri && cargo test` |
| Frontend tests | `npm test` |
| TypeScript check | `npm run typecheck` |
| Frontend build | `npm run build` |
| App bundle (debug, for sprint verification) | `npm run build:app` |
| App bundle (release, final sprint only) | `npm run build:app:release` |

## Sprint workflow

Follow AGENTS.md §Sprint workflow exactly: implement scope only → tests →
typecheck → build → fix reds → update docs → commit with a conventional
message (`feat: …`, `chore: …`). Never skip the build; never commit green
intent, only green reality.

## Performance guardrails

- Grid never decodes full-res images; thumbnails are cached on disk keyed by
  path+size+mtime+thumb-version, generated lazily with bounded concurrency.
- Analysis runs off the UI thread with a small worker pool; SQLite writes are
  incremental per photo.
- 5,000 photos must browse smoothly; 10,000 usable. Memory bounded by the
  worker pool, not by the library size.

## Release (v0.1.0, Sprint 10)

`build:app:release` (`tauri build`) is the release path — the only place the
`[profile.release]` settings in `src-tauri/Cargo.toml` apply:
`opt-level = "s"`, `lto = true`, `codegen-units = 1`, `strip = true`,
`panic = "abort"`. Produces (Linux):

- `src-tauri/target/release/photogremlin` — the stripped binary (~7.7 MB;
  the entire app + embedded YuNet model + bundled SQLite)
- `src-tauri/target/release/bundle/deb/PhotoGremlin_0.1.0_amd64.deb`
  (~3.5 MB)

macOS → `.dmg` / `app`, Windows → `.nsis` (the same command on those
platforms; `tauri.conf.json` lists `deb`/`app`/`dmg`/`nsis` as targets).

Ship notes:

- **No installer-time downloads, no bundled ONNX Runtime.** A machine
  without the ONNX Runtime gets the full app with face detection reported
  unavailable in Settings (see LOCAL_AI.md §Runtime for install options per
  OS).
- **Smoke before shipping:** fresh-data-dir headless boot of the release
  binary (see TESTING.md §Automated smoke), plus the manual per-platform
  checklist for the owner.
- `build:app` (debug) stays the sprint-verification bundle — never ship it
  (large + slow; no LTO).

### Package anatomy (Linux `.deb`)

The bundle is fully self-contained — the installed application needs no
repository, no Node, no Python, no Rust toolchain:

- `usr/bin/photogremlin` — the entire desktop app (frontend assets and the
  YuNet model embedded in the binary; system dependency: WebKitGTK 4.1 +
  GTK 3, declared in the deb control)
- `usr/share/applications/PhotoGremlin.desktop` — launcher entry
  (`Name=PhotoGremlin`, `Icon=photogremlin`, `Terminal=false`,
  `Categories=Graphics;Photography;` set via `bundle.category`)
- `usr/share/icons/hicolor/{32x32,128x128,256x256@2}/apps/photogremlin.png`
  — hicolor icons generated from the existing app icon set in
  `src-tauri/icons/`

Install with `sudo apt install ./PhotoGremlin_<ver>_amd64.deb` (or
`dpkg -i`) and launch "PhotoGremlin" from the application launcher.

Packaging metadata lives in `src-tauri/tauri.conf.json` (`bundle.*`:
`productName`, `identifier`, `category`, `shortDescription`,
`longDescription`, `homepage`, `publisher`, `licenseFile`,
`linux.deb.section`). `icons/` holds the single PhotoGremlin icon identity;
all platform variants (`.png` sizes, `.ico`, `.icns`) are generated forms of
that one artwork — never introduce a second design.

### Cross-platform artifacts

`bundle.targets` = `deb` (Linux), `dmg` + `app` (macOS), `nsis` (Windows).
Each target only builds on its own OS: a Linux machine produces the `.deb`,
a Mac produces the `.dmg`/`.app`, Windows produces the `.exe` installer.
Run the same `npm run build:app:release` command on each platform; nothing
in the core code is platform-branching.

## Repo hygiene

- Lockfiles are committed (`package-lock.json`, `Cargo.lock`).
- `src/` (UI) and `src-tauri/` (core) stay the two halves; `docs/` tracks
  reality; `RESUME.md` is transient (never committed long-term).
