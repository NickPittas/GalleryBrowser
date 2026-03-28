"""Debug script for tree navigation and video issues."""

import sys
import logging

sys.path.insert(0, "/home/npittas/GalleryBrowser/src")

logging.basicConfig(
    level=logging.DEBUG, format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
)
logger = logging.getLogger(__name__)

from PyQt6.QtWidgets import QApplication

app = QApplication([])

print("=" * 60)
print("DEBUGGING TREE PANE")
print("=" * 60)

from gallerybrowser.views.tree_pane import TreePane
from pathlib import Path

tree_pane = TreePane()
tree_pane.show()
tree_pane.resize(250, 500)

# Check tree configuration
print(f"\nTree Configuration:")
print(f"  Root is decorated: {tree_pane.tree.rootIsDecorated()}")
print(f"  Top level items: {tree_pane.tree.topLevelItemCount()}")
print(f"  Indentation: {tree_pane.tree.indentation()}")

# Check each top level item
for i in range(tree_pane.tree.topLevelItemCount()):
    item = tree_pane.tree.topLevelItem(i)
    print(f"\n  Item {i}: {item.text(0)}")
    print(f"    Child count: {item.childCount()}")
    print(f"    Is expanded: {item.isExpanded()}")
    print(f"    Data: {item.data(0, 256)}")

print("\n" + "=" * 60)
print("DEBUGGING VIDEO PREVIEW")
print("=" * 60)

from gallerybrowser.views.preview_pane import VideoPreviewWidget, has_gstreamer

print(f"\nGStreamer available: {has_gstreamer()}")

video = VideoPreviewWidget()
video.show()
video.resize(400, 300)

print(f"Video widget created")
print(f"Has GStreamer: {video._has_gstreamer}")

# Try to load a test video
test_video = "/tmp/gallerybrowser_test.mp4"
import subprocess

if not Path(test_video).exists():
    print(f"\nCreating test video...")
    subprocess.run(
        [
            "ffmpeg",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=5:size=320x240:rate=30",
            "-pix_fmt",
            "yuv420p",
            test_video,
            "-y",
        ],
        capture_output=True,
    )

if Path(test_video).exists():
    print(f"\nLoading test video: {test_video}")
    success = video.set_video(test_video)
    print(f"set_video returned: {success}")
    print(f"Duration: {video._duration}")

print("\n" + "=" * 60)
print("Running diagnostic window for 5 seconds...")
print("=" * 60)

from PyQt6.QtCore import QTimer

QTimer.singleShot(5000, app.quit)

# Layout
from PyQt6.QtWidgets import QWidget, QHBoxLayout

window = QWidget()
layout = QHBoxLayout(window)
layout.addWidget(tree_pane)
layout.addWidget(video)
window.show()
window.resize(800, 500)

app.exec()
print("\nDone!")
