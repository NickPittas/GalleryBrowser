"""Tags panel for managing file tags."""

from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtWidgets import (
    QLabel,
    QHBoxLayout,
    QInputDialog,
    QListWidget,
    QListWidgetItem,
    QMenu,
    QPushButton,
    QVBoxLayout,
    QWidget,
)

import qtawesome as qta


class TagsPanel(QWidget):
    """Panel for managing tags."""

    tag_selected = pyqtSignal(str)  # Emits tag name when selected
    tag_cleared = pyqtSignal()
    tag_created = pyqtSignal(str)
    tag_deleted = pyqtSignal(str)

    def __init__(self):
        super().__init__()
        self.setup_ui()
        self.tags = {}  # name -> tag object or dict

    def setup_ui(self):
        """Set up the UI."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)
        layout.setSpacing(8)

        # Header with add button
        header_layout = QHBoxLayout()
        title = QLabel("Tags")
        title.setStyleSheet("color: #a0a0a0; font-weight: 600;")
        header_layout.addWidget(title)
        clear_btn = QPushButton("All")
        clear_btn.clicked.connect(self.tag_cleared.emit)
        header_layout.addWidget(clear_btn)
        header_layout.addStretch()
        add_btn = QPushButton(qta.icon("fa5s.plus", color="#a0a0a0"), "Add Tag")
        add_btn.clicked.connect(self.on_add_tag)
        header_layout.addWidget(add_btn)
        layout.addLayout(header_layout)

        # Tags list
        self.tags_list = QListWidget()
        self.tags_list.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.tags_list.customContextMenuRequested.connect(self.show_context_menu)
        self.tags_list.itemClicked.connect(self.on_tag_clicked)
        self.tags_list.setStyleSheet("""
            QListWidget {
                background-color: #181818;
                border: none;
                outline: none;
                padding: 4px;
            }
            QListWidget::item {
                padding: 8px 10px;
                border-radius: 6px;
                margin: 1px 2px;
                color: #e8e8e8;
            }
            QListWidget::item:selected {
                background-color: #1a3a5c;
            }
            QListWidget::item:hover:!selected {
                background-color: #2c2c2c;
            }
        """)
        layout.addWidget(self.tags_list)

    def set_tags(self, tags):
        """Set the list of available tags."""
        current = self.get_selected_tag()
        self.tags = {}
        self.tags_list.clear()
        for tag in sorted(tags, key=lambda t: getattr(t, "name", str(t)).lower()):
            tag_name = getattr(tag, "name", str(tag))
            self.tags[tag_name] = tag
            item = QListWidgetItem(qta.icon("fa5s.tag", color="#E2B340"), tag_name)
            self.tags_list.addItem(item)
        if current:
            self.set_active_tag(current)

    def on_add_tag(self):
        """Add a new tag."""
        text, ok = QInputDialog.getText(self, "New Tag", "Tag name:")
        if ok and text:
            self.tag_created.emit(text.strip())

    def on_tag_clicked(self, item):
        """Handle tag click."""
        self.tag_selected.emit(item.text())

    def show_context_menu(self, position):
        """Show context menu for tag."""
        item = self.tags_list.itemAt(position)
        if item:
            menu = QMenu(self)
            delete_action = menu.addAction("Delete Tag")
            action = menu.exec(self.tags_list.mapToGlobal(position))
            if action == delete_action:
                self.tag_deleted.emit(item.text())

    def get_selected_tag(self):
        """Get currently selected tag."""
        item = self.tags_list.currentItem()
        return item.text() if item else None

    def set_active_tag(self, tag_name: str | None):
        """Select the given tag in the list when present."""
        if not tag_name:
            self.tags_list.clearSelection()
            self.tags_list.setCurrentItem(None)
            return
        for row in range(self.tags_list.count()):
            item = self.tags_list.item(row)
            if item.text() == tag_name:
                self.tags_list.setCurrentItem(item)
                item.setSelected(True)
                return
        self.tags_list.clearSelection()
        self.tags_list.setCurrentItem(None)
