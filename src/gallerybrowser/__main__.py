"""Main entry point for GalleryBrowser."""

import signal
import sys

from PyQt6.QtCore import QTimer
from PyQt6.QtWidgets import QApplication

from gallerybrowser.app import GalleryBrowserApp
from gallerybrowser.config import Config
from gallerybrowser.database.manager import DatabaseManager


def main():
    """Main entry point."""
    # Check for single instance
    from gallerybrowser.utils.single_instance import SingleInstance

    single_instance = SingleInstance()
    if not single_instance.try_lock():
        print("GalleryBrowser is already running!")
        sys.exit(1)

    # Initialize database
    db_manager = DatabaseManager()
    db_manager.initialize(str(Config.get_database_path()))

    # Create Qt application
    app = QApplication(sys.argv)
    app.setApplicationName(Config.APP_NAME)
    app.setApplicationVersion(Config.VERSION)
    app.setOrganizationName(Config.APP_AUTHOR)

    # Apply modern dark theme
    from gallerybrowser.resources.styles import apply_theme

    apply_theme(app)

    # Handle Ctrl+C gracefully using a timer-based approach
    def handle_sigint(signum, frame):
        """Handle SIGINT signal."""
        app.quit()

    signal.signal(signal.SIGINT, handle_sigint)

    # Timer to allow Python signal handling in Qt event loop
    timer = QTimer()
    timer.start(250)
    timer.timeout.connect(lambda: None)

    # Create and show main window
    gallery_app = GalleryBrowserApp()
    gallery_app.show()

    # Ensure clean shutdown
    def cleanup():
        """Cleanup resources on exit."""
        db_manager.dispose()
        single_instance.unlock()

    app.aboutToQuit.connect(cleanup)

    # Run application
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
