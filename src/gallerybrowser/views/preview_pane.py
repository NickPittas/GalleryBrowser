"""Preview pane with GStreamer support for images and videos."""

import os
from pathlib import Path

from PyQt6.QtCore import QThread, QTimer, Qt, pyqtSignal
from PyQt6.QtGui import QColor, QImage, QPixmap, QPainter
from PyQt6.QtWidgets import (
    QComboBox,
    QFrame,
    QGraphicsScene,
    QGraphicsView,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QSlider,
    QVBoxLayout,
    QWidget,
    QStackedWidget,
)

import qtawesome as qta

from gallerybrowser.config import Config, HAS_OIIO


def has_gstreamer():
    """Check if GStreamer and required Python bindings are available."""
    try:
        import gi

        gi.require_version("Gst", "1.0")
        from gi.repository import Gst

        Gst.init(None)
        return True
    except (ImportError, ValueError):
        return False


class _ImageLoaderWorker(QThread):
    """Persistent background thread for loading and downscaling images.

    Uses PIL/Pillow for I/O — PIL's ``Image.thumbnail()`` can exploit
    JPEG's built-in DCT scaling to avoid decoding full resolution, which
    is dramatically faster for large images read over a NAS.  The result
    is converted to a QImage for emission to the GUI thread.

    The thread stays alive and processes requests from an internal queue,
    avoiding the overhead of creating/destroying a QThread per image.
    """

    image_ready = pyqtSignal(str, QImage)  # file_path, downscaled QImage
    image_failed = pyqtSignal(str)  # file_path

    def __init__(self, parent=None):
        super().__init__(parent)
        self._file_path: str = ""
        self._target_width: int = 0
        self._target_height: int = 0
        self._request_pending = False
        self._stop = False

        import threading

        self._lock = threading.Lock()
        self._event = threading.Event()

    def request(self, file_path: str, target_width: int, target_height: int):
        """Submit a load request. Supersedes any previous pending request."""
        with self._lock:
            self._file_path = file_path
            self._target_width = max(target_width, 256)
            self._target_height = max(target_height, 256)
            self._request_pending = True
        self._event.set()

    def stop(self):
        """Signal the thread to exit."""
        self._stop = True
        self._event.set()

    def run(self):
        """Event loop — wait for requests and process them."""
        from PIL import Image

        while not self._stop:
            self._event.wait()
            self._event.clear()
            if self._stop:
                break

            # Grab the latest request (may have been superseded)
            with self._lock:
                if not self._request_pending:
                    continue
                file_path = self._file_path
                tw = self._target_width
                th = self._target_height
                self._request_pending = False

            try:
                qimage = self._load_with_pil(Image, file_path, tw, th)
                if qimage is None and HAS_OIIO:
                    qimage = self._load_with_oiio(file_path, tw, th)

                if qimage is not None and not qimage.isNull():
                    self.image_ready.emit(file_path, qimage)
                else:
                    self.image_failed.emit(file_path)
            except Exception:
                self.image_failed.emit(file_path)

    @staticmethod
    def _load_with_pil(Image, file_path: str, tw: int, th: int):
        """Load an image using PIL.  Returns QImage or None."""
        try:
            img = Image.open(file_path)

            max_dim = max(tw, th) * 2
            if hasattr(img, "draft") and img.format == "JPEG":
                img.draft("RGB", (max_dim, max_dim))

            img.thumbnail((max_dim, max_dim), Image.Resampling.LANCZOS)

            if img.mode not in ("RGB", "RGBA"):
                img = img.convert("RGBA" if img.mode in ("RGBA", "LA", "PA", "P") else "RGB")

            if img.mode == "RGBA":
                data = img.tobytes("raw", "RGBA")
                qimage = QImage(
                    data, img.width, img.height, img.width * 4, QImage.Format.Format_RGBA8888
                )
            else:
                data = img.tobytes("raw", "RGB")
                qimage = QImage(
                    data, img.width, img.height, img.width * 3, QImage.Format.Format_RGB888
                )

            qimage = qimage.copy()
            img.close()
            return qimage
        except Exception:
            return None

    @staticmethod
    def _load_with_oiio(file_path: str, tw: int, th: int):
        """Load an image using OpenImageIO with NAS-buffered I/O."""
        from gallerybrowser.core.image_io import load_oiio_qimage

        return load_oiio_qimage(file_path, max_dim=max(tw, th) * 2)


class ImagePreviewWidget(QGraphicsView):
    """Widget for displaying images with zoom and pan."""

    zoom_changed = pyqtSignal(float)

    def __init__(self, parent=None):
        super().__init__(parent)
        self._scene = QGraphicsScene(self)
        self.setScene(self._scene)

        self.setRenderHints(QPainter.RenderHint.SmoothPixmapTransform)
        self.setDragMode(QGraphicsView.DragMode.ScrollHandDrag)
        self.setTransformationAnchor(QGraphicsView.ViewportAnchor.AnchorUnderMouse)
        self.setResizeAnchor(QGraphicsView.ViewportAnchor.AnchorUnderMouse)

        self._pixmap_item = None
        self._current_file = None
        self._zoom_level = 1.0

        self.setFrameStyle(QFrame.Shape.NoFrame)
        self.setStyleSheet("background-color: #181818;")

        # Background image loader (persistent thread)
        self._loader = _ImageLoaderWorker(self)
        self._loader.image_ready.connect(self._on_image_loaded)
        self._loader.image_failed.connect(self._on_image_failed)
        self._loader.start()  # Start once, stays alive

    def set_image(self, file_path: str):
        """Load and display an image file (asynchronously).

        Returns True immediately to indicate the load has been started.
        The actual image will appear once the background thread finishes.
        """
        self._current_file = file_path

        # Show a loading placeholder
        self._scene.clear()
        self._pixmap_item = None
        loading_text = self._scene.addText("Loading...")
        loading_text.setDefaultTextColor(Qt.GlobalColor.gray)

        # Submit request to persistent loader thread
        vp = self.viewport().size()
        self._loader.request(file_path, vp.width(), vp.height())
        return True

    def _on_image_loaded(self, file_path: str, qimage: QImage):
        """Handle the loaded image from the background thread."""
        # Discard stale results
        if file_path != self._current_file:
            return

        pixmap = QPixmap.fromImage(qimage)
        if pixmap.isNull():
            self._on_image_failed(file_path)
            return

        self._scene.clear()
        self._pixmap_item = self._scene.addPixmap(pixmap)
        rect = pixmap.rect()
        self._scene.setSceneRect(rect.x(), rect.y(), rect.width(), rect.height())
        self.reset_zoom()

    def _on_image_failed(self, file_path: str):
        """Handle a failed image load."""
        if file_path != self._current_file:
            return

        self._scene.clear()
        self._pixmap_item = None
        error_text = self._scene.addText("Failed to load image")
        error_text.setDefaultTextColor(Qt.GlobalColor.gray)

    def reset_zoom(self):
        """Reset zoom to fit image in view."""
        if self._pixmap_item:
            self.fitInView(self._pixmap_item, Qt.AspectRatioMode.KeepAspectRatio)
            self._zoom_level = 1.0
            self.zoom_changed.emit(self._zoom_level)

    def zoom_in(self):
        """Zoom in."""
        self.scale(1.25, 1.25)
        self._zoom_level *= 1.25
        self.zoom_changed.emit(self._zoom_level)

    def zoom_out(self):
        """Zoom out."""
        self.scale(0.8, 0.8)
        self._zoom_level *= 0.8
        self.zoom_changed.emit(self._zoom_level)

    def wheelEvent(self, event):
        """Handle mouse wheel for zoom."""
        delta = event.angleDelta().y()
        if delta > 0:
            self.zoom_in()
        else:
            self.zoom_out()
        event.accept()

    def clear(self):
        """Clear the preview."""
        self._scene.clear()
        self._pixmap_item = None
        self._current_file = None


class VideoPreviewWidget(QWidget):
    """Widget for displaying videos using GStreamer."""

    duration_changed = pyqtSignal(float)
    position_changed = pyqtSignal(float)
    state_changed = pyqtSignal(str)

    def __init__(self, parent=None):
        super().__init__(parent)
        self._setup_ui()

        self._pipeline = None
        self._appsink = None
        self._has_gstreamer = has_gstreamer()

        if not self._has_gstreamer:
            self._show_fallback_ui()

    def _setup_ui(self):
        """Setup the UI."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(8)

        # Video frame display
        self._video_frame = QLabel("No video loaded")
        self._video_frame.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self._video_frame.setStyleSheet("background-color: #0f0f0f; color: #666666;")
        self._video_frame.setMinimumSize(320, 240)
        self._video_frame.setScaledContents(False)  # We'll scale manually to preserve aspect ratio
        layout.addWidget(self._video_frame, stretch=1)

        # Controls — two-row layout
        controls_container = QVBoxLayout()
        controls_container.setSpacing(4)

        # Row 1: Timeline + time label
        timeline_row = QHBoxLayout()
        timeline_row.setSpacing(8)

        # Timeline slider
        self._timeline = QSlider(Qt.Orientation.Horizontal)
        self._timeline.setRange(0, 1000)
        self._timeline.setStyleSheet("""
            QSlider::groove:horizontal {
                height: 4px;
                background-color: #2a2a2a;
                border-radius: 2px;
            }
            QSlider::handle:horizontal {
                background-color: #2196F3;
                width: 14px;
                height: 14px;
                margin: -5px 0;
                border-radius: 7px;
                border: 2px solid #1e1e1e;
            }
            QSlider::handle:horizontal:hover {
                background-color: #42A5F5;
                width: 16px;
                height: 16px;
                margin: -6px 0;
                border-radius: 8px;
            }
            QSlider::sub-page:horizontal {
                background-color: #2196F3;
                border-radius: 2px;
            }
        """)
        self._timeline.sliderReleased.connect(self._seek)
        self._timeline.valueChanged.connect(self._on_slider_moved)
        timeline_row.addWidget(self._timeline, stretch=1)

        # Timer for throttling live scrubbing updates
        self._scrub_timer = QTimer()
        self._scrub_timer.setSingleShot(True)
        self._scrub_timer.timeout.connect(self._perform_live_seek)

        # Time label
        self._time_label = QLabel("0:00 / 0:00")
        self._time_label.setStyleSheet("color: #a0a0a0; font-size: 11px;")
        timeline_row.addWidget(self._time_label)

        controls_container.addLayout(timeline_row)

        # Row 2: Playback controls
        buttons_row = QHBoxLayout()
        buttons_row.setSpacing(8)

        # Play/Pause button
        self._play_btn = QPushButton()
        self._play_btn.setIcon(qta.icon("fa5s.play", color="#e8e8e8"))
        self._play_btn.setFixedSize(32, 32)
        self._play_btn.setStyleSheet(
            "QPushButton { background-color: #2196F3; border: none; border-radius: 6px; }"
            "QPushButton:hover { background-color: #42A5F5; }"
            "QPushButton:pressed { background-color: #1976D2; }"
        )
        self._play_btn.clicked.connect(self._toggle_playback)
        buttons_row.addWidget(self._play_btn)

        # Stop button
        self._stop_btn = QPushButton()
        self._stop_btn.setIcon(qta.icon("fa5s.stop", color="#e8e8e8"))
        self._stop_btn.setFixedSize(32, 32)
        self._stop_btn.setStyleSheet(
            "QPushButton { background-color: #252525; border: 1px solid #2a2a2a; border-radius: 6px; }"
            "QPushButton:hover { background-color: #2c2c2c; border: 1px solid #3a3a3a; }"
        )
        self._stop_btn.clicked.connect(self.stop)
        buttons_row.addWidget(self._stop_btn)

        # Loop button
        self._loop_btn = QPushButton()
        self._loop_btn.setIcon(qta.icon("fa5s.redo", color="#666666"))
        self._loop_btn.setFixedSize(32, 32)
        self._loop_btn.setToolTip("Loop playback")
        self._loop_btn.setStyleSheet(
            "QPushButton { background-color: #252525; border: 1px solid #2a2a2a; border-radius: 6px; }"
            "QPushButton:hover { background-color: #2c2c2c; border: 1px solid #3a3a3a; }"
        )
        self._loop_btn.clicked.connect(self._toggle_loop)
        buttons_row.addWidget(self._loop_btn)

        buttons_row.addStretch()

        # Volume control
        self._volume_btn = QPushButton()
        self._volume_btn.setIcon(qta.icon("fa5s.volume-up", color="#a0a0a0"))
        self._volume_btn.setFixedSize(32, 32)
        self._volume_btn.setStyleSheet(
            "QPushButton { background-color: transparent; border: none; }"
            "QPushButton:hover { background-color: #2c2c2c; border-radius: 6px; }"
        )
        self._volume_btn.clicked.connect(self._toggle_mute)
        buttons_row.addWidget(self._volume_btn)

        self._volume_slider = QSlider(Qt.Orientation.Horizontal)
        self._volume_slider.setRange(0, 100)
        self._volume_slider.setValue(100)
        self._volume_slider.setFixedWidth(80)
        self._volume_slider.setStyleSheet("""
            QSlider::groove:horizontal {
                height: 4px;
                background-color: #2a2a2a;
                border-radius: 2px;
            }
            QSlider::handle:horizontal {
                background-color: #2196F3;
                width: 12px;
                height: 12px;
                margin: -4px 0;
                border-radius: 6px;
                border: 2px solid #1e1e1e;
            }
            QSlider::handle:horizontal:hover {
                background-color: #42A5F5;
            }
            QSlider::sub-page:horizontal {
                background-color: #2196F3;
                border-radius: 2px;
            }
        """)
        self._volume_slider.valueChanged.connect(self._set_volume)
        buttons_row.addWidget(self._volume_slider)

        # Playback speed
        self._speed_combo = QComboBox()
        self._speed_combo.addItems(["0.5x", "1.0x", "1.5x", "2.0x"])
        self._speed_combo.setCurrentText("1.0x")
        self._speed_combo.setFixedWidth(60)
        self._speed_combo.setStyleSheet("""
            QComboBox {
                background-color: #252525;
                color: #e8e8e8;
                border: 1px solid #2a2a2a;
                border-radius: 4px;
                padding: 2px 6px;
                font-size: 11px;
            }
            QComboBox:hover {
                border: 1px solid #3a3a3a;
                background-color: #2c2c2c;
            }
            QComboBox::drop-down {
                border: none;
                width: 14px;
            }
        """)
        self._speed_combo.currentTextChanged.connect(self._set_playback_speed)
        buttons_row.addWidget(self._speed_combo)

        controls_container.addLayout(buttons_row)
        layout.addLayout(controls_container)

        self._current_file = None
        self._duration = 0
        self._is_playing = False
        self._is_muted = False
        self._loop_enabled = False
        self._fps = 30.0  # Default FPS, updated from video caps
        self._frame_timer = QTimer()
        self._frame_timer.timeout.connect(self._update_frame)
        self._current_pixmap = None  # Store original pixmap for rescaling on resize

        # Timer to poll GStreamer bus for EOS / error messages.
        # GStreamer bus messages arrive on the GStreamer thread; polling
        # from a QTimer on the main thread avoids cross-thread Qt issues.
        self._bus_timer = QTimer()
        self._bus_timer.setInterval(100)  # 10 Hz is plenty
        self._bus_timer.timeout.connect(self._poll_bus)

        # Timer used to defer frame-pull after an async seek
        # Pending preroll frame data set by _on_new_preroll (GStreamer thread)
        # and consumed by _display_preroll_frame (Qt main thread).
        self._pending_preroll = None  # QImage or None

    def _show_fallback_ui(self):
        """Show fallback UI when GStreamer is not available."""
        self._video_frame.setText("Video preview not available\nInstall gstreamer1.0")
        self._play_btn.setEnabled(False)
        self._timeline.setEnabled(False)

    def _toggle_loop(self):
        """Toggle loop playback on/off."""
        self._loop_enabled = not self._loop_enabled
        if self._loop_enabled:
            self._loop_btn.setIcon(qta.icon("fa5s.redo", color="#2196F3"))
            self._loop_btn.setStyleSheet(
                "QPushButton { background-color: #1a3a5c; border: 1px solid #2196F3; border-radius: 6px; }"
                "QPushButton:hover { background-color: #1e4a6e; border: 1px solid #42A5F5; }"
            )
        else:
            self._loop_btn.setIcon(qta.icon("fa5s.redo", color="#666666"))
            self._loop_btn.setStyleSheet(
                "QPushButton { background-color: #252525; border: 1px solid #2a2a2a; border-radius: 6px; }"
                "QPushButton:hover { background-color: #2c2c2c; border: 1px solid #3a3a3a; }"
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
            # Seek back to start and keep playing
            try:
                import gi

                gi.require_version("Gst", "1.0")
                from gi.repository import Gst

                self._pipeline.seek_simple(
                    Gst.Format.TIME,
                    Gst.SeekFlags.FLUSH | Gst.SeekFlags.KEY_UNIT,
                    0,
                )
                # Pipeline stays in PLAYING state; frame timer keeps running
            except Exception as e:
                print(f"Error looping video: {e}")
                self.stop()
        else:
            # Stop playback, seek to start, show first frame
            self._frame_timer.stop()
            self._is_playing = False
            self._play_btn.setIcon(qta.icon("fa5s.play", color="#e8e8e8"))
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
                # new-preroll signal will fire to display the first frame
            except Exception:
                pass
            self._timeline.blockSignals(True)
            self._timeline.setValue(0)
            self._timeline.blockSignals(False)
            self._update_time_label(0, self._duration)
            self.state_changed.emit("stopped")

    def set_video(self, file_path: str):
        """Load and display a video file.

        Uses dynamic pad linking so that files without an audio stream
        (e.g. many .mov files) still play correctly.
        """
        if not self._has_gstreamer:
            return False

        self.stop()

        self._current_file = file_path

        try:
            import gi

            gi.require_version("Gst", "1.0")
            from gi.repository import Gst

            # Build the pipeline manually so we can use decodebin's pad-added
            # signal to dynamically link video and (optional) audio branches.
            # IMPORTANT: Audio elements are only added to the pipeline when an
            # audio pad actually appears.  Pre-adding unlinked audio elements
            # causes the pipeline to hang in ASYNC state for files without audio.
            self._pipeline = Gst.Pipeline.new("preview-pipeline")

            # Source + decoder
            filesrc = Gst.ElementFactory.make("filesrc", "source")
            filesrc.set_property("location", file_path)
            decodebin = Gst.ElementFactory.make("decodebin", "decoder")

            # Video branch: videoconvert -> capsfilter (RGB) -> appsink
            videoconvert = Gst.ElementFactory.make("videoconvert", "vconv")
            capsfilter = Gst.ElementFactory.make("capsfilter", "vcaps")
            capsfilter.set_property("caps", Gst.Caps.from_string("video/x-raw,format=RGB"))
            appsink = Gst.ElementFactory.make("appsink", "sink")
            appsink.set_property("emit-signals", True)
            appsink.set_property("max-buffers", 1)
            appsink.set_property("drop", True)

            # Only video elements are added statically
            for el in (filesrc, decodebin, videoconvert, capsfilter, appsink):
                self._pipeline.add(el)

            filesrc.link(decodebin)
            videoconvert.link(capsfilter)
            capsfilter.link(appsink)

            # Keep a reference to the pipeline for the closure
            pipeline_ref = self._pipeline

            # Dynamic pad linking from decodebin — audio elements are created
            # and added on the fly only when an audio pad is emitted.
            # Guard: only link the first audio stream; files exported from
            # DaVinci Resolve can have 8+ audio streams and creating separate
            # audio pipelines for each one will fail.
            audio_linked = [False]

            def _on_pad_added(_decodebin, pad):
                pad_caps = pad.get_current_caps()
                if pad_caps is None:
                    return
                struct = pad_caps.get_structure(0)
                name = struct.get_name()
                if name.startswith("video/"):
                    sink_pad = videoconvert.get_static_pad("sink")
                    if sink_pad and not sink_pad.is_linked():
                        pad.link(sink_pad)
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
                    sink_pad = aconv.get_static_pad("sink")
                    if sink_pad:
                        pad.link(sink_pad)

            decodebin.connect("pad-added", _on_pad_added)

            self._appsink = appsink

            # Connect new-preroll signal for frame updates after seeks in PAUSED state.
            # The signal fires on the GStreamer streaming thread, so the callback
            # extracts raw pixel data and schedules a Qt-thread display update.
            appsink.connect("new-preroll", self._on_new_preroll)

            # Start playing briefly to decode first frame, then pause
            self._pipeline.set_state(Gst.State.PLAYING)

            # Short non-blocking wait — just enough for demuxer to start.
            # Do NOT block the UI for seconds waiting on full preroll over NAS.
            state_result = self._pipeline.get_state(int(0.5 * Gst.SECOND))

            if state_result[0] in (
                Gst.StateChangeReturn.SUCCESS,
                Gst.StateChangeReturn.NO_PREROLL,
                Gst.StateChangeReturn.ASYNC,
            ):
                success, duration = self._pipeline.query_duration(Gst.Format.TIME)
                if success and duration > 0:
                    self._duration = duration / Gst.SECOND
                    self._update_time_label(0, self._duration)

            # Pull first frame and display it
            self._update_frame()

            # Extract actual framerate from the appsink pad's negotiated caps.
            # This is more reliable than last-sample which may not exist yet.
            self._fps = self._detect_fps()

            # Pause after getting first frame (ready to play but not playing)
            self._pipeline.set_state(Gst.State.PAUSED)
            self._is_playing = False
            self._bus_timer.start()  # Start polling bus for EOS/errors

            # If duration wasn't available yet (ASYNC pipeline), retry now
            # that the pipeline has processed some data.
            if self._duration <= 0 and self._pipeline:
                self._pipeline.get_state(int(0.5 * Gst.SECOND))  # brief retry
                success, duration = self._pipeline.query_duration(Gst.Format.TIME)
                if success and duration > 0:
                    self._duration = duration / Gst.SECOND
                    self._update_time_label(0, self._duration)

            return True

        except Exception as e:
            print(f"Error loading video: {e}")
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
        """Start playback."""
        if not self._has_gstreamer or not self._pipeline:
            return

        try:
            import gi

            gi.require_version("Gst", "1.0")
            from gi.repository import Gst

            self._pipeline.set_state(Gst.State.PLAYING)
            self._is_playing = True
            self._frame_timer.start(33)  # ~30 fps
            self._bus_timer.start()
            self._play_btn.setIcon(qta.icon("fa5s.pause", color="#e8e8e8"))
            self.state_changed.emit("playing")

        except Exception as e:
            print(f"Error starting playback: {e}")

    def pause(self):
        """Pause playback."""
        if not self._has_gstreamer or not self._pipeline:
            return

        try:
            import gi

            gi.require_version("Gst", "1.0")
            from gi.repository import Gst

            self._pipeline.set_state(Gst.State.PAUSED)
            self._is_playing = False
            self._frame_timer.stop()
            self._play_btn.setIcon(qta.icon("fa5s.play", color="#e8e8e8"))
            self.state_changed.emit("paused")

        except Exception as e:
            print(f"Error pausing playback: {e}")

    def stop(self):
        """Stop playback and clean up the pipeline."""
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
            except Exception as e:
                print(f"Error stopping pipeline: {e}")
            finally:
                self._pipeline = None
                self._appsink = None

        self._play_btn.setIcon(qta.icon("fa5s.play", color="#e8e8e8"))

        # Block signals while resetting slider to avoid triggering _on_slider_moved
        self._timeline.blockSignals(True)
        self._timeline.setValue(0)
        self._timeline.blockSignals(False)

        self._update_time_label(0, self._duration)
        self.state_changed.emit("stopped")

    def _toggle_playback(self):
        """Toggle between play and pause."""
        if self._is_playing:
            self.pause()
        else:
            self.play()

    def _seek(self):
        """Seek to position on timeline (non-blocking)."""
        if not self._has_gstreamer or not self._pipeline:
            return

        try:
            import gi

            gi.require_version("Gst", "1.0")
            from gi.repository import Gst

            position = (self._timeline.value() / 1000.0) * self._duration
            self._pipeline.seek_simple(
                Gst.Format.TIME,
                Gst.SeekFlags.FLUSH | Gst.SeekFlags.ACCURATE,
                int(position * Gst.SECOND),
            )

            # When paused, the new-preroll signal will fire and display the frame.

            # Update time label
            self._update_time_label(position, self._duration)

        except Exception as e:
            print(f"Error seeking: {e}")

    def step_frame(self, direction: int):
        """Step forward or backward by exactly one frame.

        Seeks to position ± frame_duration while staying PAUSED, then
        pulls the decoded frame after the seek completes.  Unlike
        scrubbing, we never set the pipeline to PLAYING so the position
        doesn't drift.

        Args:
            direction: +1 for next frame, -1 for previous frame.
        """
        if not self._has_gstreamer or not self._pipeline:
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
                self._update_time_label(pos_sec, self._duration)

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
        self._rescale_frame()

    def _on_slider_moved(self):
        """Handle slider movement for live scrubbing."""
        # Only update if video is paused (not playing)
        if not self._is_playing and self._pipeline:
            # Use a timer to throttle updates (don't update on every pixel movement)
            self._scrub_timer.start(50)  # 50ms delay for smooth live scrubbing

    def _perform_live_seek(self):
        """Perform seek during live scrubbing (while dragging slider)."""
        if not self._has_gstreamer or not self._pipeline:
            return

        try:
            import gi

            gi.require_version("Gst", "1.0")
            from gi.repository import Gst

            position = (self._timeline.value() / 1000.0) * self._duration

            # Seek without blocking
            self._pipeline.seek_simple(
                Gst.Format.TIME,
                Gst.SeekFlags.FLUSH | Gst.SeekFlags.ACCURATE,
                int(position * Gst.SECOND),
            )

            # When paused, the new-preroll signal will fire and display the frame.

            # Update time label
            self._update_time_label(position, self._duration)

        except Exception as e:
            print(f"Error in live scrubbing: {e}")

    def _update_frame(self):
        """Pull frame from GStreamer and display it."""
        if not self._appsink:
            return

        try:
            import gi

            gi.require_version("Gst", "1.0")
            from gi.repository import Gst

            # Use try_pull_sample with timeout to avoid blocking indefinitely
            sample = self._appsink.emit("try-pull-sample", 100000000)  # 100ms timeout in ns
            if sample:
                buffer = sample.get_buffer()
                caps = sample.get_caps()

                structure = caps.get_structure(0)
                width = structure.get_value("width")
                height = structure.get_value("height")

                success, mapinfo = buffer.map(Gst.MapFlags.READ)
                if success:
                    # Create QImage from buffer
                    image = QImage(
                        mapinfo.data, width, height, width * 3, QImage.Format.Format_RGB888
                    )
                    self._current_pixmap = QPixmap.fromImage(image)
                    self._rescale_frame()
                    buffer.unmap(mapinfo)

            # Update position (block signals to avoid feedback loop)
            if self._is_playing and self._pipeline:
                success, position = self._pipeline.query_position(Gst.Format.TIME)
                if success:
                    pos_sec = position / Gst.SECOND
                    if self._duration > 0:
                        value = int((pos_sec / self._duration) * 1000)
                        self._timeline.blockSignals(True)
                        self._timeline.setValue(value)
                        self._timeline.blockSignals(False)
                    self._update_time_label(pos_sec, self._duration)

        except Exception as e:
            print(f"Error updating frame: {e}")

    def _update_time_label(self, position: float, duration: float):
        """Update the time label."""
        pos_str = self._format_time(position)
        dur_str = self._format_time(duration)
        self._time_label.setText(f"{pos_str} / {dur_str}")

    @staticmethod
    def _format_time(seconds: float) -> str:
        """Format time as MM:SS."""
        minutes = int(seconds // 60)
        secs = int(seconds % 60)
        return f"{minutes}:{secs:02d}"

    def _rescale_frame(self):
        """Rescale the current frame to fit the widget while preserving aspect ratio."""
        if self._current_pixmap and not self._current_pixmap.isNull():
            frame_size = self._video_frame.size()
            scaled_pixmap = self._current_pixmap.scaled(
                frame_size,
                Qt.AspectRatioMode.KeepAspectRatio,
                Qt.TransformationMode.SmoothTransformation,
            )
            self._video_frame.setText("")
            self._video_frame.setPixmap(scaled_pixmap)

    def resizeEvent(self, event):
        """Handle resize to rescale the video frame."""
        super().resizeEvent(event)
        self._rescale_frame()

    def clear(self):
        """Clear the video preview."""
        self.stop()
        self._current_file = None
        self._duration = 0
        self._current_pixmap = None
        self._video_frame.setText("No video loaded")
        self._video_frame.setPixmap(QPixmap())

    def _toggle_mute(self):
        """Toggle mute state."""
        if not self._has_gstreamer or not self._pipeline:
            return

        try:
            # Get volume element
            volume = self._pipeline.get_by_name("volume")
            if volume:
                self._is_muted = not self._is_muted
                volume.set_property("mute", self._is_muted)

                # Update icon
                if self._is_muted:
                    self._volume_btn.setIcon(qta.icon("fa5s.volume-mute", color="#e8e8e8"))
                else:
                    self._volume_btn.setIcon(qta.icon("fa5s.volume-up", color="#e8e8e8"))
        except Exception as e:
            print(f"Error toggling mute: {e}")

    def _set_volume(self, value: int):
        """Set volume level (0-100)."""
        if not self._has_gstreamer or not self._pipeline:
            return

        try:
            volume = self._pipeline.get_by_name("volume")
            if volume:
                # Convert 0-100 to 0.0-1.0
                volume.set_property("volume", value / 100.0)

                # Update icon based on volume level
                if value == 0:
                    self._volume_btn.setIcon(qta.icon("fa5s.volume-off", color="#e8e8e8"))
                elif value < 50:
                    self._volume_btn.setIcon(qta.icon("fa5s.volume-down", color="#e8e8e8"))
                else:
                    self._volume_btn.setIcon(qta.icon("fa5s.volume-up", color="#e8e8e8"))
        except Exception as e:
            print(f"Error setting volume: {e}")

    def _set_playback_speed(self, speed_text: str):
        """Set playback speed."""
        if not self._has_gstreamer or not self._pipeline:
            return

        try:
            import gi

            gi.require_version("Gst", "1.0")
            from gi.repository import Gst

            # Parse speed from text (e.g., "1.5x" -> 1.5)
            speed = float(speed_text.replace("x", ""))

            # Set playback rate
            # Note: This requires seeking with the new rate
            position = 0
            success, pos = self._pipeline.query_position(Gst.Format.TIME)
            if success:
                position = pos

            self._pipeline.seek(
                speed,
                Gst.Format.TIME,
                Gst.SeekFlags.FLUSH | Gst.SeekFlags.ACCURATE,
                Gst.SeekType.SET,
                position,
                Gst.SeekType.NONE,
                0,
            )
        except Exception as e:
            print(f"Error setting playback speed: {e}")

    def __del__(self):
        """Clean up GStreamer pipeline on destruction."""
        try:
            if self._pipeline:
                import gi

                gi.require_version("Gst", "1.0")
                from gi.repository import Gst

                self._pipeline.set_state(Gst.State.NULL)
                self._pipeline = None
        except Exception:
            pass


class CacheBarWidget(QWidget):
    """Thin bar showing per-frame cache status above the timeline.

    Cached frames are drawn as red segments; uncached regions are transparent.
    """

    _BAR_COLOR = QColor("#F44336")  # red
    _BG_COLOR = QColor("#1e1e1e")

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

        # Draw background
        painter.fillRect(0, 0, w, h, self._BG_COLOR)

        # Draw cached segments
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(self._BAR_COLOR)

        total = self._total_frames
        for idx in self._cached_indices:
            x0 = int(idx * w / total)
            x1 = int((idx + 1) * w / total)
            painter.drawRect(x0, 0, max(x1 - x0, 1), h)

        painter.end()


class SequencePreviewWidget(QWidget):
    """Widget for previewing image sequences with playback controls.

    Uses a SequencePlayer to drive playback from a RAM cache,
    with timeline scrubbing, play/pause, frame stepping, and FPS display.
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
        self._frame_label.setStyleSheet("background-color: #0f0f0f; color: #666666;")
        self._frame_label.setMinimumSize(320, 240)
        self._frame_label.setScaledContents(False)
        layout.addWidget(self._frame_label, stretch=1)

        # Controls
        controls = QVBoxLayout()
        controls.setSpacing(4)

        # Cache bar (thin red indicator above the timeline)
        self._cache_bar = CacheBarWidget()
        controls.addWidget(self._cache_bar)

        # Row 1: Timeline + frame counter
        timeline_row = QHBoxLayout()
        timeline_row.setSpacing(8)

        self._timeline = QSlider(Qt.Orientation.Horizontal)
        self._timeline.setRange(0, 1000)
        self._timeline.setStyleSheet("""
            QSlider::groove:horizontal {
                height: 4px;
                background-color: #2a2a2a;
                border-radius: 2px;
            }
            QSlider::handle:horizontal {
                background-color: #2196F3;
                width: 14px;
                height: 14px;
                margin: -5px 0;
                border-radius: 7px;
                border: 2px solid #1e1e1e;
            }
            QSlider::handle:horizontal:hover {
                background-color: #42A5F5;
                width: 16px;
                height: 16px;
                margin: -6px 0;
                border-radius: 8px;
            }
            QSlider::sub-page:horizontal {
                background-color: #2196F3;
                border-radius: 2px;
            }
        """)
        self._timeline.sliderReleased.connect(self._on_timeline_released)
        self._timeline.valueChanged.connect(self._on_timeline_moved)
        timeline_row.addWidget(self._timeline, stretch=1)

        self._frame_label_info = QLabel("0 / 0")
        self._frame_label_info.setStyleSheet("color: #a0a0a0; font-size: 11px;")
        self._frame_label_info.setMinimumWidth(80)
        timeline_row.addWidget(self._frame_label_info)

        controls.addLayout(timeline_row)

        # Row 2: Playback buttons
        buttons_row = QHBoxLayout()
        buttons_row.setSpacing(8)

        self._play_btn = QPushButton()
        self._play_btn.setIcon(qta.icon("fa5s.play", color="#e8e8e8"))
        self._play_btn.setFixedSize(32, 32)
        self._play_btn.setStyleSheet(
            "QPushButton { background-color: #2196F3; border: none; border-radius: 6px; }"
            "QPushButton:hover { background-color: #42A5F5; }"
            "QPushButton:pressed { background-color: #1976D2; }"
        )
        self._play_btn.clicked.connect(self._toggle_playback)
        buttons_row.addWidget(self._play_btn)

        self._stop_btn = QPushButton()
        self._stop_btn.setIcon(qta.icon("fa5s.stop", color="#e8e8e8"))
        self._stop_btn.setFixedSize(32, 32)
        self._stop_btn.setStyleSheet(
            "QPushButton { background-color: #252525; border: 1px solid #2a2a2a; border-radius: 6px; }"
            "QPushButton:hover { background-color: #2c2c2c; border: 1px solid #3a3a3a; }"
        )
        self._stop_btn.clicked.connect(self._on_stop)
        buttons_row.addWidget(self._stop_btn)

        buttons_row.addStretch()

        # Cache progress
        self._cache_label = QLabel("")
        self._cache_label.setStyleSheet("color: #666666; font-size: 10px;")
        buttons_row.addWidget(self._cache_label)

        buttons_row.addStretch()

        # FPS label
        self._fps_label = QLabel("24 fps")
        self._fps_label.setStyleSheet("color: #a0a0a0; font-size: 11px;")
        buttons_row.addWidget(self._fps_label)

        # Speed combo
        self._speed_combo = QComboBox()
        self._speed_combo.addItems(["0.5x", "1.0x", "1.5x", "2.0x"])
        self._speed_combo.setCurrentText("1.0x")
        self._speed_combo.setFixedWidth(60)
        self._speed_combo.setStyleSheet("""
            QComboBox {
                background-color: #252525;
                color: #e8e8e8;
                border: 1px solid #2a2a2a;
                border-radius: 4px;
                padding: 2px 6px;
                font-size: 11px;
            }
            QComboBox:hover {
                border: 1px solid #3a3a3a;
                background-color: #2c2c2c;
            }
            QComboBox::drop-down {
                border: none;
                width: 14px;
            }
        """)
        self._speed_combo.currentTextChanged.connect(self._on_speed_changed)
        buttons_row.addWidget(self._speed_combo)

        # Prevent child widgets from stealing keyboard focus so that
        # comma/period frame-stepping in MainWindow.keyPressEvent works.
        for w in (
            self._timeline,
            self._play_btn,
            self._stop_btn,
            self._speed_combo,
        ):
            w.setFocusPolicy(Qt.FocusPolicy.NoFocus)

        # Prevent the slider from jumping by pageStep on accidental key input
        self._timeline.setPageStep(1)
        self._timeline.setSingleStep(1)

        controls.addLayout(buttons_row)
        layout.addLayout(controls)

        # Scrub timer for live timeline dragging
        self._scrub_timer = QTimer()
        self._scrub_timer.setSingleShot(True)
        self._scrub_timer.timeout.connect(self._perform_scrub_seek)
        self._is_user_scrubbing = False

    # -- public API --

    def set_sequence(self, frame_paths, frame_range=(0, 0), fps=24.0):
        """Load a sequence for preview."""
        self._player.set_sequence(frame_paths, frame_range=frame_range, fps=fps)
        total = len(frame_paths)
        self._timeline.setRange(0, max(total - 1, 1))
        self._cache_bar.set_total(total)
        self._fps_label.setText(f"{fps:.0f} fps")
        self._update_frame_info()

    def step_frame(self, direction):
        """Step forward/backward one frame."""
        self._player.step(direction)

    def clear(self):
        """Stop and clear the sequence."""
        self._player.stop()
        self._player._cache.clear()
        self._current_pixmap = None
        self._frame_label.setPixmap(QPixmap())
        self._frame_label.setText("No sequence loaded")
        self._cache_label.setText("")

    def cleanup(self):
        self._player.cleanup()

    # -- slots --

    def _on_frame_changed(self, index, qimage):
        """Display a decoded frame."""
        self._current_pixmap = QPixmap.fromImage(qimage)
        self._rescale_frame()
        # Update timeline (block signals to avoid feedback)
        self._timeline.blockSignals(True)
        self._timeline.setValue(index)
        self._timeline.blockSignals(False)
        self._update_frame_info()

    def _on_state_changed(self, state):
        if state == "playing":
            self._play_btn.setIcon(qta.icon("fa5s.pause", color="#e8e8e8"))
        else:
            self._play_btn.setIcon(qta.icon("fa5s.play", color="#e8e8e8"))

    def _on_cache_progress(self, cached, total):
        if total > 0:
            pct = cached * 100 // total
            self._cache_label.setText(f"Cache: {cached}/{total} ({pct}%)")

    def _on_cache_bitmap(self, indices):
        """Update the cache bar with per-frame status."""
        self._cache_bar.set_cached(indices)

    def _toggle_playback(self):
        self._player.toggle_playback()

    def _on_stop(self):
        self._player.stop()
        self._player.seek(0)

    def _on_timeline_released(self):
        """Seek on slider release."""
        self._is_user_scrubbing = False
        index = self._timeline.value()
        self._player.seek(index)

    def _on_timeline_moved(self):
        """Handle live scrubbing while dragging the slider."""
        if not self._player.is_playing:
            self._is_user_scrubbing = True
            self._scrub_timer.start(50)

    def _perform_scrub_seek(self):
        """Perform the scrub seek."""
        if self._is_user_scrubbing:
            index = self._timeline.value()
            self._player.seek(index)

    def _on_speed_changed(self, text):
        speed = float(text.replace("x", ""))
        self._player.set_speed(speed)

    def _update_frame_info(self):
        """Update the frame counter label."""
        fr = self._player.frame_range
        cur = self._player.current_frame_number
        total = self._player.frame_count
        idx = self._player.current_index
        self._frame_label_info.setText(f"{cur} ({idx + 1}/{total})")

    def _rescale_frame(self):
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
        self._rescale_frame()


class PreviewPane(QWidget):
    """Right top pane for file preview with image, video, and sequence support."""

    def __init__(self):
        super().__init__()
        self._setup_ui()
        self._current_file = None
        self._current_mode = None  # "image", "video", or "sequence"

    def _setup_ui(self):
        """Set up the UI."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(0)

        self._image_preview = ImagePreviewWidget()
        self._video_preview = VideoPreviewWidget()
        self._sequence_preview = SequencePreviewWidget()

        zoom_layout = QHBoxLayout()
        zoom_layout.setContentsMargins(8, 4, 8, 4)
        zoom_layout.setSpacing(4)

        self._zoom_out_btn = QPushButton()
        self._zoom_out_btn.setIcon(qta.icon("fa5s.search-minus", color="#e8e8e8"))
        self._zoom_out_btn.setFixedSize(28, 28)
        self._zoom_out_btn.setStyleSheet(
            "QPushButton { background-color: #252525; border: 1px solid #2a2a2a; border-radius: 6px; }"
            "QPushButton:hover { background-color: #2c2c2c; border: 1px solid #3a3a3a; }"
        )
        self._zoom_out_btn.clicked.connect(self._image_preview.zoom_out)
        zoom_layout.addWidget(self._zoom_out_btn)

        self._zoom_label = QLabel("100%")
        self._zoom_label.setStyleSheet("color: #a0a0a0; font-size: 11px;")
        zoom_layout.addWidget(self._zoom_label)

        self._zoom_in_btn = QPushButton()
        self._zoom_in_btn.setIcon(qta.icon("fa5s.search-plus", color="#e8e8e8"))
        self._zoom_in_btn.setFixedSize(28, 28)
        self._zoom_in_btn.setStyleSheet(
            "QPushButton { background-color: #252525; border: 1px solid #2a2a2a; border-radius: 6px; }"
            "QPushButton:hover { background-color: #2c2c2c; border: 1px solid #3a3a3a; }"
        )
        self._zoom_in_btn.clicked.connect(self._image_preview.zoom_in)
        zoom_layout.addWidget(self._zoom_in_btn)

        self._fit_btn = QPushButton("Fit")
        self._fit_btn.setFixedSize(40, 28)
        self._fit_btn.setStyleSheet(
            "QPushButton { background-color: #252525; color: #e8e8e8; border: 1px solid #2a2a2a; "
            "border-radius: 6px; font-size: 11px; }"
            "QPushButton:hover { background-color: #2c2c2c; border: 1px solid #3a3a3a; }"
        )
        self._fit_btn.clicked.connect(self._image_preview.reset_zoom)
        zoom_layout.addWidget(self._fit_btn)

        zoom_layout.addStretch()

        image_container = QWidget()
        image_layout = QVBoxLayout(image_container)
        image_layout.setContentsMargins(0, 0, 0, 0)
        image_layout.setSpacing(0)
        image_layout.addLayout(zoom_layout)
        image_layout.addWidget(self._image_preview)

        self._stack = QStackedWidget()
        self._stack.addWidget(image_container)  # index 0
        self._stack.addWidget(self._video_preview)  # index 1
        self._stack.addWidget(self._sequence_preview)  # index 2

        layout.addWidget(self._stack)

        self._image_preview.zoom_changed.connect(self._on_zoom_changed)

    def _on_zoom_changed(self, zoom: float):
        """Update zoom label."""
        self._zoom_label.setText(f"{int(zoom * 100)}%")

    def set_file(self, file_path: str, file_info: dict = None):
        """Set the file to preview.

        Parameters
        ----------
        file_path : str
            Path to the file (or representative frame for sequences).
        file_info : dict, optional
            The file_info dict from FilePane.  If provided and the type
            is "sequence", the sequence preview is shown instead of a
            static image.
        """
        # Stop previous mode
        if self._current_mode == "video":
            try:
                self._video_preview.stop()
            except Exception:
                pass
        elif self._current_mode == "sequence":
            try:
                self._sequence_preview.clear()
            except Exception:
                pass

        self._current_file = file_path

        # Detect sequence from file_info
        if file_info and file_info.get("type") == "sequence":
            self._current_mode = "sequence"
            self._stack.setCurrentIndex(2)
            frame_paths = file_info.get("sequence_files", [])
            frame_range = file_info.get("frame_range", (0, 0))
            self._sequence_preview.set_sequence(frame_paths, frame_range=frame_range, fps=24.0)
            return

        file_type = Config.get_file_type(file_path)
        self._current_mode = file_type

        if file_type == "image":
            self._stack.setCurrentIndex(0)
            self._image_preview.set_image(file_path)

        elif file_type == "video":
            self._stack.setCurrentIndex(1)
            try:
                success = self._video_preview.set_video(file_path)
                if not success:
                    self._show_error("Failed to load video")
            except Exception as e:
                print(f"Error loading video: {e}")
                self._show_error("Video playback not available")

        else:
            self._image_preview.clear()
            self._show_error("Preview not available")

    def _show_error(self, message: str):
        """Show error message in image view."""
        self._stack.setCurrentIndex(0)
        self._image_preview._scene.clear()
        text = self._image_preview._scene.addText(message)
        text.setDefaultTextColor(Qt.GlobalColor.gray)

    def cleanup(self):
        """Clean up resources."""
        self._video_preview.stop()
        self._sequence_preview.cleanup()
        self._image_preview._loader.stop()
        self._image_preview._loader.wait(2000)
