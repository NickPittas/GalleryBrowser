"""Basic tests for GalleryBrowser core functionality."""

import os
import sys
import tempfile
from pathlib import Path

import pytest
from PIL import Image

# Add src to path
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

from gallerybrowser.config import Config
from gallerybrowser.database.manager import DatabaseManager
from gallerybrowser.database.models import File, Tag


def _write_test_png(path: Path, color: tuple[int, int, int] = (255, 0, 0)) -> None:
    """Create a tiny valid PNG for tests that touch image-loading paths."""
    Image.new("RGB", (2, 2), color).save(path, format="PNG")


def _prepare_isolated_app_state(monkeypatch, root: Path) -> Path:
    """Point app state to a per-test location and initialize a fresh DB."""
    from gallerybrowser.config import Config
    from gallerybrowser.database.manager import DatabaseManager

    cache_dir = root / "cache"
    config_dir = root / "config"
    db_path = cache_dir / "database.sqlite"
    settings_path = config_dir / "settings.json"
    cache_dir.mkdir(parents=True, exist_ok=True)
    config_dir.mkdir(parents=True, exist_ok=True)

    monkeypatch.setattr(Config, "get_cache_dir", classmethod(lambda cls: cache_dir))
    monkeypatch.setattr(Config, "get_config_dir", classmethod(lambda cls: config_dir))
    monkeypatch.setattr(Config, "get_database_path", classmethod(lambda cls: db_path))
    monkeypatch.setattr(Config, "get_settings_path", classmethod(lambda cls: settings_path))

    DatabaseManager._instance = None
    db = DatabaseManager()
    db.initialize(str(db_path))
    return db_path


def _prime_window_file_selection(window, file_path: Path) -> None:
    """Set up a simple single-file browsing state for action-handler tests."""
    window.current_path = str(file_path.parent)
    file_info = {
        "path": str(file_path),
        "name": file_path.name,
        "type": "image",
        "size": file_path.stat().st_size,
        "modified": file_path.stat().st_mtime,
    }
    window.file_pane.files = [file_info]
    window.file_pane.filtered_files = [file_info]
    window.file_pane.selected_files = {str(file_path)}


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

    def test_parse_video_frame_rate(self):
        """Video FPS parsing should handle fractional ffprobe values."""
        from gallerybrowser.core.metadata import MetadataExtractor

        assert MetadataExtractor._parse_frame_rate("30000/1001") == pytest.approx(29.97003, rel=1e-4)
        assert MetadataExtractor._parse_frame_rate("24/1") == 24.0
        assert MetadataExtractor._parse_frame_rate("0/0") is None
        assert MetadataExtractor._parse_frame_rate(None) is None

    def test_single_instance_uses_cache_local_lock(self, monkeypatch, tmp_path):
        """Single-instance locking should use the configured cache directory."""
        from gallerybrowser.config import Config
        from gallerybrowser.utils.single_instance import SingleInstance

        cache_dir = tmp_path / "cache"
        cache_dir.mkdir()
        monkeypatch.setattr(Config, "get_cache_dir", classmethod(lambda cls: cache_dir))

        instance = SingleInstance()
        assert Path(instance.lock_path).parent == cache_dir / "locks"


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

    def test_upsert_and_tag_assignment(self):
        """Test file upsert and tag assignment queries."""
        file_path = Path(self.temp_dir) / "tagged.png"
        _write_test_png(file_path)

        record = self.db.upsert_file_from_path(str(file_path))
        assert record.path == str(file_path)

        self.db.assign_tag_to_file(str(file_path), "reviewed")
        tags = self.db.get_tags_for_file(str(file_path))
        assert [tag.name for tag in tags] == ["reviewed"]

        tagged_files = self.db.get_files_by_tag("reviewed")
        assert {file.path for file in tagged_files} == {str(file_path)}

    def test_collection_membership_and_rating(self):
        """Test collection membership and rating queries."""
        file_path = Path(self.temp_dir) / "rated.png"
        _write_test_png(file_path)

        self.db.add_file_to_collection(str(file_path), "Favorites")
        collections = self.db.get_collections_for_file(str(file_path))
        assert [collection.name for collection in collections] == ["Favorites"]

        self.db.set_rating(str(file_path), 4)
        assert self.db.get_rating_for_file(str(file_path)) == 4
        assert {file.path for file in self.db.get_files_by_rating(4)} == {str(file_path)}

        self.db.clear_rating(str(file_path))
        assert self.db.get_rating_for_file(str(file_path)) is None

    def test_settings_file_merge_preserves_navigation_lists(self, monkeypatch):
        """Saving settings should preserve favorites and recents."""
        from gallerybrowser.config import Config

        settings_path = Path(self.temp_dir) / "settings.json"
        monkeypatch.setattr(
            Config,
            "get_settings_path",
            classmethod(lambda cls: settings_path),
        )
        Config.save_settings({"favorites": ["/tmp/fav"], "recents": ["/tmp/recent"]})
        Config.save_settings({"thumbnail_size": 256})
        saved = Config.load_settings()
        assert saved["favorites"] == ["/tmp/fav"]
        assert saved["recents"] == ["/tmp/recent"]
        assert saved["thumbnail_size"] == 256


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

    def test_open_folder_dialog_routes_to_selection(self, monkeypatch, qtbot, tmp_path):
        """Open Folder should route through MainWindow.on_folder_selected."""
        from gallerybrowser.views.main_window import MainWindow

        _prepare_isolated_app_state(monkeypatch, tmp_path / "state")
        window = MainWindow()
        qtbot.addWidget(window)

        selected = []

        def fake_get_existing_directory(*args, **kwargs):
            return "/tmp"

        def capture(folder_path):
            selected.append(folder_path)

        monkeypatch.setattr(
            "gallerybrowser.views.main_window.QFileDialog.getExistingDirectory",
            fake_get_existing_directory,
        )
        monkeypatch.setattr(window, "on_folder_selected", capture)

        window.open_folder_dialog()
        assert selected == ["/tmp"]
        window.close()

    def test_tag_filter_respects_folder_and_library_scope(self, monkeypatch, qtbot, tmp_path):
        """Tag filtering should stay folder-scoped unless Library mode is selected."""
        from gallerybrowser.database.manager import DatabaseManager
        from gallerybrowser.views.main_window import MainWindow

        folder_a = tmp_path / "a"
        folder_b = tmp_path / "b"
        folder_a.mkdir()
        folder_b.mkdir()
        file_a = folder_a / "one.png"
        file_b = folder_b / "two.png"
        _write_test_png(file_a, (255, 0, 0))
        _write_test_png(file_b, (0, 255, 0))

        _prepare_isolated_app_state(monkeypatch, tmp_path / "state")
        db = DatabaseManager()
        db.assign_tag_to_file(str(file_a), "hero")
        db.assign_tag_to_file(str(file_b), "hero")

        window = MainWindow()
        qtbot.addWidget(window)
        window.current_path = str(folder_a)
        window.query_state.selected_tag = "hero"
        window.query_state.scope = "folder"
        window.refresh_view()

        assert [item["path"] for item in window.file_pane.files] == [str(file_a)]

        window.query_state.scope = "library"
        window.refresh_view()

        assert {item["path"] for item in window.file_pane.files} == {str(file_a), str(file_b)}
        window.close()

    def test_source_label_tracks_search_and_type_filters(self, monkeypatch, qtbot, tmp_path):
        """Source label should reflect lightweight search/type filter changes."""
        from gallerybrowser.views.main_window import MainWindow

        _prepare_isolated_app_state(monkeypatch, tmp_path / "state")

        window = MainWindow()
        qtbot.addWidget(window)
        window.current_path = "/tmp"
        window.on_filter_changed("Videos")
        window.on_search_text_changed("clip")

        assert "type:video" in window.path_label.text()
        assert "search:clip" in window.path_label.text()
        assert window.breadcrumb_bar.current_path == "/tmp"
        assert [segment[0] for segment in window.breadcrumb_bar.current_segments] == ["/", "tmp"]
        window.close()

    def test_breadcrumb_click_routes_to_folder_selection(self, monkeypatch, qtbot, tmp_path):
        """Clicking a breadcrumb segment should reuse the standard folder selection flow."""
        from gallerybrowser.views.main_window import MainWindow

        _prepare_isolated_app_state(monkeypatch, tmp_path / "state")
        nested = tmp_path / "projects" / "shots"
        nested.mkdir(parents=True)

        window = MainWindow()
        qtbot.addWidget(window)
        window.current_path = str(nested)
        window.refresh_view()

        selected = []

        def capture(folder_path):
            selected.append(folder_path)

        window.breadcrumb_bar.path_selected.disconnect()
        window.breadcrumb_bar.path_selected.connect(capture)
        target_button = window.breadcrumb_bar.segment_buttons[-2]
        target_path = str(nested.parent)
        assert target_button.toolTip() == target_path

        target_button.click()

        assert selected == [target_path]
        window.close()

    def test_create_delete_tag_updates_panel_selection(self, monkeypatch, qtbot, tmp_path):
        """Creating and clearing a tag filter should keep the panel selection in sync."""
        from gallerybrowser.views.main_window import MainWindow

        _prepare_isolated_app_state(monkeypatch, tmp_path / "state")
        window = MainWindow()
        qtbot.addWidget(window)

        window.on_create_tag("Review")
        assert "Review" in [window.tags_panel.tags_list.item(i).text() for i in range(window.tags_panel.tags_list.count())]

        window.on_tag_selected("Review")
        assert window.tags_panel.get_selected_tag() == "Review"

        window.clear_tag_filter()
        assert window.tags_panel.get_selected_tag() is None

        window.on_delete_tag("Review")
        assert "Review" not in [window.tags_panel.tags_list.item(i).text() for i in range(window.tags_panel.tags_list.count())]
        window.close()

    def test_create_delete_collection_updates_panel_selection(self, monkeypatch, qtbot, tmp_path):
        """Creating and clearing a collection filter should keep the panel selection in sync."""
        from gallerybrowser.views.main_window import MainWindow

        _prepare_isolated_app_state(monkeypatch, tmp_path / "state")
        window = MainWindow()
        qtbot.addWidget(window)

        window.on_create_collection("Shots")
        assert "Shots" in [
            window.collections_panel.collections_list.item(i).text()
            for i in range(window.collections_panel.collections_list.count())
        ]

        window.on_collection_selected("Shots")
        assert window.collections_panel.get_selected_collection() == "Shots"

        window.clear_collection_filter()
        assert window.collections_panel.get_selected_collection() is None

        window.on_delete_collection("Shots")
        assert "Shots" not in [
            window.collections_panel.collections_list.item(i).text()
            for i in range(window.collections_panel.collections_list.count())
        ]
        window.close()

    def test_rating_handler_updates_db_and_refreshes_info(self, monkeypatch, qtbot, tmp_path):
        """Rating edits should update the DB and refresh the current selection details."""
        from gallerybrowser.database.manager import DatabaseManager
        from gallerybrowser.views.main_window import MainWindow

        folder = tmp_path / "files"
        folder.mkdir()
        file_path = folder / "clip.png"
        _write_test_png(file_path)

        _prepare_isolated_app_state(monkeypatch, tmp_path / "state")
        db = DatabaseManager()
        db.upsert_file_from_path(str(file_path))

        window = MainWindow()
        qtbot.addWidget(window)
        _prime_window_file_selection(window, file_path)

        refreshed = []

        def capture_refresh(path, file_info=None):
            refreshed.append((path, file_info["path"] if file_info else None))

        monkeypatch.setattr(window.info_pane, "set_file", capture_refresh)

        window.on_set_rating(3)
        assert db.get_rating_for_file(str(file_path)) == 3
        assert refreshed[-1] == (str(file_path), str(file_path))

        window.on_clear_rating()
        assert db.get_rating_for_file(str(file_path)) is None
        assert refreshed[-1] == (str(file_path), str(file_path))
        window.close()

    def test_assign_remove_tag_updates_db_and_refreshes_info(self, monkeypatch, qtbot, tmp_path):
        """Tag assignment/removal handlers should update the DB and refresh selection info."""
        from gallerybrowser.database.manager import DatabaseManager
        from gallerybrowser.views.main_window import MainWindow

        folder = tmp_path / "tag-actions"
        folder.mkdir()
        file_path = folder / "tagged.png"
        _write_test_png(file_path)

        _prepare_isolated_app_state(monkeypatch, tmp_path / "state")
        db = DatabaseManager()
        db.upsert_file_from_path(str(file_path))

        window = MainWindow()
        qtbot.addWidget(window)
        _prime_window_file_selection(window, file_path)

        refreshed = []

        def capture_refresh(path, file_info=None):
            refreshed.append((path, file_info["path"] if file_info else None))

        monkeypatch.setattr(window.info_pane, "set_file", capture_refresh)
        monkeypatch.setattr(
            "gallerybrowser.views.main_window.QInputDialog.getText",
            lambda *args, **kwargs: ("Review", True),
        )
        monkeypatch.setattr(
            "gallerybrowser.views.main_window.QInputDialog.getItem",
            lambda *args, **kwargs: ("Review", True),
        )

        window.on_assign_tag()
        assert [tag.name for tag in db.get_tags_for_file(str(file_path))] == ["Review"]
        assert refreshed[-1] == (str(file_path), str(file_path))

        window.on_remove_tag()
        assert db.get_tags_for_file(str(file_path)) == []
        assert refreshed[-1] == (str(file_path), str(file_path))
        window.close()

    def test_add_remove_collection_updates_db_and_refreshes_info(self, monkeypatch, qtbot, tmp_path):
        """Collection add/remove handlers should update the DB and refresh selection info."""
        from gallerybrowser.database.manager import DatabaseManager
        from gallerybrowser.views.main_window import MainWindow

        folder = tmp_path / "collection-actions"
        folder.mkdir()
        file_path = folder / "shot.png"
        _write_test_png(file_path)

        _prepare_isolated_app_state(monkeypatch, tmp_path / "state")
        db = DatabaseManager()
        db.upsert_file_from_path(str(file_path))

        window = MainWindow()
        qtbot.addWidget(window)
        _prime_window_file_selection(window, file_path)

        refreshed = []

        def capture_refresh(path, file_info=None):
            refreshed.append((path, file_info["path"] if file_info else None))

        monkeypatch.setattr(window.info_pane, "set_file", capture_refresh)
        monkeypatch.setattr(
            "gallerybrowser.views.main_window.QInputDialog.getText",
            lambda *args, **kwargs: ("Shots", True),
        )
        monkeypatch.setattr(
            "gallerybrowser.views.main_window.QInputDialog.getItem",
            lambda *args, **kwargs: ("Shots", True),
        )

        window.on_add_to_collection()
        assert [c.name for c in db.get_collections_for_file(str(file_path))] == ["Shots"]
        assert refreshed[-1] == (str(file_path), str(file_path))

        window.on_remove_from_collection()
        assert db.get_collections_for_file(str(file_path)) == []
        assert refreshed[-1] == (str(file_path), str(file_path))
        window.close()

    def test_collection_filter_respects_folder_and_library_scope(self, monkeypatch, qtbot, tmp_path):
        """Collection filtering should stay folder-scoped unless Library mode is selected."""
        from gallerybrowser.database.manager import DatabaseManager
        from gallerybrowser.views.main_window import MainWindow

        folder_a = tmp_path / "collections-a"
        folder_b = tmp_path / "collections-b"
        folder_a.mkdir()
        folder_b.mkdir()
        file_a = folder_a / "one.png"
        file_b = folder_b / "two.png"
        _write_test_png(file_a, (255, 0, 0))
        _write_test_png(file_b, (0, 255, 0))

        _prepare_isolated_app_state(monkeypatch, tmp_path / "state")
        db = DatabaseManager()
        db.add_file_to_collection(str(file_a), "Shots")
        db.add_file_to_collection(str(file_b), "Shots")

        window = MainWindow()
        qtbot.addWidget(window)
        window.current_path = str(folder_a)
        window.query_state.selected_collection = "Shots"
        window.query_state.scope = "folder"
        window.refresh_view()

        assert [item["path"] for item in window.file_pane.files] == [str(file_a)]

        window.query_state.scope = "library"
        window.refresh_view()

        assert {item["path"] for item in window.file_pane.files} == {str(file_a), str(file_b)}
        window.close()

    def test_unrated_filter_respects_folder_and_library_scope(self, monkeypatch, qtbot, tmp_path):
        """Unrated filtering should return only unrated files in the active scope."""
        from gallerybrowser.database.manager import DatabaseManager
        from gallerybrowser.views.main_window import MainWindow

        folder_a = tmp_path / "ratings-a"
        folder_b = tmp_path / "ratings-b"
        folder_a.mkdir()
        folder_b.mkdir()
        rated_file = folder_a / "rated.png"
        unrated_file = folder_a / "unrated.png"
        other_unrated = folder_b / "other-unrated.png"
        _write_test_png(rated_file, (255, 255, 0))
        _write_test_png(unrated_file, (0, 255, 255))
        _write_test_png(other_unrated, (255, 0, 255))

        _prepare_isolated_app_state(monkeypatch, tmp_path / "state")
        db = DatabaseManager()
        db.upsert_file_from_path(str(rated_file))
        db.upsert_file_from_path(str(unrated_file))
        db.upsert_file_from_path(str(other_unrated))
        db.set_rating(str(rated_file), 5)

        window = MainWindow()
        qtbot.addWidget(window)
        window.current_path = str(folder_a)
        window.query_state.unrated_only = True
        window.query_state.scope = "folder"
        window.refresh_view()

        assert [item["path"] for item in window.file_pane.files] == [str(unrated_file)]

        window.query_state.scope = "library"
        window.refresh_view()

        assert {item["path"] for item in window.file_pane.files} == {
            str(unrated_file),
            str(other_unrated),
        }
        window.close()


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
