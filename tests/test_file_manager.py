"""Tests for file manager."""

import os
import sys
import tempfile
from pathlib import Path

import pytest

# Add src to path
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

from gallerybrowser.core.file_manager import FileManager, FileOperation


class TestFileManager:
    """Test file manager functionality."""

    def setup_method(self):
        """Set up test files."""
        self.temp_dir = tempfile.mkdtemp()
        self.file_manager = FileManager()

        # Create test files
        self.test_file = Path(self.temp_dir) / "test_file.txt"
        self.test_file.write_text("Test content")

        self.test_dir = Path(self.temp_dir) / "test_dir"
        self.test_dir.mkdir()
        (self.test_dir / "file_in_dir.txt").write_text("Content")

    def teardown_method(self):
        """Clean up test files."""
        import shutil

        if Path(self.temp_dir).exists():
            shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test_copy_file(self):
        """Test copying a file."""
        source = str(self.test_file)
        dest = str(Path(self.temp_dir) / "copied_file.txt")

        success = self.file_manager.copy(source, dest)

        assert success
        assert Path(dest).exists()
        assert Path(dest).read_text() == "Test content"
        assert self.file_manager.can_undo()

    def test_copy_undo(self):
        """Test undoing a copy operation."""
        source = str(self.test_file)
        dest = str(Path(self.temp_dir) / "copied_file.txt")

        self.file_manager.copy(source, dest)
        assert Path(dest).exists()

        success = self.file_manager.undo()

        assert success
        assert not Path(dest).exists()
        assert self.file_manager.can_redo()

    def test_copy_redo(self):
        """Test redoing a copy operation."""
        source = str(self.test_file)
        dest = str(Path(self.temp_dir) / "copied_file.txt")

        self.file_manager.copy(source, dest)
        self.file_manager.undo()
        assert not Path(dest).exists()

        success = self.file_manager.redo()

        assert success
        assert Path(dest).exists()

    def test_rename_file(self):
        """Test renaming a file."""
        source = str(self.test_file)
        new_name = "renamed_file.txt"
        expected_dest = Path(self.temp_dir) / new_name

        success = self.file_manager.rename(source, new_name)

        assert success
        assert expected_dest.exists()
        assert not self.test_file.exists()
        assert self.file_manager.can_undo()

    def test_rename_undo(self):
        """Test undoing a rename operation."""
        source = str(self.test_file)
        new_name = "renamed_file.txt"

        self.file_manager.rename(source, new_name)

        success = self.file_manager.undo()

        assert success
        assert self.test_file.exists()

    def test_undo_stack_limit(self):
        """Test that undo stack is limited."""
        # Create many files and copy them
        for i in range(60):
            source_file = Path(self.temp_dir) / f"file_{i}.txt"
            source_file.write_text(f"Content {i}")
            dest = str(Path(self.temp_dir) / f"copied_{i}.txt")
            self.file_manager.copy(str(source_file), dest)

        # Stack should be limited to 50
        assert len(self.file_manager.undo_stack) == 50

    def test_clear_history(self):
        """Test clearing history."""
        source = str(self.test_file)
        dest = str(Path(self.temp_dir) / "copied.txt")

        self.file_manager.copy(source, dest)
        assert self.file_manager.can_undo()

        self.file_manager.clear_history()

        assert not self.file_manager.can_undo()
        assert not self.file_manager.can_redo()


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
