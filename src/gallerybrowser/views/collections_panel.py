"""Collections panel for managing file collections."""

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


class CollectionsPanel(QWidget):
    """Panel for managing collections."""

    collection_selected = pyqtSignal(str)
    collection_cleared = pyqtSignal()
    collection_created = pyqtSignal(str)
    collection_deleted = pyqtSignal(str)

    def __init__(self):
        super().__init__()
        self.collections = {}
        self.setup_ui()

    def setup_ui(self):
        """Set up the UI."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(8, 8, 8, 8)
        layout.setSpacing(8)

        header_layout = QHBoxLayout()
        title = QLabel("Collections")
        title.setStyleSheet("color: #a0a0a0; font-weight: 600;")
        header_layout.addWidget(title)
        clear_btn = QPushButton("All")
        clear_btn.clicked.connect(self.collection_cleared.emit)
        header_layout.addWidget(clear_btn)
        header_layout.addStretch()
        add_btn = QPushButton(qta.icon("fa5s.folder-plus", color="#a0a0a0"), "Add Collection")
        add_btn.clicked.connect(self.on_add_collection)
        header_layout.addWidget(add_btn)
        layout.addLayout(header_layout)

        self.collections_list = QListWidget()
        self.collections_list.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.collections_list.customContextMenuRequested.connect(self.show_context_menu)
        self.collections_list.itemClicked.connect(self.on_collection_clicked)
        self.collections_list.setStyleSheet("""
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
        layout.addWidget(self.collections_list)

    def set_collections(self, collections):
        """Set the list of available collections."""
        current = self.get_selected_collection()
        self.collections = {}
        self.collections_list.clear()
        for collection in sorted(collections, key=lambda c: getattr(c, "name", str(c)).lower()):
            name = getattr(collection, "name", str(collection))
            self.collections[name] = collection
            item = QListWidgetItem(qta.icon("fa5s.folder", color="#7eb6ff"), name)
            self.collections_list.addItem(item)
        if current:
            self.set_active_collection(current)

    def on_add_collection(self):
        """Add a new collection."""
        text, ok = QInputDialog.getText(self, "New Collection", "Collection name:")
        if ok and text:
            self.collection_created.emit(text.strip())

    def on_collection_clicked(self, item):
        """Handle collection click."""
        self.collection_selected.emit(item.text())

    def show_context_menu(self, position):
        """Show context menu for a collection."""
        item = self.collections_list.itemAt(position)
        if item:
            menu = QMenu(self)
            delete_action = menu.addAction("Delete Collection")
            action = menu.exec(self.collections_list.mapToGlobal(position))
            if action == delete_action:
                self.collection_deleted.emit(item.text())

    def get_selected_collection(self):
        """Get currently selected collection."""
        item = self.collections_list.currentItem()
        return item.text() if item else None

    def set_active_collection(self, collection_name: str | None):
        """Select the given collection in the list when present."""
        if not collection_name:
            self.collections_list.clearSelection()
            self.collections_list.setCurrentItem(None)
            return
        for row in range(self.collections_list.count()):
            item = self.collections_list.item(row)
            if item.text() == collection_name:
                self.collections_list.setCurrentItem(item)
                item.setSelected(True)
                return
        self.collections_list.clearSelection()
        self.collections_list.setCurrentItem(None)
