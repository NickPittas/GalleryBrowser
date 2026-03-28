"""File operations with undo/redo support."""

import os
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, List, Optional

from PyQt6.QtCore import QObject, pyqtSignal


@dataclass
class FileOperation:
    """Represents a file operation that can be undone/redone."""

    operation_type: str  # 'copy', 'move', 'delete', 'rename'
    source: str
    destination: Optional[str] = None
    original_name: Optional[str] = None
    undo_func: Optional[Callable] = None
    redo_func: Optional[Callable] = None


class FileManager(QObject):
    """Manages file operations with undo/redo support."""

    operation_started = pyqtSignal(str)  # operation description
    operation_completed = pyqtSignal(str)  # operation description
    operation_failed = pyqtSignal(str, str)  # operation, error message
    progress_updated = pyqtSignal(int, int)  # current, total

    def __init__(self):
        super().__init__()
        self.undo_stack: List[FileOperation] = []
        self.redo_stack: List[FileOperation] = []
        self.max_undo = 50

    def can_undo(self) -> bool:
        """Check if undo is available."""
        return len(self.undo_stack) > 0

    def can_redo(self) -> bool:
        """Check if redo is available."""
        return len(self.redo_stack) > 0

    def undo(self) -> bool:
        """Undo the last operation.

        Returns:
            True if successful, False otherwise
        """
        if not self.can_undo():
            return False

        operation = self.undo_stack.pop()

        try:
            if operation.operation_type == "copy":
                # Undo copy = delete the copied file
                if operation.destination and Path(operation.destination).exists():
                    if Path(operation.destination).is_dir():
                        shutil.rmtree(operation.destination)
                    else:
                        os.remove(operation.destination)

            elif operation.operation_type == "move":
                # Undo move = move back to source
                if operation.destination and operation.source:
                    self._do_move(operation.destination, operation.source)

            elif operation.operation_type == "delete":
                # Undo delete is not supported (file is in trash)
                self.operation_failed.emit("Undo Delete", "Cannot undo delete - file is in trash")
                self.undo_stack.append(operation)  # Put it back
                return False

            elif operation.operation_type == "rename":
                # Undo rename = rename back
                if operation.destination and operation.original_name:
                    self._do_move(operation.destination, operation.original_name)

            # Move to redo stack
            self.redo_stack.append(operation)
            if len(self.redo_stack) > self.max_undo:
                self.redo_stack.pop(0)

            self.operation_completed.emit(f"Undid {operation.operation_type}")
            return True

        except Exception as e:
            self.operation_failed.emit(f"Undo {operation.operation_type}", str(e))
            # Put it back on undo stack
            self.undo_stack.append(operation)
            return False

    def redo(self) -> bool:
        """Redo the last undone operation.

        Returns:
            True if successful, False otherwise
        """
        if not self.can_redo():
            return False

        operation = self.redo_stack.pop()

        try:
            if operation.operation_type == "copy":
                success = self._do_copy(operation.source, operation.destination)

            elif operation.operation_type == "move":
                success = self._do_move(operation.source, operation.destination)

            elif operation.operation_type == "rename":
                success = self._do_rename(operation.source, operation.destination)

            else:
                success = False

            if success:
                self.undo_stack.append(operation)
                self.operation_completed.emit(f"Redid {operation.operation_type}")
                return True
            else:
                self.redo_stack.append(operation)
                return False

        except Exception as e:
            self.operation_failed.emit(f"Redo {operation.operation_type}", str(e))
            self.redo_stack.append(operation)
            return False

    def copy(self, source: str, destination: str) -> bool:
        """Copy a file or directory.

        Args:
            source: Source path
            destination: Destination path

        Returns:
            True if successful, False otherwise
        """
        self.operation_started.emit(f"Copying {Path(source).name}...")

        try:
            success = self._do_copy(source, destination)

            if success:
                operation = FileOperation(
                    operation_type="copy", source=source, destination=destination
                )
                self._add_to_undo_stack(operation)
                self.operation_completed.emit(f"Copied {Path(source).name}")

            return success

        except Exception as e:
            self.operation_failed.emit("Copy", str(e))
            return False

    def move(self, source: str, destination: str) -> bool:
        """Move a file or directory.

        Args:
            source: Source path
            destination: Destination path

        Returns:
            True if successful, False otherwise
        """
        self.operation_started.emit(f"Moving {Path(source).name}...")

        try:
            success = self._do_move(source, destination)

            if success:
                operation = FileOperation(
                    operation_type="move", source=source, destination=destination
                )
                self._add_to_undo_stack(operation)
                self.operation_completed.emit(f"Moved {Path(source).name}")

            return success

        except Exception as e:
            self.operation_failed.emit("Move", str(e))
            return False

    def delete(self, path: str, use_trash: bool = True) -> bool:
        """Delete a file or directory.

        Args:
            path: Path to delete
            use_trash: If True, move to trash. If False, permanent delete.

        Returns:
            True if successful, False otherwise
        """
        self.operation_started.emit(f"Deleting {Path(path).name}...")

        try:
            if use_trash:
                success = self._do_trash(path)
            else:
                success = self._do_permanent_delete(path)

            if success:
                operation = FileOperation(operation_type="delete", source=path)
                self._add_to_undo_stack(operation)
                self.operation_completed.emit(f"Deleted {Path(path).name}")

            return success

        except Exception as e:
            self.operation_failed.emit("Delete", str(e))
            return False

    def rename(self, source: str, new_name: str) -> bool:
        """Rename a file or directory.

        Args:
            source: Current path
            new_name: New name (not full path)

        Returns:
            True if successful, False otherwise
        """
        source_path = Path(source)
        destination = source_path.parent / new_name

        self.operation_started.emit(f"Renaming {source_path.name} to {new_name}...")

        try:
            success = self._do_rename(source, str(destination))

            if success:
                operation = FileOperation(
                    operation_type="rename",
                    source=source,
                    destination=str(destination),
                    original_name=source,
                )
                self._add_to_undo_stack(operation)
                self.operation_completed.emit(f"Renamed to {new_name}")

            return success

        except Exception as e:
            self.operation_failed.emit("Rename", str(e))
            return False

    def _do_copy(self, source: str, destination: str) -> bool:
        """Perform the actual copy operation.

        Uses ``gio copy`` first (handles CIFS/NFS metadata correctly),
        then falls back to shutil.
        """
        source_path = Path(source)

        if source_path.is_dir():
            # gio copy doesn't do recursive dirs well, use shutil
            shutil.copytree(source, destination, dirs_exist_ok=True)
            return True

        # Try gio copy first — avoids EROFS from xattr issues on CIFS/NFS
        try:
            result = subprocess.run(
                ["gio", "copy", source, destination],
                capture_output=True,
                text=True,
            )
            if result.returncode == 0:
                return True
        except FileNotFoundError:
            pass  # gio not available

        # Fallback to shutil
        shutil.copy2(source, destination)
        return True

    def _do_move(self, source: str, destination: str) -> bool:
        """Perform the actual move operation.

        Uses ``gio move`` first (handles cross-device and CIFS/NFS
        correctly), then falls back to shutil.
        """
        # Try gio move first — avoids EROFS from xattr issues on CIFS/NFS
        try:
            result = subprocess.run(
                ["gio", "move", source, destination],
                capture_output=True,
                text=True,
            )
            if result.returncode == 0:
                return True
        except FileNotFoundError:
            pass  # gio not available

        # Fallback to shutil
        shutil.move(source, destination)
        return True

    def _do_trash(self, path: str) -> bool:
        """Move file to trash using gio trash."""
        # Try gio trash (GNOME/freedesktop standard)
        try:
            result = subprocess.run(["gio", "trash", path], capture_output=True, text=True)
            if result.returncode == 0:
                return True
            gio_err = result.stderr.strip()
        except FileNotFoundError:
            gio_err = "gio not found"

        # Fallback to trash-cli if available
        try:
            result = subprocess.run(["trash-put", path], capture_output=True, text=True)
            if result.returncode == 0:
                return True
            trash_err = result.stderr.strip()
        except FileNotFoundError:
            trash_err = "trash-put not found"

        raise Exception(
            f"Failed to move to trash.\n  gio trash: {gio_err}\n  trash-put: {trash_err}"
        )

    def _do_permanent_delete(self, path: str) -> bool:
        """Permanently delete file or directory."""
        path_obj = Path(path)

        if path_obj.is_dir():
            shutil.rmtree(path)
        else:
            os.remove(path)

        return True

    def _do_rename(self, source: str, destination: str) -> bool:
        """Perform the actual rename operation.

        Uses ``gio move`` first (handles CIFS/NFS correctly),
        then falls back to shutil.
        """
        # Try gio move first — avoids EROFS from xattr issues on CIFS/NFS
        try:
            result = subprocess.run(
                ["gio", "move", source, destination],
                capture_output=True,
                text=True,
            )
            if result.returncode == 0:
                return True
        except FileNotFoundError:
            pass  # gio not available

        # Fallback to shutil
        shutil.move(source, destination)
        return True

    def _add_to_undo_stack(self, operation: FileOperation):
        """Add operation to undo stack and clear redo stack."""
        self.undo_stack.append(operation)
        self.redo_stack.clear()  # Clear redo stack on new operation

        # Limit undo stack size
        if len(self.undo_stack) > self.max_undo:
            self.undo_stack.pop(0)

    def clear_history(self):
        """Clear undo/redo history."""
        self.undo_stack.clear()
        self.redo_stack.clear()
