"""Thumbnail generation and caching."""

import hashlib
import json
from pathlib import Path
from typing import Optional

import numpy as np
from PIL import Image
from PyQt6.QtCore import Qt
from PyQt6.QtGui import QImage, QPixmap

from gallerybrowser.config import Config, HAS_OIIO


def generate_image_thumbnail(image_path: str, size: int, cache_path: str) -> Optional[QImage]:
    """Generate a thumbnail from an image file.

    Args:
        image_path: Path to the source image
        size: Desired thumbnail size (width and height)
        cache_path: Path to save the thumbnail

    Returns:
        QImage of the thumbnail, or None if generation failed
    """
    try:
        # Open image with Pillow
        with Image.open(image_path) as img:
            # Convert to RGB if necessary (for CMYK, RGBA, etc.)
            if img.mode in ("RGBA", "LA", "P"):
                # Keep transparency for PNG
                if image_path.lower().endswith(".png"):
                    img = img.convert("RGBA")
                else:
                    # Create white background for others
                    background = Image.new("RGB", img.size, (255, 255, 255))
                    if img.mode == "P":
                        img = img.convert("RGBA")
                    if img.mode in ("RGBA", "LA"):
                        background.paste(
                            img, mask=img.split()[-1] if len(img.split()) > 1 else None
                        )
                        img = background
                    else:
                        img = img.convert("RGB")
            elif img.mode != "RGB":
                img = img.convert("RGB")

            # Calculate thumbnail size maintaining aspect ratio
            img.thumbnail((size, size), Image.Resampling.LANCZOS)

            # Save to cache
            cache_file = Path(cache_path)
            cache_file.parent.mkdir(parents=True, exist_ok=True)
            img.save(cache_path, "PNG", optimize=True)

            # Convert to QImage (thread-safe, unlike QPixmap)
            return pil_to_qimage(img)

    except Exception:
        # Silently return None — the caller will try OIIO as a fallback
        # for formats PIL can't handle (EXR, etc.)
        return None


def generate_oiio_thumbnail(image_path: str, size: int, cache_path: str) -> Optional[QImage]:
    """Generate a thumbnail using OpenImageIO with NAS-buffered I/O.

    Args:
        image_path: Path to the source image (typically EXR)
        size: Desired thumbnail size (width and height)
        cache_path: Path to save the thumbnail

    Returns:
        QImage of the thumbnail, or None if generation failed or OIIO unavailable
    """
    if not HAS_OIIO:
        return None
    try:
        from gallerybrowser.core.image_io import load_oiio_array

        result = load_oiio_array(image_path)
        if result is None:
            return None

        rgb_uint8, w, h = result

        # Convert to PIL for thumbnail generation + cache save
        pil_img = Image.fromarray(rgb_uint8, "RGB")
        pil_img.thumbnail((size, size), Image.Resampling.LANCZOS)

        # Save to cache
        cache_file = Path(cache_path)
        cache_file.parent.mkdir(parents=True, exist_ok=True)
        pil_img.save(cache_path, "PNG", optimize=True)

        return pil_to_qimage(pil_img)

        return pil_to_qimage(pil_img)

    except Exception as e:
        print(f"Error generating OIIO thumbnail for {image_path}: {e}")
        return None


def generate_video_thumbnail(video_path: str, size: int, cache_path: str) -> Optional[QImage]:
    """Generate a thumbnail from a video file using FFmpeg.

    Seeks to the middle of the video (via ffprobe duration query) to avoid
    black leader frames.  Falls back to 0.0s for very short videos.

    Args:
        video_path: Path to the source video
        size: Desired thumbnail size (width and height)
        cache_path: Path to save the thumbnail

    Returns:
        QImage of the thumbnail, or None if generation failed
    """
    try:
        import subprocess

        # Get video duration to seek to middle frame
        duration = _get_video_duration(video_path)
        if duration and duration > 0:
            seek_time = duration / 2.0
        else:
            seek_time = 0.0

        seek_str = f"{seek_time:.3f}"

        # Use FFmpeg to extract frame at the calculated position
        # Place -ss before -i for fast keyframe seeking
        cmd = [
            "ffmpeg",
            "-ss",
            seek_str,
            "-i",
            video_path,
            "-vframes",
            "1",
            "-vf",
            f"scale={size}:{size}:force_original_aspect_ratio=decrease",
            "-y",  # Overwrite output
            cache_path,
        ]

        result = subprocess.run(
            cmd, capture_output=True, encoding="utf-8", errors="replace", timeout=30
        )

        if result.returncode == 0 and Path(cache_path).exists():
            # Load the generated thumbnail as QImage (thread-safe)
            qimage = QImage(cache_path)
            if not qimage.isNull():
                return qimage.scaled(
                    size,
                    size,
                    Qt.AspectRatioMode.KeepAspectRatio,
                    Qt.TransformationMode.SmoothTransformation,
                )

        return None

    except Exception as e:
        print(f"Error generating video thumbnail for {video_path}: {e}")
        return None


def pil_to_qimage(image: Image.Image) -> QImage:
    """Convert PIL Image to QImage (thread-safe).

    Args:
        image: PIL Image object

    Returns:
        QImage
    """
    # Convert to RGBA if necessary
    if image.mode != "RGBA":
        image = image.convert("RGBA")

    # Get image data
    data = image.tobytes("raw", "RGBA")

    # Create QImage from data
    qimage = QImage(data, image.width, image.height, QImage.Format.Format_RGBA8888)
    # Pin the data buffer to the QImage to prevent GC from freeing it
    # QImage does NOT copy the data buffer, so we must keep it alive
    qimage._data = data

    # Return a deep copy so the QImage owns its own data independently
    return qimage.copy()


def pil_to_pixmap(image: Image.Image) -> QPixmap:
    """Convert PIL Image to QPixmap.

    Note: Only safe to call from the GUI thread. For worker threads, use pil_to_qimage().

    Args:
        image: PIL Image object

    Returns:
        QPixmap
    """
    return QPixmap.fromImage(pil_to_qimage(image))


def get_thumbnail_path(file_path: str, size: int) -> str:
    """Get the cache path for a thumbnail.

    Args:
        file_path: Original file path
        size: Thumbnail size

    Returns:
        Cache file path
    """
    # Create hash of file path + mtime + size for cache invalidation
    try:
        stat = Path(file_path).stat()
        key = f"{file_path}:{stat.st_mtime}:{stat.st_size}"
    except OSError:
        key = file_path
    file_hash = hashlib.md5(key.encode()).hexdigest()

    # Get cache directory for this size
    cache_dir = Config.get_thumbnails_dir(size)

    return str(cache_dir / f"{file_hash}.png")


def generate_thumbnail(file_path: str, size: int) -> Optional[QImage]:
    """Generate a thumbnail for any supported file type.

    Returns QImage instead of QPixmap so it can be safely called from worker threads.

    Args:
        file_path: Path to the file
        size: Desired thumbnail size

    Returns:
        QImage of the thumbnail, or None if generation failed
    """
    # Check if thumbnail already exists in cache
    cache_path = get_thumbnail_path(file_path, size)

    if Path(cache_path).exists():
        # Load from cache as QImage (thread-safe)
        qimage = QImage(cache_path)
        if not qimage.isNull():
            return qimage

    # Generate new thumbnail
    file_type = Config.get_file_type(file_path)

    if file_type == "image":
        # Try PIL first; fall back to OIIO for formats PIL can't handle (EXR etc.)
        result = generate_image_thumbnail(file_path, size, cache_path)
        if result is None:
            result = generate_oiio_thumbnail(file_path, size, cache_path)
        return result
    elif file_type == "video":
        return generate_video_thumbnail(file_path, size, cache_path)

    return None


def get_placeholder_icon(file_type: str) -> str:
    """Get a placeholder icon for a file type.

    Args:
        file_type: Type of file ('image', 'video', or 'unknown')

    Returns:
        Font Awesome icon name
    """
    if file_type == "image":
        return "fa5s.image"
    elif file_type == "video":
        return "fa5s.video"
    elif file_type == "sequence":
        return "fa5s.images"
    return "fa5s.file"


def _get_video_duration(video_path: str) -> Optional[float]:
    """Get video duration in seconds using ffprobe.

    Returns:
        Duration in seconds, or None on failure.
    """
    try:
        import subprocess

        cmd = [
            "ffprobe",
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            video_path,
        ]
        result = subprocess.run(
            cmd, capture_output=True, encoding="utf-8", errors="replace", timeout=15
        )
        if result.returncode == 0:
            info = json.loads(result.stdout)
            duration = info.get("format", {}).get("duration")
            if duration:
                return float(duration)
    except Exception:
        pass
    return None
