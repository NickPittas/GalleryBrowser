"""GalleryBrowser application class."""

from PyQt6.QtGui import QAction, QKeySequence
from PyQt6.QtWidgets import QMainWindow

import qtawesome as qta

from gallerybrowser.views.main_window import MainWindow


class GalleryBrowserApp(QMainWindow):
    """Main application window."""

    def __init__(self):
        super().__init__()
        self.setWindowTitle("GalleryBrowser")
        self.setMinimumSize(1200, 800)

        # Create main window content
        self.main_window = MainWindow()
        self.setCentralWidget(self.main_window)

        # Add toolbar
        self.addToolBar(self.main_window.toolbar)

        # Create menu bar
        self.create_menu_bar()

    def create_menu_bar(self):
        """Create the menu bar."""
        menu_bar = self.menuBar()

        # File menu
        file_menu = menu_bar.addMenu("File")

        action_open = QAction(qta.icon("fa5s.folder-open", color="#a0a0a0"), "Open Folder...", self)
        action_open.setShortcut(QKeySequence("Ctrl+O"))
        action_open.triggered.connect(self.main_window.open_folder_dialog)
        file_menu.addAction(action_open)

        file_menu.addSeparator()

        action_exit = QAction(qta.icon("fa5s.sign-out-alt", color="#a0a0a0"), "Exit", self)
        action_exit.setShortcut(QKeySequence("Ctrl+Q"))
        action_exit.triggered.connect(self.close)
        file_menu.addAction(action_exit)

        # Edit menu
        edit_menu = menu_bar.addMenu("Edit")
        edit_menu.addAction(self.main_window.action_copy)
        edit_menu.addAction(self.main_window.action_cut)
        edit_menu.addAction(self.main_window.action_paste)
        edit_menu.addSeparator()
        edit_menu.addAction(self.main_window.action_delete)
        edit_menu.addAction(self.main_window.action_rename)

        # View menu
        view_menu = menu_bar.addMenu("View")
        view_menu.addAction(self.main_window.action_grid_view)
        view_menu.addAction(self.main_window.action_list_view)
        view_menu.addSeparator()
        view_menu.addAction(self.main_window.action_toggle_preview)

        # Help menu
        help_menu = menu_bar.addMenu("Help")

        action_about = QAction(qta.icon("fa5s.info-circle", color="#a0a0a0"), "About", self)
        help_menu.addAction(action_about)

    def closeEvent(self, event):
        """Clean up child widgets on close."""
        try:
            self.main_window.preview_pane.cleanup()
        except Exception:
            pass
        try:
            self.main_window.info_pane.cleanup()
        except Exception:
            pass
        super().closeEvent(event)
