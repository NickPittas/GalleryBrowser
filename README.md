# GalleryBrowser

A Linux-native asset management application for viewing and organizing images and videos, with special support for ComfyUI-generated content.

## Features

- **3-Pane Interface**: Tree navigation, file grid/list, and preview/info panel
- **ComfyUI Support**: Parse and display PNG workflow metadata
- **Advanced Preview**: GStreamer-based playback for all major formats
- **File Management**: Copy, move, delete, batch rename with tokens
- **Organization**: Tags, collections, ratings, and smart filters
- **Image Sequences**: Detect and play numbered image sequences
- **Format Conversion**: Convert between image and video formats
- **Linux Integration**: Context menus, single instance, Flatpak support

## Supported Formats

**Images**: PNG, JPG, TIFF, BMP, GIF, WEBP, EXR, TGA, PSD
**Videos**: MP4, MOV, AVI, MKV, MXF, ProRes, H.264, H.265, AV1

## Installation

### Flatpak (Recommended)

```bash
flatpak install flathub com.nickpittas.gallerybrowser
flatpak run com.nickpittas.gallerybrowser
```

### From Source

```bash
# Clone repository
git clone https://github.com/NickPittas/GalleryBrowser.git
cd GalleryBrowser

# Create virtual environment
python3 -m venv venv
source venv/bin/activate

# Install dependencies
pip install -r requirements.txt

# Install in development mode
pip install -e .

# Run
gallerybrowser
```

## Development

### Setup Development Environment

```bash
pip install -e ".[dev]"
```

### Run Tests

```bash
pytest
```

### Code Formatting

```bash
black src/ tests/
ruff check src/ tests/
```

## Usage

### Basic Navigation

- **Open Folder**: `Ctrl+O` or File → Open Folder
- **Navigate**: Click folders in left tree pane
- **View Files**: Grid or List view in center pane
- **Preview**: Select file to see preview in right pane

### File Operations

- **Copy**: `Ctrl+C`
- **Cut**: `Ctrl+X`
- **Paste**: `Ctrl+V`
- **Delete**: `Del` (moves to trash)
- **Rename**: `F2`
- **Batch Rename**: Select multiple files, right-click → Batch Rename

### ComfyUI Metadata

When viewing ComfyUI-generated PNG files, the info panel shows:
- Positive and negative prompts
- Model name and settings
- Seed, steps, CFG scale
- Full workflow JSON

### Keyboard Shortcuts

See `docs/user_guide/KEYBOARD_SHORTCUTS.md`

## Architecture

See `docs/DESIGN.md` for detailed architecture and design decisions.

## License

MIT License - See LICENSE file for details.

## Acknowledgments

Inspired by KAssetManager for Windows.
