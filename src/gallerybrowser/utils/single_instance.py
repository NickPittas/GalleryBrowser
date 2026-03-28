"""Single instance enforcement using lock file."""

import os
import sys
from pathlib import Path

from PyQt6.QtCore import QLockFile, QDir


class SingleInstance:
    """Ensures only one instance of the application runs."""
    
    def __init__(self, app_name="gallerybrowser"):
        self.app_name = app_name
        self.lock_file = None
        
        # Use temp directory for lock file
        temp_dir = QDir.tempPath()
        self.lock_path = os.path.join(temp_dir, f"{app_name}.lock")
        
    def try_lock(self):
        """Try to acquire the lock.
        
        Returns:
            True if lock acquired, False if another instance is running
        """
        self.lock_file = QLockFile(self.lock_path)
        self.lock_file.setStaleLockTime(0)  # Lock is stale immediately if holder crashes
        
        if not self.lock_file.tryLock():
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
