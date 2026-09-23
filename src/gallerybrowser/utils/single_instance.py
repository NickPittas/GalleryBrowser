"""Single-instance enforcement with a kernel advisory lock."""

import fcntl
import os
from pathlib import Path

from gallerybrowser.config import Config


class SingleInstance:
    """Ensures only one instance of the application runs."""

    def __init__(self, app_name="gallerybrowser"):
        self.app_name = app_name
        self.lock_fd = None

        lock_dir = Config.get_cache_dir() / "locks"
        lock_dir.mkdir(parents=True, exist_ok=True)
        self.lock_path = str(lock_dir / f"{app_name}.lock")

    def try_lock(self):
        """Acquire the lock, or return False if another instance holds it."""
        fd = os.open(self.lock_path, os.O_CREAT | os.O_RDWR, 0o600)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            os.close(fd)
            print(f"Another instance of {self.app_name} is already running.")
            return False
        except OSError:
            os.close(fd)
            raise

        os.ftruncate(fd, 0)
        self.lock_fd = fd
        return True

    def unlock(self):
        """Release the kernel lock; closing also releases it after a crash."""
        if self.lock_fd is not None:
            os.close(self.lock_fd)
            self.lock_fd = None

    def __del__(self):
        self.unlock()
