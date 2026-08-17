# Privacy

Privacy is a feature, and the strongest one. The contract:

## What never happens

- No account, login, or profile of any kind.
- No cloud storage, upload, or sync.
- No backend, no runtime network requests — the app is fully functional
  with the network cable pulled. (Bundle installers are the only network
  contact, and only if you choose to download one.)
- No telemetry, analytics, crash reporting, or tracking.
- No external/remote AI inference.
- Photos are never read by anything other than the local Rust analysis
  pipeline on this machine.

## What is stored, and where

OS-conventional per-user locations (no hidden dotfiles at arbitrary paths):

| item | Linux | macOS | Windows |
|---|---|---|---|
| database | `~/.local/share/com.photogremlin.app/database.sqlite` | `~/Library/Application Support/com.photogremlin.app/` | `%APPDATA%/com.photogremlin.app/` |
| thumbnails | `~/.cache/com.photogremlin.app/thumbnails/` | `~/Library/Caches/...` | `%LOCALAPPDATA%/.../cache` |
| logs | `~/.local/share/com.photogremlin.app/logs/photogremlin.<date>.log` | `~/Library/Logs/...` | `%LOCALAPPDATA%/.../logs` |

All of it is visible, inspectable and deletable by the user; deleting the
data directory is a complete uninstall data-wise. The Settings view shows
these exact paths.

## GPS

PhotoGremlin detects **whether** EXIF GPS data exists (`gps_present`), and
the dashboard can report the share of photos carrying GPS. It does **not**
store, display or transmit coordinates. A "remove GPS metadata" feature is
future work and will be done carefully because metadata modification is
destructive (§51 of the product spec).

## Logs

Logs contain diagnostics and file paths (local paths only), never pixel
data, never photo contents, never coordinates.

## Supply chain note

Dependencies are pinned in lockfiles (`package-lock.json`, `Cargo.lock`) and
kept minimal (see AGENTS.md). Every new dependency must be justified in the
PR/commit note.
