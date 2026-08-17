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

## Repo hygiene

- Lockfiles are committed (`package-lock.json`, `Cargo.lock`).
- `src/` (UI) and `src-tauri/` (core) stay the two halves; `docs/` tracks
  reality; `RESUME.md` is transient (never committed long-term).
