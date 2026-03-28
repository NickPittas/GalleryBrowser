"""Database models for GalleryBrowser."""

from datetime import datetime, timezone
from typing import List, Optional

from sqlalchemy import (
    Boolean,
    CheckConstraint,
    Column,
    DateTime,
    Float,
    ForeignKey,
    Index,
    Integer,
    String,
    Table,
    Text,
    UniqueConstraint,
)
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column, relationship


def _utcnow() -> datetime:
    """Return timezone-aware UTC datetime."""
    return datetime.now(timezone.utc)


class Base(DeclarativeBase):
    """Base class for all models."""

    pass


# Association table for many-to-many relationship between files and tags
file_tags_table = Table(
    "file_tags",
    Base.metadata,
    Column("file_id", Integer, ForeignKey("files.id", ondelete="CASCADE"), primary_key=True),
    Column(
        "tag_id", Integer, ForeignKey("tags.id", ondelete="CASCADE"), primary_key=True, index=True
    ),
    Column("added_at", DateTime, default=_utcnow),
)


class File(Base):
    """Represents a file in the database."""

    __tablename__ = "files"

    id: Mapped[int] = mapped_column(primary_key=True)
    path: Mapped[str] = mapped_column(String, unique=True, nullable=False)
    filename: Mapped[str] = mapped_column(String, nullable=False)
    folder_path: Mapped[str] = mapped_column(String, nullable=False, index=True)
    file_type: Mapped[str] = mapped_column(
        String, nullable=False, index=True
    )  # 'image', 'video', 'sequence'
    format: Mapped[Optional[str]] = mapped_column(String, index=True)  # 'png', 'jpg', 'mp4', etc.
    size_bytes: Mapped[Optional[int]] = mapped_column(Integer)
    created_at: Mapped[Optional[datetime]] = mapped_column(DateTime)
    modified_at: Mapped[Optional[datetime]] = mapped_column(DateTime)
    width: Mapped[Optional[int]] = mapped_column(Integer)
    height: Mapped[Optional[int]] = mapped_column(Integer)
    duration_seconds: Mapped[Optional[float]] = mapped_column(Float)  # For videos
    frame_count: Mapped[Optional[int]] = mapped_column(Integer)  # For sequences
    checksum: Mapped[Optional[str]] = mapped_column(String)  # For change detection
    is_sequence: Mapped[bool] = mapped_column(Boolean, default=False)
    sequence_start: Mapped[Optional[int]] = mapped_column(Integer)
    sequence_end: Mapped[Optional[int]] = mapped_column(Integer)
    sequence_pattern: Mapped[Optional[str]] = mapped_column(String)  # e.g., "frame_%04d.png"
    added_at: Mapped[datetime] = mapped_column(DateTime, default=_utcnow)
    last_accessed: Mapped[Optional[datetime]] = mapped_column(DateTime)

    # Relationships
    thumbnails: Mapped[List["Thumbnail"]] = relationship(
        back_populates="file", cascade="all, delete-orphan"
    )
    comfyui_metadata: Mapped[Optional["ComfyUIMetadata"]] = relationship(
        back_populates="file", cascade="all, delete-orphan", uselist=False
    )
    tags: Mapped[List["Tag"]] = relationship(secondary=file_tags_table, back_populates="files")
    collections: Mapped[List["Collection"]] = relationship(
        secondary="collection_items", back_populates="files"
    )
    rating: Mapped[Optional["Rating"]] = relationship(
        back_populates="file", cascade="all, delete-orphan", uselist=False
    )

    def __repr__(self) -> str:
        return f"<File(id={self.id}, filename='{self.filename}')>"


class Thumbnail(Base):
    """Represents a generated thumbnail."""

    __tablename__ = "thumbnails"

    id: Mapped[int] = mapped_column(primary_key=True)
    file_id: Mapped[int] = mapped_column(ForeignKey("files.id", ondelete="CASCADE"), nullable=False)
    size: Mapped[int] = mapped_column(Integer, nullable=False)  # e.g., 128, 256, 512
    cache_path: Mapped[str] = mapped_column(String, nullable=False)
    generated_at: Mapped[datetime] = mapped_column(DateTime, default=_utcnow)

    # Relationships
    file: Mapped["File"] = relationship(back_populates="thumbnails")

    __table_args__ = (UniqueConstraint("file_id", "size", name="uix_thumbnail_file_size"),)

    def __repr__(self) -> str:
        return f"<Thumbnail(file_id={self.file_id}, size={self.size})>"


class ComfyUIMetadata(Base):
    """Stores ComfyUI workflow metadata extracted from PNG files."""

    __tablename__ = "comfyui_metadata"

    id: Mapped[int] = mapped_column(primary_key=True)
    file_id: Mapped[int] = mapped_column(
        ForeignKey("files.id", ondelete="CASCADE"), nullable=False, unique=True
    )
    workflow_json: Mapped[Optional[str]] = mapped_column(Text)  # Full workflow JSON
    prompt: Mapped[Optional[str]] = mapped_column(Text)  # Extracted positive prompt
    negative_prompt: Mapped[Optional[str]] = mapped_column(Text)
    model: Mapped[Optional[str]] = mapped_column(String, index=True)
    sampler: Mapped[Optional[str]] = mapped_column(String, index=True)
    scheduler: Mapped[Optional[str]] = mapped_column(String)
    steps: Mapped[Optional[int]] = mapped_column(Integer)
    cfg_scale: Mapped[Optional[float]] = mapped_column(Float)
    seed: Mapped[Optional[int]] = mapped_column(Integer, index=True)
    width: Mapped[Optional[int]] = mapped_column(Integer)
    height: Mapped[Optional[int]] = mapped_column(Integer)
    extracted_at: Mapped[datetime] = mapped_column(DateTime, default=_utcnow)

    # Relationships
    file: Mapped["File"] = relationship(back_populates="comfyui_metadata")

    def __repr__(self) -> str:
        return f"<ComfyUIMetadata(file_id={self.file_id}, model='{self.model}')>"


class Tag(Base):
    """Represents a tag that can be assigned to files."""

    __tablename__ = "tags"

    id: Mapped[int] = mapped_column(primary_key=True)
    name: Mapped[str] = mapped_column(String, unique=True, nullable=False)
    color: Mapped[Optional[str]] = mapped_column(String)  # Hex color code
    created_at: Mapped[datetime] = mapped_column(DateTime, default=_utcnow)

    # Relationships
    files: Mapped[List["File"]] = relationship(secondary=file_tags_table, back_populates="tags")

    def __repr__(self) -> str:
        return f"<Tag(id={self.id}, name='{self.name}')>"


class Collection(Base):
    """Represents a virtual collection of files."""

    __tablename__ = "collections"

    id: Mapped[int] = mapped_column(primary_key=True)
    name: Mapped[str] = mapped_column(String, nullable=False)
    description: Mapped[Optional[str]] = mapped_column(Text)
    color: Mapped[Optional[str]] = mapped_column(String)  # Hex color code
    created_at: Mapped[datetime] = mapped_column(DateTime, default=_utcnow)
    updated_at: Mapped[datetime] = mapped_column(DateTime, default=_utcnow, onupdate=_utcnow)

    # Relationships
    files: Mapped[List["File"]] = relationship(
        secondary="collection_items", back_populates="collections"
    )

    def __repr__(self) -> str:
        return f"<Collection(id={self.id}, name='{self.name}')>"


class CollectionItem(Base):
    """Association between collections and files."""

    __tablename__ = "collection_items"

    collection_id: Mapped[int] = mapped_column(
        ForeignKey("collections.id", ondelete="CASCADE"), primary_key=True
    )
    file_id: Mapped[int] = mapped_column(
        ForeignKey("files.id", ondelete="CASCADE"), primary_key=True, index=True
    )
    added_at: Mapped[datetime] = mapped_column(DateTime, default=_utcnow)


class Rating(Base):
    """Represents a 5-star rating for a file."""

    __tablename__ = "ratings"

    file_id: Mapped[int] = mapped_column(
        ForeignKey("files.id", ondelete="CASCADE"), primary_key=True
    )
    rating: Mapped[int] = mapped_column(Integer, nullable=False)  # 0-5 stars
    rated_at: Mapped[datetime] = mapped_column(DateTime, default=_utcnow)

    __table_args__ = (CheckConstraint("rating >= 0 AND rating <= 5", name="ck_rating_range"),)

    # Relationships
    file: Mapped["File"] = relationship(back_populates="rating")

    def __repr__(self) -> str:
        return f"<Rating(file_id={self.file_id}, rating={self.rating})>"


class Setting(Base):
    """Application settings stored in database."""

    __tablename__ = "settings"

    key: Mapped[str] = mapped_column(String, primary_key=True)
    value: Mapped[Optional[str]] = mapped_column(Text)
    updated_at: Mapped[datetime] = mapped_column(DateTime, default=_utcnow, onupdate=_utcnow)

    def __repr__(self) -> str:
        return f"<Setting(key='{self.key}')>"
