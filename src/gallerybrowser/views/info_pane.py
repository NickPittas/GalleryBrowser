"""Info pane for displaying file metadata with tabs."""

import threading

from PyQt6.QtCore import QThread, Qt, pyqtSignal
from PyQt6.QtWidgets import (
    QLabel,
    QScrollArea,
    QTabWidget,
    QTreeWidget,
    QTreeWidgetItem,
    QVBoxLayout,
    QWidget,
)

from gallerybrowser.config import Config
from gallerybrowser.core.metadata import MetadataExtractor
from gallerybrowser.database.manager import DatabaseManager


class _InfoLoaderWorker(QThread):
    """Background metadata loader for the info pane."""

    info_ready = pyqtSignal(str, dict)

    def __init__(self, parent=None):
        super().__init__(parent)
        self._db = DatabaseManager()
        self._lock = threading.Lock()
        self._event = threading.Event()
        self._stop = False
        self._file_path = ""

    def request(self, file_path: str):
        """Queue a metadata load request."""
        with self._lock:
            self._file_path = file_path
        self._event.set()

    def stop(self):
        """Stop the worker thread."""
        self._stop = True
        self._event.set()

    def run(self):
        """Process metadata requests."""
        while not self._stop:
            self._event.wait()
            self._event.clear()
            if self._stop:
                break

            with self._lock:
                file_path = self._file_path

            file_type = Config.get_file_type(file_path)
            if file_type == "image":
                metadata = MetadataExtractor.extract_image_metadata(file_path)
            elif file_type == "video":
                metadata = MetadataExtractor.extract_video_metadata(file_path)
            else:
                metadata = {}

            payload = {
                "file_type": file_type,
                "metadata": metadata,
                "tags": [tag.name for tag in self._db.get_tags_for_file(file_path)],
                "collections": [
                    collection.name for collection in self._db.get_collections_for_file(file_path)
                ],
                "rating": self._db.get_rating_for_file(file_path),
            }
            self.info_ready.emit(file_path, payload)


class InfoPane(QWidget):
    """Right bottom pane for file information with tabs."""

    def __init__(self):
        super().__init__()
        self._db = DatabaseManager()
        self._setup_ui()
        self._current_file = None
        self._loader = _InfoLoaderWorker(self)
        self._loader.info_ready.connect(self._on_info_loaded)
        self._loader.start()

    def _setup_ui(self):
        """Set up the UI."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        # Tab widget for different info categories
        self._tabs = QTabWidget()
        self._tabs.setStyleSheet("""
            QTabWidget::pane {
                border: none;
                background-color: #181818;
                border-top: 1px solid #2a2a2a;
            }
            QTabBar::tab {
                background-color: transparent;
                color: #a0a0a0;
                padding: 8px 16px;
                margin-right: 0px;
                border-bottom: 2px solid transparent;
                font-size: 11px;
                font-weight: 500;
            }
            QTabBar::tab:selected {
                color: #e8e8e8;
                border-bottom: 2px solid #2196F3;
            }
            QTabBar::tab:hover:!selected {
                color: #e8e8e8;
                background-color: #2c2c2c;
            }
        """)

        # General tab
        self._general_widget = QScrollArea()
        self._general_widget.setWidgetResizable(True)
        self._general_content = QWidget()
        self._general_layout = QVBoxLayout(self._general_content)
        self._general_layout.setAlignment(Qt.AlignmentFlag.AlignTop)
        self._general_layout.setSpacing(4)
        self._general_layout.setContentsMargins(8, 8, 8, 8)
        self._general_widget.setWidget(self._general_content)
        self._tabs.addTab(self._general_widget, "General")

        # Technical tab
        self._technical_widget = QScrollArea()
        self._technical_widget.setWidgetResizable(True)
        self._technical_content = QWidget()
        self._technical_layout = QVBoxLayout(self._technical_content)
        self._technical_layout.setAlignment(Qt.AlignmentFlag.AlignTop)
        self._technical_layout.setSpacing(4)
        self._technical_layout.setContentsMargins(8, 8, 8, 8)
        self._technical_widget.setWidget(self._technical_content)
        self._tabs.addTab(self._technical_widget, "Technical")

        # EXIF tab (for images)
        self._exif_tree = QTreeWidget()
        self._exif_tree.setHeaderLabels(["Tag", "Value"])
        self._exif_tree.setColumnWidth(0, 150)
        self._exif_tree.setStyleSheet("""
            QTreeWidget {
                background-color: #181818;
                border: none;
                outline: none;
                padding: 4px;
            }
            QTreeWidget::item {
                padding: 4px 8px;
                border-radius: 4px;
                color: #e8e8e8;
            }
            QTreeWidget::item:selected {
                background-color: #1a3a5c;
            }
            QTreeWidget::item:hover:!selected {
                background-color: #2c2c2c;
            }
            QHeaderView::section {
                background-color: #1e1e1e;
                padding: 6px 12px;
                border: none;
                border-bottom: 1px solid #2a2a2a;
                color: #a0a0a0;
                font-weight: 600;
                font-size: 11px;
            }
        """)
        self._tabs.addTab(self._exif_tree, "EXIF")

        self._library_widget = QScrollArea()
        self._library_widget.setWidgetResizable(True)
        self._library_content = QWidget()
        self._library_layout = QVBoxLayout(self._library_content)
        self._library_layout.setAlignment(Qt.AlignmentFlag.AlignTop)
        self._library_layout.setSpacing(4)
        self._library_layout.setContentsMargins(8, 8, 8, 8)
        self._library_widget.setWidget(self._library_content)
        self._tabs.addTab(self._library_widget, "Library")

        layout.addWidget(self._tabs)

    def set_file(self, file_path: str, file_info: dict | None = None):
        """Set the file to display information for."""
        self._current_file = file_path

        # Clear previous content
        self._clear_tabs()
        file_type = Config.get_file_type(file_path)
        if file_type in {"image", "video"}:
            self._display_loading_state(file_path)
            self._loader.request(file_path)
        else:
            self.set_basic_file(file_path)

    def set_basic_file(self, file_path: str, message: str | None = None):
        """Show only lightweight file info without metadata/database lookups."""
        self._current_file = file_path
        self._clear_tabs()
        self._display_basic_info(file_path)
        if message:
            self._add_info_row(self._technical_layout, "Status:", message)

    def _clear_tabs(self):
        """Clear all tab content."""
        # Clear general tab
        while self._general_layout.count():
            item = self._general_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()

        # Clear technical tab
        while self._technical_layout.count():
            item = self._technical_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()

        # Clear EXIF tree
        self._exif_tree.clear()

        while self._library_layout.count():
            item = self._library_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()

    def _display_image_info(self, file_path: str):
        """Display information for an image file."""
        metadata = MetadataExtractor.extract_image_metadata(file_path)

        # General tab
        self._add_info_row(self._general_layout, "File:", self._get_filename(file_path))
        self._add_info_row(
            self._general_layout,
            "Size:",
            MetadataExtractor.format_file_size(metadata.get("file_size", 0)),
        )

        if metadata.get("width") and metadata.get("height"):
            self._add_info_row(
                self._general_layout, "Dimensions:", f"{metadata['width']} x {metadata['height']}"
            )

        if metadata.get("format"):
            self._add_info_row(self._general_layout, "Format:", metadata["format"])

        if metadata.get("mode"):
            self._add_info_row(self._general_layout, "Mode:", metadata["mode"])

        # Technical tab
        if metadata.get("width") and metadata.get("height"):
            total_pixels = metadata["width"] * metadata["height"]
            megapixels = total_pixels / 1000000
            self._add_info_row(self._technical_layout, "Megapixels:", f"{megapixels:.1f} MP")

        # EXIF tab
        if metadata.get("exif"):
            for tag, value in metadata["exif"].items():
                item = QTreeWidgetItem(self._exif_tree)
                item.setText(0, tag)
                item.setText(1, str(value))

    def _display_video_info(self, file_path: str):
        """Display information for a video file."""
        metadata = MetadataExtractor.extract_video_metadata(file_path)

        # General tab
        self._add_info_row(self._general_layout, "File:", self._get_filename(file_path))
        self._add_info_row(
            self._general_layout,
            "Size:",
            MetadataExtractor.format_file_size(metadata.get("file_size", 0)),
        )

        if metadata.get("duration"):
            self._add_info_row(
                self._general_layout,
                "Duration:",
                MetadataExtractor.format_duration(metadata["duration"]),
            )

        # Technical tab
        if metadata.get("width") and metadata.get("height"):
            self._add_info_row(
                self._technical_layout, "Resolution:", f"{metadata['width']} x {metadata['height']}"
            )

        if metadata.get("codec"):
            self._add_info_row(self._technical_layout, "Codec:", metadata["codec"])

        if metadata.get("fps"):
            self._add_info_row(self._technical_layout, "Frame Rate:", f"{metadata['fps']:.2f} fps")

        if metadata.get("bitrate"):
            bitrate_mbps = metadata["bitrate"] / 1000000
            self._add_info_row(self._technical_layout, "Bitrate:", f"{bitrate_mbps:.2f} Mbps")

        if metadata.get("format"):
            self._add_info_row(self._technical_layout, "Format:", metadata["format"])

    def _display_basic_info(self, file_path: str):
        """Display basic information for any file."""
        from pathlib import Path

        path = Path(file_path)

        self._add_info_row(self._general_layout, "File:", path.name)
        if path.exists():
            self._add_info_row(
                self._general_layout,
                "Size:",
                MetadataExtractor.format_file_size(path.stat().st_size),
            )
        self._add_info_row(self._general_layout, "Path:", str(path.parent))

    def _display_library_info(self, file_path: str):
        """Display DB-backed user metadata for the current file."""
        tags = [tag.name for tag in self._db.get_tags_for_file(file_path)]
        collections = [collection.name for collection in self._db.get_collections_for_file(file_path)]
        rating = self._db.get_rating_for_file(file_path)

        self._add_info_row(self._library_layout, "Tags:", ", ".join(tags) if tags else "None")
        self._add_info_row(
            self._library_layout,
            "Collections:",
            ", ".join(collections) if collections else "None",
        )
        self._add_info_row(
            self._library_layout,
            "Rating:",
            "Unrated" if rating is None else f"{rating} star{'s' if rating != 1 else ''}",
        )

    def _display_loading_state(self, file_path: str):
        """Show immediate lightweight file info while metadata loads."""
        from pathlib import Path

        path = Path(file_path)
        self._add_info_row(self._general_layout, "File:", path.name)
        if path.exists():
            self._add_info_row(
                self._general_layout,
                "Size:",
                MetadataExtractor.format_file_size(path.stat().st_size),
            )
            self._add_info_row(self._general_layout, "Path:", str(path.parent))
        self._add_info_row(self._technical_layout, "Status:", "Loading metadata...")
        self._add_info_row(self._library_layout, "Status:", "Loading library data...")

    def _on_info_loaded(self, file_path: str, payload: dict):
        """Apply asynchronously loaded metadata on the UI thread."""
        if file_path != self._current_file:
            return

        self._clear_tabs()
        file_type = payload.get("file_type")
        metadata = payload.get("metadata", {})

        if file_type == "image":
            self._display_image_info_from_metadata(file_path, metadata)
        elif file_type == "video":
            self._display_video_info_from_metadata(file_path, metadata)
        else:
            self._display_basic_info(file_path)

        self._display_library_info_from_payload(payload)

    def _display_image_info_from_metadata(self, file_path: str, metadata: dict):
        """Render image metadata previously loaded in the worker."""
        self._add_info_row(self._general_layout, "File:", self._get_filename(file_path))
        self._add_info_row(
            self._general_layout,
            "Size:",
            MetadataExtractor.format_file_size(metadata.get("file_size", 0)),
        )
        if metadata.get("width") and metadata.get("height"):
            self._add_info_row(
                self._general_layout, "Dimensions:", f"{metadata['width']} x {metadata['height']}"
            )
            total_pixels = metadata["width"] * metadata["height"]
            megapixels = total_pixels / 1000000
            self._add_info_row(self._technical_layout, "Megapixels:", f"{megapixels:.1f} MP")
        if metadata.get("format"):
            self._add_info_row(self._general_layout, "Format:", metadata["format"])
        if metadata.get("mode"):
            self._add_info_row(self._general_layout, "Mode:", metadata["mode"])
        if metadata.get("exif"):
            for tag, value in metadata["exif"].items():
                item = QTreeWidgetItem(self._exif_tree)
                item.setText(0, tag)
                item.setText(1, str(value))

    def _display_library_info_from_payload(self, payload: dict):
        """Render DB-backed user metadata previously loaded in the worker."""
        tags = payload.get("tags", [])
        collections = payload.get("collections", [])
        rating = payload.get("rating")
        self._add_info_row(self._library_layout, "Tags:", ", ".join(tags) if tags else "None")
        self._add_info_row(
            self._library_layout,
            "Collections:",
            ", ".join(collections) if collections else "None",
        )
        self._add_info_row(
            self._library_layout,
            "Rating:",
            "Unrated" if rating is None else f"{rating} star{'s' if rating != 1 else ''}",
        )

    def _display_video_info_from_metadata(self, file_path: str, metadata: dict):
        """Render video metadata previously loaded in the worker."""
        self._add_info_row(self._general_layout, "File:", self._get_filename(file_path))
        self._add_info_row(
            self._general_layout,
            "Size:",
            MetadataExtractor.format_file_size(metadata.get("file_size", 0)),
        )
        if metadata.get("duration") is not None:
            self._add_info_row(
                self._general_layout,
                "Duration:",
                MetadataExtractor.format_duration(metadata["duration"]),
            )
        if metadata.get("width") and metadata.get("height"):
            self._add_info_row(
                self._general_layout,
                "Resolution:",
                f"{metadata['width']} x {metadata['height']}",
            )
        if metadata.get("width") and metadata.get("height"):
            self._add_info_row(
                self._technical_layout,
                "Resolution:",
                f"{metadata['width']} x {metadata['height']}",
            )
        if metadata.get("fps") is not None:
            self._add_info_row(self._technical_layout, "Frame Rate:", f"{metadata['fps']:.2f} fps")
        if metadata.get("codec"):
            self._add_info_row(self._technical_layout, "Codec:", metadata["codec"])
        if metadata.get("bitrate"):
            bitrate_mbps = metadata["bitrate"] / 1000000
            self._add_info_row(self._technical_layout, "Bitrate:", f"{bitrate_mbps:.2f} Mbps")
        if metadata.get("format"):
            self._add_info_row(self._technical_layout, "Format:", metadata["format"])
        if metadata.get("error"):
            self._add_info_row(self._technical_layout, "Status:", metadata["error"])

    def cleanup(self):
        """Clean up the metadata worker."""
        self._loader.stop()
        self._loader.wait(2000)

    def _add_info_row(self, layout, label_text: str, value_text: str):
        """Add an info row with label and value."""
        row = QWidget()
        row_layout = QVBoxLayout(row)
        row_layout.setContentsMargins(0, 0, 0, 4)
        row_layout.setSpacing(2)

        label = QLabel(f"<b>{label_text}</b>")
        label.setStyleSheet("color: #666666; font-size: 10px; text-transform: uppercase;")
        row_layout.addWidget(label)

        value = QLabel(value_text)
        value.setStyleSheet("color: #e8e8e8; font-size: 12px;")
        value.setWordWrap(True)
        row_layout.addWidget(value)

        layout.addWidget(row)

    @staticmethod
    def _get_filename(file_path: str) -> str:
        """Get filename from path."""
        from pathlib import Path

        return Path(file_path).name
