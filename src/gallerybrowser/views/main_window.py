"""Main window with 3-pane layout and modern styling."""

from dataclasses import dataclass
from pathlib import Path

from PyQt6.QtCore import Qt, QSize
from PyQt6.QtGui import QAction, QCloseEvent, QKeyEvent, QKeySequence
from PyQt6.QtWidgets import (
    QComboBox,
    QFileDialog,
    QHBoxLayout,
    QInputDialog,
    QLabel,
    QLineEdit,
    QMessageBox,
    QSizePolicy,
    QSlider,
    QSplitter,
    QStatusBar,
    QTabWidget,
    QToolBar,
    QVBoxLayout,
    QWidget,
)

import qtawesome as qta

from gallerybrowser.views.batch_rename_dialog import BatchRenameDialog
from gallerybrowser.views.collections_panel import CollectionsPanel
from gallerybrowser.views.file_pane import FilePane
from gallerybrowser.views.info_pane import InfoPane
from gallerybrowser.views.preview_pane import PreviewPane
from gallerybrowser.views.settings_dialog import SettingsDialog
from gallerybrowser.views.tags_panel import TagsPanel
from gallerybrowser.views.tree_pane import TreePane
from gallerybrowser.core.file_manager import FileManager
from gallerybrowser.config import Config
from gallerybrowser.database.manager import DatabaseManager


@dataclass
class QueryState:
    """Holds the active file browsing query."""

    scope: str = "folder"
    file_type: str = "all"
    search_text: str = ""
    selected_tag: str | None = None
    selected_collection: str | None = None
    selected_rating: int | None = None
    unrated_only: bool = False


class MainWindow(QWidget):
    """Main window content with 3-pane layout."""

    def __init__(self):
        super().__init__()
        self.current_path = ""
        self.clipboard = []  # List of (file_path, operation_type) tuples
        self.file_manager = FileManager()
        self.db = DatabaseManager()
        if self.db._session_factory is None:
            self.db.initialize(str(Config.get_database_path()))
        self.query_state = QueryState()
        self.setup_ui()
        self.setup_menu()
        self.connect_actions()
        self.load_collections()
        self.load_tags()
        self._apply_saved_settings()

    def setup_ui(self):
        """Set up the user interface."""
        # Main layout
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        # Create main splitter with three panes
        self.main_splitter = QSplitter(Qt.Orientation.Horizontal)
        self.main_splitter.setHandleWidth(2)
        self.main_splitter.setChildrenCollapsible(True)

        # Left pane: Tabs for Tree and Tags
        self.left_tabs = QTabWidget()
        self.left_tabs.setMinimumWidth(200)
        self.left_tabs.setMaximumWidth(400)

        # Tree tab
        self.tree_pane = TreePane()
        self.tree_pane.folder_selected.connect(self.on_folder_selected)
        self.tree_pane.files_dropped.connect(self.on_files_dropped)
        self.left_tabs.addTab(self.tree_pane, "Folders")

        # Tags tab
        self.tags_panel = TagsPanel()
        self.tags_panel.tag_selected.connect(self.on_tag_selected)
        self.tags_panel.tag_cleared.connect(self.clear_tag_filter)
        self.tags_panel.tag_created.connect(self.on_create_tag)
        self.tags_panel.tag_deleted.connect(self.on_delete_tag)
        self.left_tabs.addTab(self.tags_panel, "Tags")

        # Collections tab
        self.collections_panel = CollectionsPanel()
        self.collections_panel.collection_selected.connect(self.on_collection_selected)
        self.collections_panel.collection_cleared.connect(self.clear_collection_filter)
        self.collections_panel.collection_created.connect(self.on_create_collection)
        self.collections_panel.collection_deleted.connect(self.on_delete_collection)
        self.left_tabs.addTab(self.collections_panel, "Collections")

        self.main_splitter.addWidget(self.left_tabs)

        # Center pane: File view
        self.file_pane = FilePane()
        self.file_pane.file_selected.connect(self.on_file_selected)
        self.file_pane.setMinimumWidth(300)
        self.main_splitter.addWidget(self.file_pane)

        # Right pane: Preview and info (nested splitter)
        self.right_splitter = QSplitter(Qt.Orientation.Vertical)
        self.right_splitter.setHandleWidth(2)

        self.preview_pane = PreviewPane()
        self.preview_pane.setMinimumHeight(200)
        self.right_splitter.addWidget(self.preview_pane)

        self.info_pane = InfoPane()
        self.info_pane.setMinimumHeight(150)
        self.right_splitter.addWidget(self.info_pane)

        self.right_splitter.setSizes([400, 300])
        self.right_splitter.setStretchFactor(0, 2)
        self.right_splitter.setStretchFactor(1, 1)

        self.right_widget = QWidget()
        right_layout = QVBoxLayout(self.right_widget)
        right_layout.setContentsMargins(0, 0, 0, 0)
        right_layout.addWidget(self.right_splitter)

        self.right_widget.setMinimumWidth(250)
        self.right_widget.setMaximumWidth(500)
        self.main_splitter.addWidget(self.right_widget)

        # Set initial splitter sizes (20%, 50%, 30%)
        self.main_splitter.setSizes([250, 650, 300])
        self.main_splitter.setStretchFactor(0, 0)
        self.main_splitter.setStretchFactor(1, 1)
        self.main_splitter.setStretchFactor(2, 0)

        # Add toolbar before the main splitter
        self.setup_toolbar()
        layout.addWidget(self.toolbar)

        layout.addWidget(self.main_splitter)

        # Create status bar
        self.status_bar = self.create_status_bar()
        layout.addWidget(self.status_bar)

    def setup_toolbar(self):
        """Create the main toolbar with icons."""
        self.toolbar = QToolBar()
        self.toolbar.setMovable(False)
        self.toolbar.setIconSize(QSize(24, 24))
        self.toolbar.setToolButtonStyle(Qt.ToolButtonStyle.ToolButtonIconOnly)

        # File operations
        self.action_new_folder = QAction(
            qta.icon("fa5s.folder-plus", color="#a0a0a0"), "New Folder", self
        )
        self.action_new_folder.setShortcut(QKeySequence("Ctrl+Shift+N"))
        self.action_new_folder.setToolTip("Create new folder (Ctrl+Shift+N)")
        self.toolbar.addAction(self.action_new_folder)

        self.toolbar.addSeparator()

        self.action_copy = QAction(qta.icon("fa5s.copy", color="#a0a0a0"), "Copy", self)
        self.action_copy.setShortcut(QKeySequence("Ctrl+C"))
        self.action_copy.setToolTip("Copy selected items (Ctrl+C)")
        self.toolbar.addAction(self.action_copy)

        self.action_cut = QAction(qta.icon("fa5s.cut", color="#a0a0a0"), "Cut", self)
        self.action_cut.setShortcut(QKeySequence("Ctrl+X"))
        self.action_cut.setToolTip("Cut selected items (Ctrl+X)")
        self.toolbar.addAction(self.action_cut)

        self.action_paste = QAction(qta.icon("fa5s.paste", color="#a0a0a0"), "Paste", self)
        self.action_paste.setShortcut(QKeySequence("Ctrl+V"))
        self.action_paste.setToolTip("Paste items (Ctrl+V)")
        self.toolbar.addAction(self.action_paste)

        self.toolbar.addSeparator()

        self.action_delete = QAction(qta.icon("fa5s.trash-alt", color="#a0a0a0"), "Delete", self)
        self.action_delete.setShortcut(QKeySequence("Delete"))
        self.action_delete.setToolTip("Move to trash (Del)")
        self.toolbar.addAction(self.action_delete)

        self.action_rename = QAction(qta.icon("fa5s.edit", color="#a0a0a0"), "Rename", self)
        self.action_rename.setShortcut(QKeySequence("F2"))
        self.action_rename.setToolTip("Rename selected item (F2)")
        self.toolbar.addAction(self.action_rename)

        self.action_refresh = QAction(qta.icon("fa5s.sync-alt", color="#a0a0a0"), "Refresh", self)
        self.action_refresh.setShortcut(QKeySequence("F5"))
        self.action_refresh.setToolTip("Refresh current folder (F5)")
        self.toolbar.addAction(self.action_refresh)

        self.toolbar.addSeparator()

        # View controls
        self.action_grid_view = QAction(
            qta.icon("fa5s.th-large", color="#2196F3"), "Grid View", self
        )
        self.action_grid_view.setCheckable(True)
        self.action_grid_view.setChecked(True)
        self.action_grid_view.setToolTip("Grid view")
        self.action_grid_view.triggered.connect(self.on_grid_view)
        self.toolbar.addAction(self.action_grid_view)

        self.action_list_view = QAction(qta.icon("fa5s.list", color="#a0a0a0"), "List View", self)
        self.action_list_view.setCheckable(True)
        self.action_list_view.setToolTip("List view")
        self.action_list_view.triggered.connect(self.on_list_view)
        self.toolbar.addAction(self.action_list_view)

        # Sort dropdown
        sort_widget = QWidget()
        sort_layout = QHBoxLayout(sort_widget)
        sort_layout.setContentsMargins(8, 0, 0, 0)
        sort_layout.setSpacing(4)

        sort_label = QLabel("Sort:")
        sort_label.setStyleSheet("color: #a0a0a0;")
        sort_layout.addWidget(sort_label)

        self.sort_combo = QComboBox()
        self.sort_combo.addItems(["Name", "Size", "Date", "Type"])
        self.sort_combo.setFixedWidth(80)
        self.sort_combo.setStyleSheet("""
            QComboBox {
                background-color: #252525;
                border: 1px solid #2a2a2a;
                border-radius: 4px;
                padding: 4px 8px;
                color: #e8e8e8;
                font-size: 12px;
            }
            QComboBox:hover {
                border: 1px solid #3a3a3a;
                background-color: #2c2c2c;
            }
            QComboBox::drop-down {
                border: none;
                width: 16px;
            }
            QComboBox QAbstractItemView {
                background-color: #252525;
                color: #e8e8e8;
                border: 1px solid #2a2a2a;
                border-radius: 4px;
                padding: 4px;
                selection-background-color: #2196F3;
                selection-color: #ffffff;
            }
        """)
        self.sort_combo.currentTextChanged.connect(self.on_sort_changed)
        sort_layout.addWidget(self.sort_combo)

        # Sort order toggle
        self.action_sort_asc = QAction(
            qta.icon("fa5s.sort-alpha-down", color="#a0a0a0"), "Ascending", self
        )
        self.action_sort_asc.setCheckable(True)
        self.action_sort_asc.setChecked(True)
        self.action_sort_asc.triggered.connect(self.on_sort_order_changed)
        self.toolbar.addAction(self.action_sort_asc)

        self.toolbar.addWidget(sort_widget)

        self.toolbar.addSeparator()

        # Filter by type
        filter_widget = QWidget()
        filter_layout = QHBoxLayout(filter_widget)
        filter_layout.setContentsMargins(8, 0, 0, 0)
        filter_layout.setSpacing(4)

        filter_label = QLabel("Show:")
        filter_label.setStyleSheet("color: #a0a0a0;")
        filter_layout.addWidget(filter_label)

        self.filter_combo = QComboBox()
        self.filter_combo.addItems(["All", "Images", "Videos", "Sequences"])
        self.filter_combo.setFixedWidth(80)
        self.filter_combo.setStyleSheet("""
            QComboBox {
                background-color: #252525;
                border: 1px solid #2a2a2a;
                border-radius: 4px;
                padding: 4px 8px;
                color: #e8e8e8;
                font-size: 12px;
            }
            QComboBox:hover {
                border: 1px solid #3a3a3a;
                background-color: #2c2c2c;
            }
            QComboBox::drop-down {
                border: none;
                width: 16px;
            }
            QComboBox QAbstractItemView {
                background-color: #252525;
                color: #e8e8e8;
                border: 1px solid #2a2a2a;
                border-radius: 4px;
                padding: 4px;
                selection-background-color: #2196F3;
                selection-color: #ffffff;
            }
        """)
        self.filter_combo.currentTextChanged.connect(self.on_filter_changed)
        filter_layout.addWidget(self.filter_combo)

        self.toolbar.addWidget(filter_widget)

        self.toolbar.addSeparator()

        scope_widget = QWidget()
        scope_layout = QHBoxLayout(scope_widget)
        scope_layout.setContentsMargins(8, 0, 0, 0)
        scope_layout.setSpacing(4)
        scope_label = QLabel("Scope:")
        scope_label.setStyleSheet("color: #a0a0a0;")
        scope_layout.addWidget(scope_label)
        self.scope_combo = QComboBox()
        self.scope_combo.addItems(["Folder", "Library"])
        self.scope_combo.setFixedWidth(90)
        self.scope_combo.currentTextChanged.connect(self.on_scope_changed)
        scope_layout.addWidget(self.scope_combo)
        self.toolbar.addWidget(scope_widget)

        rating_widget = QWidget()
        rating_layout = QHBoxLayout(rating_widget)
        rating_layout.setContentsMargins(8, 0, 0, 0)
        rating_layout.setSpacing(4)
        rating_label = QLabel("Rating:")
        rating_label.setStyleSheet("color: #a0a0a0;")
        rating_layout.addWidget(rating_label)
        self.rating_filter_combo = QComboBox()
        self.rating_filter_combo.addItems(
            ["Any Rating", "Unrated", "0 Stars", "1 Star", "2 Stars", "3 Stars", "4 Stars", "5 Stars"]
        )
        self.rating_filter_combo.setFixedWidth(110)
        self.rating_filter_combo.currentTextChanged.connect(self.on_rating_filter_changed)
        rating_layout.addWidget(self.rating_filter_combo)
        self.toolbar.addWidget(rating_widget)

        self.toolbar.addSeparator()

        # Sequence collapse toggle
        self.action_collapse_sequences = QAction(
            qta.icon("fa5s.layer-group", color="#a0a0a0"), "Collapse Sequences", self
        )
        self.action_collapse_sequences.setCheckable(True)
        self.action_collapse_sequences.setChecked(True)
        self.action_collapse_sequences.setToolTip("Collapse image sequences into single entries")
        self.action_collapse_sequences.triggered.connect(self.on_collapse_sequences_toggled)
        self.toolbar.addAction(self.action_collapse_sequences)

        self.toolbar.addSeparator()

        # Thumbnail size slider
        size_widget = QWidget()
        size_layout = QHBoxLayout(size_widget)
        size_layout.setContentsMargins(0, 0, 0, 0)
        size_layout.setSpacing(8)

        size_icon = QLabel()
        size_icon.setPixmap(qta.icon("fa5s.image", color="#666666").pixmap(16, 16))
        size_layout.addWidget(size_icon)

        self.size_slider = QSlider(Qt.Orientation.Horizontal)
        self.size_slider.setMinimum(64)
        self.size_slider.setMaximum(512)
        self.size_slider.setValue(128)
        self.size_slider.setFixedWidth(120)
        self.size_slider.setToolTip("Thumbnail size")
        self.size_slider.valueChanged.connect(self.on_thumbnail_size_changed)
        size_layout.addWidget(self.size_slider)

        self.toolbar.addWidget(size_widget)

        self.toolbar.addSeparator()

        # Search
        self.search_edit = QLineEdit()
        self.search_edit.setPlaceholderText("Search files...")
        self.search_edit.setFixedWidth(200)
        self.search_edit.setClearButtonEnabled(True)
        self.search_edit.textChanged.connect(self.on_search_text_changed)
        self.toolbar.addWidget(self.search_edit)

        self.toolbar.addSeparator()

        # Toggle preview pane
        self.action_toggle_preview = QAction(
            qta.icon("fa5s.eye", color="#a0a0a0"), "Toggle Preview", self
        )
        self.action_toggle_preview.setCheckable(True)
        self.action_toggle_preview.setChecked(True)
        self.action_toggle_preview.setToolTip("Show/hide preview pane")
        self.action_toggle_preview.triggered.connect(self.toggle_preview_pane)
        self.toolbar.addAction(self.action_toggle_preview)

        # Settings
        self.action_settings = QAction(qta.icon("fa5s.cog", color="#a0a0a0"), "Settings", self)
        self.action_settings.setToolTip("Settings")
        self.action_settings.triggered.connect(self.on_settings)
        self.toolbar.addAction(self.action_settings)

        # Batch rename
        self.action_batch_rename = QAction(
            qta.icon("fa5s.edit", color="#a0a0a0"), "Batch Rename", self
        )
        self.action_batch_rename.setToolTip("Batch rename selected files")
        self.action_batch_rename.triggered.connect(self.on_batch_rename)
        self.toolbar.addAction(self.action_batch_rename)

        # Add spacer to push remaining items to the right
        spacer = QWidget()
        spacer.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Preferred)
        self.toolbar.addWidget(spacer)

    def setup_menu(self):
        """Set up the menu bar."""
        # Note: Menu bar is added by QMainWindow, not in this widget
        pass

    def connect_actions(self):
        """Connect toolbar actions to handlers."""
        # File operations
        self.action_copy.triggered.connect(self.on_copy)
        self.action_cut.triggered.connect(self.on_cut)
        self.action_paste.triggered.connect(self.on_paste)
        self.action_delete.triggered.connect(self.on_delete)
        self.action_rename.triggered.connect(self.on_rename)
        self.action_refresh.triggered.connect(self.on_refresh)

        # Connect file manager signals
        self.file_manager.operation_completed.connect(self.on_operation_completed)
        self.file_manager.operation_failed.connect(self.on_operation_failed)

        # Connect file pane selection changes
        self.file_pane.files_selected.connect(self.on_files_selected)

        # Connect file pane context menu actions
        self.file_pane.action_open.connect(self._on_ctx_open)
        self.file_pane.action_cut.connect(lambda _: self.on_cut())
        self.file_pane.action_copy.connect(lambda _: self.on_copy())
        self.file_pane.action_rename.connect(lambda _: self.on_rename())
        self.file_pane.action_batch_rename.connect(lambda _: self.on_batch_rename())
        self.file_pane.action_delete.connect(lambda _: self.on_delete())
        self.file_pane.action_add_favorite.connect(self._on_add_favorite_from_ctx)
        self.file_pane.action_add_tag.connect(self.on_assign_tag)
        self.file_pane.action_remove_tag.connect(self.on_remove_tag)
        self.file_pane.action_add_to_collection.connect(self.on_add_to_collection)
        self.file_pane.action_remove_from_collection.connect(self.on_remove_from_collection)
        self.file_pane.action_set_rating.connect(self.on_set_rating)
        self.file_pane.action_clear_rating.connect(self.on_clear_rating)

    def on_copy(self):
        """Copy selected files to clipboard."""
        selected = self.file_pane.get_selected_files()
        if selected:
            self.clipboard = [(path, "copy") for path in selected]
            self.status_bar.showMessage(f"Copied {len(selected)} item(s) to clipboard", 2000)

    def on_cut(self):
        """Cut selected files to clipboard."""
        selected = self.file_pane.get_selected_files()
        if selected:
            self.clipboard = [(path, "cut") for path in selected]
            self.status_bar.showMessage(f"Cut {len(selected)} item(s) to clipboard", 2000)

    def on_paste(self):
        """Paste files from clipboard to current folder."""
        if not self.clipboard or not self.current_path:
            return

        from pathlib import Path

        dest_folder = Path(self.current_path)

        for file_path, operation in self.clipboard:
            source = Path(file_path)
            dest = dest_folder / source.name

            # Handle duplicate names
            counter = 1
            original_dest = dest
            while dest.exists():
                stem = original_dest.stem
                suffix = original_dest.suffix
                dest = dest_folder / f"{stem} ({counter}){suffix}"
                counter += 1

            if operation == "copy":
                self.file_manager.copy(str(source), str(dest))
            else:  # cut
                self.file_manager.move(str(source), str(dest))

        # Clear clipboard if it was a cut operation
        if self.clipboard and self.clipboard[0][1] == "cut":
            self.clipboard.clear()

        # Refresh the view
        self.refresh_view()

    def on_delete(self):
        """Delete selected files (with confirmation)."""
        selected = self.file_pane.get_selected_files()
        if selected:
            count = len(selected)
            reply = QMessageBox.question(
                self,
                "Confirm Delete",
                f"Move {count} item{'s' if count > 1 else ''} to trash?",
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
                QMessageBox.StandardButton.No,
            )
            if reply == QMessageBox.StandardButton.Yes:
                for file_path in selected:
                    self.file_manager.delete(file_path, use_trash=True)
                self.refresh_view()

    def on_rename(self):
        """Rename selected file."""
        selected = self.file_pane.get_selected_files()
        if len(selected) == 1:
            # Show rename dialog for single file
            from PyQt6.QtWidgets import QInputDialog
            from pathlib import Path

            file_path = selected[0]
            current_name = Path(file_path).name

            new_name, ok = QInputDialog.getText(self, "Rename", "New name:", text=current_name)

            if ok and new_name and new_name != current_name:
                self.file_manager.rename(file_path, new_name)
                self.refresh_view()

    def on_refresh(self):
        """Refresh the current folder view."""
        self.refresh_view()

    def open_folder_dialog(self):
        """Open a folder picker and navigate to the selected folder."""
        folder = QFileDialog.getExistingDirectory(
            self,
            "Open Folder",
            self.current_path or str(Path.home()),
        )
        if folder:
            self.on_folder_selected(folder)

    def refresh_view(self):
        """Refresh the file view using the current query state."""
        source_label = self.current_path or "No folder selected"
        previous_selection = set(self.file_pane.get_selected_files())

        if self.query_state.scope == "folder":
            if not self.current_path:
                self.file_pane.set_files([], current_folder="", source_label="No folder selected")
                self.path_label.setText("No folder selected")
                self.update_status_count()
                return

            base_files = self.file_pane.scan_folder(self.current_path)
            filtered_files = self._apply_library_filters_to_folder(base_files)
            source_label = self._build_source_label(self.current_path)
            self.file_pane.set_files(
                filtered_files,
                current_folder=self.current_path,
                source_label=source_label,
            )
        else:
            library_files = self._build_library_results()
            source_label = self._build_source_label("Library")
            self.file_pane.set_files(library_files, current_folder="", source_label=source_label)

        if previous_selection:
            visible_paths = {item["path"] for item in self.file_pane.filtered_files}
            retained_selection = [
                file_path for file_path in previous_selection if file_path in visible_paths
            ]
            if retained_selection:
                self.file_pane.set_selection(retained_selection)

        self.path_label.setText(source_label)
        self.update_status_count()

    def _current_source_prefix(self) -> str:
        """Return the base label for the current results source."""
        if self.query_state.scope == "library":
            return "Library"
        return self.current_path or "No folder selected"

    def _sync_source_label(self):
        """Update the path/source label without rebuilding the current results."""
        self.path_label.setText(self._build_source_label(self._current_source_prefix()))

    def _build_source_label(self, prefix: str) -> str:
        """Build a human-readable source label for the status bar."""
        filters = []
        if self.query_state.selected_tag:
            filters.append(f"tag:{self.query_state.selected_tag}")
        if self.query_state.selected_collection:
            filters.append(f"collection:{self.query_state.selected_collection}")
        if self.query_state.file_type != "all":
            filters.append(f"type:{self.query_state.file_type}")
        if self.query_state.search_text:
            filters.append(f"search:{self.query_state.search_text}")
        if self.query_state.unrated_only:
            filters.append("rating:unrated")
        elif self.query_state.selected_rating is not None:
            filters.append(f"rating:{self.query_state.selected_rating}")
        if not filters:
            return prefix
        return f"{prefix} [{', '.join(filters)}]"

    def _collect_filtered_paths(self, folder_path: str | None = None) -> set[str] | None:
        """Return the DB-backed file paths allowed by the active metadata filters."""
        allowed_paths = None

        def _intersect(paths: set[str]):
            nonlocal allowed_paths
            allowed_paths = paths if allowed_paths is None else allowed_paths & paths

        if self.query_state.selected_tag:
            _intersect(
                {
                    file_record.path
                    for file_record in self.db.get_files_by_tag(
                        self.query_state.selected_tag, folder_path=folder_path
                    )
                }
            )

        if self.query_state.selected_collection:
            _intersect(
                {
                    file_record.path
                    for file_record in self.db.get_files_by_collection(
                        self.query_state.selected_collection,
                        folder_path=folder_path,
                    )
                }
            )

        if self.query_state.unrated_only:
            _intersect(
                {
                    file_record.path for file_record in self.db.get_unrated_files(folder_path=folder_path)
                }
            )
        elif self.query_state.selected_rating is not None:
            _intersect(
                {
                    file_record.path
                    for file_record in self.db.get_files_by_rating(
                        self.query_state.selected_rating,
                        folder_path=folder_path,
                    )
                }
            )

        return allowed_paths

    def _apply_library_filters_to_folder(self, files: list[dict]) -> list[dict]:
        """Apply DB-backed tag/collection/rating filters to folder scan results."""
        allowed_paths = self._collect_filtered_paths(folder_path=self.current_path)
        if allowed_paths is None:
            return files
        return [file_info for file_info in files if file_info["path"] in allowed_paths]

    def _file_record_to_view_item(self, file_record) -> dict | None:
        """Convert a DB file record into a FilePane view row."""
        path_obj = Path(file_record.path)
        if not path_obj.exists():
            return None

        modified = file_record.modified_at.timestamp() if file_record.modified_at else path_obj.stat().st_mtime
        return {
            "path": file_record.path,
            "name": file_record.filename or path_obj.name,
            "type": file_record.file_type or Config.get_file_type(file_record.path),
            "size": file_record.size_bytes or path_obj.stat().st_size,
            "modified": modified,
        }

    def _build_library_results(self) -> list[dict]:
        """Build explicit library-wide result rows from DB-backed filters."""
        result_files = []
        allowed_paths = self._collect_filtered_paths()

        db_files = []
        if allowed_paths is None:
            db_files = self.db.get_all_files()
        else:
            for file_path in sorted(allowed_paths):
                file_record = self.db.get_file_by_path(file_path)
                if file_record is not None:
                    db_files.append(file_record)

        for file_record in db_files:
            view_item = self._file_record_to_view_item(file_record)
            if view_item is not None:
                result_files.append(view_item)

        return result_files

    def _on_ctx_open(self, file_paths):
        """Open files from context menu with default application."""
        import subprocess

        for fp in file_paths:
            subprocess.Popen(["xdg-open", fp])

    def _on_add_favorite_from_ctx(self, folder_path):
        """Add a folder to favorites via context menu."""
        from pathlib import Path

        self.tree_pane.add_favorite(folder_path)
        self.status_bar.showMessage(f"Added to favorites: {Path(folder_path).name}", 3000)

    def on_files_selected(self, file_paths):
        """Handle file selection changes."""
        # Update action states based on selection
        has_selection = len(file_paths) > 0
        self.action_copy.setEnabled(has_selection)
        self.action_cut.setEnabled(has_selection)
        self.action_delete.setEnabled(has_selection)
        self.action_rename.setEnabled(len(file_paths) == 1)
        self.action_batch_rename.setEnabled(len(file_paths) > 1)

        # Update status bar with selection info
        from pathlib import Path

        if len(file_paths) == 0:
            self.update_status_count()
        elif len(file_paths) == 1:
            # Show file name for single selection
            self.status_label.setText(f"Selected: {Path(file_paths[0]).name}")
        else:
            # Show count for multiple selection
            total_size = 0
            for path in file_paths:
                try:
                    total_size += Path(path).stat().st_size
                except OSError:
                    pass
            size_str = self._format_size(total_size)
            self.status_label.setText(f"Selected {len(file_paths)} items ({size_str})")

    def _format_size(self, size_bytes):
        """Format file size for display."""
        for unit in ["B", "KB", "MB", "GB"]:
            if size_bytes < 1024.0:
                return f"{size_bytes:.1f} {unit}"
            size_bytes /= 1024.0
        return f"{size_bytes:.1f} TB"

    def on_operation_completed(self, message):
        """Handle file operation completion."""
        self.status_bar.showMessage(message, 3000)

    def on_operation_failed(self, operation, error):
        """Handle file operation failure."""
        from PyQt6.QtWidgets import QMessageBox

        QMessageBox.critical(self, "Operation Failed", f"{operation}:\n{error}")

    def create_status_bar(self) -> QStatusBar:
        """Create the status bar."""
        status_bar = QStatusBar()
        status_bar.setFixedHeight(24)
        status_bar.setSizeGripEnabled(False)

        # Left side: Current path
        self.path_label = QLabel("No folder selected")
        self.path_label.setStyleSheet("padding: 0px; margin: 0px;")
        status_bar.addWidget(self.path_label, stretch=1)

        # Middle: Selection info
        self.status_label = QLabel("")
        self.status_label.setStyleSheet("padding: 0px; margin: 0px; color: rgba(255,255,255,0.7);")
        status_bar.addWidget(self.status_label)

        # Right side: Item count
        self.count_label = QLabel("0 items")
        self.count_label.setStyleSheet("padding: 0px; margin: 0px;")
        status_bar.addPermanentWidget(self.count_label)

        return status_bar

    def on_folder_selected(self, folder_path: str):
        """Handle folder selection from tree pane."""
        self.current_path = folder_path
        self.refresh_view()
        # Track in recents
        self.tree_pane.add_recent(folder_path)
        # Sync tree view: expand filesystem tree to this folder
        self.tree_pane.navigate_to_path(folder_path)

    def on_files_dropped(self, target_folder: str, file_paths: list, is_copy: bool = False):
        """Handle files dropped on tree item.

        By default files are *moved*.  Hold Ctrl during drop to *copy*.
        """
        from pathlib import Path

        target = Path(target_folder)
        for file_path in file_paths:
            source = Path(file_path)
            dest = target / source.name

            # Handle duplicate names
            counter = 1
            original_dest = dest
            while dest.exists():
                stem = original_dest.stem
                suffix = original_dest.suffix
                dest = target / f"{stem} ({counter}){suffix}"
                counter += 1

            if is_copy:
                self.file_manager.copy(str(source), str(dest))
            else:
                self.file_manager.move(str(source), str(dest))

        # Refresh views
        if self.current_path == target_folder or (
            not is_copy and self.current_path in {str(Path(p).parent) for p in file_paths}
        ):
            self.refresh_view()

    def on_file_selected(self, file_path: str):
        """Handle file selection from file pane."""
        # Look up the full file_info dict so PreviewPane can detect sequences
        file_info = None
        for f in self.file_pane.filtered_files:
            if f["path"] == file_path:
                file_info = f
                break
        if file_info is None:
            file_info = {
                "path": file_path,
                "name": Path(file_path).name,
                "type": Config.get_file_type(file_path),
            }
        self.preview_pane.set_file(file_path, file_info=file_info)
        self.info_pane.set_file(file_path, file_info=file_info)

    def on_thumbnail_size_changed(self, value: int):
        """Handle thumbnail size slider change."""
        self.file_pane.set_thumbnail_size(value)

    def toggle_preview_pane(self, checked: bool):
        """Toggle visibility of the preview pane."""
        self.right_widget.setVisible(checked)

    def on_collapse_sequences_toggled(self, checked: bool):
        """Toggle sequence collapsing in the file pane."""
        self.file_pane.set_collapse_sequences(checked)
        self.update_status_count()

    def update_status_count(self):
        """Update the status bar item count."""
        files = self.file_pane.files
        count = len(files)
        seq_count = sum(1 for f in files if f.get("type") == "sequence")
        if seq_count > 0:
            label = f"{count} item{'s' if count != 1 else ''} ({seq_count} sequence{'s' if seq_count != 1 else ''})"
        else:
            label = f"{count} item{'s' if count != 1 else ''}"
        self.count_label.setText(label)

    def on_grid_view(self):
        """Switch to grid view."""
        self.file_pane.set_view_mode("grid")
        self.action_grid_view.setChecked(True)
        self.action_list_view.setChecked(False)

    def on_list_view(self):
        """Switch to list view."""
        self.file_pane.set_view_mode("list")
        self.action_grid_view.setChecked(False)
        self.action_list_view.setChecked(True)

    def on_search_text_changed(self, text: str):
        """Handle search text change."""
        self.query_state.search_text = text
        self.file_pane.set_search_text(text)
        self._sync_source_label()

    def on_settings(self):
        """Open settings dialog."""
        dialog = SettingsDialog(self)
        if dialog.exec():
            settings = dialog.get_settings()
            self._apply_settings(settings)
            self.status_bar.showMessage("Settings saved", 3000)

    def _apply_saved_settings(self):
        """Load settings from disk and apply them at startup."""
        settings = Config.load_settings()
        self._apply_settings(settings)

    def _apply_settings(self, settings: dict):
        """Apply a settings dict to the running application."""
        # Thumbnail size
        thumb_size = settings.get("thumbnail_size", 128)
        self.file_pane.set_thumbnail_size(thumb_size)
        # Sync the toolbar slider
        self.size_slider.blockSignals(True)
        self.size_slider.setValue(thumb_size)
        self.size_slider.blockSignals(False)

        # RAM preview budget
        ram_mb = settings.get("ram_preview_mb", 2048)
        ram_bytes = ram_mb * 1024 * 1024
        # Update the sequence player in the preview pane
        seq_preview = getattr(self.preview_pane, "_sequence_preview", None)
        if seq_preview is not None:
            player = getattr(seq_preview, "_player", None)
            if player is not None:
                player.set_max_cache_bytes(ram_bytes)

    def on_create_tag(self, tag_name: str):
        """Create a tag and refresh the panel."""
        if not tag_name:
            return
        if self.db.get_tag_by_name(tag_name) is None:
            self.db.add_tag(tag_name)
        self.load_tags()

    def on_delete_tag(self, tag_name: str):
        """Delete a tag and refresh the panel."""
        tag = self.db.get_tag_by_name(tag_name)
        if tag is None:
            return
        self.db.delete_tag(tag.id)
        if self.query_state.selected_tag == tag_name:
            self.query_state.selected_tag = None
        self.load_tags()
        self.tags_panel.set_active_tag(self.query_state.selected_tag)
        self.refresh_view()

    def on_create_collection(self, collection_name: str):
        """Create a collection and refresh the panel."""
        if not collection_name:
            return
        if self.db.get_collection_by_name(collection_name) is None:
            self.db.add_collection(collection_name)
        self.load_collections()

    def on_delete_collection(self, collection_name: str):
        """Delete a collection and refresh the panel."""
        collection = self.db.get_collection_by_name(collection_name)
        if collection is None:
            return
        self.db.delete_collection(collection.id)
        if self.query_state.selected_collection == collection_name:
            self.query_state.selected_collection = None
        self.load_collections()
        self.collections_panel.set_active_collection(self.query_state.selected_collection)
        self.refresh_view()

    def _refresh_selected_file_details(self):
        """Refresh the info pane for the current single-file selection when possible."""
        selected = self.file_pane.get_selected_files()
        if len(selected) != 1:
            return
        file_path = selected[0]
        file_info = next(
            (item for item in self.file_pane.filtered_files if item["path"] == file_path),
            None,
        )
        self.info_pane.set_file(file_path, file_info=file_info)

    def _apply_to_selected_files(
        self,
        operation,
        *,
        reload_tags: bool = False,
        reload_collections: bool = False,
    ) -> list[str]:
        """Apply an operation to the current selection and refresh dependent UI."""
        selected = self.file_pane.get_selected_files()
        if not selected:
            return []

        for file_path in selected:
            operation(file_path)

        if reload_tags:
            self.load_tags()
        if reload_collections:
            self.load_collections()
        self._refresh_selected_file_details()
        self.refresh_view()
        return selected

    def on_assign_tag(self):
        """Assign a tag to the selected files."""
        selected = self.file_pane.get_selected_files()
        if not selected:
            return

        tag_names = [tag.name for tag in self.db.get_all_tags()]
        if tag_names:
            tag_name, ok = QInputDialog.getItem(
                self,
                "Assign Tag",
                "Select tag:",
                tag_names,
                editable=True,
            )
        else:
            tag_name, ok = QInputDialog.getText(self, "Assign Tag", "Tag name:")
        if ok and tag_name:
            normalized_tag = tag_name.strip()
            self._apply_to_selected_files(
                lambda file_path: self.db.assign_tag_to_file(file_path, normalized_tag),
                reload_tags=True,
            )

    def on_remove_tag(self):
        """Remove a tag from the selected files."""
        selected = self.file_pane.get_selected_files()
        if not selected:
            return

        available_tags = sorted({tag.name for file_path in selected for tag in self.db.get_tags_for_file(file_path)})
        if not available_tags:
            QMessageBox.information(self, "Remove Tag", "No tags are assigned to the selected files.")
            return

        tag_name, ok = QInputDialog.getItem(self, "Remove Tag", "Tag:", available_tags, editable=False)
        if ok and tag_name:
            self._apply_to_selected_files(
                lambda file_path: self.db.remove_tag_from_file(file_path, tag_name),
                reload_tags=True,
            )

    def on_add_to_collection(self):
        """Add selected files to a collection."""
        selected = self.file_pane.get_selected_files()
        if not selected:
            return

        collection_names = [collection.name for collection in self.db.get_all_collections()]
        if collection_names:
            collection_name, ok = QInputDialog.getItem(
                self,
                "Add to Collection",
                "Select collection:",
                collection_names,
                editable=True,
            )
        else:
            collection_name, ok = QInputDialog.getText(
                self, "Add to Collection", "Collection name:"
            )
        if ok and collection_name:
            normalized_collection = collection_name.strip()
            self._apply_to_selected_files(
                lambda file_path: self.db.add_file_to_collection(file_path, normalized_collection),
                reload_collections=True,
            )

    def on_remove_from_collection(self):
        """Remove selected files from a collection."""
        selected = self.file_pane.get_selected_files()
        if not selected:
            return

        available_collections = sorted(
            {
                collection.name
                for file_path in selected
                for collection in self.db.get_collections_for_file(file_path)
            }
        )
        if not available_collections:
            QMessageBox.information(
                self,
                "Remove from Collection",
                "No collections are assigned to the selected files.",
            )
            return

        collection_name, ok = QInputDialog.getItem(
            self,
            "Remove from Collection",
            "Collection:",
            available_collections,
            editable=False,
        )
        if ok and collection_name:
            self._apply_to_selected_files(
                lambda file_path: self.db.remove_file_from_collection(file_path, collection_name)
            )

    def on_set_rating(self, rating_value: int):
        """Set the rating on selected files."""
        selected = self.file_pane.get_selected_files()
        if not selected:
            return
        self._apply_to_selected_files(lambda file_path: self.db.set_rating(file_path, rating_value))

    def on_clear_rating(self):
        """Clear the rating from selected files."""
        selected = self.file_pane.get_selected_files()
        if not selected:
            return
        self._apply_to_selected_files(self.db.clear_rating)

    def on_batch_rename(self):
        """Open batch rename dialog."""
        selected = self.file_pane.get_selected_files()
        if len(selected) < 2:
            from PyQt6.QtWidgets import QMessageBox

            QMessageBox.information(
                self, "Batch Rename", "Please select at least 2 files to batch rename."
            )
            return

        dialog = BatchRenameDialog(selected, self)
        if dialog.exec():
            renames = dialog.get_renames()
            for old_path, new_name in renames:
                self.file_manager.rename(old_path, new_name)
            self.refresh_view()
            self.status_bar.showMessage(f"Renamed {len(renames)} files", 3000)

    def on_tag_selected(self, tag_name: str):
        """Handle tag selection - filter files by tag."""
        self.query_state.selected_tag = tag_name
        self.tags_panel.set_active_tag(tag_name)
        self.refresh_view()
        self.status_bar.showMessage(f"Filter by tag: {tag_name}", 3000)

    def clear_tag_filter(self):
        """Clear tag filtering."""
        self.query_state.selected_tag = None
        self.tags_panel.set_active_tag(None)
        self.refresh_view()

    def on_collection_selected(self, collection_name: str):
        """Handle collection selection."""
        self.query_state.selected_collection = collection_name
        self.collections_panel.set_active_collection(collection_name)
        self.refresh_view()
        self.status_bar.showMessage(f"Filter by collection: {collection_name}", 3000)

    def clear_collection_filter(self):
        """Clear collection filtering."""
        self.query_state.selected_collection = None
        self.collections_panel.set_active_collection(None)
        self.refresh_view()

    def load_tags(self):
        """Load tags from database."""
        self.tags_panel.set_tags(self.db.get_all_tags())
        self.tags_panel.set_active_tag(self.query_state.selected_tag)

    def load_collections(self):
        """Load collections from database."""
        self.collections_panel.set_collections(self.db.get_all_collections())
        self.collections_panel.set_active_collection(self.query_state.selected_collection)

    def on_sort_changed(self, sort_by: str):
        """Handle sort criteria change."""
        self.file_pane.set_sort_by(sort_by.lower())

    def on_sort_order_changed(self, ascending: bool):
        """Handle sort order change."""
        self.file_pane.set_sort_order(ascending)

    def on_filter_changed(self, filter_text: str):
        """Handle filter by type change."""
        filter_map = {"All": "all", "Images": "image", "Videos": "video", "Sequences": "sequence"}
        self.query_state.file_type = filter_map.get(filter_text, "all")
        self.file_pane.set_filter_type(self.query_state.file_type)
        self._sync_source_label()

    def on_scope_changed(self, text: str):
        """Switch between folder-scoped and library-scoped metadata queries."""
        self.query_state.scope = "library" if text == "Library" else "folder"
        self.refresh_view()

    def on_rating_filter_changed(self, text: str):
        """Set the active rating filter."""
        if text == "Any Rating":
            self.query_state.selected_rating = None
            self.query_state.unrated_only = False
        elif text == "Unrated":
            self.query_state.selected_rating = None
            self.query_state.unrated_only = True
        else:
            self.query_state.unrated_only = False
            self.query_state.selected_rating = int(text.split()[0])
        self.refresh_view()

    def keyPressEvent(self, event: QKeyEvent):
        """Handle global keyboard shortcuts."""
        key = event.key()

        # Frame stepping: comma = back, period = forward
        if key in (Qt.Key.Key_Comma, Qt.Key.Key_Period):
            direction = 1 if key == Qt.Key.Key_Period else -1
            stack_idx = self.preview_pane._stack.currentIndex()
            # Video preview (stack index 1)
            if stack_idx == 1:
                self.preview_pane._video_preview.step_frame(direction)
                return
            # Sequence preview (stack index 2)
            if stack_idx == 2:
                self.preview_pane._sequence_preview.step_frame(direction)
                return

        super().keyPressEvent(event)

    def closeEvent(self, event: QCloseEvent):
        """Clean up background workers when closing the main widget directly."""
        try:
            self.preview_pane.cleanup()
        except Exception:
            pass
        try:
            self.info_pane.cleanup()
        except Exception:
            pass
        super().closeEvent(event)
