"""Tests for thumbnail generation."""

import os
import sys
import tempfile
from pathlib import Path

import pytest
from PIL import Image

# Add src to path
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

from gallerybrowser.config import Config
from gallerybrowser.core.thumbnail import (
    generate_image_thumbnail,
    generate_thumbnail,
    get_placeholder_icon,
    get_thumbnail_path,
    pil_to_pixmap,
)


class TestThumbnailGeneration:
    """Test thumbnail generation functionality."""

    def setup_method(self):
        """Set up test images."""
        self.temp_dir = tempfile.mkdtemp()

        # Create a test image
        self.test_image_path = Path(self.temp_dir) / "test_image.png"
        img = Image.new("RGB", (800, 600), color=(255, 0, 0))
        img.save(self.test_image_path)

    def teardown_method(self):
        """Clean up test files."""
        import shutil

        if Path(self.temp_dir).exists():
            shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_pil_to_pixmap_conversion(self, qtbot):
        """Test PIL to QPixmap conversion."""
        from PyQt6.QtWidgets import QApplication

        # Ensure QApplication exists
        if not QApplication.instance():
            QApplication([])

        img = Image.new("RGB", (100, 100), color=(0, 0, 255))
        pixmap = pil_to_pixmap(img)

        assert pixmap is not None
        assert not pixmap.isNull()
        assert pixmap.width() == 100
        assert pixmap.height() == 100

    def test_get_thumbnail_path(self):
        """Test thumbnail path generation."""
        path = "/home/user/image.png"
        size = 128

        thumb_path = get_thumbnail_path(path, size)

        # Should contain hash of path
        assert "gallerybrowser" in thumb_path
        assert "128" in thumb_path
        assert thumb_path.endswith(".png")

    def test_get_placeholder_icon(self):
        """Test placeholder icon selection."""
        assert get_placeholder_icon("image") == "fa5s.image"
        assert get_placeholder_icon("video") == "fa5s.video"
        assert get_placeholder_icon("unknown") == "fa5s.file"

    def test_generate_image_thumbnail(self, qtbot):
        """Test thumbnail generation from image."""
        from PyQt6.QtWidgets import QApplication

        # Ensure QApplication exists
        if not QApplication.instance():
            QApplication([])

        cache_path = Path(self.temp_dir) / "cache" / "thumb.png"

        pixmap = generate_image_thumbnail(str(self.test_image_path), 128, str(cache_path))

        assert pixmap is not None
        assert not pixmap.isNull()
        assert cache_path.exists()

    def test_generate_thumbnail_with_caching(self, qtbot):
        """Test thumbnail caching."""
        from PyQt6.QtWidgets import QApplication

        # Ensure QApplication exists
        if not QApplication.instance():
            QApplication([])

        # First generation
        pixmap1 = generate_thumbnail(str(self.test_image_path), 128)
        assert pixmap1 is not None

        # Second generation should load from cache
        pixmap2 = generate_thumbnail(str(self.test_image_path), 128)
        assert pixmap2 is not None
        assert not pixmap2.isNull()


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
