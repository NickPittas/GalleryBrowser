"""Metadata extraction for various file types."""

import json
from pathlib import Path
from typing import Any, Dict, Optional

from PIL import Image
from PIL.ExifTags import TAGS


class MetadataExtractor:
    """Extracts metadata from image and video files."""

    @staticmethod
    def _parse_frame_rate(value: Optional[str]) -> Optional[float]:
        """Parse an ffprobe frame-rate field into a float."""
        if not value:
            return None

        try:
            if "/" in value:
                num_text, den_text = value.split("/", 1)
                numerator = float(num_text)
                denominator = float(den_text)
                if denominator == 0:
                    return None
                fps = numerator / denominator
            else:
                fps = float(value)
        except (TypeError, ValueError, ZeroDivisionError):
            return None

        return fps if fps > 0 else None

    @staticmethod
    def extract_image_metadata(file_path: str) -> Dict[str, Any]:
        """Extract metadata from an image file.

        Args:
            file_path: Path to the image file

        Returns:
            Dictionary containing metadata
        """
        metadata = {
            "format": None,
            "width": None,
            "height": None,
            "mode": None,
            "exif": {},
            "file_size": None,
        }

        try:
            path = Path(file_path)
            metadata["file_size"] = path.stat().st_size

            with Image.open(file_path) as img:
                metadata["format"] = img.format
                metadata["width"] = img.width
                metadata["height"] = img.height
                metadata["mode"] = img.mode

                # Extract EXIF data (use public API since Pillow 6.0)
                exif_data = img.getexif()
                if exif_data:
                    for tag_id, value in exif_data.items():
                        tag = TAGS.get(tag_id, tag_id)
                        metadata["exif"][tag] = str(value)

        except Exception as e:
            metadata["error"] = str(e)

        return metadata

    @staticmethod
    def extract_video_metadata(file_path: str) -> Dict[str, Any]:
        """Extract metadata from a video file using FFprobe.

        Args:
            file_path: Path to the video file

        Returns:
            Dictionary containing metadata
        """
        import subprocess

        metadata = {
            "format": None,
            "duration": None,
            "width": None,
            "height": None,
            "codec": None,
            "bitrate": None,
            "fps": None,
            "file_size": None,
        }

        try:
            path = Path(file_path)
            metadata["file_size"] = path.stat().st_size

            # Use ffprobe to get video metadata
            cmd = [
                "ffprobe",
                "-v",
                "error",
                "-show_entries",
                "format=duration,bit_rate,format_name",
                "-show_entries",
                "stream=codec_type,width,height,codec_name,avg_frame_rate,r_frame_rate,duration",
                "-of",
                "json",
                file_path,
            ]

            result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)

            if result.returncode == 0:
                data = json.loads(result.stdout)

                # Format info
                if "format" in data:
                    fmt = data["format"]
                    metadata["format"] = fmt.get("format_name", "").split(",")[0]
                    duration = fmt.get("duration")
                    metadata["duration"] = float(duration) if duration else None
                    metadata["bitrate"] = (
                        int(fmt.get("bit_rate", 0)) if fmt.get("bit_rate") else None
                    )

                # Stream info (video)
                if "streams" in data:
                    for stream in data["streams"]:
                        if stream.get("codec_type") == "video" or "width" in stream:
                            metadata["width"] = stream.get("width")
                            metadata["height"] = stream.get("height")
                            metadata["codec"] = stream.get("codec_name")
                            metadata["fps"] = MetadataExtractor._parse_frame_rate(
                                stream.get("avg_frame_rate")
                            ) or MetadataExtractor._parse_frame_rate(stream.get("r_frame_rate"))
                            if metadata["duration"] is None and stream.get("duration"):
                                metadata["duration"] = float(stream["duration"])
                            break

        except Exception as e:
            metadata["error"] = str(e)

        return metadata

    @staticmethod
    def format_duration(seconds: float) -> str:
        """Format duration in seconds to HH:MM:SS.

        Args:
            seconds: Duration in seconds

        Returns:
            Formatted string
        """
        hours = int(seconds // 3600)
        minutes = int((seconds % 3600) // 60)
        secs = int(seconds % 60)

        if hours > 0:
            return f"{hours}:{minutes:02d}:{secs:02d}"
        else:
            return f"{minutes}:{secs:02d}"

    @staticmethod
    def format_file_size(size_bytes: int) -> str:
        """Format file size to human readable.

        Args:
            size_bytes: Size in bytes

        Returns:
            Formatted string (e.g., "1.5 MB")
        """
        for unit in ["B", "KB", "MB", "GB", "TB"]:
            if size_bytes < 1024.0:
                return f"{size_bytes:.1f} {unit}"
            size_bytes /= 1024.0
        return f"{size_bytes:.1f} PB"
