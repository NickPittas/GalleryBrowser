"""Diagnostic script for GalleryBrowser UI issues."""

import sys
from pathlib import Path

sys.path.insert(0, "/home/npittas/GalleryBrowser/src")

from PyQt6.QtWidgets import QApplication, QWidget, QVBoxLayout, QHBoxLayout
from PyQt6.QtCore import Qt

app = QApplication([])

print("=== Testing Tree Pane ===")
from gallerybrowser.views.tree_pane import TreePane

tree = TreePane()
tree.show()
tree.resize(200, 400)

print(f"Tree has {tree.tree.topLevelItemCount()} top level items")
print(f"Tree root is decorated: {tree.tree.rootIsDecorated()}")

print("\n=== Testing File Pane ===")
from gallerybrowser.views.file_pane import FilePane, FileItemWidget

# Create a test file widget
file_info = {"path": "/test/image.png", "name": "test.png", "type": "image"}
item = FileItemWidget(file_info, 128)

print(f"File widget created")
print(f"Widget size: {item.size()}")

# Test selection
item.set_selected(True)
print(f"Selection style applied")
print(f"Style sheet: {item.styleSheet()}")

item.show()
item.resize(200, 200)

print("\n=== Testing Video Preview ===")
from gallerybrowser.views.preview_pane import VideoPreviewWidget

video = VideoPreviewWidget()
print(f"Video widget created")
print(f"Has GStreamer: {video._has_gstreamer}")
print(f"Pipeline exists: {video._pipeline is not None}")

video.show()
video.resize(400, 300)

print("\n=== Running diagnostic window ===")
window = QWidget()
layout = QHBoxLayout(window)
layout.addWidget(tree)
layout.addWidget(item)
layout.addWidget(video)

window.show()
window.resize(800, 400)

from PyQt6.QtCore import QTimer

QTimer.singleShot(3000, app.quit)

app.exec()
print("\nDone!")
