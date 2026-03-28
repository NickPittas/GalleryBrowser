"""Info pane for displaying file metadata with tabs."""

from PyQt6.QtCore import Qt
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


class InfoPane(QWidget):
    """Right bottom pane for file information with tabs."""

    def __init__(self):
        super().__init__()
        self._setup_ui()
        self._current_file = None

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

        layout.addWidget(self._tabs)

    def set_file(self, file_path: str):
        """Set the file to display information for."""
        self._current_file = file_path
        file_type = Config.get_file_type(file_path)

        # Clear previous content
        self._clear_tabs()

        if file_type == "image":
            self._display_image_info(file_path)
        elif file_type == "video":
            self._display_video_info(file_path)
        else:
            self._display_basic_info(file_path)

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
