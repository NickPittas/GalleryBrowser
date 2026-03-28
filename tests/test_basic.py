"""Basic tests for GalleryBrowser core functionality."""

import os
import sys
import tempfile
from pathlib import Path

import pytest

# Add src to path
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

from gallerybrowser.config import Config
from gallerybrowser.database.manager import DatabaseManager
from gallerybrowser.database.models import File, Tag


class TestConfig:
    """Test configuration module."""

    def test_cache_dir_creation(self):
        """Test that cache directory is created."""
        cache_dir = Config.get_cache_dir()
        assert cache_dir.exists()
        assert cache_dir.name == "gallerybrowser"

    def test_file_type_detection(self):
        """Test file type detection."""
        assert Config.get_file_type("test.png") == "image"
        assert Config.get_file_type("test.jpg") == "image"
        assert Config.get_file_type("test.mp4") == "video"
        assert Config.get_file_type("test.txt") == "unknown"

    def test_supported_extensions(self):
        """Test that image and video extensions are defined."""
        assert ".png" in Config.IMAGE_EXTENSIONS
        assert ".mp4" in Config.VIDEO_EXTENSIONS


class TestDatabase:
    """Test database functionality."""

    def setup_method(self):
        """Set up test database."""
        # Create temp database
        self.temp_dir = tempfile.mkdtemp()
        self.db_path = Path(self.temp_dir) / "test.db"

        self.db = DatabaseManager()
        # Reset singleton for testing
        DatabaseManager._instance = None
        self.db = DatabaseManager()
        self.db.initialize(str(self.db_path))

    def teardown_method(self):
        """Clean up test database."""
        import shutil

        # Close database connections first
        if hasattr(self.db, "_engine") and self.db._engine:
            self.db._engine.dispose()

        # Remove all database files including WAL
        for ext in ["", "-shm", "-wal"]:
            file_path = Path(str(self.db_path) + ext)
            if file_path.exists():
                os.remove(file_path)

        # Remove temp directory
        if self.temp_dir and os.path.exists(self.temp_dir):
            shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_database_initialization(self):
        """Test database initializes correctly."""
        assert self.db._engine is not None
        assert self.db._session_factory is not None

    def test_file_crud(self):
        """Test file create, read, update, delete."""
        # Create file
        file = File(
            path="/test/image.png",
            filename="image.png",
            folder_path="/test",
            file_type="image",
            format="png",
            size_bytes=1024,
        )

        added = self.db.add_file(file)
        assert added.id is not None

        # Read file
        retrieved = self.db.get_file_by_path("/test/image.png")
        assert retrieved is not None
        assert retrieved.filename == "image.png"

        # Delete file
        self.db.delete_file(retrieved.id)
        deleted = self.db.get_file_by_path("/test/image.png")
        assert deleted is None

    def test_tag_crud(self):
        """Test tag create and delete."""
        # Create tag
        tag = self.db.add_tag("test-tag", color="#FF0000")
        assert tag.id is not None
        assert tag.name == "test-tag"
        assert tag.color == "#FF0000"

        # List tags
        tags = self.db.get_all_tags()
        assert len(tags) >= 1

        # Delete tag
        self.db.delete_tag(tag.id)

    def test_collection_crud(self):
        """Test collection create and delete."""
        # Create collection
        collection = self.db.add_collection(
            "Test Collection", description="A test collection", color="#00FF00"
        )
        assert collection.id is not None
        assert collection.name == "Test Collection"

        # List collections
        collections = self.db.get_all_collections()
        assert len(collections) >= 1

        # Delete collection
        self.db.delete_collection(collection.id)

    def test_settings(self):
        """Test settings storage."""
        # Set setting
        self.db.set_setting("test_key", "test_value")

        # Get setting
        value = self.db.get_setting("test_key")
        assert value == "test_value"

        # Get non-existent with default
        default = self.db.get_setting("non_existent", "default")
        assert default == "default"


class TestUI:
    """Test UI components (requires PyQt)."""

    def test_imports(self):
        """Test that all UI modules can be imported."""
        from gallerybrowser.views.main_window import MainWindow
        from gallerybrowser.views.tree_pane import TreePane
        from gallerybrowser.views.file_pane import FilePane
        from gallerybrowser.views.preview_pane import PreviewPane
        from gallerybrowser.views.info_pane import InfoPane

        # If we get here, imports work
        assert True


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
