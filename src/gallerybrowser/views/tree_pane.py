"""Tree pane for folder navigation with icons."""

from pathlib import Path

from PyQt6.QtCore import Qt, QSize, pyqtSignal
from PyQt6.QtWidgets import QMenu, QTreeWidget, QTreeWidgetItem, QVBoxLayout, QWidget

import qtawesome as qta

from gallerybrowser.config import Config


class _DropTreeWidget(QTreeWidget):
    """QTreeWidget subclass that handles drag/drop itself.

    By handling drag events directly on the tree widget (rather than
    on the parent ``TreePane``), the position coordinates passed to
    ``itemAt()`` are always in the tree's own coordinate system.  This
    fixes the offset bug where drops would land on the wrong item
    because the ``TreePane`` layout offsets were not accounted for.
    """

    # Re-emit to the parent TreePane
    # args: target_folder, file_paths, is_copy
    files_dropped_signal = pyqtSignal(str, list, bool)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setAcceptDrops(True)
        self.setDragDropMode(QTreeWidget.DragDropMode.DropOnly)
        self._drop_hover_item = None
        self._pre_drag_current = None  # saved current item before drag

    # -- visual helpers --

    def _set_hover_item(self, item):
        """Highlight *item* as the current drop target using selection style."""
        if item is self._drop_hover_item:
            return
        self._drop_hover_item = item
        if item is not None:
            self.setCurrentItem(item)
        elif self._pre_drag_current is not None:
            self.setCurrentItem(self._pre_drag_current)

    def _clear_hover(self):
        """Restore the tree's current-item to what it was before the drag."""
        self._drop_hover_item = None
        if self._pre_drag_current is not None:
            self.setCurrentItem(self._pre_drag_current)
            self._pre_drag_current = None

    # -- drag / drop --

    def _is_valid_drop(self, mime_data):
        return mime_data.hasUrls() or mime_data.hasText()

    def _item_accepts_drop(self, item):
        """Return True if *item* is a valid drop target (a real folder)."""
        if item is None:
            return False
        path = item.data(0, Qt.ItemDataRole.UserRole)
        if not path or str(path).startswith("__"):
            return False
        return True

    def dragEnterEvent(self, event):
        if self._is_valid_drop(event.mimeData()):
            self._pre_drag_current = self.currentItem()
            event.acceptProposedAction()
        else:
            event.ignore()

    def dragMoveEvent(self, event):
        if not self._is_valid_drop(event.mimeData()):
            event.ignore()
            return

        item = self.itemAt(event.position().toPoint())
        if self._item_accepts_drop(item):
            self._set_hover_item(item)
            event.acceptProposedAction()
        else:
            self._set_hover_item(None)
            event.ignore()

    def dragLeaveEvent(self, event):
        self._clear_hover()
        super().dragLeaveEvent(event)

    def dropEvent(self, event):
        self._clear_hover()
        mime_data = event.mimeData()

        # Position is now in the tree's own coordinates — no offset
        target_item = self.itemAt(event.position().toPoint())
        if not self._item_accepts_drop(target_item):
            event.ignore()
            return

        target_path = target_item.data(0, Qt.ItemDataRole.UserRole)

        # Ctrl held = copy, otherwise move
        is_copy = bool(event.modifiers() & Qt.KeyboardModifier.ControlModifier)

        # Collect file paths
        file_paths = []
        if mime_data.hasUrls():
            for url in mime_data.urls():
                if url.isLocalFile():
                    file_paths.append(url.toLocalFile())
        elif mime_data.hasText():
            text = mime_data.text()
            for line in text.strip().splitlines():
                line = line.strip()
                if line:
                    file_paths.append(line)

        if file_paths:
            self.files_dropped_signal.emit(target_path, file_paths, is_copy)
            event.acceptProposedAction()


class TreePane(QWidget):
    """Left pane showing folder tree."""

    folder_selected = pyqtSignal(str)  # Emits folder path
    files_dropped = pyqtSignal(str, list, bool)  # (target_folder, file_paths, is_copy)

    def __init__(self):
        super().__init__()
        self._favorites_item = None
        self._recent_item = None
        self.setup_ui()
        self.populate_tree()

    def setup_ui(self):
        """Set up the UI."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        # Create tree widget (custom subclass for correct drag-drop coords)
        self.tree = _DropTreeWidget()
        self.tree.setHeaderLabel("Folders")
        self.tree.setIndentation(18)
        self.tree.setIconSize(QSize(15, 15))

        # CRITICAL: Enable root decoration to show expand/collapse arrows
        self.tree.setRootIsDecorated(True)

        # Connect signals
        self.tree.itemClicked.connect(self.on_item_clicked)
        self.tree.itemExpanded.connect(self._on_item_expanded)
        self.tree.itemCollapsed.connect(self._on_item_collapsed)
        self.tree.files_dropped_signal.connect(
            lambda target, paths, copy: self.files_dropped.emit(target, paths, copy)
        )

        # Context menu on tree items
        self.tree.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.tree.customContextMenuRequested.connect(self._on_context_menu)

        layout.addWidget(self.tree)

    # ------------------------------------------------------------------
    # Tree population
    # ------------------------------------------------------------------

    def populate_tree(self):
        """Populate the tree with filesystem roots like a file manager."""
        # Add Favorites section
        self._favorites_item = QTreeWidgetItem(self.tree)
        self._favorites_item.setText(0, "Favorites")
        self._favorites_item.setIcon(0, qta.icon("fa5s.star", color="#ffd700"))
        self._favorites_item.setData(0, Qt.ItemDataRole.UserRole, "__favorites__")
        self._rebuild_favorites()

        # Add Recent section
        self._recent_item = QTreeWidgetItem(self.tree)
        self._recent_item.setText(0, "Recent")
        self._recent_item.setIcon(0, qta.icon("fa5s.history", color="#808080"))
        self._recent_item.setData(0, Qt.ItemDataRole.UserRole, "__recent__")
        self._rebuild_recents()

        # Add root filesystem
        root_item = QTreeWidgetItem(self.tree)
        root_item.setText(0, "Computer")
        root_item.setIcon(0, qta.icon("fa5s.desktop", color="#dcb67a"))
        root_item.setData(0, Qt.ItemDataRole.UserRole, "/")
        dummy = QTreeWidgetItem(root_item)
        dummy.setText(0, "Loading...")

        # Add home directory
        home_path = Path.home()
        home_item = QTreeWidgetItem(self.tree)
        home_item.setText(0, "Home")
        home_item.setIcon(0, qta.icon("fa5s.home", color="#0078d4"))
        home_item.setData(0, Qt.ItemDataRole.UserRole, str(home_path))
        dummy = QTreeWidgetItem(home_item)
        dummy.setText(0, "Loading...")

        # Add mounted drives/volumes
        self._add_mounted_volumes()

        # Expand Favorites and Recent by default
        self._favorites_item.setExpanded(True)
        self._recent_item.setExpanded(True)

    # ------------------------------------------------------------------
    # Favorites
    # ------------------------------------------------------------------

    def _load_favorites(self) -> list:
        """Load favorites list from settings."""
        settings = Config.load_settings()
        return list(settings.get("favorites", []))

    def _save_favorites(self, favorites: list):
        """Save favorites list to settings."""
        settings = Config.load_settings()
        settings["favorites"] = favorites
        Config.save_settings(settings)

    def _rebuild_favorites(self):
        """Rebuild the Favorites section from persisted settings."""
        parent = self._favorites_item
        # Clear existing children
        while parent.childCount() > 0:
            parent.removeChild(parent.child(0))

        favorites = self._load_favorites()
        for path in favorites:
            if Path(path).exists():
                item = QTreeWidgetItem(parent)
                item.setText(0, Path(path).name or path)
                item.setIcon(0, qta.icon("fa5s.folder", color="#dcb67a"))
                item.setData(0, Qt.ItemDataRole.UserRole, path)

    def add_favorite(self, folder_path: str):
        """Add a folder to the favorites list (persisted)."""
        folder_path = str(Path(folder_path).resolve())
        favorites = self._load_favorites()
        if folder_path not in favorites:
            favorites.append(folder_path)
            self._save_favorites(favorites)
            self._rebuild_favorites()

    def remove_favorite(self, folder_path: str):
        """Remove a folder from the favorites list (persisted)."""
        folder_path = str(Path(folder_path).resolve())
        favorites = self._load_favorites()
        if folder_path in favorites:
            favorites.remove(folder_path)
            self._save_favorites(favorites)
            self._rebuild_favorites()

    # ------------------------------------------------------------------
    # Recents
    # ------------------------------------------------------------------

    def _load_recents(self) -> list:
        """Load recents list from settings."""
        settings = Config.load_settings()
        return list(settings.get("recents", []))

    def _save_recents(self, recents: list):
        """Save recents list to settings."""
        settings = Config.load_settings()
        settings["recents"] = recents
        Config.save_settings(settings)

    def _rebuild_recents(self):
        """Rebuild the Recent section from persisted settings."""
        parent = self._recent_item
        while parent.childCount() > 0:
            parent.removeChild(parent.child(0))

        recents = self._load_recents()
        for path in recents:
            if Path(path).exists():
                item = QTreeWidgetItem(parent)
                item.setText(0, Path(path).name or path)
                item.setIcon(0, qta.icon("fa5s.folder", color="#808080"))
                item.setData(0, Qt.ItemDataRole.UserRole, path)

    def add_recent(self, folder_path: str):
        """Add a folder to the recents list (persisted).

        Moves it to the front if it already exists, and trims to max size.
        """
        folder_path = str(Path(folder_path).resolve())
        recents = self._load_recents()
        # Remove if already present so it moves to front
        if folder_path in recents:
            recents.remove(folder_path)
        recents.insert(0, folder_path)
        # Trim
        settings = Config.load_settings()
        max_recents = settings.get("recents_max", 15)
        recents = recents[:max_recents]
        self._save_recents(recents)
        self._rebuild_recents()

    def clear_recents(self):
        """Clear the recents list."""
        self._save_recents([])
        self._rebuild_recents()

    # ------------------------------------------------------------------
    # Context menu
    # ------------------------------------------------------------------

    def _on_context_menu(self, pos):
        """Show a context menu for tree items."""
        item = self.tree.itemAt(pos)
        if not item:
            return

        path = item.data(0, Qt.ItemDataRole.UserRole)
        if not path:
            return

        menu = QMenu(self)

        # Check if this item is under Favorites
        is_favorite_child = item.parent() == self._favorites_item
        is_recent_child = item.parent() == self._recent_item
        is_favorites_header = path == "__favorites__"
        is_recent_header = path == "__recent__"

        if is_favorites_header:
            # No useful actions on the header itself
            return
        elif is_recent_header:
            clear_action = menu.addAction("Clear Recents")
            action = menu.exec(self.tree.viewport().mapToGlobal(pos))
            if action == clear_action:
                self.clear_recents()
            return

        # Regular folder item
        if is_favorite_child:
            remove_fav = menu.addAction("Remove from Favorites")
            action = menu.exec(self.tree.viewport().mapToGlobal(pos))
            if action == remove_fav:
                self.remove_favorite(path)
        elif is_recent_child:
            add_fav = menu.addAction("Add to Favorites")
            remove_recent = menu.addAction("Remove from Recents")
            action = menu.exec(self.tree.viewport().mapToGlobal(pos))
            if action == add_fav:
                self.add_favorite(path)
            elif action == remove_recent:
                recents = self._load_recents()
                resolved = str(Path(path).resolve())
                recents = [r for r in recents if r != resolved and r != path]
                self._save_recents(recents)
                self._rebuild_recents()
        else:
            # Normal filesystem folder
            add_fav = menu.addAction("Add to Favorites")
            action = menu.exec(self.tree.viewport().mapToGlobal(pos))
            if action == add_fav:
                self.add_favorite(path)

    # ------------------------------------------------------------------
    # Expand / collapse
    # ------------------------------------------------------------------

    def _on_item_expanded(self, item: QTreeWidgetItem):
        """Handle item expansion."""
        path = item.data(0, Qt.ItemDataRole.UserRole)

        # Special sections don't lazy-load children
        if path and str(path).startswith("__"):
            return

        # Remove dummy children
        while item.childCount() > 0:
            item.removeChild(item.child(0))

        # Change folder icon to open
        item.setIcon(0, qta.icon("fa5s.folder-open", color="#dcb67a"))

        # Load actual children
        if path:
            self.populate_children(item, Path(path))

    def _on_item_collapsed(self, item: QTreeWidgetItem):
        """Handle item collapse."""
        path = item.data(0, Qt.ItemDataRole.UserRole)

        # Special sections: just collapse, don't replace children
        if path and str(path).startswith("__"):
            return

        # Change folder icon to closed
        item.setIcon(0, qta.icon("fa5s.folder", color="#dcb67a"))

        # Remove all children and add dummy back
        while item.childCount() > 0:
            item.removeChild(item.child(0))

        # Add dummy child so expand arrow reappears
        dummy = QTreeWidgetItem(item)
        dummy.setText(0, "Loading...")

    def _add_mounted_volumes(self):
        """Add mounted volumes/drives to the tree."""
        try:
            mount_points = [
                "/mnt",
                "/media",
                "/run/media",
            ]

            for mount_base in mount_points:
                mount_path = Path(mount_base)
                if mount_path.exists():
                    for entry in mount_path.iterdir():
                        if entry.is_dir() and not entry.name.startswith("."):
                            volume_item = QTreeWidgetItem(self.tree)
                            volume_item.setText(0, entry.name)
                            volume_item.setIcon(0, qta.icon("fa5s.hdd", color="#808080"))
                            volume_item.setData(0, Qt.ItemDataRole.UserRole, str(entry))
                            dummy = QTreeWidgetItem(volume_item)
                            dummy.setText(0, "Loading...")
        except (PermissionError, OSError):
            pass

    def populate_children(self, parent_item: QTreeWidgetItem, parent_path: Path):
        """Populate children of a tree item."""
        try:
            entries = list(parent_path.iterdir())
            dirs = [p for p in entries if p.is_dir() and not p.name.startswith(".")]

            for child_path in sorted(dirs, key=lambda x: x.name.lower()):
                child_item = QTreeWidgetItem(parent_item)
                child_item.setText(0, child_path.name)
                child_item.setIcon(0, qta.icon("fa5s.folder", color="#dcb67a"))
                child_item.setData(0, Qt.ItemDataRole.UserRole, str(child_path))

                try:
                    has_subdirs = any(
                        p.is_dir() and not p.name.startswith(".") for p in child_path.iterdir()
                    )
                    if has_subdirs:
                        dummy = QTreeWidgetItem(child_item)
                        dummy.setText(0, "Loading...")
                except (PermissionError, OSError):
                    pass

        except (PermissionError, OSError):
            pass

    def on_item_clicked(self, item: QTreeWidgetItem, column: int):
        """Handle tree item click."""
        path = item.data(0, Qt.ItemDataRole.UserRole)
        if path and not str(path).startswith("__"):
            self.folder_selected.emit(path)

        # Toggle expansion on click if folder has children (but not for special sections)
        if item.childCount() > 0 and not (path and str(path).startswith("__")):
            if item.isExpanded():
                item.setExpanded(False)
            else:
                item.setExpanded(True)

    def navigate_to_path(self, folder_path: str):
        """Expand the filesystem tree to *folder_path* and select the item.

        Walks from the closest top-level mount point (Home, Computer, or
        /mnt/… volume) downward, expanding each ancestor so that the
        target folder is visible and selected in the tree.

        Does NOT emit ``folder_selected`` — this is purely a tree-UI sync.
        """
        target = Path(folder_path).resolve()

        # Find the best-matching top-level item (longest prefix wins)
        best_item = None
        best_prefix_len = 0
        root = self.tree.invisibleRootItem()
        for i in range(root.childCount()):
            item = root.child(i)
            item_path = item.data(0, Qt.ItemDataRole.UserRole)
            if not item_path or str(item_path).startswith("__"):
                continue
            try:
                ip = Path(item_path).resolve()
                if target == ip or ip in target.parents:
                    prefix_len = len(str(ip))
                    if prefix_len > best_prefix_len:
                        best_prefix_len = prefix_len
                        best_item = item
            except Exception:
                continue

        if best_item is None:
            return  # No matching root found

        # Walk from best_item downward, expanding each ancestor
        current_item = best_item
        item_path = Path(best_item.data(0, Qt.ItemDataRole.UserRole)).resolve()

        if item_path == target:
            # Already at target — select it
            self.tree.blockSignals(True)
            self.tree.setCurrentItem(current_item)
            self.tree.scrollToItem(current_item)
            self.tree.blockSignals(False)
            return

        # Build list of path components we need to traverse
        try:
            remaining = target.relative_to(item_path)
        except ValueError:
            return
        parts = list(remaining.parts)

        for part in parts:
            # Expand current item so children are populated
            if not current_item.isExpanded():
                current_item.setExpanded(True)

            # Find child matching this part
            found = False
            for j in range(current_item.childCount()):
                child = current_item.child(j)
                child_text = child.text(0)
                if child_text == part:
                    current_item = child
                    found = True
                    break
            if not found:
                break  # Path doesn't exist in tree (permission denied, etc.)

        # Select and scroll to the item we ended up at
        self.tree.blockSignals(True)
        self.tree.setCurrentItem(current_item)
        self.tree.scrollToItem(current_item)
        self.tree.blockSignals(False)
