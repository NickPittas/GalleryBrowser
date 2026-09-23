"""File pane for displaying files in grid or list view with thumbnails."""

from pathlib import Path
from typing import Optional

import time

from PyQt6.QtCore import Qt, QRectF, QUrl, pyqtSignal, QMimeData, QThread, QMutex
from PyQt6.QtGui import QColor, QDrag, QFont, QImage, QKeyEvent, QPainter, QPixmap
from PyQt6.QtWidgets import (
    QAbstractItemView,
    QGridLayout,
    QHeaderView,
    QLabel,
    QLineEdit,
    QScrollArea,
    QStackedWidget,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
    QWidget,
)

import fileseq
import qtawesome as qta

from gallerybrowser.config import Config
from gallerybrowser.core.thumbnail import get_placeholder_icon
from gallerybrowser.workers.thumbnail_worker import ThumbnailWorker


class _CtrlScrubWorker(QThread):
    """Background thread that creates a GStreamer pipeline and handles seeks.

    This avoids blocking the UI during:
    - moov atom pre-read (MOV/MP4 files with moov at end)
    - Pipeline preroll
    - Frame seeking + decoding (especially slow on NAS for ProRes/H.264 MOV)

    Signals
    -------
    pipeline_ready(int)
        Emitted once the pipeline is prerolled with the duration in nanoseconds.
    frame_ready(QImage, float)
        Emitted each time a new frame is decoded, with the image and the
        seek ratio (0.0–1.0) it corresponds to.
    """

    pipeline_ready = pyqtSignal(int)  # duration_ns
    frame_ready = pyqtSignal(QImage, float)  # image, ratio

    def __init__(self, file_path: str, thumb_size: int, parent=None):
        super().__init__(parent)
        self._file_path = file_path
        self._thumb_size = thumb_size
        self._stop_flag = False
        # Seek request: protected by mutex.  Only the *latest* ratio matters.
        self._mutex = QMutex()
        self._pending_ratio: Optional[float] = None
        self._pipeline = None
        self._appsink = None
        self._duration_ns: int = 0
        self._video_linked = False

    def stop(self):
        """Request the worker to stop and wait for it to finish."""
        self._stop_flag = True
        # Poke a dummy seek so the run-loop wakes up
        self._mutex.lock()
        self._pending_ratio = -1.0
        self._mutex.unlock()
        self.wait(2000)

    def request_seek(self, ratio: float):
        """Queue a seek request (main thread → worker thread)."""
        self._mutex.lock()
        self._pending_ratio = ratio
        self._mutex.unlock()

    def run(self):
        """Worker thread entry point."""
        try:
            self._setup_pipeline()
            if self._stop_flag or self._pipeline is None:
                return
            self._seek_loop()
        except Exception:
            pass
        finally:
            self._teardown()

    # ---- internal helpers (run on worker thread) ----

    def _pre_read_moov(self):
        """Read the last 1 MB of the file to warm the kernel page cache.

        MOV/MP4 containers with the ``moov`` atom at the end of the file
        cause GStreamer's qtdemux to issue a long seek over NAS.  Pre-reading
        the tail into the page cache makes the subsequent open instant.
        """
        import os

        try:
            fsize = os.path.getsize(self._file_path)
            if fsize > 1024 * 1024:
                with open(self._file_path, "rb") as f:
                    f.seek(max(0, fsize - 1024 * 1024))
                    f.read()  # just populate page cache
        except Exception:
            pass

    def _query_duration(self, pipeline, qtdemux):
        """Try to get the duration in nanoseconds from the pipeline or qtdemux."""
        import gi

        gi.require_version("Gst", "1.0")
        from gi.repository import Gst

        success, duration_ns = pipeline.query_duration(Gst.Format.TIME)
        if not success or duration_ns <= 0:
            success, duration_ns = qtdemux.query_duration(Gst.Format.TIME)
            if not success or duration_ns <= 0:
                return 0
        return duration_ns

    def _setup_pipeline(self):
        """Create and preroll the GStreamer pipeline.

        Uses ``qtdemux`` as the demuxer instead of ``decodebin`` at the
        top level.  This avoids the hang that ``decodebin`` causes on MOV
        files with many audio streams (e.g. 8-channel ProRes recordings).
        ``qtdemux`` handles both MOV and MP4 containers.  A secondary
        ``decodebin`` is attached *only* to the video pad from ``qtdemux``
        so the correct decoder (avdec_prores, avdec_h264, …) is selected
        automatically without ever touching the audio pads.
        """
        import gi

        gi.require_version("Gst", "1.0")
        gi.require_version("GstApp", "1.0")
        from gi.repository import Gst, GstApp  # noqa: F401

        Gst.init(None)

        # Pre-warm the page cache so moov-at-end doesn't stall qtdemux
        self._pre_read_moov()

        if self._stop_flag:
            return

        pipeline = Gst.Pipeline.new("ctrl-scrub-pipeline")

        filesrc = Gst.ElementFactory.make("filesrc", "source")
        filesrc.set_property("location", self._file_path)
        qtdemux = Gst.ElementFactory.make("qtdemux", "demux")

        # Secondary decodebin — will only receive the video elementary stream
        video_decodebin = Gst.ElementFactory.make("decodebin", "vdecoder")

        videoconvert = Gst.ElementFactory.make("videoconvert", "vconv")
        videoscale = Gst.ElementFactory.make("videoscale", "vscale")
        capsfilter = Gst.ElementFactory.make("capsfilter", "vcaps")
        capsfilter.set_property("caps", Gst.Caps.from_string("video/x-raw,format=RGB"))
        appsink = Gst.ElementFactory.make("appsink", "sink")
        appsink.set_property("emit-signals", False)
        appsink.set_property("max-buffers", 1)
        appsink.set_property("drop", True)

        for el in (
            filesrc,
            qtdemux,
            video_decodebin,
            videoconvert,
            videoscale,
            capsfilter,
            appsink,
        ):
            pipeline.add(el)

        filesrc.link(qtdemux)
        videoconvert.link(videoscale)
        videoscale.link(capsfilter)
        capsfilter.link(appsink)

        # Link decodebin's dynamic output to videoconvert
        def _on_vdec_pad_added(_decodebin, pad):
            caps = pad.get_current_caps()
            if caps is None:
                return
            struct = caps.get_structure(0)
            if struct.get_name().startswith("video/"):
                sink_pad = videoconvert.get_static_pad("sink")
                if sink_pad and not sink_pad.is_linked():
                    pad.link(sink_pad)

        video_decodebin.connect("pad-added", _on_vdec_pad_added)

        # Link only the first video pad from qtdemux to the video decodebin.
        # Audio and data pads are completely ignored — this is what prevents
        # the hang on multi-audio-stream MOV files.
        self._video_linked = False

        def _on_demux_pad_added(_qtdemux, pad):
            caps = pad.get_current_caps()
            if caps is None:
                caps = pad.query_caps(None)
            if caps is None or caps.is_empty():
                return
            struct = caps.get_structure(0)
            name = struct.get_name()
            if name.startswith("video/") and not self._video_linked:
                sink_pad = video_decodebin.get_static_pad("sink")
                if sink_pad and not sink_pad.is_linked():
                    pad.link(sink_pad)
                    self._video_linked = True

        qtdemux.connect("pad-added", _on_demux_pad_added)

        # Preroll (blocks this worker thread, NOT the UI)
        pipeline.set_state(Gst.State.PAUSED)
        ret = pipeline.get_state(int(5 * Gst.SECOND))  # generous timeout for NAS

        if self._stop_flag:
            pipeline.set_state(Gst.State.NULL)
            return

        # Query duration — with qtdemux, duration is available even if
        # get_state returned ASYNC (unlinked audio pads).
        duration_ns = self._query_duration(pipeline, qtdemux)

        # For very short videos the duration may not be available in
        # PAUSED state (especially when get_state returned ASYNC due to
        # unlinked audio pads).  Briefly switch to PLAYING so the demuxer
        # finishes parsing the moov atom and reports a valid duration.
        if duration_ns <= 0 and not self._stop_flag:
            pipeline.set_state(Gst.State.PLAYING)
            # Pull the first decoded frame — this guarantees the demuxer
            # has fully parsed the stream headers.
            sample = appsink.try_pull_sample(int(1 * Gst.SECOND))
            pipeline.set_state(Gst.State.PAUSED)
            pipeline.get_state(int(2 * Gst.SECOND))
            duration_ns = self._query_duration(pipeline, qtdemux)

        self._pipeline = pipeline
        self._appsink = appsink
        self._duration_ns = duration_ns

        # Notify the main thread
        self.pipeline_ready.emit(duration_ns)

    def _seek_loop(self):
        """Continuously process seek requests until stopped."""
        import gi

        gi.require_version("Gst", "1.0")
        gi.require_version("GstApp", "1.0")
        from gi.repository import Gst, GstApp  # noqa: F401

        while not self._stop_flag:
            # Grab the latest pending ratio
            self._mutex.lock()
            ratio = self._pending_ratio
            self._pending_ratio = None
            self._mutex.unlock()

            if ratio is None:
                # No pending request — sleep briefly and retry
                self.msleep(5)
                continue

            if ratio < 0 or self._stop_flag:
                break  # poison pill

            if self._duration_ns <= 0:
                continue

            target_ns = int(ratio * self._duration_ns)
            target_ns = max(0, min(target_ns, self._duration_ns))

            try:
                # Switch to PLAYING so the seek produces a decodable
                # buffer that flows all the way to appsink.
                self._pipeline.set_state(Gst.State.PLAYING)

                self._pipeline.seek_simple(
                    Gst.Format.TIME,
                    Gst.SeekFlags.FLUSH | Gst.SeekFlags.ACCURATE,
                    target_ns,
                )

                # Pull the decoded frame (blocks until available or timeout)
                sample = self._appsink.try_pull_sample(int(1 * Gst.SECOND))
                if sample is None:
                    # Pause and retry next request
                    self._pipeline.set_state(Gst.State.PAUSED)
                    continue

                # Immediately pause to stop further decoding
                self._pipeline.set_state(Gst.State.PAUSED)

                buf = sample.get_buffer()
                caps = sample.get_caps()
                struct = caps.get_structure(0)
                width = struct.get_value("width")
                height = struct.get_value("height")

                ok, mapinfo = buf.map(Gst.MapFlags.READ)
                if not ok:
                    continue

                try:
                    qimg = QImage(
                        mapinfo.data,
                        width,
                        height,
                        width * 3,
                        QImage.Format.Format_RGB888,
                    ).copy()
                finally:
                    buf.unmap(mapinfo)

                if not qimg.isNull():
                    self.frame_ready.emit(qimg, ratio)

            except Exception:
                pass

    def _teardown(self):
        """Clean up the pipeline."""
        if self._pipeline is not None:
            try:
                import gi

                gi.require_version("Gst", "1.0")
                from gi.repository import Gst

                self._pipeline.set_state(Gst.State.NULL)
            except Exception:
                pass
            self._pipeline = None
            self._appsink = None


class FileItemWidget(QWidget):
    """Widget representing a single file item in the grid."""

    clicked = pyqtSignal(str, object)  # Emits file path and mouse event
    doubleClicked = pyqtSignal(str)  # Emits file path on double click

    # Class-level shared GStreamer pipeline for Ctrl+hover real-time scrubbing.
    # Only one widget can own the pipeline at a time.
    _gst_pipeline = None  # The active Gst.Pipeline (or None)
    _gst_appsink = None  # The appsink element
    _gst_owner = None  # The FileItemWidget instance that owns the pipeline
    _gst_duration_ns: int = 0  # Duration of the loaded video in nanoseconds
    _gst_worker = None  # The _CtrlScrubWorker QThread (or None)

    def __init__(self, file_info: dict, size: int = 128, parent=None):
        super().__init__(parent)
        self.file_info = file_info
        self.thumb_size = size
        self.is_selected = False
        self._original_pixmap: Optional[QPixmap] = None
        self._is_scrubbing = False
        self._drag_start_pos = None
        # Ctrl+hover real-time scrubbing state
        self._ctrl_scrubbing = False
        self._last_seek_time: float = 0.0  # monotonic timestamp of last seek
        self.setup_ui()

    def setup_ui(self):
        """Set up the UI."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(2, 2, 2, 4)
        layout.setSpacing(2)
        layout.setAlignment(Qt.AlignmentFlag.AlignCenter)

        # Thumbnail or icon
        self.thumb_label = QLabel()
        self.thumb_label.setFixedSize(self.thumb_size, self.thumb_size)
        self.thumb_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.thumb_label.setStyleSheet(
            "background-color: #1e1e1e; border-radius: 4px; border: 1px solid #2a2a2a;"
        )

        # Show placeholder icon initially
        icon_name = get_placeholder_icon(self.file_info["type"])
        icon = qta.icon(icon_name, color="#666666", scale_factor=2.0)
        self.thumb_label.setPixmap(icon.pixmap(self.thumb_size // 2, self.thumb_size // 2))

        layout.addWidget(self.thumb_label, alignment=Qt.AlignmentFlag.AlignCenter)

        # Filename
        self.name_label = QLabel(self.file_info["name"])
        self.name_label.setWordWrap(True)
        self.name_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.name_label.setMaximumWidth(self.thumb_size + 4)
        self.name_label.setStyleSheet("color: #e8e8e8; font-size: 11px; background: transparent;")
        layout.addWidget(self.name_label)

        # Set fixed size for the widget
        self.setFixedWidth(self.thumb_size + 8)

        # Enable mouse tracking for hover scrubbing on video items
        if self.file_info.get("type") == "video":
            self.setMouseTracking(True)
            self.thumb_label.setMouseTracking(True)

    def set_thumbnail(self, pixmap: QPixmap):
        """Set the thumbnail image, with a frame-count badge for sequences."""
        if pixmap and not pixmap.isNull():
            scaled = pixmap.scaled(
                self.thumb_size,
                self.thumb_size,
                Qt.AspectRatioMode.KeepAspectRatio,
                Qt.TransformationMode.SmoothTransformation,
            )
            # Paint frame count badge for sequences
            if self.file_info.get("type") == "sequence":
                scaled = self._paint_sequence_badge(scaled)
            self._original_pixmap = scaled
            if not self._is_scrubbing:
                self.thumb_label.setPixmap(scaled)

    def _paint_sequence_badge(self, pixmap: QPixmap) -> QPixmap:
        """Paint a frame-count badge in the top-right corner of a pixmap."""
        frame_count = self.file_info.get("frame_count", 0)
        if frame_count < 2:
            return pixmap

        display = QPixmap(pixmap)
        painter = QPainter(display)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        label = f"{frame_count}f"
        from PyQt6.QtGui import QFont, QFontMetrics

        font = QFont("sans-serif", 9, QFont.Weight.Bold)
        painter.setFont(font)
        fm = QFontMetrics(font)
        text_width = fm.horizontalAdvance(label)
        text_height = fm.height()

        pad_x, pad_y = 5, 2
        badge_w = text_width + pad_x * 2
        badge_h = text_height + pad_y * 2
        badge_x = display.width() - badge_w - 3
        badge_y = 3

        # Draw rounded-rect background
        from PyQt6.QtCore import QRectF
        from PyQt6.QtGui import QBrush

        badge_rect = QRectF(badge_x, badge_y, badge_w, badge_h)
        painter.setBrush(QBrush(QColor("#1565C0")))
        painter.setPen(Qt.PenStyle.NoPen)
        painter.drawRoundedRect(badge_rect, 4, 4)

        # Draw text
        painter.setPen(QColor("#ffffff"))
        painter.drawText(badge_rect, Qt.AlignmentFlag.AlignCenter, label)
        painter.end()
        return display

    def _restore_thumbnail(self):
        """Restore the original thumbnail after scrubbing ends."""
        self._is_scrubbing = False
        self._current_scrub_index = -1
        if self._original_pixmap and not self._original_pixmap.isNull():
            self.thumb_label.setPixmap(self._original_pixmap)

    # ------------------------------------------------------------------
    # Ctrl+hover real-time GStreamer scrubbing
    # ------------------------------------------------------------------

    @classmethod
    def _destroy_gst_pipeline(cls):
        """Tear down the shared GStreamer pipeline and worker thread."""
        # Stop the worker thread first
        if cls._gst_worker is not None:
            cls._gst_worker.stop()
            cls._gst_worker = None
        if cls._gst_pipeline is not None:
            try:
                import gi

                gi.require_version("Gst", "1.0")
                from gi.repository import Gst

                cls._gst_pipeline.set_state(Gst.State.NULL)
            except Exception:
                pass
            cls._gst_pipeline = None
            cls._gst_appsink = None
            cls._gst_duration_ns = 0
        cls._gst_owner = None

    def _start_ctrl_scrub(self):
        """Create a GStreamer pipeline for real-time Ctrl+hover scrubbing.

        Pipeline creation + moov pre-read happen in a background thread
        so the UI stays responsive even for large MOV files on NAS.
        """
        # Tear down any existing pipeline (another widget may own it)
        FileItemWidget._destroy_gst_pipeline()

        FileItemWidget._gst_owner = self
        self._ctrl_scrubbing = True
        self._last_seek_time = 0.0

        # Create and start the worker thread
        worker = _CtrlScrubWorker(self.file_info["path"], self.thumb_size)
        worker.pipeline_ready.connect(self._on_pipeline_ready)
        worker.frame_ready.connect(self._on_scrub_frame)
        FileItemWidget._gst_worker = worker
        worker.start()

    def _on_pipeline_ready(self, duration_ns: int):
        """Called (on main thread) when the background pipeline is ready."""
        if FileItemWidget._gst_owner is not self:
            return
        worker = FileItemWidget._gst_worker
        if worker is not None:
            FileItemWidget._gst_pipeline = worker._pipeline
            FileItemWidget._gst_appsink = worker._appsink
        FileItemWidget._gst_duration_ns = duration_ns

    def _on_scrub_frame(self, image: QImage, ratio: float):
        """Called (on main thread) when a scrub frame has been decoded."""
        if FileItemWidget._gst_owner is not self or not self._ctrl_scrubbing:
            return
        if image.isNull():
            return

        # ponytail: QPixmap belongs to the GUI thread; pass QImage from the worker.
        scaled = QPixmap.fromImage(image).scaled(
            self.thumb_size,
            self.thumb_size,
            Qt.AspectRatioMode.KeepAspectRatio,
            Qt.TransformationMode.SmoothTransformation,
        )

        # Paint a thin progress bar at the bottom
        display = QPixmap(scaled)
        painter = QPainter(display)
        bar_h = 3
        bar_y = display.height() - bar_h
        painter.fillRect(0, bar_y, display.width(), bar_h, Qt.GlobalColor.black)
        fill_w = int(display.width() * ratio)
        painter.fillRect(0, bar_y, fill_w, bar_h, QColor("#4CAF50"))  # green = live
        painter.end()

        self.thumb_label.setPixmap(display)

    def _stop_ctrl_scrub(self):
        """Destroy the pipeline and restore the thumbnail."""
        self._ctrl_scrubbing = False
        if FileItemWidget._gst_owner is self:
            FileItemWidget._destroy_gst_pipeline()
        self._restore_thumbnail()

    def _seek_ctrl_scrub(self, ratio: float):
        """Request a seek to *ratio* (0.0 – 1.0) of the video.

        The actual seek happens asynchronously in the worker thread.
        """
        if FileItemWidget._gst_owner is not self:
            return

        worker = FileItemWidget._gst_worker
        if worker is None or not worker.isRunning():
            return

        # Throttle: at most one seek request every ~33 ms
        now = time.monotonic()
        if now - self._last_seek_time < 0.033:
            return
        self._last_seek_time = now

        worker.request_seek(ratio)

    def enterEvent(self, event):
        """Enable mouse tracking for video thumbnails on enter.

        The actual scrub pipeline only starts when Ctrl is held during
        ``mouseMoveEvent``, so merely hovering has zero cost.
        """
        if self.file_info.get("type") == "video":
            self._is_scrubbing = True
        super().enterEvent(event)

    def leaveEvent(self, event):
        """End scrubbing mode when mouse leaves."""
        if self._ctrl_scrubbing:
            self._stop_ctrl_scrub()
        if self._is_scrubbing:
            self._restore_thumbnail()
        super().leaveEvent(event)

    def set_selected(self, selected: bool):
        """Set the selection state."""
        self.is_selected = selected
        if selected:
            # Selected state - subtle accent surface with accent border
            self.setStyleSheet(
                "background-color: #1a3a5c; border-radius: 4px; border: 1px solid #2196F3;"
            )
            # Thumbnail with accent tint
            self.thumb_label.setStyleSheet(
                "background-color: #0d47a1; border-radius: 4px; border: 1px solid #42A5F5;"
            )
            # Bright filename
            self.name_label.setStyleSheet(
                "color: #ffffff; font-size: 11px; background: transparent; "
                "font-weight: 600; border: none;"
            )
        else:
            # Unselected state
            self.setStyleSheet(
                "background-color: transparent; border-radius: 4px; border: 1px solid transparent;"
            )
            self.thumb_label.setStyleSheet(
                "background-color: #1e1e1e; border-radius: 4px; border: 1px solid #2a2a2a;"
            )
            self.name_label.setStyleSheet(
                "color: #e8e8e8; font-size: 11px; background: transparent; border: none;"
            )

        # Force immediate repaint
        self.update()

    def mousePressEvent(self, event):
        """Handle mouse press."""
        if event.button() == Qt.MouseButton.LeftButton:
            self._drag_start_pos = event.position().toPoint()
            self.clicked.emit(self.file_info["path"], event)
        super().mousePressEvent(event)

    def mouseDoubleClickEvent(self, event):
        """Handle double click."""
        if event.button() == Qt.MouseButton.LeftButton:
            self.doubleClicked.emit(self.file_info["path"])
        super().mouseDoubleClickEvent(event)

    def mouseMoveEvent(self, event):
        """Handle mouse move for Ctrl+hover scrub and drag initiation."""
        # --- Video Ctrl+hover scrubbing (no button pressed) ---
        if self._is_scrubbing and self.file_info.get("type") == "video" and not event.buttons():
            ctrl_held = bool(event.modifiers() & Qt.KeyboardModifier.ControlModifier)

            if ctrl_held:
                # Ctrl is held → start or continue live GStreamer scrub
                if not self._ctrl_scrubbing:
                    self._start_ctrl_scrub()

                if self._ctrl_scrubbing:
                    thumb_pos = self.thumb_label.mapFromParent(event.pos())
                    x = thumb_pos.x()
                    w = self.thumb_label.width()
                    if 0 <= x < w and 0 <= thumb_pos.y() < self.thumb_label.height():
                        ratio = max(0.0, min(1.0, x / w))
                        self._seek_ctrl_scrub(ratio)
                return
            else:
                # Ctrl released → tear down pipeline, restore thumbnail
                if self._ctrl_scrubbing:
                    self._stop_ctrl_scrub()
                    self._is_scrubbing = True  # stay ready for next Ctrl press
                return

        # Drag initiation: left button held
        if not (event.buttons() & Qt.MouseButton.LeftButton):
            return

        # Check if we've moved enough to start a drag
        if self._drag_start_pos is None:
            return

        mouse_pos = event.position().toPoint()
        if (mouse_pos - self._drag_start_pos).manhattanLength() < 10:
            return

        # Gather all selected files (multi-select aware)
        paths = [self.file_info["path"]]
        file_pane = self._find_file_pane()
        if file_pane is not None:
            selected = file_pane.get_selected_files()
            if selected and self.file_info["path"] in selected:
                paths = selected

        # Start drag
        drag = QDrag(self)
        mime_data = QMimeData()
        mime_data.setText("\n".join(paths))
        mime_data.setUrls([QUrl.fromLocalFile(p) for p in paths])

        drag.setMimeData(mime_data)

        # Build a small drag pixmap from the thumbnail (not self.grab(),
        # which can produce oversized images and wrong hotspot offsets).
        thumb_pix = self.thumb_label.pixmap()
        if thumb_pix and not thumb_pix.isNull():
            # Scale down to a reasonable drag preview size
            drag_size = 96
            pixmap = thumb_pix.scaled(
                drag_size,
                drag_size,
                Qt.AspectRatioMode.KeepAspectRatio,
                Qt.TransformationMode.SmoothTransformation,
            )
        else:
            pixmap = self.grab().scaled(
                96,
                96,
                Qt.AspectRatioMode.KeepAspectRatio,
                Qt.TransformationMode.SmoothTransformation,
            )

        # Add count badge for multi-file drag
        if len(paths) > 1:
            badge_pixmap = pixmap.copy()
            painter = QPainter(badge_pixmap)
            painter.setRenderHint(QPainter.RenderHint.Antialiasing)
            badge_text = str(len(paths))
            font = QFont()
            font.setPixelSize(12)
            font.setBold(True)
            painter.setFont(font)
            fm = painter.fontMetrics()
            text_width = fm.horizontalAdvance(badge_text)
            badge_size = max(20, text_width + 10)
            badge_x = badge_pixmap.width() - badge_size - 2
            badge_y = 2
            painter.setPen(Qt.PenStyle.NoPen)
            painter.setBrush(QColor("#2196F3"))
            painter.drawEllipse(badge_x, badge_y, badge_size, badge_size)
            painter.setPen(QColor("#ffffff"))
            text_rect = QRectF(badge_x, badge_y, badge_size, badge_size)
            painter.drawText(text_rect, Qt.AlignmentFlag.AlignCenter, badge_text)
            painter.end()
            pixmap = badge_pixmap

        drag.setPixmap(pixmap)
        # Centre the drag pixmap under the cursor
        drag.setHotSpot(pixmap.rect().center())

        drag.exec(Qt.DropAction.MoveAction | Qt.DropAction.CopyAction)

    def contextMenuEvent(self, event):
        """Show context menu on right click.

        Delegates all actions to the parent FilePane so they work
        correctly with multi-selection.
        """
        from PyQt6.QtWidgets import QMenu

        # Find the owning FilePane
        file_pane = self._find_file_pane()
        if not file_pane:
            return

        # If this item isn't selected, select it first (like a file manager)
        if self.file_info["path"] not in file_pane.selected_files:
            file_pane.clear_selection()
            file_pane.selected_files.add(self.file_info["path"])
            self.set_selected(True)
            file_pane.files_selected.emit(list(file_pane.selected_files))

        selected = file_pane.get_selected_files()
        count = len(selected)

        menu = QMenu(self)

        open_action = menu.addAction("Open")
        menu.addSeparator()
        cut_action = menu.addAction("Cut")
        copy_action = menu.addAction("Copy")
        menu.addSeparator()

        rename_action = None
        batch_rename_action = None
        if count == 1:
            rename_action = menu.addAction("Rename")
        if count > 1:
            batch_rename_action = menu.addAction(f"Batch Rename ({count} files)...")

        delete_action = menu.addAction("Delete")
        menu.addSeparator()

        tags_menu = menu.addMenu("Tags")
        add_tag_action = tags_menu.addAction("Assign Tag...")
        remove_tag_action = tags_menu.addAction("Remove Tag...")

        collections_menu = menu.addMenu("Collections")
        add_collection_action = collections_menu.addAction("Add to Collection...")
        remove_collection_action = collections_menu.addAction("Remove from Collection...")

        rating_menu = menu.addMenu("Rating")
        rating_actions = {}
        for value in range(0, 6):
            label = "0 Stars" if value == 0 else f"{value} Star{'s' if value != 1 else ''}"
            rating_action = rating_menu.addAction(label)
            rating_actions[rating_action] = value
        clear_rating_action = rating_menu.addAction("Clear Rating")

        # "Add folder to Favorites" — if current folder is set
        add_fav_action = None
        if file_pane.current_folder:
            add_fav_action = menu.addAction("Add Folder to Favorites")

        action = menu.exec(event.globalPos())
        if not action:
            return

        if action == open_action:
            file_pane.action_open.emit(selected)
        elif action == cut_action:
            file_pane.action_cut.emit(selected)
        elif action == copy_action:
            file_pane.action_copy.emit(selected)
        elif rename_action and action == rename_action:
            file_pane.action_rename.emit(selected)
        elif batch_rename_action and action == batch_rename_action:
            file_pane.action_batch_rename.emit(selected)
        elif action == delete_action:
            file_pane.action_delete.emit(selected)
        elif action == add_tag_action:
            file_pane.action_add_tag.emit()
        elif action == remove_tag_action:
            file_pane.action_remove_tag.emit()
        elif action == add_collection_action:
            file_pane.action_add_to_collection.emit()
        elif action == remove_collection_action:
            file_pane.action_remove_from_collection.emit()
        elif action in rating_actions:
            file_pane.action_set_rating.emit(rating_actions[action])
        elif action == clear_rating_action:
            file_pane.action_clear_rating.emit()
        elif add_fav_action and action == add_fav_action:
            file_pane.action_add_favorite.emit(file_pane.current_folder)

    def _find_file_pane(self):
        """Walk up the widget hierarchy to find the owning FilePane."""
        w = self.parent()
        while w is not None:
            if isinstance(w, FilePane):
                return w
            w = w.parent()
        return None

    def on_open(self):
        """Open the file with default application."""
        import subprocess

        subprocess.run(["xdg-open", self.file_info["path"]])

    def on_cut(self):
        """Emit signal to cut this file."""
        # This would need to be connected to parent
        pass

    def on_copy(self):
        """Emit signal to copy this file."""
        pass

    def on_rename(self):
        """Emit signal to rename this file."""
        pass

    def on_delete(self):
        """Emit signal to delete this file."""
        pass

    def on_properties(self):
        """Show file properties."""
        pass


class FilePane(QWidget):
    """Center pane showing files in current folder."""

    file_selected = pyqtSignal(str)  # Emits file path when single selected
    files_selected = pyqtSignal(list)  # Emits list of selected file paths
    # Context menu action signals (emitted with list of selected file paths)
    action_open = pyqtSignal(list)
    action_cut = pyqtSignal(list)
    action_copy = pyqtSignal(list)
    action_rename = pyqtSignal(list)
    action_batch_rename = pyqtSignal(list)
    action_delete = pyqtSignal(list)
    action_add_favorite = pyqtSignal(str)  # Emits folder path
    action_add_tag = pyqtSignal()
    action_remove_tag = pyqtSignal()
    action_add_to_collection = pyqtSignal()
    action_remove_from_collection = pyqtSignal()
    action_set_rating = pyqtSignal(int)
    action_clear_rating = pyqtSignal()

    def __init__(self):
        super().__init__()
        self.current_folder: str = ""
        self.source_label: str = ""
        self.files: list = []  # All files in folder
        self.filtered_files: list = []  # Files after search filter
        self.file_widgets: dict = {}  # path -> FileItemWidget
        self.selected_files: set = set()  # Set of selected file paths
        self.last_selected_index: int = -1  # For shift-click range selection
        self.thumbnail_size = 128
        self.columns = 4
        self.view_mode = "grid"  # "grid" or "list"
        self.search_text = ""
        self.sort_by = "name"  # name, size, date, type
        self.sort_ascending = True
        self.filter_type = "all"  # all, image, video
        self.collapse_sequences = True  # When True, image sequences are collapsed

        # Create thumbnail worker
        self.thumbnail_worker = ThumbnailWorker()
        self.thumbnail_worker.thumbnail_ready.connect(self.on_thumbnail_ready)

        # Track visible items for lazy loading
        self._visible_items = set()
        self._scroll_debounce_timer = None

        self.setup_ui()

    def setup_ui(self):
        """Set up the UI."""
        layout = QVBoxLayout(self)
        layout.setContentsMargins(2, 2, 2, 2)
        layout.setSpacing(0)

        # Create stacked widget to hold both views
        self.stacked_widget = QStackedWidget()

        # Grid view
        self.grid_scroll = QScrollArea()
        self.grid_scroll.setWidgetResizable(True)
        self.grid_scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)

        self.grid_container = QWidget()
        self.grid_layout = QGridLayout(self.grid_container)
        self.grid_layout.setSpacing(4)
        self.grid_layout.setContentsMargins(2, 2, 2, 2)
        self.grid_layout.setAlignment(Qt.AlignmentFlag.AlignTop | Qt.AlignmentFlag.AlignLeft)
        self.grid_scroll.setWidget(self.grid_container)

        self.stacked_widget.addWidget(self.grid_scroll)

        # List view (Table with columns)
        self.list_widget = QTableWidget()
        self.list_widget.setColumnCount(4)
        self.list_widget.setHorizontalHeaderLabels(["Name", "Size", "Date", "Type"])
        self.list_widget.horizontalHeader().setSectionResizeMode(0, QHeaderView.ResizeMode.Stretch)
        self.list_widget.horizontalHeader().setSectionResizeMode(1, QHeaderView.ResizeMode.Fixed)
        self.list_widget.horizontalHeader().setSectionResizeMode(2, QHeaderView.ResizeMode.Fixed)
        self.list_widget.horizontalHeader().setSectionResizeMode(3, QHeaderView.ResizeMode.Fixed)
        self.list_widget.setColumnWidth(1, 80)
        self.list_widget.setColumnWidth(2, 140)
        self.list_widget.setColumnWidth(3, 100)
        self.list_widget.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self.list_widget.setSelectionMode(QAbstractItemView.SelectionMode.ExtendedSelection)
        self.list_widget.setStyleSheet("""
            QTableWidget {
                background-color: #181818;
                border: none;
                gridline-color: #2a2a2a;
            }
            QTableWidget::item {
                padding: 4px 8px;
                color: #e8e8e8;
                border-bottom: 1px solid #2a2a2a;
            }
            QTableWidget::item:selected {
                background-color: #1a3a5c;
                color: #e8e8e8;
            }
            QTableWidget::item:hover:!selected {
                background-color: #2c2c2c;
            }
            QHeaderView::section {
                background-color: #1e1e1e;
                padding: 6px 12px;
                border: none;
                border-bottom: 1px solid #2a2a2a;
                color: #a0a0a0;
                font-weight: 600;
                font-size: 11px;
            }
            QHeaderView::section:hover {
                background-color: #252525;
                color: #e8e8e8;
            }
        """)
        self.list_widget.itemSelectionChanged.connect(self._on_list_selection_changed)
        self.list_widget.itemClicked.connect(self._on_list_item_clicked)
        self.list_widget.itemDoubleClicked.connect(self._on_list_item_double_clicked)

        self.stacked_widget.addWidget(self.list_widget)

        layout.addWidget(self.stacked_widget)

    def set_collapse_sequences(self, enabled: bool):
        """Toggle sequence collapsing and refresh the current folder."""
        if self.collapse_sequences == enabled:
            return
        self.collapse_sequences = enabled
        if self.current_folder:
            self.set_folder(self.current_folder)

    def set_folder(self, folder_path: str):
        """Set the current folder and display its files."""
        self.set_files(
            self.scan_folder(folder_path),
            current_folder=folder_path,
            source_label=folder_path,
        )

    def set_files(self, files: list, current_folder: str = "", source_label: str = ""):
        """Set an explicit file source for display."""
        self.current_folder = current_folder
        self.source_label = source_label or current_folder
        self.files = list(files)
        self.clear_selection()

        # Stop any running thumbnail worker
        if self.thumbnail_worker.isRunning():
            self.thumbnail_worker.stop()
        self.thumbnail_worker.clear_queue()

        self.display_files()

        # Start generating thumbnails
        self.generate_thumbnails()

    def scan_folder(self, folder_path: str) -> list:
        """Scan folder for supported files, optionally collapsing image sequences.

        When ``self.collapse_sequences`` is True, uses ``fileseq`` to detect
        numbered image sequences (e.g. ``render.0001.exr`` … ``render.0100.exr``)
        and collapses them into a single ``type="sequence"`` entry.  When False,
        all files are listed individually.  Non-sequence images and videos are
        always returned as normal ``type="image"`` / ``type="video"`` entries.
        """
        files: list[dict] = []
        try:
            path = Path(folder_path)
            entries = sorted(path.iterdir(), key=lambda x: x.name.lower())
        except PermissionError:
            return files

        # Separate videos (never part of a sequence) from image candidates.
        video_entries: list[Path] = []
        image_names: list[str] = []  # just filenames for fileseq
        image_entries: dict[str, Path] = {}  # name -> full Path for later lookup

        for item in entries:
            if not item.is_file():
                continue
            ftype = Config.get_file_type(str(item))
            if ftype == "video":
                video_entries.append(item)
            elif ftype == "image":
                image_names.append(item.name)
                image_entries[item.name] = item

        # --- Fast path: no sequence detection ---
        if not self.collapse_sequences:
            for name, item in image_entries.items():
                try:
                    stat = item.stat()
                    files.append(
                        {
                            "path": str(item),
                            "name": item.name,
                            "type": "image",
                            "size": stat.st_size,
                            "modified": stat.st_mtime,
                        }
                    )
                except OSError:
                    pass

            for item in video_entries:
                try:
                    stat = item.stat()
                    files.append(
                        {
                            "path": str(item),
                            "name": item.name,
                            "type": "video",
                            "size": stat.st_size,
                            "modified": stat.st_mtime,
                        }
                    )
                except OSError:
                    pass

            files.sort(key=lambda x: x["name"].lower())
            return files

        # --- Detect sequences among image files ---
        sequences = fileseq.findSequencesInList(image_names)
        consumed_names: set[str] = set()

        for seq in sequences:
            frame_set = seq.frameSet()
            if frame_set is not None and len(seq) >= Config.SEQUENCE_MIN_FRAMES:
                # This is a real sequence — collapse it
                frame_files: list[str] = []
                total_size = 0
                latest_mtime = 0.0
                for frame_num in frame_set:
                    fname = seq.frame(frame_num)
                    full_path = image_entries.get(fname)
                    if full_path is not None:
                        frame_files.append(str(full_path))
                        consumed_names.add(fname)
                        try:
                            stat = full_path.stat()
                            total_size += stat.st_size
                            latest_mtime = max(latest_mtime, stat.st_mtime)
                        except OSError:
                            pass

                if not frame_files:
                    continue

                # Use the middle frame as the representative path (for thumbnail)
                mid_idx = len(frame_files) // 2
                representative = frame_files[mid_idx]

                # Build display name: "render.[0001-0100].exr"
                display_name = f"{seq.basename()}[{seq.frameRange()}]{seq.extension()}"

                files.append(
                    {
                        "path": representative,
                        "name": display_name,
                        "type": "sequence",
                        "size": total_size,
                        "modified": latest_mtime,
                        "sequence_files": frame_files,
                        "frame_range": (seq.start(), seq.end()),
                        "frame_count": len(frame_files),
                    }
                )
            else:
                # Single file (or under threshold) — treat as normal image
                fname = str(seq)
                full_path = image_entries.get(fname)
                if full_path is not None:
                    consumed_names.add(fname)
                    try:
                        stat = full_path.stat()
                        files.append(
                            {
                                "path": str(full_path),
                                "name": full_path.name,
                                "type": "image",
                                "size": stat.st_size,
                                "modified": stat.st_mtime,
                            }
                        )
                    except OSError:
                        pass

        # Any image files that fileseq somehow didn't consume (shouldn't happen,
        # but be safe)
        for name, item in image_entries.items():
            if name not in consumed_names:
                try:
                    stat = item.stat()
                    files.append(
                        {
                            "path": str(item),
                            "name": item.name,
                            "type": "image",
                            "size": stat.st_size,
                            "modified": stat.st_mtime,
                        }
                    )
                except OSError:
                    pass

        # Add video files
        for item in video_entries:
            try:
                stat = item.stat()
                files.append(
                    {
                        "path": str(item),
                        "name": item.name,
                        "type": "video",
                        "size": stat.st_size,
                        "modified": stat.st_mtime,
                    }
                )
            except OSError:
                pass

        # Sort by name by default (maintaining original behaviour)
        files.sort(key=lambda x: x["name"].lower())
        return files

    def display_files(self):
        """Display files based on current view mode, filter, and sort."""
        # Start with all files
        self.filtered_files = self.files.copy()

        # Filter by type
        if self.filter_type != "all":
            self.filtered_files = [f for f in self.filtered_files if f["type"] == self.filter_type]

        # Sort files
        reverse = not self.sort_ascending
        if self.sort_by == "name":
            self.filtered_files.sort(key=lambda x: x["name"].lower(), reverse=reverse)
        elif self.sort_by == "size":
            self.filtered_files.sort(key=lambda x: x["size"], reverse=reverse)
        elif self.sort_by == "date":
            self.filtered_files.sort(key=lambda x: x["modified"], reverse=reverse)
        elif self.sort_by == "type":
            self.filtered_files.sort(key=lambda x: (x["type"], x["name"].lower()), reverse=reverse)

        # Filter files by search text
        if self.search_text:
            self.filtered_files = [
                f for f in self.filtered_files if self.search_text.lower() in f["name"].lower()
            ]

        if self.view_mode == "grid":
            self._display_grid()
        else:
            self._display_list()

    def _display_grid(self):
        """Display files in grid view."""
        # Switch to grid view (index 0)
        self.stacked_widget.setCurrentIndex(0)

        # Clear existing widgets
        self.file_widgets.clear()
        while self.grid_layout.count():
            item = self.grid_layout.takeAt(0)
            if item.widget():
                item.widget().deleteLater()

        # Calculate number of columns based on width
        self.update_columns()

        # Display files
        for i, file_info in enumerate(self.filtered_files):
            row = i // self.columns
            col = i % self.columns

            file_widget = FileItemWidget(file_info, self.thumbnail_size)
            file_widget.clicked.connect(self.on_file_clicked)
            file_widget.doubleClicked.connect(self.open_fullscreen)
            self.file_widgets[file_info["path"]] = file_widget
            self.grid_layout.addWidget(file_widget, row, col)

    def _display_list(self):
        """Display files in list view."""
        # Switch to list view (index 1)
        self.stacked_widget.setCurrentIndex(1)

        # Clear existing items
        self.list_widget.setRowCount(0)
        self.file_widgets.clear()

        # Display files
        for i, file_info in enumerate(self.filtered_files):
            self.list_widget.insertRow(i)

            # Name
            name_item = QTableWidgetItem(file_info["name"])
            name_item.setData(Qt.ItemDataRole.UserRole, file_info["path"])
            self.list_widget.setItem(i, 0, name_item)

            # Size
            size_item = QTableWidgetItem(self._format_size(file_info["size"]))
            self.list_widget.setItem(i, 1, size_item)

            # Date
            date_item = QTableWidgetItem(self._format_date(file_info["modified"]))
            self.list_widget.setItem(i, 2, date_item)

            # Type
            ftype = file_info["type"].upper()
            if file_info["type"] == "sequence":
                ftype = f"SEQ ({file_info.get('frame_count', '?')}f)"
            type_item = QTableWidgetItem(ftype)
            self.list_widget.setItem(i, 3, type_item)

    def _format_size(self, size_bytes):
        """Format file size."""
        for unit in ["B", "KB", "MB", "GB"]:
            if size_bytes < 1024.0:
                return f"{size_bytes:.1f} {unit}"
            size_bytes /= 1024.0
        return f"{size_bytes:.1f} TB"

    def _format_date(self, timestamp):
        """Format timestamp."""
        from datetime import datetime

        return datetime.fromtimestamp(timestamp).strftime("%Y-%m-%d %H:%M")

    def _on_list_item_clicked(self, item):
        """Handle list item click."""
        row = item.row() if hasattr(item, "row") else self.list_widget.currentRow()
        if row >= 0:
            name_item = self.list_widget.item(row, 0)
            file_path = name_item.data(Qt.ItemDataRole.UserRole)
            self.on_file_clicked(file_path)

    def _on_list_item_double_clicked(self, item):
        """Handle list item double click."""
        row = item.row()
        if row >= 0:
            name_item = self.list_widget.item(row, 0)
            file_path = name_item.data(Qt.ItemDataRole.UserRole)
            self.open_fullscreen(file_path)

    def _on_list_selection_changed(self):
        """Handle list selection change."""
        self.selected_files.clear()
        for item in self.list_widget.selectedItems():
            if item.column() == 0:  # Only process name column
                file_path = item.data(Qt.ItemDataRole.UserRole)
                if file_path:
                    self.selected_files.add(file_path)
        self.files_selected.emit(list(self.selected_files))

    def update_columns(self):
        """Update the number of columns based on available width."""
        available_width = self.grid_scroll.viewport().width() - 8
        item_width = self.thumbnail_size + 12  # Card width (size+8) + 4px grid spacing
        self.columns = max(1, available_width // item_width)

    def generate_thumbnails(self):
        """Start generating thumbnails for visible files."""
        for file_info in self.filtered_files:
            self.thumbnail_worker.add_to_queue(file_info["path"], self.thumbnail_size)

        if not self.thumbnail_worker.isRunning():
            self.thumbnail_worker.start()

    def on_thumbnail_ready(self, file_path: str, size: int, qimage: QImage):
        """Handle thumbnail generation completion.

        Converts QImage to QPixmap in the GUI thread (required by Qt).
        """
        if file_path in self.file_widgets:
            pixmap = QPixmap.fromImage(qimage)
            self.file_widgets[file_path].set_thumbnail(pixmap)

    def on_file_clicked(self, file_path: str, event=None):
        """Handle file click with support for multiselection."""
        modifiers = event.modifiers() if event else Qt.KeyboardModifier.NoModifier

        # Find index of clicked file in filtered_files (not self.files)
        try:
            clicked_index = next(
                i for i, f in enumerate(self.filtered_files) if f["path"] == file_path
            )
        except StopIteration:
            return

        if modifiers & Qt.KeyboardModifier.ControlModifier:
            # Ctrl+Click: Toggle selection
            if file_path in self.selected_files:
                self.selected_files.remove(file_path)
                if file_path in self.file_widgets:
                    self.file_widgets[file_path].set_selected(False)
            else:
                self.selected_files.add(file_path)
                if file_path in self.file_widgets:
                    self.file_widgets[file_path].set_selected(True)
            self.last_selected_index = clicked_index

        elif modifiers & Qt.KeyboardModifier.ShiftModifier and self.last_selected_index >= 0:
            # Shift+Click: Select range
            start_idx = min(self.last_selected_index, clicked_index)
            end_idx = max(self.last_selected_index, clicked_index)

            # Clear previous selection if not holding Ctrl
            if not (modifiers & Qt.KeyboardModifier.ControlModifier):
                for path in self.selected_files:
                    if path in self.file_widgets:
                        self.file_widgets[path].set_selected(False)
                self.selected_files.clear()

            # Select range from filtered_files
            for i in range(start_idx, end_idx + 1):
                path = self.filtered_files[i]["path"]
                self.selected_files.add(path)
                if path in self.file_widgets:
                    self.file_widgets[path].set_selected(True)

        else:
            # Normal click: Clear selection and select only clicked
            for path in self.selected_files:
                if path in self.file_widgets:
                    self.file_widgets[path].set_selected(False)
            self.selected_files.clear()

            self.selected_files.add(file_path)
            if file_path in self.file_widgets:
                self.file_widgets[file_path].set_selected(True)
            self.last_selected_index = clicked_index
            self.file_selected.emit(file_path)

        # Emit list of all selected files
        self.files_selected.emit(list(self.selected_files))

    def set_view_mode(self, mode: str):
        """Set view mode ("grid" or "list")."""
        if mode != self.view_mode and mode in ("grid", "list"):
            self.view_mode = mode
            self.display_files()

            # Regenerate thumbnails when switching to grid view
            if mode == "grid":
                self.generate_thumbnails()

    def set_search_text(self, text: str):
        """Set search filter text."""
        self.search_text = text
        self.display_files()

    def set_thumbnail_size(self, size: int):
        """Set the thumbnail size and redisplay at the new size.

        Unlike the old implementation, this does NOT rescan the filesystem.
        It only rebuilds the grid widgets at the new size and regenerates
        thumbnails (which will hit cache for sizes already generated).
        """
        if size == self.thumbnail_size:
            return
        self.thumbnail_size = size
        if self.current_folder and self.files:
            # Stop any running thumbnail worker
            if self.thumbnail_worker.isRunning():
                self.thumbnail_worker.stop()
            self.thumbnail_worker.clear_queue()

            # Redisplay existing files at new size (no rescan)
            self.display_files()
            # Regenerate thumbnails at new size
            self.generate_thumbnails()

    def resizeEvent(self, event):
        """Handle resize to update grid layout."""
        super().resizeEvent(event)

        if self.view_mode != "grid":
            return

        old_columns = self.columns
        self.update_columns()

        # Only redraw if column count actually changed
        if self.files and self.columns != old_columns:
            self.display_files()

        # Update visible items after resize
        self._update_visible_items()

    def _update_visible_items(self):
        """Update which items are visible and queue thumbnails for them."""
        if self.view_mode != "grid":
            return

        # Get visible viewport rectangle
        viewport = self.grid_scroll.viewport()
        visible_rect = viewport.rect()

        # Find visible items
        newly_visible = set()
        for file_path, widget in self.file_widgets.items():
            # Map widget position to viewport coordinates
            widget_rect = widget.geometry()
            # Check if widget intersects with visible area
            if visible_rect.intersects(widget_rect):
                newly_visible.add(file_path)

        # Queue thumbnails for newly visible items
        for file_path in newly_visible - self._visible_items:
            # Find file info
            for file_info in self.filtered_files:
                if file_info["path"] == file_path:
                    self.thumbnail_worker.add_to_queue(
                        file_path, self.thumbnail_size, priority=True
                    )
                    break

        self._visible_items = newly_visible

        # Start worker if not running
        if not self.thumbnail_worker.isRunning():
            self.thumbnail_worker.start()

    def keyPressEvent(self, event):
        """Handle keyboard shortcuts for selection and navigation."""
        key = event.key()
        modifiers = event.modifiers()

        # Ctrl+A: Select all
        if key == Qt.Key.Key_A and modifiers & Qt.KeyboardModifier.ControlModifier:
            self.select_all()
            return

        # Escape: Clear selection
        if key == Qt.Key.Key_Escape:
            self.clear_selection()
            return

        # Enter or Space: Open selected file in media viewer
        if key in (Qt.Key.Key_Return, Qt.Key.Key_Enter, Qt.Key.Key_Space):
            selected = self.get_selected_files()
            if len(selected) == 1:
                self.open_viewer(selected[0])
            return

        # Backspace: Go to parent folder
        if key == Qt.Key.Key_Backspace:
            from pathlib import Path

            if self.current_folder:
                parent = Path(self.current_folder).parent
                if parent.exists() and str(parent) != self.current_folder:
                    # Emit signal to parent to navigate
                    self.file_selected.emit(str(parent))
            return

        # Arrow keys: Navigate selection
        if key in (Qt.Key.Key_Up, Qt.Key.Key_Down, Qt.Key.Key_Left, Qt.Key.Key_Right):
            self._navigate_with_arrows(key, modifiers)
            return

        super().keyPressEvent(event)

    def _navigate_with_arrows(self, key, modifiers):
        """Handle arrow key navigation."""
        if not self.filtered_files:
            return

        # Get current selection index
        current_index = -1
        if self.selected_files:
            # Find the first selected file in filtered_files
            for i, f in enumerate(self.filtered_files):
                if f["path"] in self.selected_files:
                    current_index = i
                    break

        # Calculate new index based on key and view mode
        new_index = current_index

        if self.view_mode == "grid":
            # Grid navigation
            if key == Qt.Key.Key_Right:
                new_index = min(current_index + 1, len(self.filtered_files) - 1)
            elif key == Qt.Key.Key_Left:
                new_index = max(current_index - 1, 0)
            elif key == Qt.Key.Key_Down:
                new_index = min(current_index + self.columns, len(self.filtered_files) - 1)
            elif key == Qt.Key.Key_Up:
                new_index = max(current_index - self.columns, 0)
        else:
            # List navigation
            if key == Qt.Key.Key_Down:
                new_index = min(current_index + 1, len(self.filtered_files) - 1)
            elif key == Qt.Key.Key_Up:
                new_index = max(current_index - 1, 0)

        # Update selection
        if new_index != current_index and 0 <= new_index < len(self.filtered_files):
            if not (modifiers & Qt.KeyboardModifier.ShiftModifier):
                # Clear previous selection unless shift is held
                self.clear_selection()

            file_path = self.filtered_files[new_index]["path"]
            self.selected_files.add(file_path)

            # Update widget selection for grid view
            if self.view_mode == "grid" and file_path in self.file_widgets:
                self.file_widgets[file_path].set_selected(True)

            # Update list widget selection for list view
            if self.view_mode == "list":
                self.list_widget.selectRow(new_index)

            self.last_selected_index = new_index
            self.file_selected.emit(file_path)
            self.files_selected.emit(list(self.selected_files))

    def select_all(self):
        """Select all files."""
        if self.view_mode == "grid":
            for path, widget in self.file_widgets.items():
                self.selected_files.add(path)
                widget.set_selected(True)
        else:
            # List view - select all rows
            self.list_widget.selectAll()
            for item in self.list_widget.selectedItems():
                if item.column() == 0:
                    file_path = item.data(Qt.ItemDataRole.UserRole)
                    if file_path:
                        self.selected_files.add(file_path)
        self.files_selected.emit(list(self.selected_files))

    def clear_selection(self):
        """Clear all selections."""
        for path in self.selected_files:
            if path in self.file_widgets:
                self.file_widgets[path].set_selected(False)
        self.selected_files.clear()
        self.last_selected_index = -1
        self.files_selected.emit([])

    def get_selected_files(self) -> list:
        """Get list of selected file paths."""
        return list(self.selected_files)

    def set_selection(self, file_paths: list):
        """Set selection to specific files."""
        # Clear current
        self.clear_selection()
        # Select new
        for path in file_paths:
            if path in self.file_widgets:
                self.selected_files.add(path)
                self.file_widgets[path].set_selected(True)
        self.files_selected.emit(list(self.selected_files))

    def set_sort_by(self, sort_by: str):
        """Set sort criteria (name, size, date, type)."""
        if sort_by in ("name", "size", "date", "type"):
            self.sort_by = sort_by
            self.display_files()

    def set_sort_order(self, ascending: bool):
        """Set sort order (True for ascending, False for descending)."""
        self.sort_ascending = ascending
        self.display_files()

    def set_filter_type(self, filter_type: str):
        """Set filter by type (all, image, video, sequence)."""
        if filter_type in ("all", "image", "video", "sequence"):
            self.filter_type = filter_type
            self.display_files()

    def open_viewer(self, file_path: str):
        """Open the media viewer for a file, with navigation through the
        current filtered file list.

        Sequences are passed as single entries (type="sequence") so the
        MediaViewer can use _SequenceViewer for playback.  Non-sequence
        entries are passed as-is.
        """
        from gallerybrowser.views.fullscreen_preview import MediaViewer

        if not self.filtered_files:
            return

        # Build the viewer file list — sequences stay as single entries
        viewer_files: list[dict] = list(self.filtered_files)

        # Find the index of the requested file in the viewer list
        start_index = 0
        for i, f in enumerate(viewer_files):
            if f["path"] == file_path:
                start_index = i
                break
            # Also check if file_path is inside a sequence's frame list
            if f.get("type") == "sequence" and file_path in f.get("sequence_files", []):
                start_index = i
                break

        viewer = MediaViewer(viewer_files, start_index=start_index, parent=self.window())
        viewer.selection_changed.connect(self._on_viewer_navigated)
        viewer.exec()

    def _on_viewer_navigated(self, file_path: str):
        """Handle the media viewer navigating to a different file.

        Syncs the file-pane selection to the file the viewer is showing.
        If the file is a frame within a collapsed sequence, select the
        sequence entry instead.
        """
        # Clear current selection
        for path in list(self.selected_files):
            if path in self.file_widgets:
                self.file_widgets[path].set_selected(False)
        self.selected_files.clear()

        # Check if file_path belongs to a collapsed sequence
        select_path = file_path
        if file_path not in self.file_widgets:
            for f in self.filtered_files:
                if f["type"] == "sequence" and file_path in f.get("sequence_files", []):
                    select_path = f["path"]  # Use the sequence's representative path
                    break

        # Select the file (or its parent sequence)
        self.selected_files.add(select_path)
        if select_path in self.file_widgets:
            self.file_widgets[select_path].set_selected(True)

            # Scroll to make it visible in grid view
            if self.view_mode == "grid":
                self.grid_scroll.ensureWidgetVisible(self.file_widgets[select_path])

        # Update last_selected_index
        for i, f in enumerate(self.filtered_files):
            if f["path"] == select_path:
                self.last_selected_index = i
                break

        # Sync list widget selection
        if self.view_mode == "list":
            for i in range(self.list_widget.rowCount()):
                item = self.list_widget.item(i, 0)
                if item and item.data(Qt.ItemDataRole.UserRole) == select_path:
                    self.list_widget.selectRow(i)
                    break

        self.file_selected.emit(select_path)
        self.files_selected.emit(list(self.selected_files))

    # Keep old name as alias
    def open_fullscreen(self, file_path: str):
        """Alias for open_viewer (backward compatibility)."""
        self.open_viewer(file_path)

    def cleanup(self):
        """Clean up resources."""
        self.thumbnail_worker.stop()
