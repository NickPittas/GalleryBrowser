# GalleryBrowser Design Document

**Version:** 1.0  
**Status:** Draft  
**Date:** 2025-02-26

---

## 1. Executive Summary

GalleryBrowser is a Linux-native asset management application built with Python, PyQt6, and GStreamer. It specializes in viewing and organizing images and videos, with particular focus on ComfyUI-generated content (reading PNG workflow metadata). The application features a three-pane file manager interface with integrated preview, metadata display, and batch operations.

---

## 2. Core Architecture

### 2.1 Tech Stack

| Component | Technology | Purpose |
|-----------|-----------|---------|
| GUI Framework | **PyQt6** | Main application UI |
| Architecture Pattern | MVC (Model-View-Controller) | Clean separation of concerns |
| Media Preview | **GStreamer** with `qtvideosink` | Video/image playback |
| Image Processing | **OpenImageIO** via `imageio` | Thumbnail generation, EXR support |
| Metadata | **Pillow** + custom PNG parser | Image metadata, ComfyUI workflows |
| Video Info | **mediainfo** or **ffprobe** | Technical codec information |
| Database | **SQLite** with **SQLAlchemy** | Metadata indexing, collections |
| Async I/O | **asyncio** + **Qt signals** | Non-blocking file operations |

### 2.2 Project Structure

```
gallerybrowser/
├── src/
│   ├── gallerybrowser/           # Main package
│   │   ├── __init__.py
│   │   ├── __main__.py           # Entry point
│   │   ├── app.py                # QApplication setup
│   │   ├── config.py             # Settings and configuration
│   │   ├── database/
│   │   │   ├── __init__.py
│   │   │   ├── models.py         # SQLAlchemy models
│   │   │   ├── manager.py        # DB operations
│   │   │   └── migrations/       # Schema versioning
│   │   ├── core/
│   │   │   ├── __init__.py
│   │   │   ├── file_manager.py   # File operations
│   │   │   ├── metadata.py       # Metadata extraction
│   │   │   ├── thumbnail.py      # Thumbnail generation
│   │   │   └── sequences.py      # Image sequence handling
│   │   ├── models/
│   │   │   ├── __init__.py
│   │   │   ├── file_model.py     # File system model
│   │   │   ├── tree_model.py     # Folder tree model
│   │   │   └── collection_model.py # Collections model
│   │   ├── views/
│   │   │   ├── __init__.py
│   │   │   ├── main_window.py    # Main window
│   │   │   ├── tree_pane.py      # Left tree pane
│   │   │   ├── file_pane.py      # Center file grid/list
│   │   │   ├── preview_pane.py   # Right preview pane
│   │   │   ├── info_pane.py      # Metadata display
│   │   │   └── widgets/          # Reusable widgets
│   │   ├── controllers/
│   │   │   ├── __init__.py
│   │   │   ├── main_controller.py # Main window controller
│   │   │   └── file_controller.py # File operations controller
│   │   ├── utils/
│   │   │   ├── __init__.py
│   │   │   ├── paths.py          # Path utilities
│   │   │   ├── image.py          # Image utilities
│   │   │   └── comfyui.py        # ComfyUI metadata parser
│   │   └── workers/
│   │       ├── __init__.py
│   │       ├── thumbnail_worker.py # Background thumbnail gen
│   │       └── file_worker.py    # Background file operations
│   └── scripts/                  # Build/packaging scripts
├── docs/
│   ├── plans/                    # Design documents
│   ├── api/                      # API documentation
│   └── user_guide/               # User documentation
├── tests/                        # Test suite
├── resources/                    # Icons, stylesheets
├── flatpak/                      # Flatpak packaging
└── requirements.txt
```

---

## 3. Database Schema (SQLite)

### 3.1 Core Tables

```sql
-- Files table (primary)
CREATE TABLE files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT UNIQUE NOT NULL,
    filename TEXT NOT NULL,
    folder_path TEXT NOT NULL,
    file_type TEXT NOT NULL,  -- 'image', 'video', 'sequence'
    format TEXT,              -- 'png', 'jpg', 'mp4', etc.
    size_bytes INTEGER,
    created_at TIMESTAMP,
    modified_at TIMESTAMP,
    width INTEGER,
    height INTEGER,
    duration_seconds REAL,    -- For videos
    frame_count INTEGER,      -- For sequences
    checksum TEXT,            -- For change detection
    is_sequence INTEGER DEFAULT 0,
    sequence_start INTEGER,   -- For image sequences
    sequence_end INTEGER,
    sequence_pattern TEXT,    -- e.g., "frame_%04d.png"
    added_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    last_accessed TIMESTAMP
);

-- Thumbnails table
CREATE TABLE thumbnails (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    size INTEGER NOT NULL,    -- e.g., 128, 256, 512
    cache_path TEXT NOT NULL, -- Path in ~/.cache/gallerybrowser/
    generated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(file_id, size)
);

-- ComfyUI workflow metadata
CREATE TABLE comfyui_metadata (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER UNIQUE NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    workflow_json TEXT,       -- Full workflow JSON
    prompt TEXT,              -- Extracted positive prompt
    negative_prompt TEXT,
    model TEXT,
    sampler TEXT,
    scheduler TEXT,
    steps INTEGER,
    cfg_scale REAL,
    seed INTEGER,
    width INTEGER,
    height INTEGER,
    extracted_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Tags table
CREATE TABLE tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT UNIQUE NOT NULL,
    color TEXT,               -- Hex color for UI
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- File-Tag relationship
CREATE TABLE file_tags (
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    added_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (file_id, tag_id)
);

-- Collections (virtual folders)
CREATE TABLE collections (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    description TEXT,
    color TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Collection items
CREATE TABLE collection_items (
    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    added_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (collection_id, file_id)
);

-- Ratings
CREATE TABLE ratings (
    file_id INTEGER PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,
    rating INTEGER NOT NULL CHECK(rating >= 0 AND rating <= 5),
    rated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Settings
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```

### 3.2 Indexes

```sql
-- Performance indexes
CREATE INDEX idx_files_folder ON files(folder_path);
CREATE INDEX idx_files_type ON files(file_type);
CREATE INDEX idx_files_format ON files(format);
CREATE INDEX idx_files_added ON files(added_at);
CREATE INDEX idx_files_modified ON files(modified_at);
CREATE INDEX idx_comfyui_model ON comfyui_metadata(model);
CREATE INDEX idx_comfyui_sampler ON comfyui_metadata(sampler);
CREATE INDEX idx_comfyui_seed ON comfyui_metadata(seed);
CREATE INDEX idx_file_tags_tag ON file_tags(tag_id);
CREATE INDEX idx_collection_items_collection ON collection_items(collection_id);
```

---

## 4. User Interface Design

### 4.1 Layout: Classic 3-Pane (Default)

```
+-------------------------------------------------------------+
|  Menu Bar  |  Toolbar (New, Copy, Cut, Paste, Delete, ...) |
+------------+------------------------------------+------------+
|            |                                    |            |
|   TREE     |           FILE VIEW                |  PREVIEW   |
|   PANE     |        (Grid or List)              |   + INFO   |
|            |                                    |            |
|  Folder    |  [Thumb] [Thumb] [Thumb] [Thumb]   |  [Large    |
|  Tree      |  [Thumb] [Thumb] [Thumb] [Thumb]   |   Preview] |
|            |  [Thumb] [Thumb] [Thumb] [Thumb]   |            |
|            |                                    |  Metadata  |
|            |                                    |  Panel     |
+------------+------------------------------------+------------+
|                    Status Bar                               |
+-------------------------------------------------------------+
```

### 4.2 Panes

#### Tree Pane (Left)
- **File system tree**: Browse local directories
- **Collections section**: Virtual folders (Tags, Ratings, Collections)
- **Quick filters**: Favorites, Recent, Untagged
- **Draggable**: Drag folders to collections

#### File Pane (Center)
- **View modes**:
  - **Grid view**: Scalable thumbnails (64px to 512px)
  - **List view**: Details with columns (Name, Size, Date, Type, Rating)
  - **Sequence view**: Collapsed image sequences as single items
- **Sorting**: By name, date, size, type, rating
- **Selection**: Single, multi-select (Ctrl), range (Shift)
- **Context menu**: Open, Copy, Cut, Paste, Delete, Rename, Properties
- **Drag-drop**: Reorder within collections, move between folders

#### Preview + Info Pane (Right)
- **Preview widget**: GStreamer-based
  - Images: Zoom, pan, fit
  - Videos: Play/pause, timeline scrub, volume
  - Sequences: Frame scrub, FPS control
- **Info tabs**:
  - **General**: File properties (size, dates, dimensions)
  - **Technical**: Codec, bitrate, color space (from mediainfo)
  - **ComfyUI**: Parsed workflow (if present)
  - **EXIF**: Camera metadata
  - **Tags**: Assign/remove tags
  - **Rating**: Star rating

### 4.3 Toolbars

**Top Toolbar (Left-Aligned)**:
- New Folder
- Copy (Ctrl+C)
- Cut (Ctrl+X)
- Paste (Ctrl+V)
- Delete (Del)
- Rename (F2)
- Separator
- View Toggle (Grid/List)
- Grid Size Slider
- Group Sequences Toggle
- Separator
- Filter Input (search)

**Top Toolbar (Right-Aligned)**:
- Preview Toggle (show/hide right pane)
- Settings

**Bottom Status Bar**:
- Current folder path
- Item count (files + folders)
- Selected count
- Thumbnail generation progress
- Database sync status

---

## 5. Core Features

### 5.1 Thumbnail System

**Cache Location**: `~/.cache/gallerybrowser/thumbnails/`

**Sizes**: 64px, 128px, 256px, 512px (user-selectable default)

**Generation Strategy**:
1. **On-demand**: Generate when item enters viewport
2. **Background worker**: Thread pool processes queue
3. **Priority queue**: Currently visible items first
4. **LRU cache**: In-memory cache for recent thumbnails

**Supported Formats**:
- Images: PNG, JPG, TIFF, BMP, GIF, WEBP, EXR, TGA, PSD
- Videos: MP4, MOV, AVI, MKV, MXF, ProRes (via FFmpeg frame extraction)
- Sequences: Detect numbered patterns, generate from first frame

**Implementation**:
```python
class ThumbnailWorker(QObject):
    thumbnail_ready = pyqtSignal(int, QPixmap)  # file_id, pixmap
    
    def generate_thumbnail(self, file_path: str, size: int) -> QPixmap:
        # Use OpenImageIO for images
        # Use FFmpeg for video frame extraction
        # Save to cache, store path in DB
        pass
```

### 5.2 File Operations

**Basic Operations**:
- Copy/Move/Delete with progress dialogs
- Undo/Redo stack (for non-destructive operations)
- Recycle Bin integration (via `gio trash` or `trash-cli`)

**Batch Rename**:
- Pattern-based: `{original}_{date}_{counter}`
- Tokens:
  - `{name}` - Original filename
  - `{ext}` - Extension
  - `{date}` - Creation date (YYYY-MM-DD)
  - `{time}` - Creation time (HH-MM-SS)
  - `{datetime}` - Combined
  - `{counter}` - Sequential number
  - `{model}` - ComfyUI model name (if available)
  - `{seed}` - ComfyUI seed (if available)
  - `{width}`, `{height}` - Dimensions
- Preview before apply
- Preserve timestamps option

**Drag and Drop**:
- Internal: Reorder collections, move files
- External: Accept drops from file manager
- External drag-out: Drag to file manager or other apps

### 5.3 ComfyUI Metadata Handling

**PNG Chunk Parsing**:
- Read `tEXt` chunks with `workflow` and `prompt` keys
- Store parsed JSON in `comfyui_metadata` table
- Extract key parameters (prompt, model, seed, etc.)

**Display**:
- Workflow viewer: Tree view of node graph
- Prompt display: Formatted positive/negative prompts
- Parameters table: Steps, CFG, sampler, etc.
- Copy-to-clipboard buttons for prompts

**No Write Support (v1)**: Read-only metadata display

### 5.4 Image Sequence Support

**Detection**:
- Pattern matching: `frame_%04d.png`, `render.####.exr`, etc.
- Number range detection
- Group as single item in file view

**Playback**:
- Frame-by-frame navigation
- Timeline scrub
- FPS control (12, 24, 30, 60, custom)
- Play/Pause/Stop
- Loop modes

**Export**:
- Convert to video (MP4, ProRes, etc.)
- Re-frame (change numbering)
- Extract single frames

### 5.5 Collections and Tags

**Tags**:
- Create, rename, delete tags
- Assign multiple tags per file
- Color-coded in UI
- Filter by tags (AND/OR modes)

**Collections (Virtual Folders)**:
- Create named collections
- Add files without moving them
- Organize by project, theme, quality, etc.
- Nested collections support

**Smart Collections (Auto-updating)**:
- Filter-based: "All 5-star images"
- Tag-based: "All images with 'portrait' tag"
- Date-based: "Images from last week"

### 5.6 Comparison Mode

**Side-by-Side (A/B)**:
- Select two images, open comparison
- Synchronized zoom and pan
- Swap A/B
- Metadata comparison table

**Swipe Comparison**:
- Vertical or horizontal wipe
- Draggable divider
- Show differences highlight

**Parameters Comparison**:
- For ComfyUI images: Show which parameters differ
- Highlight changed values

### 5.7 Format Conversion

**Via FFmpeg Integration**:
- Images: Convert between PNG, JPG, TIFF, WEBP, EXR
- Videos: Convert between MP4, MOV, ProRes, AVI
- Sequences: Export as video or reformat

**Options**:
- Quality/compression settings
- Color space conversion
- Resolution scaling
- Preserve metadata option

---

## 6. Linux Integration

### 6.1 Single Instance

**Implementation**: Socket-based IPC

```python
class SingleInstanceManager:
    def __init__(self, socket_path: str):
        self.socket_path = socket_path
        self.server = None
        
    def try_start(self) -> bool:
        # Try to bind to socket
        # If success: start server, listen for commands
        # If fail: connect to existing, send file path
        pass
```

**Behavior**:
- First launch: Start app normally
- Subsequent launches: Send file/folder path to running instance, raise window

### 6.2 Context Menu Integration

**For Nemo (Cinnamon)**:
```
~/.local/share/nemo/actions/gallerybrowser.nemo_action
```

**For Dolphin (KDE)**:
```
~/.local/share/kservices5/ServiceMenus/gallerybrowser.desktop
```

**For Nautilus (GNOME)**:
```
~/.local/share/nautilus-python/extensions/gallerybrowser.py
```

**Action**: "Open in GalleryBrowser"

### 6.3 MIME Association

**Optional**: Associate as default viewer for:
- Images: PNG, JPG, EXR, TIFF, TGA, PSD
- Videos: MP4, MOV, AVI, MKV

Register in:
```
~/.config/mimeapps.list
```

### 6.4 Thumbnail Cache Location

```
~/.cache/gallerybrowser/
├── thumbnails/          # Generated thumbnails
│   ├── 64/
│   ├── 128/
│   ├── 256/
│   └── 512/
├── previews/            # Full-size preview cache
├── database.sqlite      # Main database
└── settings.json        # User preferences
```

---

## 7. Performance Optimizations

### 7.1 Lazy Loading

- Thumbnails generated only when visible
- Metadata loaded on-demand
- Database queries paginated

### 7.2 Virtualized Views

```python
class VirtualFileGrid(QAbstractItemView):
    # Only render visible items
    # Recycle item widgets
    # Handle 10,000+ files smoothly
    pass
```

### 7.3 Background Workers

- `ThumbnailWorker`: Thread pool for thumbnail generation
- `FileOperationWorker`: Async copy/move operations
- `MetadataWorker`: Background metadata extraction

### 7.4 Caching Strategy

**In-Memory**:
- LRU cache for recent thumbnails (512MB limit)
- Recently viewed files metadata
- Collection/tag lists

**On-Disk**:
- Thumbnail cache (persistent)
- Preview frames for videos
- Database with proper indexes

### 7.5 Database Optimization

- WAL mode for better concurrency
- Proper indexes on all query columns
- Prepared statements for common queries
- Connection pooling (SQLAlchemy)

---

## 8. Packaging and Distribution

### 8.1 Primary: Flatpak

**Why Flatpak**:
- Cross-distro compatibility
- Sandboxed security
- GStreamer, FFmpeg dependencies bundled
- Auto-updates via flathub

**Flatpak manifest**:
```yaml
app-id: com.nickpittas.gallerybrowser
runtime: org.kde.Platform
runtime-version: '6.6'
sdk: org.kde.Sdk
base: com.riverbankcomputing.PyQt.BaseApp
command: gallerybrowser
modules:
  - python-dependencies.json
  - gstreamer-plugins.json
  - ffmpeg.json
```

### 8.2 Secondary: pip

**For developers/power users**:
```bash
pip install gallerybrowser
gallerybrowser  # CLI entry point
```

### 8.3 System Requirements

**Minimum**:
- Linux kernel 5.4+
- 4GB RAM
- OpenGL 3.0+
- Python 3.10+ (for pip install)

**Recommended**:
- 8GB+ RAM
- SSD for cache
- Hardware video decoding

---

## 9. Implementation Phases

### Phase 1: Core Foundation (Weeks 1-2)
- [ ] Project setup (PyQt6, SQLAlchemy, GStreamer)
- [ ] Database schema and models
- [ ] Basic 3-pane UI layout
- [ ] File system tree navigation
- [ ] Basic thumbnail generation (images only)

### Phase 2: File Management (Weeks 3-4)
- [ ] Grid/List view with virtual scrolling
- [ ] File operations (copy, move, delete, rename)
- [ ] Drag and drop support
- [ ] Thumbnail caching system
- [ ] Background worker threads

### Phase 3: Preview System (Weeks 5-6)
- [ ] GStreamer preview widget
- [ ] Image viewing with zoom/pan
- [ ] Video playback controls
- [ ] Info panel with metadata
- [ ] Supported formats: PNG, JPG, MP4, MOV

### Phase 4: ComfyUI & Advanced Features (Weeks 7-8)
- [ ] PNG metadata parsing
- [ ] ComfyUI workflow display
- [ ] Batch rename with tokens
- [ ] Image sequence detection
- [ ] Format conversion (FFmpeg)

### Phase 5: Organization Features (Weeks 9-10)
- [ ] Tags system
- [ ] Collections/virtual folders
- [ ] Comparison mode
- [ ] Search and filtering
- [ ] Rating system

### Phase 6: Polish & Integration (Weeks 11-12)
- [ ] Linux context menu integration
- [ ] Single instance support
- [ ] Settings/preferences
- [ ] Performance optimization
- [ ] Flatpak packaging

---

## 10. API and Extension Points

### 10.1 Plugin Architecture (v2)

```python
class GalleryBrowserPlugin:
    name: str
    version: str
    
    def register_file_handlers(self) -> List[FileHandler]:
        pass
    
    def register_metadata_parsers(self) -> List[MetadataParser]:
        pass
    
    def register_views(self) -> List[View]:
        pass
```

### 10.2 Scripting Support (v2)

```python
# Python scripting for automation
from gallerybrowser import API

api = API()
files = api.search(prompt="masterpiece", model="SDXL")
for f in files:
    f.move_to("/path/to/collection")
    f.add_tag("curated")
```

---

## 11. Testing Strategy

### 11.1 Unit Tests
- Database operations
- Metadata parsing
- File utilities
- Thumbnail generation

### 11.2 Integration Tests
- File operations
- UI interactions
- GStreamer pipeline
- Database migrations

### 11.3 Manual Testing
- Large folder performance (10k+ files)
- Various image/video formats
- ComfyUI PNG files
- Linux desktop integration

---

## 12. Future Enhancements (Post-v1)

- **ComfyUI API Integration**: Send workflows back to ComfyUI
- **AI Features**: Auto-tagging, duplicate detection, NSFW filtering
- **Network Features**: Share collections, cloud sync
- **Mobile Companion**: View collections on mobile
- **Advanced Editing**: Basic image adjustments, crop, rotate
- **3D Support**: Preview GLTF, OBJ files
- **Plugin System**: Third-party extensions
- **Scripting**: Python automation API
- **Collaboration**: Multi-user annotations
- **Version Control**: Git-like versioning for assets

---

## 13. Appendix

### 13.1 Dependencies

```txt
# Core
PyQt6>=6.5.0
SQLAlchemy>=2.0.0
alembic>=1.12.0

# Image processing
Pillow>=10.0.0
imageio>=2.31.0
imageio-ffmpeg>=0.4.8
OpenImageIO>=2.4.0  # Optional for EXR

# Video/Metadata
pymediainfo>=6.0.0
python-magic>=0.4.27

# GStreamer
PyGObject>=3.44.0

# Utilities
appdirs>=1.4.4
python-dateutil>=2.8.0
```

### 13.2 Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+O` | Open folder |
| `Ctrl+C` | Copy |
| `Ctrl+X` | Cut |
| `Ctrl+V` | Paste |
| `Ctrl+A` | Select all |
| `Del` | Delete to trash |
| `F2` | Rename |
| `Space` | Quick preview |
| `Enter` | Open in full preview |
| `Ctrl+F` | Search/filter |
| `Ctrl+T` | New tag |
| `Ctrl+N` | New collection |
| `1-5` | Set rating |
| `Ctrl+1/2/3` | Toggle view mode |
| `Ctrl++/-` | Zoom thumbnails |
| `Ctrl+W` | Close window |
| `Ctrl+Q` | Quit application |

---

**Document History:**
- v1.0 (2025-02-26): Initial design based on brainstorm results
