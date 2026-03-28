"""Debug video viewer."""

import sys

sys.path.insert(0, "/home/npittas/GalleryBrowser/src")

from PyQt6.QtWidgets import QApplication

app = QApplication([])

from gallerybrowser.config import Config
from gallerybrowser.views.preview_pane import VideoPreviewWidget, has_gstreamer

# Check if GStreamer is available
print(f"GStreamer available: {has_gstreamer()}")

# Create video widget
video_widget = VideoPreviewWidget()

# Test with a video
test_video = "/tmp/gallerybrowser_test.mp4"

import os

if not os.path.exists(test_video):
    print(f"Test video not found: {test_video}")
    print("Creating test video...")
    import subprocess

    cmd = [
        "ffmpeg",
        "-f",
        "lavfi",
        "-i",
        "testsrc=duration=5:size=320x240:rate=30",
        "-pix_fmt",
        "yuv420p",
        test_video,
        "-y",
    ]
    subprocess.run(cmd, capture_output=True)

print(f"\nTesting with video: {test_video}")
print(f"File exists: {os.path.exists(test_video)}")

# Try to load video
success = video_widget.set_video(test_video)
print(f"set_video returned: {success}")

# Check if GStreamer initialization worked
print(f"Has GStreamer: {video_widget._has_gstreamer}")
print(f"Pipeline exists: {video_widget._pipeline is not None}")

# Show the widget
video_widget.show()
video_widget.resize(400, 300)

# Run app briefly
from PyQt6.QtCore import QTimer

timer = QTimer()
timer.singleShot(2000, app.quit)

app.exec()
