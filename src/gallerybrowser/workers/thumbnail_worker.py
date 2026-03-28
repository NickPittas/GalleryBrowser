"""Background worker for thumbnail generation."""

import queue
import threading

from PyQt6.QtCore import QThread, pyqtSignal
from PyQt6.QtGui import QImage

from gallerybrowser.core.thumbnail import generate_thumbnail


class ThumbnailWorker(QThread):
    """Worker thread for generating thumbnails in the background.

    Uses a thread-safe queue and emits QImage (not QPixmap) because
    QPixmap is not safe to create outside the GUI thread.
    """

    thumbnail_ready = pyqtSignal(str, int, QImage)  # file_path, size, qimage
    thumbnail_error = pyqtSignal(str, str)  # file_path, error_message
    progress = pyqtSignal(int, int)  # current, total

    def __init__(self, parent=None):
        super().__init__(parent)
        self._queue: queue.Queue = queue.Queue()
        self.current_size: int = 128
        self._stop_event = threading.Event()
        # Track queued items to avoid duplicates
        self._queued: set = set()
        self._queued_lock = threading.Lock()

    def add_to_queue(self, file_path: str, size: int = 128, priority: bool = False):
        """Add a file to the thumbnail generation queue.

        Args:
            file_path: Path to the file
            size: Desired thumbnail size
            priority: If True, item is still appended (Queue has no insert),
                      but it will be processed FIFO. For true priority,
                      callers can use a PriorityQueue in the future.
        """
        key = (file_path, size)
        with self._queued_lock:
            if key in self._queued:
                return
            self._queued.add(key)
        self._queue.put({"path": file_path, "size": size})

    def clear_queue(self):
        """Clear the thumbnail queue."""
        while True:
            try:
                self._queue.get_nowait()
            except queue.Empty:
                break
        with self._queued_lock:
            self._queued.clear()

    def stop(self):
        """Stop the worker thread."""
        self._stop_event.set()
        # Wait with a timeout to avoid hanging if the thread is stuck
        self.wait(5000)

    def run(self):
        """Process the thumbnail queue."""
        self._stop_event.clear()
        total = self._queue.qsize()

        processed = 0
        while not self._stop_event.is_set():
            try:
                item = self._queue.get(timeout=0.1)
            except queue.Empty:
                break

            file_path = item["path"]
            size = item["size"]

            # Remove from tracking set
            with self._queued_lock:
                self._queued.discard((file_path, size))

            try:
                # generate_thumbnail returns QImage (thread-safe)
                qimage = generate_thumbnail(file_path, size)

                if qimage and not qimage.isNull():
                    self.thumbnail_ready.emit(file_path, size, qimage)
                else:
                    self.thumbnail_error.emit(file_path, "Failed to generate thumbnail")

            except Exception as e:
                self.thumbnail_error.emit(file_path, str(e))

            # Update progress
            processed += 1
            self.progress.emit(processed, max(total, processed))

            # Small delay to prevent starving the GUI thread
            self.msleep(10)
