# GalleryBrowser Implementation TODO

## Phase 1: Core Foundation (Weeks 1-2) - IN PROGRESS

### Week 1 - COMPLETED ✅

#### Day 1-2: Project Setup ✅
- [x] Write design document to docs/DESIGN.md
- [x] Create project directory structure
  - [x] src/gallerybrowser/ with all subdirectories
  - [x] docs/, tests/, resources/, flatpak/
- [x] Create requirements.txt with all dependencies
- [x] Create pyproject.toml for modern Python packaging
- [x] Set up git repository with .gitignore

#### Day 3-4: Database Foundation ✅
- [x] Install SQLAlchemy and create models.py (228 lines)
- [x] Implement database schema from design doc
- [x] Create database/manager.py with CRUD operations (184 lines)
- [ ] Set up database migrations with Alembic
- [x] Write unit tests for database operations (9 tests passing)

#### Day 5-7: Basic UI Skeleton ✅
- [x] Create main_window.py with 3-pane layout (109 lines)
- [x] Implement tree_pane.py (left folder tree) (74 lines)
- [x] Create basic menu bar and toolbar
- [x] Implement splitter management for resizable panes
- [x] Add status bar with basic info

#### Day 8: Testing & Verification ✅
- [x] Install dependencies in virtual environment
- [x] Fix SQLAlchemy 2.0 compatibility (text() wrapper for PRAGMA)
- [x] Test application launch (runs without errors)
- [x] Verify database creation (9 tables created)
- [x] Write and run unit tests (9/9 tests passing)
- [x] Test database CRUD operations
- [x] Test configuration module
- [x] Verify UI imports work correctly
- [x] Add proper Ctrl+C signal handling to prevent crashes on exit

### Week 2 - NEXT

#### Day 8-10: File System Integration
- [ ] Implement file_model.py for file listing
- [ ] Create tree_model.py for folder navigation
- [ ] Connect tree selection to file view updates
- [ ] Add basic file information display
- [ ] Implement breadcrumb navigation

#### Day 11-14: Thumbnail Foundation
- [ ] Create thumbnail.py module
- [ ] Implement basic PIL-based thumbnail generation
- [ ] Set up thumbnail cache directory structure
- [ ] Create ThumbnailWorker for background generation
- [ ] Display thumbnails in file pane (grid view)

### Phase 1 Deliverables
- [ ] Working 3-pane UI with folder navigation
- [ ] SQLite database storing file metadata
- [ ] Basic thumbnail generation for common image formats
- [ ] Can browse folders and see files with thumbnails

## Phase 2: File Management (Weeks 3-4)

### Week 3

#### Day 1-3: File Operations
- [ ] Implement copy/move/delete operations
- [ ] Create progress dialogs for long operations
- [ ] Add undo/redo system
- [ ] Integrate with system trash (gio trash)
- [ ] Handle errors gracefully

#### Day 4-7: Batch Rename
- [ ] Create batch rename dialog
- [ ] Implement token parsing ({name}, {date}, etc.)
- [ ] Add live preview of rename results
- [ ] Preserve timestamps option
- [ ] Support for ComfyUI tokens ({model}, {seed})

### Week 4

#### Day 8-10: Drag and Drop
- [ ] Internal drag-drop between folders
- [ ] Accept external drops from file manager
- [ ] Drag out to external applications
- [ ] Visual feedback during drag operations

#### Day 11-14: Performance Optimization
- [ ] Implement virtual scrolling for large folders
- [ ] Lazy loading of thumbnails
- [ ] Background worker thread pool
- [ ] LRU cache for recent thumbnails
- [ ] Database query optimization

### Phase 2 Deliverables
- [ ] Full file management (copy, move, delete, rename)
- [ ] Batch rename with token support
- [ ] Drag and drop support
- [ ] Handles 1000+ files smoothly

## Phase 3: Preview System (Weeks 5-6)

### Week 5

#### Day 1-4: GStreamer Setup
- [ ] Install and configure GStreamer bindings
- [ ] Create preview_pane.py widget
- [ ] Implement basic video playback
- [ ] Add play/pause/stop controls
- [ ] Timeline scrubbing

#### Day 5-7: Image Preview
- [ ] Implement image zoom and pan
- [ ] Fit to window / actual size toggle
- [ ] Support for high-DPI displays
- [ ] Background loading of large images

### Week 6

#### Day 8-10: Advanced Preview
- [ ] EXR color space handling
- [ ] HDR display support
- [ ] Video frame extraction for thumbnails
- [ ] Full-screen preview mode

#### Day 11-14: Info Panel
- [ ] Create info_pane.py with tabs
- [ ] General file properties tab
- [ ] Technical metadata from mediainfo
- [ ] EXIF data display
- [ ] Image dimensions and format info

### Phase 3 Deliverables
- [ ] GStreamer preview for images and videos
- [ ] Full preview controls (zoom, pan, playback)
- [ ] Info panel with file metadata
- [ ] Support for PNG, JPG, MP4, MOV formats

## Phase 4: ComfyUI & Advanced Features (Weeks 7-8)

### Week 7

#### Day 1-4: ComfyUI Metadata
- [ ] Implement comfyui.py PNG parser
- [ ] Extract workflow JSON from tEXt chunks
- [ ] Parse key parameters (prompt, model, seed, etc.)
- [ ] Store in database
- [ ] Display in info panel

#### Day 5-7: Workflow Display
- [ ] Create workflow viewer widget
- [ ] Display node graph as tree
- [ ] Show prompt text with copy buttons
- [ ] Highlight key parameters

### Week 8

#### Day 8-10: Image Sequences
- [ ] Detect numbered file patterns
- [ ] Group sequences in file view
- [ ] Sequence playback controls
- [ ] FPS setting and frame scrub

#### Day 11-14: Format Conversion
- [ ] Integrate FFmpeg for conversions
- [ ] Create conversion dialog
- [ ] Support image format conversion
- [ ] Video format conversion
- [ ] Image sequence to video export

### Phase 4 Deliverables
- [ ] ComfyUI metadata parsing and display
- [ ] Image sequence detection and playback
- [ ] Format conversion via FFmpeg
- [ ] Batch rename with ComfyUI tokens

## Phase 5: Organization Features (Weeks 9-10)

### Week 9

#### Day 1-4: Tags System
- [ ] Create tag management UI
- [ ] Assign/remove tags from files
- [ ] Color-coded tag display
- [ ] Filter by tags (AND/OR)
- [ ] Quick tag shortcuts

#### Day 5-7: Collections
- [ ] Implement collections in database
- [ ] Create collection management UI
- [ ] Add files to collections
- [ ] Display collections in tree pane

### Week 10

#### Day 8-10: Comparison Mode
- [ ] Side-by-side image comparison
- [ ] Synchronized zoom and pan
- [ ] Metadata comparison
- [ ] Swap A/B functionality

#### Day 11-14: Ratings & Search
- [ ] 5-star rating system
- [ ] Filter by rating
- [ ] Search by filename
- [ ] Advanced search (metadata, tags)

### Phase 5 Deliverables
- [ ] Tags and collections system
- [ ] Image comparison mode
- [ ] Rating system
- [ ] Search and filtering

## Phase 6: Polish & Integration (Weeks 11-12)

### Week 11

#### Day 1-4: Linux Integration
- [ ] Single instance implementation (socket IPC)
- [ ] Context menu for Nemo
- [ ] Context menu for Dolphin
- [ ] Context menu for Nautilus
- [ ] Desktop file and MIME associations

#### Day 5-7: Settings
- [ ] Create settings dialog
- [ ] Thumbnail size preferences
- [ ] Cache management
- [ ] Keyboard shortcuts configuration
- [ ] Theme selection

### Week 12

#### Day 8-10: Performance & Polish
- [ ] Final performance optimization
- [ ] Memory leak testing
- [ ] UI polish and consistency
- [ ] Error handling improvements
- [ ] Logging system

#### Day 11-14: Packaging
- [ ] Create Flatpak manifest
- [ ] Test on multiple distros
- [ ] Write README and user guide
- [ ] Prepare for release

### Phase 6 Deliverables
- [ ] Linux desktop integration complete
- [ ] Single instance working
- [ ] Settings/preferences dialog
- [ ] Flatpak package ready

## Testing Checklist

### Unit Tests
- [ ] Database operations
- [ ] File utilities
- [ ] Thumbnail generation
- [ ] Metadata parsing
- [ ] Batch rename logic

### Integration Tests
- [ ] File operations (copy, move, delete)
- [ ] Database migrations
- [ ] GStreamer pipeline
- [ ] Drag and drop

### Manual Testing
- [ ] Large folder with 10k+ files
- [ ] Various image formats (PNG, JPG, EXR, TIFF, PSD)
- [ ] Various video formats (MP4, MOV, AVI, MKV, ProRes)
- [ ] ComfyUI PNG files with metadata
- [ ] Image sequences
- [ ] Linux context menus
- [ ] Single instance behavior

## Release Criteria

- [ ] All Phase 1-6 features implemented
- [ ] Unit tests passing
- [ ] No critical bugs
- [ ] Documentation complete
- [ ] Flatpak package builds
- [ ] Tested on Ubuntu, Fedora, Arch

---

## Progress Summary

**Total Files Created:** 20 Python files + documentation
**Lines of Code:** ~950 lines (including tests)
**Current Phase:** Phase 1, Week 1 completed ✅
**Test Status:** 9/9 tests passing

### Completed Components:
1. ✅ Project structure and configuration
2. ✅ Database models (SQLAlchemy) - 9 tables
3. ✅ Database manager (CRUD operations) - 9 tests passing
4. ✅ Basic 3-pane UI layout
5. ✅ Folder tree navigation
6. ✅ File grid display (stub)
7. ✅ Preview and info panes (stubs)
8. ✅ Application entry point
9. ✅ Unit tests written and passing

### Verified Working:
- ✅ Application launches without errors
- ✅ SQLite database created with WAL mode
- ✅ All database tables created correctly
- ✅ Database CRUD operations work
- ✅ Configuration module works
- ✅ File type detection works
- ✅ UI imports successful

### Next Steps:
1. Implement file system models
2. Add thumbnail generation
3. Connect all UI components
4. Add actual image loading/display
