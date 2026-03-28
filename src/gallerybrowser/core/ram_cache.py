"""LRU frame cache for decoded images (QImage).

Stores decoded frames in RAM with configurable memory budget.
Thread-safe — multiple loader threads can insert concurrently while
the GUI thread reads.

At 1920x1080 RGB888 (~6 MB/frame), a 2 GB budget holds ~330 frames.
"""

import threading
from collections import OrderedDict
from typing import Optional

from PyQt6.QtGui import QImage


class RAMCache:
    """Thread-safe LRU cache for decoded QImage frames.

    Parameters
    ----------
    max_bytes : int
        Maximum RAM usage in bytes.  Defaults to 2 GiB.
    """

    def __init__(self, max_bytes: int = 2 * 1024 * 1024 * 1024):
        self._max_bytes = max_bytes
        self._current_bytes = 0
        # key → (QImage, byte_size)
        self._cache: OrderedDict[str, tuple[QImage, int]] = OrderedDict()
        self._lock = threading.Lock()

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def get(self, key: str) -> Optional[QImage]:
        """Retrieve a cached frame, or None if not present.

        Moves the entry to the end (most-recently-used).
        """
        with self._lock:
            entry = self._cache.get(key)
            if entry is None:
                return None
            self._cache.move_to_end(key)
            return entry[0]

    def put(self, key: str, image: QImage) -> None:
        """Insert a decoded frame.

        If the key already exists, it is replaced.  Old entries are
        evicted (LRU) until the memory budget is satisfied.
        """
        byte_size = _image_bytes(image)
        if byte_size <= 0:
            return

        with self._lock:
            # Replace existing
            if key in self._cache:
                old_img, old_size = self._cache.pop(key)
                self._current_bytes -= old_size

            # Evict until we fit
            while self._cache and (self._current_bytes + byte_size > self._max_bytes):
                _, (_, evicted_size) = self._cache.popitem(last=False)
                self._current_bytes -= evicted_size

            self._cache[key] = (image, byte_size)
            self._current_bytes += byte_size

    def set_max_bytes(self, max_bytes: int) -> None:
        """Update the memory budget at runtime.

        Evicts LRU entries if the new budget is smaller than current usage.
        """
        with self._lock:
            self._max_bytes = max_bytes
            while self._cache and self._current_bytes > self._max_bytes:
                _, (_, evicted_size) = self._cache.popitem(last=False)
                self._current_bytes -= evicted_size

    def contains(self, key: str) -> bool:
        """Check if a key is cached (without promoting it)."""
        with self._lock:
            return key in self._cache

    def clear(self) -> None:
        """Discard all cached frames."""
        with self._lock:
            self._cache.clear()
            self._current_bytes = 0

    def evict_prefix(self, prefix: str) -> int:
        """Remove all entries whose key starts with *prefix*.

        Useful when switching sequences — evict frames from the old one.
        Returns the number of entries removed.
        """
        with self._lock:
            to_remove = [k for k in self._cache if k.startswith(prefix)]
            for k in to_remove:
                _, sz = self._cache.pop(k)
                self._current_bytes -= sz
            return len(to_remove)

    def evict_except(self, keep_keys: set) -> int:
        """Remove all entries whose key is NOT in *keep_keys*.

        Used for rolling-window eviction: only keep frames near the
        current playback position.  Returns the number evicted.
        """
        with self._lock:
            to_remove = [k for k in self._cache if k not in keep_keys]
            for k in to_remove:
                _, sz = self._cache.pop(k)
                self._current_bytes -= sz
            return len(to_remove)

    def cached_keys(self) -> set:
        """Return the set of keys currently in cache.

        Used by the UI to render per-frame cache status indicators.
        """
        with self._lock:
            return set(self._cache.keys())

    # ------------------------------------------------------------------
    # Diagnostics
    # ------------------------------------------------------------------

    @property
    def used_bytes(self) -> int:
        with self._lock:
            return self._current_bytes

    @property
    def max_bytes(self) -> int:
        return self._max_bytes

    @property
    def frame_count(self) -> int:
        with self._lock:
            return len(self._cache)

    def stats(self) -> dict:
        """Return a dict of cache statistics."""
        with self._lock:
            return {
                "frames": len(self._cache),
                "used_mb": self._current_bytes / (1024 * 1024),
                "max_mb": self._max_bytes / (1024 * 1024),
                "pct": (self._current_bytes / self._max_bytes * 100)
                if self._max_bytes > 0
                else 0.0,
            }


def _image_bytes(image: QImage) -> int:
    """Estimate the memory footprint of a QImage."""
    if image.isNull():
        return 0
    return image.sizeInBytes()
