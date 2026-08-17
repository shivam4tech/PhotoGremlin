# PhotoGremlin

PhotoGremlin is a **privacy-first, local-only** desktop application that helps photographers explore, analyze, filter, organize and understand their own photo collections.

It indexes the folders you choose and runs everything on your own machine:

- **Library** — recursive folder scanning, thumbnail grid, keyboard-navigable photo viewer
- **Camera metadata** — EXIF extraction (camera, lens, exposure, GPS, capture time)
- **Image analysis** — deterministic local measurements: brightness, contrast, saturation, sharpness, clipping, monochrome detection
- **Quality filtering** — structured, combinable filters over any measured property (no aesthetic judgments)
- **Similarity & bursts** — perceptual-hash grouping of near-duplicate and burst photographs
- **Organization** — saved views, collections, and safe rename / copy / move / trash operations with collision protection
- **Dashboard & sessions** — period-scoped statistics and per-import session reviews
- **Local intelligence (optional, off by default)** — on-device face detection with a small bundled model; the entire core product works with it disabled

**Your photos never leave your computer.** PhotoGremlin makes no network requests at runtime — no accounts, no cloud, no telemetry, no remote AI.

## Requirements

- A desktop OS: **Linux**, **macOS**, or **Windows**
- Linux: a Wayland or X11 desktop with WebKitGTK 4.1 and GTK 3 (standard on recent Ubuntu/Fedora/Debian desktops; the `.deb` declares them as package dependencies)

## Installation (for users)

PhotoGremlin is a normal installed desktop application: after installing, it appears in your OS application launcher, and clicking **PhotoGremlin** opens the window. No terminal, no scripts, no development tools — nothing has to run from a repository.

### Linux (`.deb`)

```sh
sudo apt install ./PhotoGremlin_0.1.0_amd64.deb
# or, with dpkg:
sudo dpkg -i PhotoGremlin_0.1.0_amd64.deb
```

The package registers the **PhotoGremlin** desktop entry (application launcher) and icons. Launch it from your application menu, or run `photogremlin` from a terminal if you prefer.

> Current availability: the release artifacts are produced on a Linux build environment, so a Linux `.deb` is available for v0.1.0. When a release is cut, the published artifacts will be linked from the **Releases** page of this repository.

### Windows and macOS

The packaging configuration supports Windows (NSIS installer) and macOS (`.dmg` / `.app`), but those installers must be built on their respective operating systems. Build them on a Windows or macOS machine with the development toolchain installed, as described under [Development](#development--building-from-source), using the same release build command. Once a release is cut, the corresponding installers will be published from the **Releases** page of this repository.

### Where your data lives

All state (index, thumbnails, settings, logs) is stored locally under the standard per-app data directory:

- Linux: `~/.local/share/com.photogremlin.app/`
- macOS: `~/Library/Application Support/com.photogremlin.app/`
- Windows: `%APPDATA%/com.photogremlin.app/`

Deleting that directory resets PhotoGremlin; it never touches your photo files unless you explicitly confirm a rename/copy/move/trash operation in the app.

## Development / building from source

For contributors who want to build PhotoGremlin. This is separate from simply using the app — end users install a release artifact (above).

**Prerequisites**

- Rust (stable) and Cargo
- Node.js 22+ and npm
- The platform system libraries Tauri 2 requires on your OS (on Linux: WebKitGTK 4.1 dev, GTK 3 dev, glib dev — see `docs/DEVELOPMENT.md` for the full list)

**Run in development**

```sh
git clone https://github.com/shivam4tech/PhotoGremlin.git
cd photogremlin
npm install
npm run tauri dev
```

**Build the release bundle**

```sh
npm run build:app:release
```

Release artifacts are written to `src-tauri/target/release/` (executable) and `src-tauri/target/release/bundle/` (packages, e.g. `bundle/deb/PhotoGremlin_0.1.0_amd64.deb` on Linux). See `docs/DEVELOPMENT.md` for build details.

## Documentation

- `docs/ARCHITECTURE.md` — system architecture, Rust/TS boundary, IPC surface
- `docs/DATABASE.md` — SQLite schema and migration history
- `docs/LOCAL_AI.md` — the on-device model pipeline (faces)
- `docs/DEVELOPMENT.md` — build instructions, toolchain, release packaging
- `docs/TESTING.md` — test strategy and how to run everything
- `AGENTS.md` — contribution rules and repository conventions

## License

MIT — see [LICENSE](LICENSE).
