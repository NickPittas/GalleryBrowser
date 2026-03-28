"""QTimer-driven image-sequence playback controller.

Ties together a RAMCache and FrameLoader to provide play/pause/stop,
frame stepping, timeline scrubbing, looping and variable FPS.

Signals
-------
frame_changed(int, QImage)
    Emitted on the GUI thread whenever the displayed frame should update.
    Arguments: (frame_index, decoded QImage).
playback_state_changed(str)
    "playing", "paused", or "stopped".
cache_progress(int, int)
    (cached_count, total_count) — for a loading indicator.
"""

from typing import List, Optional

from PyQt6.QtCore import QObject, QTimer, pyqtSignal
from PyQt6.QtGui import QImage

from gallerybrowser.core.ram_cache import RAMCache
from gallerybrowser.workers.frame_loader import FrameLoader


class SequencePlayer(QObject):
    """High-level playback controller for image sequences.

    Parameters
    ----------
    parent : QObject | None
    max_cache_bytes : int
        Memory budget for the frame cache (default 2 GiB).
    """

    frame_changed = pyqtSignal(int, QImage)  # index, image
    playback_state_changed = pyqtSignal(str)  # "playing" / "paused" / "stopped"
    cache_progress = pyqtSignal(int, int)  # cached, total
    cache_bitmap_changed = pyqtSignal(object)  # set of cached 0-based indices

    def __init__(self, parent=None, max_cache_bytes: int = 2 * 1024 * 1024 * 1024):
        super().__init__(parent)

        self._cache = RAMCache(max_bytes=max_cache_bytes)
        self._loader = FrameLoader(self._cache)
        self._loader.frame_ready.connect(self._on_frame_loaded)

        self._frame_paths: List[str] = []
        self._frame_range: tuple[int, int] = (0, 0)  # (start_frame_number, end_frame_number)
        self._current_index: int = 0  # 0-based index into _frame_paths
        self._fps: float = 24.0
        self._speed: float = 1.0
        self._loop: bool = True
        self._is_playing: bool = False

        # Playback timer
        self._timer = QTimer(self)
        self._timer.timeout.connect(self._tick)

        # Start the loader thread once
        self._loader.start()

    # ------------------------------------------------------------------
    # Runtime configuration
    # ------------------------------------------------------------------

    def set_max_cache_bytes(self, max_bytes: int) -> None:
        """Update the RAM cache budget at runtime."""
        self._cache.set_max_bytes(max_bytes)

    # ------------------------------------------------------------------
    # Sequence setup
    # ------------------------------------------------------------------

    def set_sequence(
        self,
        frame_paths: List[str],
        frame_range: tuple[int, int] = (0, 0),
        fps: float = 24.0,
    ) -> None:
        """Load a new sequence.

        Parameters
        ----------
        frame_paths : list[str]
            Ordered list of frame file paths.
        frame_range : (int, int)
            Display frame numbers (e.g. (1001, 1645)).  If (0, 0),
            defaults to (0, len-1).
        fps : float
            Target playback framerate.
        """
        self.stop()
        self._cache.clear()

        self._frame_paths = list(frame_paths)
        self._fps = max(1.0, fps)

        if frame_range == (0, 0) and frame_paths:
            self._frame_range = (0, len(frame_paths) - 1)
        else:
            self._frame_range = frame_range

        self._current_index = 0
        self._loader.set_sequence(self._frame_paths)

        # Kick off loading around frame 0
        if self._frame_paths:
            self._loader.load_around(0)
            # Immediately try to display the first frame
            self._display_frame(0)

    # ------------------------------------------------------------------
    # Playback controls
    # ------------------------------------------------------------------

    def play(self) -> None:
        """Start or resume playback."""
        if not self._frame_paths:
            return
        self._is_playing = True
        interval = max(1, int(1000.0 / (self._fps * self._speed)))
        self._timer.start(interval)
        self.playback_state_changed.emit("playing")

    def pause(self) -> None:
        """Pause playback."""
        self._timer.stop()
        self._is_playing = False
        self.playback_state_changed.emit("paused")

    def stop(self) -> None:
        """Stop playback and reset to frame 0."""
        self._timer.stop()
        self._is_playing = False
        self._current_index = 0
        self.playback_state_changed.emit("stopped")

    def toggle_playback(self) -> None:
        """Toggle between play and pause."""
        if self._is_playing:
            self.pause()
        else:
            self.play()

    @property
    def is_playing(self) -> bool:
        return self._is_playing

    # ------------------------------------------------------------------
    # Navigation
    # ------------------------------------------------------------------

    def seek(self, index: int) -> None:
        """Jump to a specific 0-based frame index."""
        if not self._frame_paths:
            return
        index = max(0, min(index, len(self._frame_paths) - 1))
        self._current_index = index
        self._loader.load_around(index)
        self._display_frame(index)

    def step(self, direction: int) -> None:
        """Step forward (+1) or backward (-1) by one frame.

        Pauses playback if currently playing.
        """
        if self._is_playing:
            self.pause()
        new_idx = self._current_index + direction
        new_idx = max(0, min(new_idx, len(self._frame_paths) - 1))
        self.seek(new_idx)

    def go_to_start(self) -> None:
        self.seek(0)

    def go_to_end(self) -> None:
        if self._frame_paths:
            self.seek(len(self._frame_paths) - 1)

    @property
    def current_index(self) -> int:
        return self._current_index

    @property
    def frame_count(self) -> int:
        return len(self._frame_paths)

    @property
    def current_frame_number(self) -> int:
        """The display frame number (e.g. 1042) for the current index."""
        if not self._frame_paths:
            return 0
        return self._frame_range[0] + self._current_index

    @property
    def frame_range(self) -> tuple[int, int]:
        return self._frame_range

    # ------------------------------------------------------------------
    # Speed / FPS / Loop
    # ------------------------------------------------------------------

    def set_fps(self, fps: float) -> None:
        """Change the target FPS."""
        self._fps = max(1.0, fps)
        if self._is_playing:
            interval = max(1, int(1000.0 / (self._fps * self._speed)))
            self._timer.setInterval(interval)

    def set_speed(self, speed: float) -> None:
        """Change the playback speed multiplier (e.g. 0.5, 1.0, 2.0)."""
        self._speed = max(0.1, speed)
        if self._is_playing:
            interval = max(1, int(1000.0 / (self._fps * self._speed)))
            self._timer.setInterval(interval)

    def set_loop(self, loop: bool) -> None:
        self._loop = loop

    @property
    def fps(self) -> float:
        return self._fps

    @property
    def speed(self) -> float:
        return self._speed

    @property
    def loop(self) -> bool:
        return self._loop

    # ------------------------------------------------------------------
    # Internal
    # ------------------------------------------------------------------

    def _tick(self) -> None:
        """Called by the timer at each frame interval."""
        next_idx = self._current_index + 1

        if next_idx >= len(self._frame_paths):
            if self._loop:
                next_idx = 0
            else:
                self.pause()
                return

        self._current_index = next_idx
        self._display_frame(next_idx)

        # Trigger read-ahead from the new position
        self._loader.load_around(next_idx)

    def _display_frame(self, index: int) -> None:
        """Try to show the frame at *index* from cache."""
        if index < 0 or index >= len(self._frame_paths):
            return

        path = self._frame_paths[index]
        img = self._cache.get(path)

        if img is not None:
            self.frame_changed.emit(index, img)
        else:
            # Frame not cached yet — request urgent single-frame load
            # (does NOT shrink the readahead window)
            self._loader.load_single(index)
            # Try again after a short delay
            QTimer.singleShot(50, lambda: self._retry_display(index, path))

    def _retry_display(self, index: int, path: str) -> None:
        """Retry displaying a frame after the loader had time to decode it."""
        # Only retry if we haven't moved on
        if self._current_index != index:
            return
        img = self._cache.get(path)
        if img is not None:
            self.frame_changed.emit(index, img)

    def _on_frame_loaded(self, path: str, index: int) -> None:
        """Slot: a frame was loaded into cache by the FrameLoader."""
        # If this is the currently displayed frame and we were waiting for it
        if index == self._current_index and path == self._frame_paths[index]:
            img = self._cache.get(path)
            if img is not None:
                self.frame_changed.emit(index, img)

        # Emit cache progress
        cached = self._cache.frame_count
        total = len(self._frame_paths)
        self.cache_progress.emit(cached, total)

        # Emit per-frame cache bitmap for the progress bar
        self._emit_cache_bitmap()

    def _emit_cache_bitmap(self) -> None:
        """Build a set of 0-based frame indices that are cached and emit it."""
        cached_keys = self._cache.cached_keys()
        path_to_index = {p: i for i, p in enumerate(self._frame_paths)}
        cached_indices = {path_to_index[k] for k in cached_keys if k in path_to_index}
        self.cache_bitmap_changed.emit(cached_indices)

    # ------------------------------------------------------------------
    # Cleanup
    # ------------------------------------------------------------------

    def cleanup(self) -> None:
        """Stop everything and release resources."""
        self._timer.stop()
        self._is_playing = False
        self._loader.stop()
        self._loader.wait(3000)
        self._cache.clear()
