"""Offscreen Ctrl-hover scrub smoke test (no desktop input required)."""

import os
import subprocess
import tempfile
import unittest
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

import imageio_ffmpeg
from PyQt6.QtCore import QEvent, QPointF, QTimer, Qt
from PyQt6.QtGui import QColor, QEnterEvent, QMouseEvent
from PyQt6.QtWidgets import QApplication

from gallerybrowser.views.file_pane import FileItemWidget


class CtrlScrubTest(unittest.TestCase):
    def test_ctrl_hover_decodes_and_displays_frame(self):
        app = QApplication.instance() or QApplication([])
        with tempfile.TemporaryDirectory() as directory:
            video = Path(directory) / "clip.mp4"
            subprocess.run(
                [imageio_ffmpeg.get_ffmpeg_exe(), "-hide_banner", "-loglevel", "error",
                 "-f", "lavfi", "-i", "testsrc2=s=64x64:r=12", "-frames:v", "24",
                 "-c:v", "libx264", "-pix_fmt", "yuv420p", "-y", str(video)],
                check=True, timeout=20,
            )
            item = FileItemWidget({"type": "video", "name": video.name, "path": str(video)})
            item.show()
            app.processEvents()
            pos = item.thumb_label.rect().center()
            QApplication.sendEvent(item, QEnterEvent(QPointF(pos), QPointF(pos), QPointF(pos)))
            QApplication.sendEvent(item.thumb_label, QMouseEvent(
                QEvent.Type.MouseMove, QPointF(pos), QPointF(pos),
                Qt.MouseButton.NoButton, Qt.MouseButton.NoButton,
                Qt.KeyboardModifier.ControlModifier,
            ))
            worker = FileItemWidget._gst_worker
            self.assertIsNotNone(worker)
            frames = []
            worker.frame_ready.connect(lambda image, ratio: (frames.append(ratio), app.quit()))
            timeout = QTimer()
            timeout.setSingleShot(True)
            timeout.timeout.connect(app.quit)
            timeout.start(12000)
            try:
                app.exec()
                self.assertTrue(frames, "Ctrl-hover did not decode a video frame")
                pixmap = item.thumb_label.pixmap().toImage()
                self.assertEqual(pixmap.pixelColor(pixmap.width() // 4, pixmap.height() - 2), QColor("#4CAF50"))
            finally:
                timeout.stop()
                item._stop_ctrl_scrub()
                item.close()


if __name__ == "__main__":
    unittest.main()
