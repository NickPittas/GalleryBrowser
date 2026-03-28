"""Single instance enforcement using a cache-local lock file."""

from pathlib import Path

from PyQt6.QtCore import QLockFile

from gallerybrowser.config import Config


class SingleInstance:
    """Ensures only one instance of the application runs."""

    def __init__(self, app_name="gallerybrowser"):
        self.app_name = app_name
        self.lock_file = None

        lock_dir = Config.get_cache_dir() / "locks"
        lock_dir.mkdir(parents=True, exist_ok=True)
        self.lock_path = str(lock_dir / f"{app_name}.lock")

    def try_lock(self):
        """Try to acquire the lock.

        Returns:
            True if lock acquired, False if another instance is running
        """
        self.lock_file = QLockFile(self.lock_path)
        self.lock_file.setStaleLockTime(1000)

        if not self.lock_file.tryLock():
            # If a previous instance crashed, clear the stale lock and retry once.
            self.lock_file.removeStaleLockFile()
            if self.lock_file.tryLock():
                return True
            print(f"Another instance of {self.app_name} is already running.")
            return False
        return True

    def unlock(self):
        """Release the lock."""
        if self.lock_file:
            self.lock_file.unlock()
            self.lock_file = None

    def __del__(self):
        """Cleanup on destruction."""
        self.unlock()
