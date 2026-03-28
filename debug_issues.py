"""Debug script for GalleryBrowser issues."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent / "src"))

from gallerybrowser.config import Config
from gallerybrowser.core.thumbnail import generate_thumbnail

# Test 1: Check if video thumbnail generation works
print("Testing video thumbnail generation...")
test_video = "/home/npittas/Videos/test.mp4"  # Adjust path as needed
if Path(test_video).exists():
    pixmap = generate_thumbnail(test_video, 128)
    if pixmap:
        print(f"✓ Video thumbnail generated: {pixmap.width()}x{pixmap.height()}")
    else:
        print("✗ Video thumbnail generation returned None")
else:
    print(f"⚠ Test video not found: {test_video}")

# Test 2: Check image thumbnail
print("\nTesting image thumbnail generation...")
test_image = "/home/npittas/Pictures/test.png"  # Adjust path as needed
if Path(test_image).exists():
    pixmap = generate_thumbnail(test_image, 128)
    if pixmap:
        print(f"✓ Image thumbnail generated: {pixmap.width()}x{pixmap.height()}")
    else:
        print("✗ Image thumbnail generation returned None")
else:
    print(f"⚠ Test image not found: {test_image}")

# Test 3: Check GStreamer availability
print("\nChecking GStreamer...")
try:
    import gi

    gi.require_version("Gst", "1.0")
    from gi.repository import Gst

    Gst.init(None)
    print("✓ GStreamer is available")
except ImportError as e:
    print(f"✗ GStreamer not available: {e}")
except Exception as e:
    print(f"✗ GStreamer error: {e}")

# Test 4: Check FFmpeg
print("\nChecking FFmpeg...")
import subprocess

result = subprocess.run(["which", "ffmpeg"], capture_output=True, text=True)
if result.returncode == 0:
    print(f"✓ FFmpeg found: {result.stdout.strip()}")
    # Test version
    version_result = subprocess.run(["ffmpeg", "-version"], capture_output=True, text=True)
    first_line = version_result.stdout.split("\n")[0]
    print(f"  {first_line}")
else:
    print("✗ FFmpeg not found in PATH")

print("\nDone!")
