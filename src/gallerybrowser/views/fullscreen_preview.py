"""Media viewer dialog — non-fullscreen, with navigation and playback controls.

Replaces the old FullscreenPreview.  Opened via Spacebar or double-click on a
file item in the file pane.  Supports:
  - Image display with zoom / pan (mouse wheel + drag)
  - Video playback with full controls (play/pause, timeline, volume, speed)
  - Previous / Next navigation that syncs the file-pane selection
  - Keyboard shortcuts (Left/Right, Space, Escape, +/-, 0 for fit)
"""

from pathlib import Path
from typing import List, Optional

from PyQt6.QtCore import QEvent, QThread, Qt, QTimer, pyqtSignal
from PyQt6.QtGui import QColor, QImage, QKeyEvent, QPixmap, QPainter
from PyQt6.QtWidgets import (
    QComboBox,
    QDialog,
    QFrame,
    QGraphicsScene,
    QGraphicsView,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QSlider,
    QVBoxLayout,
    QWidget,
)

import qtawesome as qta

from gallerybrowser.config import Config, HAS_OIIO


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _has_gstreamer() -> bool:
    try:
        import gi

        gi.require_version("Gst", "1.0")
        from gi.repository import Gst

        Gst.init(None)
        return True
    except (ImportError, ValueError):
        return False


def _format_time(seconds: float) -> str:
    m = int(seconds // 60)
    s = int(seconds % 60)
    return f"{m}:{s:02d}"


# Color tokens (match styles.py)
_BG = "#0f0f0f"
_SURFACE = "#181818"
_SURFACE_2 = "#1e1e1e"
_SURFACE_3 = "#252525"
_SURFACE_4 = "#2c2c2c"
_BORDER = "#2a2a2a"
_BORDER_HOVER = "#3a3a3a"
_ACCENT = "#2196F3"
_ACCENT_HOVER = "#42A5F5"
_ACCENT_PRESSED = "#1976D2"
_ACCENT_SURFACE = "#1a3a5c"
_TEXT = "#e8e8e8"
_TEXT_SEC = "#a0a0a0"
_TEXT_MUTED = "#666666"


# ---------------------------------------------------------------------------
# Image viewer widget (zoom + pan via QGraphicsView)
# ---------------------------------------------------------------------------


class _ImageLoaderThread(QThread):
    """Persistent background thread for loading images in the MediaViewer.

    Uses PIL/Pillow for I/O — PIL's ``Image.thumbnail()`` can exploit
    JPEG DCT scaling to avoid full-resolution decode, dramatically faster
    for large images over a NAS.  The thread stays alive and processes
    requests via an internal event, avoiding QThread start/stop overhead.
    """

    image_ready = pyqtSignal(str, QImage)  # file_path, QImage
    image_failed = pyqtSignal(str)  # file_path

    def __init__(self, parent=None):
        super().__init__(parent)
        self._file_path: str = ""
        self._max_dim: int = 4096
        self._request_pending = False
        self._stop_flag = False

        import threading

        self._lock = threading.Lock()
        self._event = threading.Event()

    def request(self, file_path: str, max_dim: int):
        """Submit a load request. Supersedes any previous pending request."""
        with self._lock:
            self._file_path = file_path
            self._max_dim = max(max_dim, 512)
            self._request_pending = True
        self._event.set()

    def stop(self):
        self._stop_flag = True
        self._event.set()

    def run(self):
        from PIL import Image

        while not self._stop_flag:
            self._event.wait()
            self._event.clear()
            if self._stop_flag:
                break

            with self._lock:
                if not self._request_pending:
                    continue
                path = self._file_path
                limit = self._max_dim * 2
                self._request_pending = False

            try:
                qimage = self._load_with_pil(Image, path, limit)
                if qimage is None and HAS_OIIO:
                    qimage = self._load_with_oiio(path, limit)

                if qimage is not None and not qimage.isNull():
                    self.image_ready.emit(path, qimage)
                else:
                    self.image_failed.emit(path)
            except Exception:
                self.image_failed.emit(path)

    @staticmethod
    def _load_with_pil(Image, path: str, limit: int):
        """Load an image using PIL.  Returns QImage or None."""
        try:
            img = Image.open(path)

            if hasattr(img, "draft") and img.format == "JPEG":
                img.draft("RGB", (limit, limit))

            img.thumbnail((limit, limit), Image.Resampling.LANCZOS)

            if img.mode not in ("RGB", "RGBA"):
                img = img.convert("RGBA" if img.mode in ("RGBA", "LA", "PA", "P") else "RGB")

            if img.mode == "RGBA":
                data = img.tobytes("raw", "RGBA")
                qimage = QImage(
                    data,
                    img.width,
                    img.height,
                    img.width * 4,
                    QImage.Format.Format_RGBA8888,
                )
            else:
                data = img.tobytes("raw", "RGB")
                qimage = QImage(
                    data,
                    img.width,
                    img.height,
                    img.width * 3,
                    QImage.Format.Format_RGB888,
                )

            qimage = qimage.copy()
            img.close()
            return qimage
        except Exception:
            return None

    @staticmethod
    def _load_with_oiio(path: str, limit: int):
        """Load an image using OpenImageIO with NAS-buffered I/O."""
        from gallerybrowser.core.image_io import load_oiio_qimage

        return load_oiio_qimage(path, max_dim=limit)


class _ImageViewer(QGraphicsView):
    zoom_changed = pyqtSignal(float)

    def __init__(self, parent=None):
        super().__init__(parent)
        self._scene = QGraphicsScene(self)
        self.setScene(self._scene)
        self.setRenderHints(QPainter.RenderHint.SmoothPixmapTransform)
        self.setDragMode(QGraphicsView.DragMode.ScrollHandDrag)
        self.setTransformationAnchor(QGraphicsView.ViewportAnchor.AnchorUnderMouse)
        self.setResizeAnchor(QGraphicsView.ViewportAnchor.AnchorUnderMouse)
        self.setFrameStyle(QFrame.Shape.NoFrame)
        self.setStyleSheet(f"background-color: {_BG};")
        self.setFocusPolicy(Qt.FocusPolicy.NoFocus)
        self._pixmap_item = None
        self._zoom = 1.0
        self._current_path: str | None = None

        # Persistent background loader
        self._loader = _ImageLoaderThread(self)
        self._loader.image_ready.connect(self._on_image_loaded)
        self._loader.image_failed.connect(self._on_image_failed)
        self._loader.start()  # Start once, stays alive

    # -- public API --

    def load(self, path: str) -> bool:
        """Start loading an image asynchronously. Always returns True."""
        self._current_path = path
        self._scene.clear()
        self._pixmap_item = None
        t = self._scene.addText("Loading...")
        t.setDefaultTextColor(Qt.GlobalColor.gray)

        vp = self.viewport().size()
        max_dim = max(vp.width(), vp.height(), 512)
        self._loader.request(path, max_dim)
        return True

    def _on_image_loaded(self, path: str, qimage: QImage):
        if path != self._current_path:
            return
        pix = QPixmap.fromImage(qimage)
        if pix.isNull():
            self._on_image_failed(path)
            return
        self._scene.clear()
        self._pixmap_item = self._scene.addPixmap(pix)
        r = pix.rect()
        self._scene.setSceneRect(r.x(), r.y(), r.width(), r.height())
        self.fit()

    def _on_image_failed(self, path: str):
        if path != self._current_path:
            return
        self._scene.clear()
        self._pixmap_item = None
        t = self._scene.addText("Failed to load image")
        t.setDefaultTextColor(Qt.GlobalColor.gray)

    def fit(self):
        if self._pixmap_item:
            self.resetTransform()
            self.fitInView(self._pixmap_item, Qt.AspectRatioMode.KeepAspectRatio)
            self._zoom = 1.0
            self.zoom_changed.emit(self._zoom)

    def zoom_in(self):
        self.scale(1.25, 1.25)
        self._zoom *= 1.25
        self.zoom_changed.emit(self._zoom)

    def zoom_out(self):
        self.scale(0.8, 0.8)
        self._zoom *= 0.8
        self.zoom_changed.emit(self._zoom)

    def clear(self):
        self._scene.clear()
        self._pixmap_item = None

    # -- events --

    def wheelEvent(self, event):
        if event.angleDelta().y() > 0:
            self.zoom_in()
        else:
            self.zoom_out()
        event.accept()


# ---------------------------------------------------------------------------
# Video viewer widget (GStreamer based, with controls)
# ---------------------------------------------------------------------------


class _VideoViewer(QWidget):
    """Embeddable video player with playback controls."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self._has_gst = _has_gstreamer()
        self._pipeline = None
        self._appsink = None
        self._duration = 0.0
        self._is_playing = False
        self._is_muted = False
        self._loop_enabled = False
        self._current_pixmap = None
        self._fps = 30.0  # Default FPS, updated from video caps

        self._setup_ui()

        self._frame_timer = QTimer()
        self._frame_timer.timeout.connect(self._update_frame)
        self._scrub_timer = QTimer()
        self._scrub_timer.setSingleShot(True)
        self._scrub_timer.timeout.connect(self._live_seek)
        # Bus poll timer for EOS/error detection
        self._bus_timer = QTimer()
        self._bus_timer.setInterval(100)
        self._bus_timer.timeout.connect(self._poll_bus)
        # Pending preroll frame data set by _on_new_preroll (GStreamer thread)
        # and consumed by _display_preroll_frame (Qt main thread).
        self._pending_preroll = None  # (QImage, w, h) or None

    # -- UI setup --

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(8)

        # Video frame
        self._frame_label = QLabel("No video loaded")
        self._frame_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._frame_label.setStyleSheet(f"background-color: {_BG}; color: {_TEXT_MUTED};")
        self._frame_label.setMinimumSize(320, 240)
        self._frame_label.setScaledContents(False)
        layout.addWidget(self._frame_label, stretch=1)

        # Controls — two-row layout
        controls_container = QVBoxLayout()
        controls_container.setSpacing(4)

        # Row 1: Timeline + time label
        timeline_row = QHBoxLayout()
        timeline_row.setSpacing(8)

        # Timeline
        self._timeline = QSlider(Qt.Orientation.Horizontal)
        self._timeline.setRange(0, 1000)
        self._timeline.setStyleSheet(f"""
            QSlider::groove:horizontal {{ height: 4px; background-color: {_BORDER}; border-radius: 2px; }}
            QSlider::handle:horizontal {{
                background-color: {_ACCENT}; width: 14px; height: 14px;
                margin: -5px 0; border-radius: 7px; border: 2px solid {_SURFACE_2};
            }}
            QSlider::handle:horizontal:hover {{ background-color: {_ACCENT_HOVER}; width: 16px; height: 16px; margin: -6px 0; border-radius: 8px; }}
            QSlider::sub-page:horizontal {{ background-color: {_ACCENT}; border-radius: 2px; }}
        """)
        self._timeline.sliderReleased.connect(self._seek)
        self._timeline.valueChanged.connect(self._on_slider_moved)
        timeline_row.addWidget(self._timeline, stretch=1)

        # Time label
        self._time_label = QLabel("0:00 / 0:00")
        self._time_label.setStyleSheet(f"color: {_TEXT_SEC}; font-size: 11px;")
        timeline_row.addWidget(self._time_label)

        controls_container.addLayout(timeline_row)

        # Row 2: Playback controls
        buttons_row = QHBoxLayout()
        buttons_row.setSpacing(8)

        # Play / Pause
        self._play_btn = QPushButton()
        self._play_btn.setIcon(qta.icon("fa5s.play", color=_TEXT))
        self._play_btn.setFixedSize(32, 32)
        self._play_btn.setStyleSheet(
            f"QPushButton {{ background-color: {_ACCENT}; border: none; border-radius: 6px; }}"
            f"QPushButton:hover {{ background-color: {_ACCENT_HOVER}; }}"
            f"QPushButton:pressed {{ background-color: {_ACCENT_PRESSED}; }}"
        )
        self._play_btn.clicked.connect(self._toggle_play)
        buttons_row.addWidget(self._play_btn)

        # Stop
        self._stop_btn = QPushButton()
        self._stop_btn.setIcon(qta.icon("fa5s.stop", color=_TEXT))
        self._stop_btn.setFixedSize(32, 32)
        self._stop_btn.setStyleSheet(
            f"QPushButton {{ background-color: {_SURFACE_3}; border: 1px solid {_BORDER}; border-radius: 6px; }}"
            f"QPushButton:hover {{ background-color: {_SURFACE_4}; border: 1px solid {_BORDER_HOVER}; }}"
        )
        self._stop_btn.clicked.connect(self.stop)
        buttons_row.addWidget(self._stop_btn)

        # Loop button
        self._loop_btn = QPushButton()
        self._loop_btn.setIcon(qta.icon("fa5s.redo", color=_TEXT_MUTED))
        self._loop_btn.setFixedSize(32, 32)
        self._loop_btn.setToolTip("Loop playback")
        self._loop_btn.setStyleSheet(
            f"QPushButton {{ background-color: {_SURFACE_3}; border: 1px solid {_BORDER}; border-radius: 6px; }}"
            f"QPushButton:hover {{ background-color: {_SURFACE_4}; border: 1px solid {_BORDER_HOVER}; }}"
        )
        self._loop_btn.clicked.connect(self._toggle_loop)
        buttons_row.addWidget(self._loop_btn)

        buttons_row.addStretch()

        # Volume button
        self._vol_btn = QPushButton()
        self._vol_btn.setIcon(qta.icon("fa5s.volume-up", color=_TEXT_SEC))
        self._vol_btn.setFixedSize(32, 32)
        self._vol_btn.setStyleSheet(
            f"QPushButton {{ background-color: transparent; border: none; }}"
            f"QPushButton:hover {{ background-color: {_SURFACE_4}; border-radius: 6px; }}"
        )
        self._vol_btn.clicked.connect(self._toggle_mute)
        buttons_row.addWidget(self._vol_btn)

        # Volume slider
        self._vol_slider = QSlider(Qt.Orientation.Horizontal)
        self._vol_slider.setRange(0, 100)
        self._vol_slider.setValue(100)
        self._vol_slider.setFixedWidth(80)
        self._vol_slider.setStyleSheet(f"""
            QSlider::groove:horizontal {{ height: 4px; background-color: {_BORDER}; border-radius: 2px; }}
            QSlider::handle:horizontal {{
                background-color: {_ACCENT}; width: 12px; height: 12px;
                margin: -4px 0; border-radius: 6px; border: 2px solid {_SURFACE_2};
            }}
            QSlider::handle:horizontal:hover {{ background-color: {_ACCENT_HOVER}; }}
            QSlider::sub-page:horizontal {{ background-color: {_ACCENT}; border-radius: 2px; }}
        """)
        self._vol_slider.valueChanged.connect(self._set_volume)
        buttons_row.addWidget(self._vol_slider)

        # Speed combo
        self._speed_combo = QComboBox()
        self._speed_combo.addItems(["0.5x", "1.0x", "1.5x", "2.0x"])
        self._speed_combo.setCurrentText("1.0x")
        self._speed_combo.setFixedWidth(60)
        self._speed_combo.setStyleSheet(f"""
            QComboBox {{
                background-color: {_SURFACE_3}; color: {_TEXT};
                border: 1px solid {_BORDER}; border-radius: 4px;
                padding: 2px 6px; font-size: 11px;
            }}
            QComboBox:hover {{ border: 1px solid {_BORDER_HOVER}; background-color: {_SURFACE_4}; }}
            QComboBox::drop-down {{ border: none; width: 14px; }}
        """)
        self._speed_combo.currentTextChanged.connect(self._set_speed)
        buttons_row.addWidget(self._speed_combo)

        # Prevent control widgets from stealing keyboard focus
        # so arrow keys reach MediaViewer.keyPressEvent
        for w in (
            self._timeline,
            self._play_btn,
            self._stop_btn,
            self._loop_btn,
            self._vol_btn,
            self._vol_slider,
            self._speed_combo,
        ):
            w.setFocusPolicy(Qt.FocusPolicy.NoFocus)

        controls_container.addLayout(buttons_row)
        layout.addLayout(controls_container)

    # -- public API --

    def load(self, path: str) -> bool:
        if not self._has_gst:
            return False
        self.stop()
        try:
            import gi

            gi.require_version("Gst", "1.0")
            from gi.repository import Gst

            self._pipeline = Gst.Pipeline.new("viewer-pipeline")

            filesrc = Gst.ElementFactory.make("filesrc", "source")
            filesrc.set_property("location", path)
            decodebin = Gst.ElementFactory.make("decodebin", "decoder")

            videoconvert = Gst.ElementFactory.make("videoconvert", "vconv")
            capsfilter = Gst.ElementFactory.make("capsfilter", "vcaps")
            capsfilter.set_property("caps", Gst.Caps.from_string("video/x-raw,format=RGB"))
            appsink = Gst.ElementFactory.make("appsink", "sink")
            appsink.set_property("emit-signals", True)
            appsink.set_property("max-buffers", 1)
            appsink.set_property("drop", True)

            # Only video elements added statically; audio added dynamically
            for el in (filesrc, decodebin, videoconvert, capsfilter, appsink):
                self._pipeline.add(el)

            filesrc.link(decodebin)
            videoconvert.link(capsfilter)
            capsfilter.link(appsink)

            pipeline_ref = self._pipeline

            # Guard: only link the first audio stream; files exported from
            # DaVinci Resolve can have 8+ audio streams and creating separate
            # audio pipelines for each one will fail.
            audio_linked = [False]

            def _on_pad_added(_dec, pad):
                caps = pad.get_current_caps()
                if caps is None:
                    return
                name = caps.get_structure(0).get_name()
                if name.startswith("video/"):
                    sink = videoconvert.get_static_pad("sink")
                    if sink and not sink.is_linked():
                        pad.link(sink)
                elif name.startswith("audio/") and not audio_linked[0]:
                    audio_linked[0] = True
                    aconv = Gst.ElementFactory.make("audioconvert", None)
                    vol = Gst.ElementFactory.make("volume", "volume")
                    asink = Gst.ElementFactory.make("autoaudiosink", None)
                    for el in (aconv, vol, asink):
                        pipeline_ref.add(el)
                    aconv.link(vol)
                    vol.link(asink)
                    aconv.sync_state_with_parent()
                    vol.sync_state_with_parent()
                    asink.sync_state_with_parent()
                    sink = aconv.get_static_pad("sink")
                    if sink:
                        pad.link(sink)

            decodebin.connect("pad-added", _on_pad_added)
            self._appsink = appsink

            # Connect new-preroll signal for frame updates after seeks in PAUSED state.
            # The signal fires on the GStreamer streaming thread, so the callback
            # extracts raw pixel data and schedules a Qt-thread display update.
            appsink.connect("new-preroll", self._on_new_preroll)

            self._pipeline.set_state(Gst.State.PLAYING)
            # Short non-blocking wait — just enough for demuxer to start;
            # do NOT block the UI for seconds waiting on full preroll.
            self._pipeline.get_state(int(0.5 * Gst.SECOND))

            # Try to get duration immediately; if not ready, a timer will
            # retry in play().
            ok, dur = self._pipeline.query_duration(Gst.Format.TIME)
            if ok and dur > 0:
                self._duration = dur / Gst.SECOND
                self._time_label.setText(f"0:00 / {_format_time(self._duration)}")

            # Extract actual framerate from the appsink pad's negotiated caps.
            # This is more reliable than last-sample which may not exist yet.
            self._fps = self._detect_fps()

            # Leave pipeline in PLAYING — play() is always called after load()
            self._is_playing = True
            self._frame_timer.start(33)
            self._bus_timer.start()
            self._play_btn.setIcon(qta.icon("fa5s.pause", color=_TEXT))

            return True
        except Exception as e:
            print(f"MediaViewer video load error: {e}")
            return False

    def _detect_fps(self) -> float:
        """Detect video FPS from appsink pad caps or last-sample.

        Tries multiple approaches in order of reliability:
        1. Appsink sink pad negotiated caps (available after preroll)
        2. Last-sample caps from appsink
        Falls back to 30.0 if nothing works.
        """
        if not self._appsink:
            return 30.0
        try:
            # Approach 1: Query the appsink's sink pad for negotiated caps
            pad = self._appsink.get_static_pad("sink")
            caps = pad.get_current_caps() if pad else None

            # Approach 2: Fallback to last-sample
            if not caps or caps.get_size() == 0:
                sample = self._appsink.get_property("last-sample")
                if sample:
                    caps = sample.get_caps()

            if caps and caps.get_size() > 0:
                struct = caps.get_structure(0)
                val = struct.get_value("framerate")
                if val is not None:
                    # GStreamer returns framerate as a Gst.Fraction
                    if hasattr(val, "num") and hasattr(val, "denom") and val.denom:
                        fps = val.num / val.denom
                        if fps > 0:
                            return fps
                # Try get_int as last resort
                ok, num = struct.get_int("framerate")
                if ok and num > 0:
                    return float(num)
        except Exception:
            pass
        return 30.0

    def play(self):
        if not self._has_gst or not self._pipeline:
            return
        try:
            import gi

            gi.require_version("Gst", "1.0")
            from gi.repository import Gst

            self._pipeline.set_state(Gst.State.PLAYING)
            self._is_playing = True
            self._frame_timer.start(33)
            self._bus_timer.start()
            self._play_btn.setIcon(qta.icon("fa5s.pause", color=_TEXT))

            # Deferred duration query — if load() couldn't get it yet
            if self._duration <= 0:
                ok, dur = self._pipeline.query_duration(Gst.Format.TIME)
                if ok and dur > 0:
                    self._duration = dur / Gst.SECOND
                    self._time_label.setText(f"0:00 / {_format_time(self._duration)}")
        except Exception as e:
            print(f"play error: {e}")

    def pause(self):
        if not self._has_gst or not self._pipeline:
            return
        try:
            import gi

            gi.require_version("Gst", "1.0")
            from gi.repository import Gst

            self._pipeline.set_state(Gst.State.PAUSED)
            self._is_playing = False
            self._frame_timer.stop()
            self._play_btn.setIcon(qta.icon("fa5s.play", color=_TEXT))
        except Exception as e:
            print(f"pause error: {e}")

    def stop(self):
        self._frame_timer.stop()
        self._bus_timer.stop()
        self._scrub_timer.stop()
        self._pending_preroll = None
        self._is_playing = False
        if self._pipeline:
            try:
                import gi

                gi.require_version("Gst", "1.0")
                from gi.repository import Gst

                self._pipeline.set_state(Gst.State.NULL)
            except Exception:
                pass
            finally:
                self._pipeline = None
                self._appsink = None
        self._play_btn.setIcon(qta.icon("fa5s.play", color=_TEXT))
        self._timeline.blockSignals(True)
        self._timeline.setValue(0)
        self._timeline.blockSignals(False)
        self._time_label.setText(f"0:00 / {_format_time(self._duration)}")
        self._current_pixmap = None
        self._frame_label.setPixmap(QPixmap())
        self._frame_label.setText("No video loaded")

    @property
    def is_playing(self):
        return self._is_playing

    # -- private --

    def _toggle_play(self):
        if self._is_playing:
            self.pause()
        else:
            self.play()

    def _seek(self):
        if not self._pipeline:
            return
        try:
            import gi

            gi.require_version("Gst", "1.0")
            from gi.repository import Gst

            pos = (self._timeline.value() / 1000.0) * self._duration
            self._pipeline.seek_simple(
                Gst.Format.TIME, Gst.SeekFlags.FLUSH | Gst.SeekFlags.ACCURATE, int(pos * Gst.SECOND)
            )
            # When paused, the new-preroll signal will fire and display the frame.
            self._time_label.setText(f"{_format_time(pos)} / {_format_time(self._duration)}")
        except Exception as e:
            print(f"seek error: {e}")

    def _on_slider_moved(self):
        if not self._is_playing and self._pipeline:
            self._scrub_timer.start(50)

    def _live_seek(self):
        if not self._pipeline:
            return
        try:
            import gi

            gi.require_version("Gst", "1.0")
            from gi.repository import Gst

            pos = (self._timeline.value() / 1000.0) * self._duration
            self._pipeline.seek_simple(
                Gst.Format.TIME, Gst.SeekFlags.FLUSH | Gst.SeekFlags.ACCURATE, int(pos * Gst.SECOND)
            )
            # When paused, the new-preroll signal will fire and display the frame.
            self._time_label.setText(f"{_format_time(pos)} / {_format_time(self._duration)}")
        except Exception as e:
            print(f"live seek error: {e}")

    def _frame_step(self, direction: int):
        """Step forward or backward by exactly one frame.

        Seeks to position ± frame_duration while staying PAUSED, then
        pulls the decoded frame after the seek completes.  Unlike
        scrubbing, we never set the pipeline to PLAYING so the position
        doesn't drift.
        """
        if not self._has_gst or not self._pipeline:
            return
        if self._is_playing:
            self.pause()

        try:
            import gi

            gi.require_version("Gst", "1.0")
            from gi.repository import Gst

            # Ensure pipeline is PAUSED (not PLAYING) so seek prerolls exactly
            self._pipeline.set_state(Gst.State.PAUSED)

            ok, position = self._pipeline.query_position(Gst.Format.TIME)
            if not ok:
                return

            frame_duration = int(Gst.SECOND / self._fps)
            new_pos = position + (direction * frame_duration)
            new_pos = max(0, new_pos)

            if self._duration > 0:
                max_pos = int(self._duration * Gst.SECOND)
                new_pos = min(new_pos, max_pos)

            self._pipeline.seek_simple(
                Gst.Format.TIME,
                Gst.SeekFlags.FLUSH | Gst.SeekFlags.ACCURATE,
                new_pos,
            )

            # Update timeline and time label from the target position
            if self._duration > 0:
                pos_sec = new_pos / Gst.SECOND
                value = int((pos_sec / self._duration) * 1000)
                self._timeline.blockSignals(True)
                self._timeline.setValue(value)
                self._timeline.blockSignals(False)
                self._time_label.setText(
                    f"{_format_time(pos_sec)} / {_format_time(self._duration)}"
                )

            # The new-preroll signal will fire and display the decoded frame.

        except Exception as e:
            print(f"Error stepping frame: {e}")

    def _on_new_preroll(self, appsink):
        """GStreamer streaming-thread callback: extract frame pixels and
        schedule a Qt-thread display update.

        Called by the ``new-preroll`` signal whenever a FLUSH seek
        completes in PAUSED state.  We must NOT touch Qt widgets here
        (wrong thread), so we copy the raw pixel data into a QImage and
        post it to the main thread via ``QTimer.singleShot(0, ...)``.
        """
        try:
            import gi

            gi.require_version("Gst", "1.0")
            from gi.repository import Gst
            from PyQt6.QtGui import QImage

            sample = appsink.emit("try-pull-preroll", 0)
            if not sample:
                sample = appsink.get_property("last-sample")
            if not sample:
                return Gst.FlowReturn.OK

            buf = sample.get_buffer()
            caps = sample.get_caps()
            s = caps.get_structure(0)
            w, h = s.get_value("width"), s.get_value("height")
            ok, mi = buf.map(Gst.MapFlags.READ)
            if ok:
                # QImage must own a copy of the data because the GStreamer
                # buffer will be freed when we return.
                img = QImage(mi.data, w, h, w * 3, QImage.Format.Format_RGB888).copy()
                buf.unmap(mi)
                self._pending_preroll = img
                QTimer.singleShot(0, self._display_preroll_frame)

            return Gst.FlowReturn.OK
        except Exception:
            return Gst.FlowReturn.OK

    def _display_preroll_frame(self):
        """Qt main-thread: display the frame captured by ``_on_new_preroll``."""
        img = self._pending_preroll
        if img is None:
            return
        self._pending_preroll = None
        self._current_pixmap = QPixmap.fromImage(img)
        self._rescale()

    def _update_frame(self):
        if not self._appsink:
            return
        try:
            import gi

            gi.require_version("Gst", "1.0")
            from gi.repository import Gst
            from PyQt6.QtGui import QImage

            sample = self._appsink.emit("try-pull-sample", 100_000_000)
            if sample:
                buf = sample.get_buffer()
                caps = sample.get_caps()
                s = caps.get_structure(0)
                w, h = s.get_value("width"), s.get_value("height")
                ok, mi = buf.map(Gst.MapFlags.READ)
                if ok:
                    img = QImage(mi.data, w, h, w * 3, QImage.Format.Format_RGB888)
                    self._current_pixmap = QPixmap.fromImage(img)
                    self._rescale()
                    buf.unmap(mi)

            if self._is_playing and self._pipeline:
                # Deferred duration query for async pipelines
                if self._duration <= 0:
                    dok, dur = self._pipeline.query_duration(Gst.Format.TIME)
                    if dok and dur > 0:
                        self._duration = dur / Gst.SECOND

                ok, pos = self._pipeline.query_position(Gst.Format.TIME)
                if ok:
                    ps = pos / Gst.SECOND
                    if self._duration > 0:
                        self._timeline.blockSignals(True)
                        self._timeline.setValue(int((ps / self._duration) * 1000))
                        self._timeline.blockSignals(False)
                    self._time_label.setText(f"{_format_time(ps)} / {_format_time(self._duration)}")
        except Exception as e:
            print(f"frame error: {e}")

    def _rescale(self):
        if self._current_pixmap and not self._current_pixmap.isNull():
            scaled = self._current_pixmap.scaled(
                self._frame_label.size(),
                Qt.AspectRatioMode.KeepAspectRatio,
                Qt.TransformationMode.SmoothTransformation,
            )
            self._frame_label.setText("")
            self._frame_label.setPixmap(scaled)

    def resizeEvent(self, event):
        super().resizeEvent(event)
        self._rescale()

    def _toggle_mute(self):
        if not self._pipeline:
            return
        try:
            vol = self._pipeline.get_by_name("volume")
            if vol:
                self._is_muted = not self._is_muted
                vol.set_property("mute", self._is_muted)
                icon = "fa5s.volume-mute" if self._is_muted else "fa5s.volume-up"
                self._vol_btn.setIcon(qta.icon(icon, color=_TEXT))
        except Exception:
            pass

    def _set_volume(self, value: int):
        if not self._pipeline:
            return
        try:
            vol = self._pipeline.get_by_name("volume")
            if vol:
                vol.set_property("volume", value / 100.0)
                if value == 0:
                    ic = "fa5s.volume-off"
                elif value < 50:
                    ic = "fa5s.volume-down"
                else:
                    ic = "fa5s.volume-up"
                self._vol_btn.setIcon(qta.icon(ic, color=_TEXT))
        except Exception:
            pass

    def _set_speed(self, text: str):
        if not self._pipeline:
            return
        try:
            import gi

            gi.require_version("Gst", "1.0")
            from gi.repository import Gst

            speed = float(text.replace("x", ""))
            ok, pos = self._pipeline.query_position(Gst.Format.TIME)
            position = pos if ok else 0
            self._pipeline.seek(
                speed,
                Gst.Format.TIME,
                Gst.SeekFlags.FLUSH | Gst.SeekFlags.ACCURATE,
                Gst.SeekType.SET,
                position,
                Gst.SeekType.NONE,
                0,
            )
        except Exception:
            pass

    def _toggle_loop(self):
        """Toggle loop playback on/off."""
        self._loop_enabled = not self._loop_enabled
        if self._loop_enabled:
            self._loop_btn.setIcon(qta.icon("fa5s.redo", color=_ACCENT))
            self._loop_btn.setStyleSheet(
                f"QPushButton {{ background-color: {_ACCENT_SURFACE}; border: 1px solid {_ACCENT}; border-radius: 6px; }}"
                f"QPushButton:hover {{ background-color: #1e4a6e; border: 1px solid {_ACCENT_HOVER}; }}"
            )
        else:
            self._loop_btn.setIcon(qta.icon("fa5s.redo", color=_TEXT_MUTED))
            self._loop_btn.setStyleSheet(
                f"QPushButton {{ background-color: {_SURFACE_3}; border: 1px solid {_BORDER}; border-radius: 6px; }}"
                f"QPushButton:hover {{ background-color: {_SURFACE_4}; border: 1px solid {_BORDER_HOVER}; }}"
            )

    def _poll_bus(self):
        """Poll GStreamer bus for EOS/error messages (called from QTimer)."""
        if not self._pipeline:
            return
        try:
            import gi

            gi.require_version("Gst", "1.0")
            from gi.repository import Gst

            bus = self._pipeline.get_bus()
            while True:
                msg = bus.pop_filtered(Gst.MessageType.EOS | Gst.MessageType.ERROR)
                if msg is None:
                    break
                if msg.type == Gst.MessageType.EOS:
                    self._on_eos()
                elif msg.type == Gst.MessageType.ERROR:
                    err, debug = msg.parse_error()
                    print(f"GStreamer error: {err.message} ({debug})")
                    self.stop()
        except Exception:
            pass

    def _on_eos(self):
        """Handle end-of-stream: loop or stop."""
        if self._loop_enabled:
            try:
                import gi

                gi.require_version("Gst", "1.0")
                from gi.repository import Gst

                self._pipeline.seek_simple(
                    Gst.Format.TIME,
                    Gst.SeekFlags.FLUSH | Gst.SeekFlags.KEY_UNIT,
                    0,
                )
            except Exception as e:
                print(f"Error looping video: {e}")
                self.stop()
        else:
            self._frame_timer.stop()
            self._is_playing = False
            self._play_btn.setIcon(qta.icon("fa5s.play", color=_TEXT))
            try:
                import gi

                gi.require_version("Gst", "1.0")
                from gi.repository import Gst

                self._pipeline.set_state(Gst.State.PAUSED)
                self._pipeline.seek_simple(
                    Gst.Format.TIME,
                    Gst.SeekFlags.FLUSH | Gst.SeekFlags.KEY_UNIT,
                    0,
                )
            except Exception:
                pass
            self._timeline.blockSignals(True)
            self._timeline.setValue(0)
            self._timeline.blockSignals(False)
            self._time_label.setText(f"0:00 / {_format_time(self._duration)}")

    def __del__(self):
        try:
            if self._pipeline:
                import gi

                gi.require_version("Gst", "1.0")
                from gi.repository import Gst

                self._pipeline.set_state(Gst.State.NULL)
                self._pipeline = None
        except Exception:
            pass


# ---------------------------------------------------------------------------
# Sequence viewer widget (RAM-cached playback with controls)
# ---------------------------------------------------------------------------


# ---------------------------------------------------------------------------
# Cache status bar widget
# ---------------------------------------------------------------------------


class _CacheBar(QWidget):
    """Thin bar showing per-frame cache status above the timeline."""

    _BAR_COLOR = QColor("#F44336")
    _BG_COLOR = QColor(_SURFACE_2)

    def __init__(self, parent=None):
        super().__init__(parent)
        self._total_frames: int = 0
        self._cached_indices: set = set()
        self.setFixedHeight(4)
        self.setStyleSheet("background: transparent;")

    def set_total(self, total: int) -> None:
        self._total_frames = total
        self._cached_indices.clear()
        self.update()

    def set_cached(self, indices: set) -> None:
        self._cached_indices = indices
        self.update()

    def paintEvent(self, event):
        if self._total_frames <= 0:
            return
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing, False)
        w = self.width()
        h = self.height()
        painter.fillRect(0, 0, w, h, self._BG_COLOR)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(self._BAR_COLOR)
        total = self._total_frames
        for idx in self._cached_indices:
            x0 = int(idx * w / total)
            x1 = int((idx + 1) * w / total)
            painter.drawRect(x0, 0, max(x1 - x0, 1), h)
        painter.end()


class _SequenceViewer(QWidget):
    """Embeddable image-sequence player with full playback controls.

    Uses SequencePlayer (QTimer + RAMCache) for frame display.
    """

    def __init__(self, parent=None):
        super().__init__(parent)
        self._setup_ui()

        from gallerybrowser.core.sequence_player import SequencePlayer
        from gallerybrowser.config import Config

        ram_mb = Config.load_settings().get("ram_preview_mb", 2048)
        self._player = SequencePlayer(self, max_cache_bytes=ram_mb * 1024 * 1024)
        self._player.frame_changed.connect(self._on_frame_changed)
        self._player.playback_state_changed.connect(self._on_state_changed)
        self._player.cache_progress.connect(self._on_cache_progress)
        self._player.cache_bitmap_changed.connect(self._on_cache_bitmap)

        self._current_pixmap = None

    def _setup_ui(self):
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(8)

        # Frame display
        self._frame_label = QLabel("No sequence loaded")
        self._frame_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._frame_label.setStyleSheet(f"background-color: {_BG}; color: {_TEXT_MUTED};")
        self._frame_label.setMinimumSize(320, 240)
        self._frame_label.setScaledContents(False)
        layout.addWidget(self._frame_label, stretch=1)

        # Controls container
        controls = QVBoxLayout()
        controls.setSpacing(4)

        # Cache bar (thin red indicator above the timeline)
        self._cache_bar = _CacheBar()
        controls.addWidget(self._cache_bar)

        # Row 1: Timeline + frame counter
        timeline_row = QHBoxLayout()
        timeline_row.setSpacing(8)

        self._timeline = QSlider(Qt.Orientation.Horizontal)
        self._timeline.setRange(0, 1000)
        self._timeline.setStyleSheet(f"""
            QSlider::groove:horizontal {{ height: 4px; background-color: {_BORDER}; border-radius: 2px; }}
            QSlider::handle:horizontal {{
                background-color: {_ACCENT}; width: 14px; height: 14px;
                margin: -5px 0; border-radius: 7px; border: 2px solid {_SURFACE_2};
            }}
            QSlider::handle:horizontal:hover {{ background-color: {_ACCENT_HOVER}; width: 16px; height: 16px; margin: -6px 0; border-radius: 8px; }}
            QSlider::sub-page:horizontal {{ background-color: {_ACCENT}; border-radius: 2px; }}
        """)
        self._timeline.setPageStep(1)
        self._timeline.setSingleStep(1)
        self._timeline.sliderReleased.connect(self._on_timeline_released)
        self._timeline.valueChanged.connect(self._on_timeline_moved)
        timeline_row.addWidget(self._timeline, stretch=1)

        self._frame_info = QLabel("0 / 0")
        self._frame_info.setStyleSheet(f"color: {_TEXT_SEC}; font-size: 11px;")
        self._frame_info.setMinimumWidth(100)
        timeline_row.addWidget(self._frame_info)

        controls.addLayout(timeline_row)

        # Row 2: Buttons
        buttons_row = QHBoxLayout()
        buttons_row.setSpacing(8)

        # Play/Pause
        self._play_btn = QPushButton()
        self._play_btn.setIcon(qta.icon("fa5s.play", color=_TEXT))
        self._play_btn.setFixedSize(32, 32)
        self._play_btn.setStyleSheet(
            f"QPushButton {{ background-color: {_ACCENT}; border: none; border-radius: 6px; }}"
            f"QPushButton:hover {{ background-color: {_ACCENT_HOVER}; }}"
            f"QPushButton:pressed {{ background-color: {_ACCENT_PRESSED}; }}"
        )
        self._play_btn.clicked.connect(self._toggle_play)
        buttons_row.addWidget(self._play_btn)

        # Stop
        self._stop_btn = QPushButton()
        self._stop_btn.setIcon(qta.icon("fa5s.stop", color=_TEXT))
        self._stop_btn.setFixedSize(32, 32)
        self._stop_btn.setStyleSheet(
            f"QPushButton {{ background-color: {_SURFACE_3}; border: 1px solid {_BORDER}; border-radius: 6px; }}"
            f"QPushButton:hover {{ background-color: {_SURFACE_4}; border: 1px solid {_BORDER_HOVER}; }}"
        )
        self._stop_btn.clicked.connect(self._on_stop)
        buttons_row.addWidget(self._stop_btn)

        buttons_row.addStretch()

        # Cache progress
        self._cache_label = QLabel("")
        self._cache_label.setStyleSheet(f"color: {_TEXT_MUTED}; font-size: 10px;")
        buttons_row.addWidget(self._cache_label)

        buttons_row.addStretch()

        # Loop toggle
        self._loop_btn = QPushButton()
        self._loop_btn.setIcon(qta.icon("fa5s.redo", color=_ACCENT))
        self._loop_btn.setFixedSize(32, 32)
        self._loop_btn.setCheckable(True)
        self._loop_btn.setChecked(True)
        self._loop_btn.setToolTip("Loop playback")
        self._loop_btn.setStyleSheet(
            f"QPushButton {{ background-color: transparent; border: none; border-radius: 6px; }}"
            f"QPushButton:hover {{ background-color: {_SURFACE_4}; }}"
            f"QPushButton:checked {{ background-color: {_SURFACE_3}; border: 1px solid {_BORDER}; }}"
        )
        self._loop_btn.toggled.connect(self._on_loop_toggled)
        buttons_row.addWidget(self._loop_btn)

        # FPS label
        self._fps_label = QLabel("24 fps")
        self._fps_label.setStyleSheet(f"color: {_TEXT_SEC}; font-size: 11px;")
        buttons_row.addWidget(self._fps_label)

        # Speed combo
        self._speed_combo = QComboBox()
        self._speed_combo.addItems(["0.25x", "0.5x", "1.0x", "1.5x", "2.0x"])
        self._speed_combo.setCurrentText("1.0x")
        self._speed_combo.setFixedWidth(60)
        self._speed_combo.setStyleSheet(f"""
            QComboBox {{
                background-color: {_SURFACE_3}; color: {_TEXT};
                border: 1px solid {_BORDER}; border-radius: 4px;
                padding: 2px 6px; font-size: 11px;
            }}
            QComboBox:hover {{ border: 1px solid {_BORDER_HOVER}; background-color: {_SURFACE_4}; }}
            QComboBox::drop-down {{ border: none; width: 14px; }}
        """)
        self._speed_combo.currentTextChanged.connect(self._on_speed_changed)
        buttons_row.addWidget(self._speed_combo)

        # Prevent focus stealing
        for w in (
            self._timeline,
            self._play_btn,
            self._stop_btn,
            self._loop_btn,
            self._speed_combo,
        ):
            w.setFocusPolicy(Qt.FocusPolicy.NoFocus)

        controls.addLayout(buttons_row)
        layout.addLayout(controls)

        # Scrub timer
        self._scrub_timer = QTimer()
        self._scrub_timer.setSingleShot(True)
        self._scrub_timer.timeout.connect(self._perform_scrub)
        self._is_user_scrubbing = False

    # -- public API --

    def load(self, frame_paths, frame_range=(0, 0), fps=24.0):
        """Load a sequence for playback."""
        self._player.set_sequence(frame_paths, frame_range=frame_range, fps=fps)
        total = len(frame_paths)
        self._timeline.setRange(0, max(total - 1, 1))
        self._cache_bar.set_total(total)
        self._fps_label.setText(f"{fps:.0f} fps")
        self._update_frame_info()

    def play(self):
        self._player.play()

    def pause(self):
        self._player.pause()

    def stop(self):
        self._player.stop()
        self._player.seek(0)

    @property
    def is_playing(self):
        return self._player.is_playing

    def step(self, direction):
        """Frame step: +1 forward, -1 backward."""
        self._player.step(direction)

    def cleanup(self):
        self._player.cleanup()

    # -- slots --

    def _on_frame_changed(self, index, qimage):
        self._current_pixmap = QPixmap.fromImage(qimage)
        self._rescale()
        self._timeline.blockSignals(True)
        self._timeline.setValue(index)
        self._timeline.blockSignals(False)
        self._update_frame_info()

    def _on_state_changed(self, state):
        if state == "playing":
            self._play_btn.setIcon(qta.icon("fa5s.pause", color=_TEXT))
        else:
            self._play_btn.setIcon(qta.icon("fa5s.play", color=_TEXT))

    def _on_cache_progress(self, cached, total):
        if total > 0:
            pct = cached * 100 // total
            self._cache_label.setText(f"Cache: {cached}/{total} ({pct}%)")

    def _on_cache_bitmap(self, indices):
        """Update the cache bar with per-frame status."""
        self._cache_bar.set_cached(indices)

    def _toggle_play(self):
        self._player.toggle_playback()

    def _on_stop(self):
        self._player.stop()
        self._player.seek(0)

    def _on_timeline_released(self):
        self._is_user_scrubbing = False
        self._player.seek(self._timeline.value())

    def _on_timeline_moved(self):
        if not self._player.is_playing:
            self._is_user_scrubbing = True
            self._scrub_timer.start(50)

    def _perform_scrub(self):
        if self._is_user_scrubbing:
            self._player.seek(self._timeline.value())

    def _on_speed_changed(self, text):
        speed = float(text.replace("x", ""))
        self._player.set_speed(speed)

    def _on_loop_toggled(self, checked):
        self._player.set_loop(checked)
        icon_color = _ACCENT if checked else _TEXT_MUTED
        self._loop_btn.setIcon(qta.icon("fa5s.redo", color=icon_color))

    def _update_frame_info(self):
        fr = self._player.frame_range
        cur = self._player.current_frame_number
        total = self._player.frame_count
        idx = self._player.current_index
        self._frame_info.setText(f"{cur} ({idx + 1}/{total})")

    def _rescale(self):
        if self._current_pixmap and not self._current_pixmap.isNull():
            scaled = self._current_pixmap.scaled(
                self._frame_label.size(),
                Qt.AspectRatioMode.KeepAspectRatio,
                Qt.TransformationMode.SmoothTransformation,
            )
            self._frame_label.setText("")
            self._frame_label.setPixmap(scaled)

    def resizeEvent(self, event):
        super().resizeEvent(event)
        self._rescale()


# ---------------------------------------------------------------------------
# MediaViewer dialog
# ---------------------------------------------------------------------------


class MediaViewer(QDialog):
    """Non-fullscreen media viewer with navigation and playback controls.

    Parameters
    ----------
    file_list : list[dict]
        The *filtered* file list from FilePane (each dict has "path", "name",
        "type", ...).
    start_index : int
        Index of the initially displayed file in *file_list*.
    parent : QWidget | None
    """

    # Emitted when the viewer navigates to a different file.
    # The connected slot should update the file-pane selection.
    selection_changed = pyqtSignal(str)  # file path

    def __init__(self, file_list: List[dict], start_index: int = 0, parent=None):
        super().__init__(parent)
        self._files = file_list
        self._index = max(0, min(start_index, len(file_list) - 1))

        self.setWindowTitle("Media Viewer")
        self.setMinimumSize(800, 600)
        # Match parent window size and position, or fall back to 85% of screen
        if parent:
            # Get the top-level window (in case parent is an embedded widget)
            top_level = parent.window()
            self.resize(top_level.size())
            self.move(top_level.pos())
        else:
            from PyQt6.QtWidgets import QApplication

            screen = QApplication.primaryScreen()
            if screen:
                geo = screen.availableGeometry()
                w = int(geo.width() * 0.85)
                h = int(geo.height() * 0.85)
                self.resize(w, h)
                self.move(
                    geo.x() + (geo.width() - w) // 2,
                    geo.y() + (geo.height() - h) // 2,
                )

        self.setModal(True)
        self._setup_ui()
        self._load_current()

    # -- UI --

    def _setup_ui(self):
        self.setStyleSheet(f"background-color: {_SURFACE};")

        root = QVBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)

        # --- top bar: filename + close button ---
        top_bar = QHBoxLayout()
        top_bar.setContentsMargins(12, 8, 12, 8)

        # Nav prev
        self._prev_btn = QPushButton()
        self._prev_btn.setIcon(qta.icon("fa5s.chevron-left", color=_TEXT))
        self._prev_btn.setFixedSize(32, 32)
        self._prev_btn.setToolTip("Previous (Left arrow)")
        self._prev_btn.setStyleSheet(
            f"QPushButton {{ background: transparent; border: none; border-radius: 6px; }}"
            f"QPushButton:hover {{ background-color: {_SURFACE_4}; }}"
            f"QPushButton:disabled {{ opacity: 0.3; }}"
        )
        self._prev_btn.clicked.connect(self._go_prev)
        top_bar.addWidget(self._prev_btn)

        # Nav next
        self._next_btn = QPushButton()
        self._next_btn.setIcon(qta.icon("fa5s.chevron-right", color=_TEXT))
        self._next_btn.setFixedSize(32, 32)
        self._next_btn.setToolTip("Next (Right arrow)")
        self._next_btn.setStyleSheet(
            f"QPushButton {{ background: transparent; border: none; border-radius: 6px; }}"
            f"QPushButton:hover {{ background-color: {_SURFACE_4}; }}"
            f"QPushButton:disabled {{ opacity: 0.3; }}"
        )
        self._next_btn.clicked.connect(self._go_next)
        top_bar.addWidget(self._next_btn)

        top_bar.addSpacing(12)

        # File counter
        self._counter_label = QLabel()
        self._counter_label.setStyleSheet(f"color: {_TEXT_SEC}; font-size: 12px;")
        top_bar.addWidget(self._counter_label)

        top_bar.addSpacing(12)

        # Filename
        self._name_label = QLabel()
        self._name_label.setStyleSheet(f"color: {_TEXT}; font-size: 13px; font-weight: 600;")
        top_bar.addWidget(self._name_label, stretch=1)

        # Zoom controls (shown for images)
        self._zoom_bar = QWidget()
        zl = QHBoxLayout(self._zoom_bar)
        zl.setContentsMargins(0, 0, 0, 0)
        zl.setSpacing(4)

        self._zoom_out_btn = QPushButton()
        self._zoom_out_btn.setIcon(qta.icon("fa5s.search-minus", color=_TEXT))
        self._zoom_out_btn.setFixedSize(28, 28)
        self._zoom_out_btn.setStyleSheet(
            f"QPushButton {{ background: {_SURFACE_3}; border: 1px solid {_BORDER}; border-radius: 6px; }}"
            f"QPushButton:hover {{ background: {_SURFACE_4}; border: 1px solid {_BORDER_HOVER}; }}"
        )
        zl.addWidget(self._zoom_out_btn)

        self._zoom_label = QLabel("100%")
        self._zoom_label.setStyleSheet(f"color: {_TEXT_SEC}; font-size: 11px;")
        zl.addWidget(self._zoom_label)

        self._zoom_in_btn = QPushButton()
        self._zoom_in_btn.setIcon(qta.icon("fa5s.search-plus", color=_TEXT))
        self._zoom_in_btn.setFixedSize(28, 28)
        self._zoom_in_btn.setStyleSheet(
            f"QPushButton {{ background: {_SURFACE_3}; border: 1px solid {_BORDER}; border-radius: 6px; }}"
            f"QPushButton:hover {{ background: {_SURFACE_4}; border: 1px solid {_BORDER_HOVER}; }}"
        )
        zl.addWidget(self._zoom_in_btn)

        self._fit_btn = QPushButton("Fit")
        self._fit_btn.setFixedSize(40, 28)
        self._fit_btn.setStyleSheet(
            f"QPushButton {{ background: {_SURFACE_3}; color: {_TEXT}; border: 1px solid {_BORDER}; "
            f"border-radius: 6px; font-size: 11px; }}"
            f"QPushButton:hover {{ background: {_SURFACE_4}; border: 1px solid {_BORDER_HOVER}; }}"
        )
        zl.addWidget(self._fit_btn)

        top_bar.addWidget(self._zoom_bar)

        top_bar.addSpacing(12)

        # Close
        self._close_btn = QPushButton()
        self._close_btn.setIcon(qta.icon("fa5s.times", color=_TEXT))
        self._close_btn.setFixedSize(32, 32)
        self._close_btn.setToolTip("Close (Escape)")
        self._close_btn.setStyleSheet(
            f"QPushButton {{ background: transparent; border: none; border-radius: 6px; }}"
            f"QPushButton:hover {{ background-color: #F44336; }}"
        )
        self._close_btn.clicked.connect(self.close)
        top_bar.addWidget(self._close_btn)

        root.addLayout(top_bar)

        # Thin separator
        sep = QFrame()
        sep.setFrameShape(QFrame.Shape.HLine)
        sep.setStyleSheet(f"color: {_BORDER};")
        sep.setFixedHeight(1)
        root.addWidget(sep)

        # --- content area ---
        self._image_viewer = _ImageViewer()
        self._video_viewer = _VideoViewer()
        self._sequence_viewer = _SequenceViewer()

        self._content_stack = QWidget()
        self._content_layout = QVBoxLayout(self._content_stack)
        self._content_layout.setContentsMargins(0, 0, 0, 0)
        self._content_layout.setSpacing(0)
        self._content_layout.addWidget(self._image_viewer)
        self._content_layout.addWidget(self._video_viewer)
        self._content_layout.addWidget(self._sequence_viewer)
        self._video_viewer.hide()
        self._sequence_viewer.hide()

        root.addWidget(self._content_stack, stretch=1)

        # Wire zoom
        self._zoom_out_btn.clicked.connect(self._image_viewer.zoom_out)
        self._zoom_in_btn.clicked.connect(self._image_viewer.zoom_in)
        self._fit_btn.clicked.connect(self._image_viewer.fit)
        self._image_viewer.zoom_changed.connect(
            lambda z: self._zoom_label.setText(f"{int(z * 100)}%")
        )

        # Prevent ALL interactive widgets from stealing keyboard focus so
        # that arrow keys, space, comma, period reach MediaViewer.event()
        for w in (
            self._prev_btn,
            self._next_btn,
            self._close_btn,
            self._zoom_out_btn,
            self._zoom_in_btn,
            self._fit_btn,
        ):
            w.setFocusPolicy(Qt.FocusPolicy.NoFocus)

    # -- navigation --

    def _update_nav_buttons(self):
        self._prev_btn.setEnabled(self._index > 0)
        self._next_btn.setEnabled(self._index < len(self._files) - 1)
        self._counter_label.setText(f"{self._index + 1} / {len(self._files)}")

    def _go_prev(self):
        if self._index > 0:
            self._unload_current()
            self._index -= 1
            self._load_current()
            self.selection_changed.emit(self._files[self._index]["path"])

    def _go_next(self):
        if self._index < len(self._files) - 1:
            self._unload_current()
            self._index += 1
            self._load_current()
            self.selection_changed.emit(self._files[self._index]["path"])

    def _unload_current(self):
        """Clean up current media before switching."""
        self._video_viewer.stop()
        if self._sequence_viewer.isVisible():
            self._sequence_viewer.stop()

    def _load_current(self):
        """Load the file at self._index."""
        if not self._files:
            return
        info = self._files[self._index]
        path = info["path"]
        ftype = info.get("type") or Config.get_file_type(path)

        self._name_label.setText(info["name"])
        self._update_nav_buttons()

        if ftype == "sequence":
            self._image_viewer.hide()
            self._video_viewer.hide()
            self._sequence_viewer.show()
            self._zoom_bar.hide()
            frame_paths = info.get("sequence_files", [])
            frame_range = info.get("frame_range", (0, 0))
            self._sequence_viewer.load(frame_paths, frame_range=frame_range, fps=24.0)
        elif ftype == "image":
            self._image_viewer.show()
            self._video_viewer.hide()
            self._sequence_viewer.hide()
            self._zoom_bar.show()
            self._image_viewer.load(path)
        elif ftype == "video":
            self._image_viewer.hide()
            self._video_viewer.show()
            self._sequence_viewer.hide()
            self._zoom_bar.hide()
            self._video_viewer.load(path)
            self._video_viewer.play()
        else:
            self._image_viewer.show()
            self._video_viewer.hide()
            self._sequence_viewer.hide()
            self._zoom_bar.hide()
            self._image_viewer.clear()

    # -- keyboard --

    def event(self, ev):
        """Intercept key presses before QDialog's focus-traversal logic.

        QDialog.event() handles arrow keys for tab-order navigation,
        consuming them before keyPressEvent() is ever called.  We
        override event() to route our shortcut keys directly to
        keyPressEvent(), bypassing that behaviour.
        """
        if ev.type() == QEvent.Type.KeyPress:
            key = ev.key()
            if key in (
                Qt.Key.Key_Left,
                Qt.Key.Key_Right,
                Qt.Key.Key_Space,
                Qt.Key.Key_Comma,
                Qt.Key.Key_Period,
                Qt.Key.Key_Escape,
                Qt.Key.Key_Plus,
                Qt.Key.Key_Equal,
                Qt.Key.Key_Minus,
                Qt.Key.Key_0,
            ):
                self.keyPressEvent(ev)
                return True
        return super().event(ev)

    def keyPressEvent(self, event: QKeyEvent):
        key = event.key()

        if key == Qt.Key.Key_Escape:
            self.close()
            return

        if key == Qt.Key.Key_Left:
            self._go_prev()
            return

        if key == Qt.Key.Key_Right:
            self._go_next()
            return

        # Space: toggle video/sequence playback
        if key == Qt.Key.Key_Space:
            if self._video_viewer.isVisible():
                if self._video_viewer.is_playing:
                    self._video_viewer.pause()
                else:
                    self._video_viewer.play()
                return
            if self._sequence_viewer.isVisible():
                self._sequence_viewer._toggle_play()
                return

        # Frame stepping: comma = back, period = forward
        if self._video_viewer.isVisible():
            if key == Qt.Key.Key_Comma:
                self._video_viewer._frame_step(-1)
                return
            if key == Qt.Key.Key_Period:
                self._video_viewer._frame_step(1)
                return

        if self._sequence_viewer.isVisible():
            if key == Qt.Key.Key_Comma:
                self._sequence_viewer.step(-1)
                return
            if key == Qt.Key.Key_Period:
                self._sequence_viewer.step(1)
                return

        # Zoom shortcuts for images
        if self._image_viewer.isVisible():
            if key == Qt.Key.Key_Plus or key == Qt.Key.Key_Equal:
                self._image_viewer.zoom_in()
                return
            if key == Qt.Key.Key_Minus:
                self._image_viewer.zoom_out()
                return
            if key == Qt.Key.Key_0:
                self._image_viewer.fit()
                return

        super().keyPressEvent(event)

    # -- cleanup --

    def closeEvent(self, event):
        self._video_viewer.stop()
        self._sequence_viewer.cleanup()
        self._image_viewer._loader.stop()
        self._image_viewer._loader.wait(2000)
        super().closeEvent(event)


# ---------------------------------------------------------------------------
# Keep the old name as an alias for backward compatibility (if anything
# still imports FullscreenPreview).
# ---------------------------------------------------------------------------


class FullscreenPreview(MediaViewer):
    """Backward-compatible alias.  Prefer ``MediaViewer``."""

    def __init__(self, file_path: str, file_type: str, parent=None):
        # Wrap single file in the expected list format
        file_info = {
            "path": file_path,
            "name": Path(file_path).name,
            "type": file_type,
        }
        super().__init__([file_info], start_index=0, parent=parent)
