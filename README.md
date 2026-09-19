# GalleryBrowser

A Linux-native browser for images, videos, and image sequences with DB-backed tags, collections, ratings, and fast preview workflows.

## Features

- **3-Pane Interface**: Tree navigation, file grid/list, and preview/info panel
- **Metadata Preview**: Image and video technical metadata in the info pane
- **Advanced Preview**: GStreamer-based playback for all major formats
- **File Management**: Copy, move, delete, batch rename with tokens
- **Organization**: Tags, collections, ratings, and folder/library filters
- **Image Sequences**: Detect and play numbered image sequences
- **Linux Integration**: Context menus, single instance, Flatpak support

## Supported Formats

**Images**: PNG, JPG, TIFF, BMP, GIF, WEBP, EXR, TGA, PSD
**Videos**: MP4, MOV, AVI, MKV, MXF, ProRes, H.264, H.265, AV1

## Installation

### Flatpak (Recommended)

Download `GalleryBrowser-0.1.0.x86_64.flatpak` from the [latest release](https://github.com/NickPittas/GalleryBrowser/releases), then:

```bash
flatpak install --user GalleryBrowser-0.1.0.x86_64.flatpak
flatpak run io.github.nickpittas.GalleryBrowser
```

The bundle pulls the KDE 6.10 runtime from Flathub automatically, and all video
codecs (MP4/MOV, H.264/H.265) ship inside the sandbox — no system packages
required. Once installed, the app appears in your application menu.

### From Source

```bash
git clone https://github.com/NickPittas/GalleryBrowser.git
cd GalleryBrowser

./setup_local_env.sh
./run_local_app.sh
```

`setup_local_env.sh` creates a repo-local `.venv`, installs runtime and dev dependencies there, and keeps runtime state inside `.local_state`. It uses `--system-site-packages` so the local venv can see Linux system packages such as `PyGObject` / GStreamer bindings without installing into system Python.

## Development

### Local Setup

```bash
./setup_local_env.sh
```

### Run the App

```bash
./run_local_app.sh
```

### Run Tests

```bash
./run_local_tests.sh
```

By default, `run_local_tests.sh` runs:

```bash
tests/test_basic.py -q
```

You can pass your own pytest arguments too:

```bash
./run_local_tests.sh tests/test_thumbnails.py -q
./run_local_tests.sh tests -q
```

The local test runner exports `QT_QPA_PLATFORM=offscreen` and the same repo-local XDG paths as the app, which is the most reliable way to run UI tests in a headless environment.

### Formatting

```bash
./.venv/bin/black src/ tests/
./.venv/bin/ruff check src/ tests/
```

## Usage

### Basic Navigation

- **Open Folder**: `Ctrl+O` or File → Open Folder
- **Navigate**: Click folders in left tree pane
- **View Files**: Grid or List view in center pane
- **Preview**: Select file to see preview in right pane
- **Filter Scope**: Switch between `Folder` and `Library` in the toolbar
- **Organize**: Use tags, collections, and rating filters from the left panel and toolbar

### File Operations

- **Copy**: `Ctrl+C`
- **Cut**: `Ctrl+X`
- **Paste**: `Ctrl+V`
- **Delete**: `Del` (moves to trash)
- **Rename**: `F2`
- **Batch Rename**: Select multiple files, right-click → Batch Rename

### ComfyUI Metadata

ComfyUI-specific metadata parsing is still a follow-up item. Current builds focus on
filesystem browsing, video/image/sequence preview, and DB-backed organization features.

## Current v1 Scope

Shipped now:

- Open-folder flow from the menu and shortcut
- Persistent settings, favorites, and recents
- DB-backed tags, collections, and ratings
- Folder-scoped and library-scoped filtering
- Video metadata in the info pane, loaded asynchronously
- Local-only setup scripts for app launch and headless tests

Still follow-up work:

- richer ComfyUI metadata display
- broader UI/integration test coverage beyond the current regression suite
- docs/design cleanup in `docs/DESIGN.md`, which still describes planned architecture in a few places
- format conversion and other advanced roadmap features

### Keyboard Shortcuts

See `docs/user_guide/KEYBOARD_SHORTCUTS.md`

## Architecture

See `docs/DESIGN.md` for the broader design direction. Some sections there are still aspirational and describe planned components rather than the exact current implementation.

## License

MIT License - See LICENSE file for details.

## Acknowledgments

Inspired by KAssetManager for Windows.
