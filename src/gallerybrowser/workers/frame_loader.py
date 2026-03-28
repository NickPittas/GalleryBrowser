"""Background frame loader for image sequences.

Loads frames into a RAMCache using OIIO (or PIL fallback), with
bidirectional read-ahead around the current playback position.

Usage::

    cache = RAMCache()
    loader = FrameLoader(cache)
    loader.frame_ready.connect(on_frame)  # key, frame_index
    loader.set_sequence(frame_paths)      # list of file paths
    loader.load_around(42)                # start loading near frame 42
    ...
    loader.stop()
"""

import threading
from typing import List, Optional

from PyQt6.QtCore import QThread, pyqtSignal
from PyQt6.QtGui import QImage

from gallerybrowser.config import HAS_OIIO
from gallerybrowser.core.ram_cache import RAMCache


class FrameLoader(QThread):
    """Persistent background thread that loads sequence frames into a RAMCache.

    Signals
    -------
    frame_ready(str, int)
        Emitted when a frame has been loaded into the cache.
        Arguments: (file_path, frame_index_in_sequence).
    batch_done()
        Emitted when the current read-ahead batch is complete.
    """

    frame_ready = pyqtSignal(str, int)  # file_path, index
    batch_done = pyqtSignal()

    def __init__(self, cache: RAMCache, parent=None):
        super().__init__(parent)
        self._cache = cache
        self._frame_paths: List[str] = []
        self._center: int = 0
        self._default_readahead: int = 60
        self._readahead: int = 60  # frames ahead + behind to load
        self._generation: int = 0  # bumped on each new request
        self._stop_flag = False

        self._lock = threading.Lock()
        self._event = threading.Event()

    # ------------------------------------------------------------------
    # Public API (call from any thread)
    # ------------------------------------------------------------------

    def set_sequence(self, frame_paths: List[str]) -> None:
        """Set or change the current sequence.

        Clears any pending work and starts fresh.
        """
        with self._lock:
            self._frame_paths = list(frame_paths)
            self._generation += 1

    def load_around(self, index: int, readahead: int = 0) -> None:
        """Request loading frames centered on *index*.

        Supersedes any previous pending request.
        Evicts distant frames only when the sequence is larger than the
        cache budget — otherwise lets the LRU policy in RAMCache handle
        memory pressure so small sequences can be fully cached.

        *readahead* may raise the readahead window but never shrinks it
        below the default (60).  Pass 0 to keep the current value.
        """
        with self._lock:
            self._center = max(0, min(index, len(self._frame_paths) - 1))
            if readahead > 0:
                # Only allow increasing — never shrink below default
                self._readahead = max(self._default_readahead, readahead)
            self._generation += 1

            # Only do rolling-window eviction when the sequence is large
            # enough that it can't all fit in the cache.  For sequences
            # that fit entirely in RAM, let the LRU eviction in
            # RAMCache.put() handle things — no need to throw frames away.
            paths = self._frame_paths
            if paths:
                # Estimate: ~8 MB per 1080p RGBA frame (conservative)
                est_frame_bytes = 8 * 1024 * 1024
                est_total = len(paths) * est_frame_bytes
                if est_total > self._cache.max_bytes:
                    # Sequence won't fit — evict distant frames
                    keep_radius = self._readahead * 2
                    lo = max(0, self._center - keep_radius)
                    hi = min(len(paths), self._center + keep_radius + 1)
                    keep_keys = set(paths[lo:hi])
                    self._cache.evict_except(keep_keys)

        self._event.set()

    def load_single(self, index: int) -> None:
        """Request loading a single specific frame with highest priority.

        This is used for the currently displayed frame — we want it
        loaded before any read-ahead work.
        """
        self.load_around(index, readahead=0)

    def set_readahead(self, count: int) -> None:
        """Change the read-ahead window size (also updates the default)."""
        with self._lock:
            count = max(1, count)
            self._default_readahead = count
            self._readahead = count

    def stop(self) -> None:
        """Signal the thread to exit."""
        self._stop_flag = True
        self._event.set()

    # ------------------------------------------------------------------
    # Thread body
    # ------------------------------------------------------------------

    def run(self) -> None:
        while not self._stop_flag:
            self._event.wait()
            self._event.clear()
            if self._stop_flag:
                break

            with self._lock:
                paths = self._frame_paths
                center = self._center
                readahead = self._readahead
                gen = self._generation

            if not paths:
                continue

            # Determine effective load radius.  If the whole sequence
            # fits in the cache, load everything (prioritising near
            # center); otherwise load only the readahead window.
            est_frame_bytes = 8 * 1024 * 1024  # ~8 MB per 1080p RGBA
            est_total = len(paths) * est_frame_bytes
            if est_total <= self._cache.max_bytes:
                load_radius = len(paths)  # entire sequence
            else:
                load_radius = readahead

            # Build load order: current frame first, then alternating
            # forward/backward from center (like DJV/RV).
            indices = self._build_load_order(center, len(paths), load_radius)

            for idx in indices:
                # Check for superseding request
                with self._lock:
                    if self._generation != gen:
                        break
                if self._stop_flag:
                    break

                path = paths[idx]
                if self._cache.contains(path):
                    continue

                qimage = self._decode_frame(path)
                if qimage is not None:
                    self._cache.put(path, qimage)
                    self.frame_ready.emit(path, idx)

            # If we completed without being superseded, signal done
            with self._lock:
                if self._generation == gen:
                    self.batch_done.emit()

    # ------------------------------------------------------------------
    # Internals
    # ------------------------------------------------------------------

    @staticmethod
    def _build_load_order(center: int, total: int, readahead: int) -> List[int]:
        """Build a list of frame indices to load.

        Order: center first, then alternating forward/backward.
        """
        if total == 0:
            return []

        result = [center]
        for offset in range(1, readahead + 1):
            fwd = center + offset
            bwd = center - offset
            if fwd < total:
                result.append(fwd)
            if bwd >= 0:
                result.append(bwd)
        return result

    @staticmethod
    def _decode_frame(path: str) -> Optional[QImage]:
        """Decode a single frame to QImage.

        Tries PIL first (for JPEG/PNG/etc.), then OIIO (for EXR/HDR).
        Uses the NAS-buffered I/O path from image_io for OIIO.
        """
        # Try PIL first (fast for common formats)
        qimage = _load_pil(path)
        if qimage is not None:
            return qimage

        # Fall back to OIIO (EXR, HDR, etc.)
        if HAS_OIIO:
            from gallerybrowser.core.image_io import load_oiio_qimage

            return load_oiio_qimage(path, max_dim=0)  # Full resolution

        return None


def _load_pil(path: str) -> Optional[QImage]:
    """Load an image via PIL and return a QImage, or None on failure."""
    try:
        from PIL import Image

        img = Image.open(path)

        if img.mode not in ("RGB", "RGBA"):
            img = img.convert("RGBA" if img.mode in ("RGBA", "LA", "PA", "P") else "RGB")

        if img.mode == "RGBA":
            data = img.tobytes("raw", "RGBA")
            qimage = QImage(
                data, img.width, img.height, img.width * 4, QImage.Format.Format_RGBA8888
            )
        else:
            data = img.tobytes("raw", "RGB")
            qimage = QImage(data, img.width, img.height, img.width * 3, QImage.Format.Format_RGB888)

        qimage = qimage.copy()  # Detach from data buffer
        img.close()
        return qimage
    except Exception:
        return None
