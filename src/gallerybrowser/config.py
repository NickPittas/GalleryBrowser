"""Configuration and paths for GalleryBrowser."""

import json
import os
from pathlib import Path
from typing import Optional

from appdirs import user_cache_dir, user_config_dir


def _check_oiio():
    """Check for OpenImageIO availability. Required dependency."""
    try:
        import OpenImageIO  # noqa: F401

        return True
    except ImportError:
        return False


HAS_OIIO = _check_oiio()


class Config:
    """Application configuration."""

    # App info
    APP_NAME = "gallerybrowser"
    APP_AUTHOR = "NickPittas"
    VERSION = "0.1.0"

    # Thumbnail sizes
    THUMBNAIL_SIZES = [64, 128, 256, 512]
    DEFAULT_THUMBNAIL_SIZE = 128

    # Supported formats
    IMAGE_EXTENSIONS = {
        ".png",
        ".jpg",
        ".jpeg",
        ".tiff",
        ".tif",
        ".bmp",
        ".gif",
        ".webp",
        ".exr",
        ".tga",
        ".psd",
    }

    VIDEO_EXTENSIONS = {
        ".mp4",
        ".mov",
        ".avi",
        ".mkv",
        ".mxf",
        ".webm",
        ".mpg",
        ".mpeg",
    }

    SEQUENCE_EXTENSIONS = IMAGE_EXTENSIONS  # Sequences use image formats

    # Minimum number of files to consider a group a "sequence"
    SEQUENCE_MIN_FRAMES = 2

    @classmethod
    def is_sequence_extension(cls, path: str) -> bool:
        """Check if a file extension is valid for image sequences."""
        return Path(path).suffix.lower() in cls.SEQUENCE_EXTENSIONS

    @classmethod
    def get_cache_dir(cls) -> Path:
        """Get the cache directory."""
        cache_dir = Path(user_cache_dir(cls.APP_NAME, cls.APP_AUTHOR))
        cache_dir.mkdir(parents=True, exist_ok=True)
        return cache_dir

    @classmethod
    def get_config_dir(cls) -> Path:
        """Get the config directory."""
        config_dir = Path(user_config_dir(cls.APP_NAME, cls.APP_AUTHOR))
        config_dir.mkdir(parents=True, exist_ok=True)
        return config_dir

    @classmethod
    def get_thumbnails_dir(cls, size: int) -> Path:
        """Get the thumbnails directory for a specific size."""
        thumbs_dir = cls.get_cache_dir() / "thumbnails" / str(size)
        thumbs_dir.mkdir(parents=True, exist_ok=True)
        return thumbs_dir

    @classmethod
    def get_previews_dir(cls) -> Path:
        """Get the previews directory."""
        previews_dir = cls.get_cache_dir() / "previews"
        previews_dir.mkdir(parents=True, exist_ok=True)
        return previews_dir

    @classmethod
    def get_scrub_frames_dir(cls, size: int) -> Path:
        """Get the scrub frames directory for a specific thumbnail size."""
        scrub_dir = cls.get_cache_dir() / "scrub_frames" / str(size)
        scrub_dir.mkdir(parents=True, exist_ok=True)
        return scrub_dir

    @classmethod
    def get_database_path(cls) -> Path:
        """Get the database file path."""
        return cls.get_cache_dir() / "database.sqlite"

    @classmethod
    def get_settings_path(cls) -> Path:
        """Get the settings file path."""
        return cls.get_config_dir() / "settings.json"

    @classmethod
    def is_image_file(cls, path: str) -> bool:
        """Check if a file is an image."""
        return Path(path).suffix.lower() in cls.IMAGE_EXTENSIONS

    @classmethod
    def is_video_file(cls, path: str) -> bool:
        """Check if a file is a video."""
        return Path(path).suffix.lower() in cls.VIDEO_EXTENSIONS

    @classmethod
    def get_file_type(cls, path: str) -> str:
        """Get the file type (image, video, or unknown)."""
        if cls.is_image_file(path):
            return "image"
        elif cls.is_video_file(path):
            return "video"
        return "unknown"

    # ------------------------------------------------------------------
    # Settings persistence
    # ------------------------------------------------------------------

    # Default settings values
    DEFAULTS = {
        "thumbnail_size": 128,
        "show_hidden": False,
        "video_preview": True,
        "thumbnail_threads": 2,
        "max_cache_mb": 1000,
        "ram_preview_mb": 2048,
        "favorites": [],  # List of folder paths
        "recents": [],  # List of recently opened folder paths (newest first)
        "recents_max": 15,  # Maximum number of recent entries to keep
    }

    @classmethod
    def load_settings(cls) -> dict:
        """Load settings from disk, returning a dict with defaults for missing keys."""
        settings = dict(cls.DEFAULTS)
        path = cls.get_settings_path()
        if path.exists():
            try:
                with open(path, "r") as f:
                    saved = json.load(f)
                settings.update(saved)
            except (json.JSONDecodeError, OSError):
                pass
        return settings

    @classmethod
    def save_settings(cls, settings: dict) -> None:
        """Save settings dict to disk as JSON."""
        path = cls.get_settings_path()
        path.parent.mkdir(parents=True, exist_ok=True)
        with open(path, "w") as f:
            json.dump(settings, f, indent=2)
