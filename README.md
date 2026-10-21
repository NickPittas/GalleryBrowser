# GalleryBrowser

A fast, video-first media browser for Linux, written in Rust (egui + wgpu) on the
[ArtCraft](https://github.com/storytold) stack. No ffmpeg, no GStreamer: all decoding is pure Rust.

## Features

- **Video-first browsing**: MOV/MP4, MXF, MKV/WebM, AVI, MPEG-TS; ProRes, DNxHD/HR, H.264, HEVC, AV1, VP8/VP9.
- **Images**: EXR, PSD, TIFF, WebP, DPX, HDR, DNG and common formats (PNG, JPEG, ...).
- **Image sequences** collapsed into one entry, with gaps detected; operations act on every frame.
- **Hover scrub** over thumbnails of videos and sequences.
- **Preview pane and fullscreen player** with decode-ahead playback, audio (cpal), stepping, speed, loop.
- **Info pane**: file properties, EXIF, ComfyUI workflow/prompt metadata from PNGs.
- **Tags, collections and 0-5 ratings**, persisted crash-safely.
- **File operations** on a background worker: copy, move, trash, permanent delete, rename, batch rename; never overwrites; undo/redo.
- **Show all files** toggle (list non-media files too) and an optional **second file pane** (F3) for copying/moving between two folders.
- **Single instance**: `gallerybrowser PATH` forwards the path to the running window.

Keyboard shortcuts: [docs/user_guide/KEYBOARD_SHORTCUTS.md](docs/user_guide/KEYBOARD_SHORTCUTS.md).

## Install

Prebuilt packages for each release are attached to the
[GitHub Releases](https://github.com/NickPittas/GalleryBrowser/releases) page (`<ver>` is the version, e.g. `0.2.0`).

### Linux (Flatpak)

Download `GalleryBrowser-<ver>.flatpak` from Releases, then install it for your user. The same command
installs fresh or upgrades an existing GalleryBrowser:

```sh
flatpak install --user -y --reinstall ./GalleryBrowser-<ver>.flatpak
flatpak run io.github.nickpittas.GalleryBrowser
```

- Always pass `--user`: a system install (no `--user`) asks for a password through polkit and hangs
  forever on desktops without an authentication agent.
- Pass `-y`: without it flatpak first resolves the runtime from Flathub (the first run downloads its
  metadata and can sit silently for a while) and then waits at a `Proceed? [Y/n]` prompt that is easy to
  miss, so it looks frozen.
- If an install ever seems stuck, add `--verbose` to see the step it is on.

### macOS (11 or later, Apple Silicon and Intel)

1. Download `GalleryBrowser-<ver>-macos-universal.dmg`, open it and drag **GalleryBrowser** to **Applications**.
2. The app is ad-hoc signed but **not notarized** (no Apple Developer ID), so Gatekeeper blocks the first launch.
   Right-click (or Control-click) the app, choose **Open**, then confirm **Open** in the dialog. Alternatively, remove
   the quarantine flag once:

   ```sh
   xattr -dr com.apple.quarantine /Applications/GalleryBrowser.app
   ```

### Windows (10/11, x64)

- **Installer**: run `GalleryBrowser-<ver>-windows-x64-setup.exe` (per-user install, no administrator rights needed;
  adds a Start menu entry and an optional desktop shortcut).
- **Portable**: unzip `GalleryBrowser-<ver>-windows-x64.zip` anywhere and run `gallerybrowser.exe`.

The binaries are **not code-signed**, so Windows SmartScreen may show "Windows protected your PC" on first launch.
Click **More info**, then **Run anyway**. If you downloaded the zip, you can instead right-click it, open
**Properties** and tick **Unblock** before extracting.

The macOS and Windows packages are built by the [Release workflow](.github/workflows/release.yml) on GitHub Actions
when a `v*` tag is pushed.

## Requirements

- Rust >= 1.95 (edition 2024)
- A Vulkan-capable GPU driver (wgpu)
- ALSA or PipeWire (PulseAudio-compatible) for audio
- No ffmpeg or GStreamer needed

## Build and run

```sh
cargo run --release -p gallerybrowser -- [PATH...]
```

## Tests

```sh
cargo test --workspace
```

## Flatpak

App id: `io.github.nickpittas.GalleryBrowser`. The manifest builds offline, so the vendored crate list
must be generated from `Cargo.lock` first (re-run whenever `Cargo.lock` changes), using
`flatpak-cargo-generator.py` from [flatpak-builder-tools](https://github.com/flatpak/flatpak-builder-tools) (`cargo/`):

```sh
python3 flatpak-cargo-generator.py Cargo.lock -o flatpak/cargo-sources.json
flatpak-builder --user --install --force-clean flatpak/build-dir \
    flatpak/io.github.nickpittas.GalleryBrowser.yml
flatpak run io.github.nickpittas.GalleryBrowser
```

The manifest uses the freedesktop 25.08 runtime with the `rust-stable` SDK extension.

## Data locations

| Data | Path |
| --- | --- |
| Settings | `~/.config/gallerybrowser/settings.json` |
| Library (tags, collections, ratings) | `~/.local/share/gallerybrowser/library` |
| Thumbnail cache | `~/.cache/gallerybrowser/thumbnails` |

If a database from the old Python version exists, its tags, collections and ratings are imported once, read-only,
into the library. The Python app's `settings.json` keys are accepted unchanged.

## Architecture

| Layer | Crate | Role |
| --- | --- | --- |
| L0 | `gallerybrowser-core` | Entries, classification, scanning, sequences, settings, XDG dirs (UI-free) |
| L1 | `gallerybrowser-fileops` | Background copy/move/trash/delete/rename, batch rename, undo |
| L1 | `gallerybrowser-media` | Video/still/PSD decoding, EXIF, thumbnails, ComfyUI metadata |
| L2 | `gallerybrowser-library` | Tags/collections/ratings on the lightcraft-catalog journal, legacy import |
| L3 | `gallerybrowser-player` | Decode-ahead playback engine, cpal audio clock |
| L5 | `gallerybrowser-ui-egui` | egui frontend |
| app | `apps/gallerybrowser` | Binary, single-instance handling |

Details: [docs/DESIGN.md](docs/DESIGN.md).

## Credits

Built on ArtCraft's FilmCraft, LightCraft and PhotoCraft crates (MIT OR Apache-2.0); see [NOTICE](NOTICE).
Known defects of the former Python implementation, all addressed by the rewrite: [docs/BUG_REVIEW.md](docs/BUG_REVIEW.md).
