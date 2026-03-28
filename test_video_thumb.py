"""Debug script for video thumbnails."""

import sys
import subprocess
from pathlib import Path

# Test video path - try to find a test video
test_paths = [
    "/home/npittas/Videos/test.mp4",
    "/home/npittas/test.mp4",
    "/tmp/test.mp4",
]

video_path = None
for path in test_paths:
    if Path(path).exists():
        video_path = path
        break

if not video_path:
    # Create a simple test video
    print("Creating test video...")
    test_video = "/tmp/gallerybrowser_test.mp4"
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
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode == 0:
        video_path = test_video
        print(f"Created test video: {video_path}")
    else:
        print(f"Failed to create test video: {result.stderr}")
        sys.exit(1)

print(f"\nTesting thumbnail generation for: {video_path}")

# Test FFmpeg directly
cache_path = "/tmp/test_video_thumb.png"
size = 128

cmd = [
    "ffmpeg",
    "-i",
    video_path,
    "-ss",
    "00:00:01.000",
    "-vframes",
    "1",
    "-vf",
    f"scale={size}:{size}:force_original_aspect_ratio=decrease",
    "-y",
    cache_path,
]

print(f"\nRunning FFmpeg command:")
print(" ".join(cmd))

result = subprocess.run(cmd, capture_output=True, text=True)

print(f"\nReturn code: {result.returncode}")

if result.returncode == 0:
    if Path(cache_path).exists():
        print(f"✓ Thumbnail created successfully: {cache_path}")
        file_size = Path(cache_path).stat().st_size
        print(f"  File size: {file_size} bytes")
    else:
        print(f"✗ Thumbnail file not found at: {cache_path}")
else:
    print(f"✗ FFmpeg failed")
    print(f"STDERR: {result.stderr}")

# Test using our module
print("\n\nTesting via gallerybrowser module:")
sys.path.insert(0, "/home/npittas/GalleryBrowser/src")

from PyQt6.QtWidgets import QApplication

app = QApplication([])

from gallerybrowser.core.thumbnail import generate_video_thumbnail

pixmap = generate_video_thumbnail(video_path, 128, "/tmp/test_module_thumb.png")

if pixmap:
    print(f"✓ Generated via module: {pixmap.width()}x{pixmap.height()}")
else:
    print("✗ Failed via module")
