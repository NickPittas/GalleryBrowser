"""Database manager for GalleryBrowser."""

import threading
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional

from sqlalchemy import create_engine, select, text
from sqlalchemy.orm import Session, sessionmaker

from gallerybrowser.config import Config

from .models import Base, Collection, File, Rating, Setting, Tag, Thumbnail


def _utcnow() -> datetime:
    """Return timezone-aware UTC datetime."""
    return datetime.now(timezone.utc)


class DatabaseManager:
    """Manages database connections and operations.

    Thread-safe singleton — uses a lock to avoid races in __new__.
    """

    _instance: Optional["DatabaseManager"] = None
    _instance_lock = threading.Lock()

    def __new__(cls) -> "DatabaseManager":
        with cls._instance_lock:
            if cls._instance is None:
                cls._instance = super().__new__(cls)
                cls._instance._initialized = False
        return cls._instance

    def __init__(self):
        if self._initialized:
            return

        self._initialized = True
        self._engine = None
        self._session_factory = None

    def initialize(self, db_path: Optional[str] = None) -> None:
        """Initialize the database connection.

        Args:
            db_path: Path to the SQLite database file. If None, uses default location.
        """
        if db_path is None:
            db_path = str(Config.get_database_path())

        self._engine = create_engine(
            f"sqlite:///{db_path}",
            echo=False,  # Set to True for debugging
            connect_args={"check_same_thread": False},
        )

        # Enable WAL mode for better concurrency
        # PRAGMA statements must be committed explicitly in SQLAlchemy 2.0+
        with self._engine.connect() as conn:
            conn.execute(text("PRAGMA journal_mode=WAL"))
            conn.execute(text("PRAGMA synchronous=NORMAL"))
            conn.commit()

        # Create all tables
        Base.metadata.create_all(self._engine)

        self._session_factory = sessionmaker(bind=self._engine)

    def dispose(self) -> None:
        """Dispose of the engine and release all connections."""
        if self._engine:
            self._engine.dispose()
            self._engine = None
            self._session_factory = None

    def get_session(self) -> Session:
        """Get a new database session."""
        if self._session_factory is None:
            raise RuntimeError("Database not initialized. Call initialize() first.")
        return self._session_factory()

    # ---------- helpers to avoid detached-object errors ----------
    # All query methods below use expunge + make_transient to detach objects
    # cleanly, or convert to dicts.  This prevents DetachedInstanceError when
    # callers access attributes outside the session scope.

    def _expunge_all(self, session: Session, objects: list) -> list:
        """Expunge a list of ORM objects from the session so they can be used outside it."""
        from sqlalchemy.orm import make_transient

        for obj in objects:
            session.expunge(obj)
            make_transient(obj)
        return objects

    def _expunge_one(self, session: Session, obj: Any) -> Any:
        """Expunge a single ORM object from the session."""
        if obj is not None:
            from sqlalchemy.orm import make_transient

            session.expunge(obj)
            make_transient(obj)
        return obj

    # File operations
    def add_file(self, file: File) -> File:
        """Add a file to the database."""
        with self.get_session() as session:
            session.add(file)
            session.commit()
            session.refresh(file)
            self._expunge_one(session, file)
            return file

    def upsert_file_from_path(self, path: str, file_info: Optional[Dict[str, Any]] = None) -> File:
        """Create or update a file record from a filesystem path."""
        path_obj = Path(path)
        stat = path_obj.stat()
        file_type = (file_info or {}).get("type") or Config.get_file_type(path)
        record_format = path_obj.suffix.lower().lstrip(".") or None

        with self.get_session() as session:
            record = session.scalar(select(File).where(File.path == path))
            if record is None:
                record = File(
                    path=path,
                    filename=path_obj.name,
                    folder_path=str(path_obj.parent),
                    file_type=file_type,
                    format=record_format,
                )
                session.add(record)

            record.filename = path_obj.name
            record.folder_path = str(path_obj.parent)
            record.file_type = file_type
            record.format = record_format
            record.size_bytes = stat.st_size
            record.created_at = datetime.fromtimestamp(stat.st_ctime, tz=timezone.utc)
            record.modified_at = datetime.fromtimestamp(stat.st_mtime, tz=timezone.utc)

            if file_info:
                if file_info.get("type") == "sequence":
                    record.is_sequence = True
                    frame_range = file_info.get("frame_range")
                    if frame_range:
                        record.sequence_start = frame_range[0]
                        record.sequence_end = frame_range[1]
                    record.sequence_pattern = file_info.get("name")
                    record.frame_count = file_info.get("frame_count")
                else:
                    record.is_sequence = False
                    record.sequence_start = None
                    record.sequence_end = None
                    record.sequence_pattern = None
                    record.frame_count = None

            session.commit()
            session.refresh(record)
            self._expunge_one(session, record)
            return record

    def get_file_by_path(self, path: str) -> Optional[File]:
        """Get a file by its path."""
        with self.get_session() as session:
            result = session.scalar(select(File).where(File.path == path))
            return self._expunge_one(session, result)

    def get_all_files(self) -> List[File]:
        """Get all known files."""
        with self.get_session() as session:
            results = list(session.scalars(select(File)))
            return self._expunge_all(session, results)

    def get_files_in_folder(self, folder_path: str) -> List[File]:
        """Get all files in a folder."""
        with self.get_session() as session:
            results = list(session.scalars(select(File).where(File.folder_path == folder_path)))
            return self._expunge_all(session, results)

    def delete_file(self, file_id: int) -> None:
        """Delete a file from the database."""
        with self.get_session() as session:
            file = session.get(File, file_id)
            if file:
                session.delete(file)
                session.commit()

    def update_file_access(self, file_id: int) -> None:
        """Update the last_accessed timestamp for a file."""
        with self.get_session() as session:
            file = session.get(File, file_id)
            if file:
                file.last_accessed = _utcnow()
                session.commit()

    # Thumbnail operations
    def get_thumbnail(self, file_id: int, size: int) -> Optional[Thumbnail]:
        """Get a thumbnail for a file at a specific size."""
        with self.get_session() as session:
            result = session.scalar(
                select(Thumbnail).where(Thumbnail.file_id == file_id, Thumbnail.size == size)
            )
            return self._expunge_one(session, result)

    def add_thumbnail(self, thumbnail: Thumbnail) -> Thumbnail:
        """Add a thumbnail to the database."""
        with self.get_session() as session:
            session.add(thumbnail)
            session.commit()
            session.refresh(thumbnail)
            self._expunge_one(session, thumbnail)
            return thumbnail

    # Tag operations
    def get_all_tags(self) -> List[Tag]:
        """Get all tags."""
        with self.get_session() as session:
            results = list(session.scalars(select(Tag)))
            return self._expunge_all(session, results)

    def add_tag(self, name: str, color: Optional[str] = None) -> Tag:
        """Add a new tag."""
        tag = Tag(name=name, color=color)
        with self.get_session() as session:
            session.add(tag)
            session.commit()
            session.refresh(tag)
            self._expunge_one(session, tag)
            return tag

    def delete_tag(self, tag_id: int) -> None:
        """Delete a tag."""
        with self.get_session() as session:
            tag = session.get(Tag, tag_id)
            if tag:
                session.delete(tag)
                session.commit()

    def get_tag_by_name(self, name: str) -> Optional[Tag]:
        """Get a tag by name."""
        with self.get_session() as session:
            result = session.scalar(select(Tag).where(Tag.name == name))
            return self._expunge_one(session, result)

    def assign_tag_to_file(
        self, file_path: str, tag_name: str, color: Optional[str] = None
    ) -> Optional[Tag]:
        """Assign a tag to a file, creating both records if needed."""
        path_obj = Path(file_path)
        if not path_obj.exists():
            return None

        with self.get_session() as session:
            file_record = session.scalar(select(File).where(File.path == file_path))
            if file_record is None:
                stat = path_obj.stat()
                file_record = File(
                    path=file_path,
                    filename=path_obj.name,
                    folder_path=str(path_obj.parent),
                    file_type=Config.get_file_type(file_path),
                    format=path_obj.suffix.lower().lstrip(".") or None,
                    size_bytes=stat.st_size,
                    created_at=datetime.fromtimestamp(stat.st_ctime, tz=timezone.utc),
                    modified_at=datetime.fromtimestamp(stat.st_mtime, tz=timezone.utc),
                )
                session.add(file_record)

            tag = session.scalar(select(Tag).where(Tag.name == tag_name))
            if tag is None:
                tag = Tag(name=tag_name, color=color)
                session.add(tag)
                session.flush()

            if tag not in file_record.tags:
                file_record.tags.append(tag)

            session.commit()
            session.refresh(tag)
            self._expunge_one(session, tag)
            return tag

    def remove_tag_from_file(self, file_path: str, tag_name: str) -> bool:
        """Remove a tag assignment from a file."""
        with self.get_session() as session:
            file_record = session.scalar(select(File).where(File.path == file_path))
            if file_record is None:
                return False
            tag = session.scalar(select(Tag).where(Tag.name == tag_name))
            if tag is None or tag not in file_record.tags:
                return False
            file_record.tags.remove(tag)
            session.commit()
            return True

    def get_tags_for_file(self, file_path: str) -> List[Tag]:
        """List tags assigned to a file."""
        with self.get_session() as session:
            file_record = session.scalar(select(File).where(File.path == file_path))
            if file_record is None:
                return []
            results = list(file_record.tags)
            return self._expunge_all(session, results)

    def get_files_by_tag(self, tag_name: str, folder_path: Optional[str] = None) -> List[File]:
        """List files assigned a given tag."""
        with self.get_session() as session:
            stmt = select(File).join(File.tags).where(Tag.name == tag_name)
            if folder_path:
                stmt = stmt.where(File.folder_path == folder_path)
            results = list(session.scalars(stmt))
            return self._expunge_all(session, results)

    # Collection operations
    def get_all_collections(self) -> List[Collection]:
        """Get all collections."""
        with self.get_session() as session:
            results = list(session.scalars(select(Collection)))
            return self._expunge_all(session, results)

    def add_collection(
        self, name: str, description: Optional[str] = None, color: Optional[str] = None
    ) -> Collection:
        """Add a new collection."""
        collection = Collection(name=name, description=description, color=color)
        with self.get_session() as session:
            session.add(collection)
            session.commit()
            session.refresh(collection)
            self._expunge_one(session, collection)
            return collection

    def delete_collection(self, collection_id: int) -> None:
        """Delete a collection."""
        with self.get_session() as session:
            collection = session.get(Collection, collection_id)
            if collection:
                session.delete(collection)
                session.commit()

    def get_collection_by_name(self, name: str) -> Optional[Collection]:
        """Get a collection by name."""
        with self.get_session() as session:
            result = session.scalar(select(Collection).where(Collection.name == name))
            return self._expunge_one(session, result)

    def add_file_to_collection(self, file_path: str, collection_name: str) -> Optional[Collection]:
        """Add a file to a collection, creating the file record if needed."""
        path_obj = Path(file_path)
        if not path_obj.exists():
            return None

        with self.get_session() as session:
            file_record = session.scalar(select(File).where(File.path == file_path))
            if file_record is None:
                stat = path_obj.stat()
                file_record = File(
                    path=file_path,
                    filename=path_obj.name,
                    folder_path=str(path_obj.parent),
                    file_type=Config.get_file_type(file_path),
                    format=path_obj.suffix.lower().lstrip(".") or None,
                    size_bytes=stat.st_size,
                    created_at=datetime.fromtimestamp(stat.st_ctime, tz=timezone.utc),
                    modified_at=datetime.fromtimestamp(stat.st_mtime, tz=timezone.utc),
                )
                session.add(file_record)

            collection = session.scalar(select(Collection).where(Collection.name == collection_name))
            if collection is None:
                collection = Collection(name=collection_name)
                session.add(collection)
                session.flush()

            if collection not in file_record.collections:
                file_record.collections.append(collection)

            session.commit()
            session.refresh(collection)
            self._expunge_one(session, collection)
            return collection

    def remove_file_from_collection(self, file_path: str, collection_name: str) -> bool:
        """Remove a file from a collection."""
        with self.get_session() as session:
            file_record = session.scalar(select(File).where(File.path == file_path))
            if file_record is None:
                return False
            collection = session.scalar(select(Collection).where(Collection.name == collection_name))
            if collection is None or collection not in file_record.collections:
                return False
            file_record.collections.remove(collection)
            session.commit()
            return True

    def get_collections_for_file(self, file_path: str) -> List[Collection]:
        """List collections containing a file."""
        with self.get_session() as session:
            file_record = session.scalar(select(File).where(File.path == file_path))
            if file_record is None:
                return []
            results = list(file_record.collections)
            return self._expunge_all(session, results)

    def get_files_by_collection(
        self, collection_name: str, folder_path: Optional[str] = None
    ) -> List[File]:
        """List files belonging to a collection."""
        with self.get_session() as session:
            stmt = select(File).join(File.collections).where(Collection.name == collection_name)
            if folder_path:
                stmt = stmt.where(File.folder_path == folder_path)
            results = list(session.scalars(stmt))
            return self._expunge_all(session, results)

    def remove_all_file_memberships(self, file_path: str) -> None:
        """Remove all collection memberships and tags for a file."""
        with self.get_session() as session:
            file_record = session.scalar(select(File).where(File.path == file_path))
            if file_record is None:
                return
            file_record.tags.clear()
            file_record.collections.clear()
            rating = session.get(Rating, file_record.id)
            if rating:
                session.delete(rating)
            session.commit()

    # Ratings
    def set_rating(self, file_path: str, rating_value: int) -> Optional[Rating]:
        """Set a 0-5 rating on a file."""
        if rating_value < 0 or rating_value > 5:
            raise ValueError("rating_value must be between 0 and 5")

        file_record = self.upsert_file_from_path(file_path)
        with self.get_session() as session:
            rating = session.get(Rating, file_record.id)
            if rating is None:
                rating = Rating(file_id=file_record.id, rating=rating_value)
                session.add(rating)
            else:
                rating.rating = rating_value
            session.commit()
            session.refresh(rating)
            self._expunge_one(session, rating)
            return rating

    def clear_rating(self, file_path: str) -> bool:
        """Remove a rating from a file."""
        with self.get_session() as session:
            file_record = session.scalar(select(File).where(File.path == file_path))
            if file_record is None:
                return False
            rating = session.get(Rating, file_record.id)
            if rating is None:
                return False
            session.delete(rating)
            session.commit()
            return True

    def get_rating_for_file(self, file_path: str) -> Optional[int]:
        """Return the rating for a file."""
        with self.get_session() as session:
            file_record = session.scalar(select(File).where(File.path == file_path))
            if file_record is None or file_record.rating is None:
                return None
            return file_record.rating.rating

    def get_files_by_rating(
        self, rating_value: Optional[int] = None, folder_path: Optional[str] = None
    ) -> List[File]:
        """List files by exact rating, or all rated files when rating_value is None."""
        with self.get_session() as session:
            stmt = select(File).join(File.rating)
            if rating_value is not None:
                stmt = stmt.where(Rating.rating == rating_value)
            if folder_path:
                stmt = stmt.where(File.folder_path == folder_path)
            results = list(session.scalars(stmt))
            return self._expunge_all(session, results)

    def get_unrated_files(self, folder_path: Optional[str] = None) -> List[File]:
        """List files that do not have a rating."""
        with self.get_session() as session:
            stmt = select(File).outerjoin(File.rating).where(Rating.file_id.is_(None))
            if folder_path:
                stmt = stmt.where(File.folder_path == folder_path)
            results = list(session.scalars(stmt))
            return self._expunge_all(session, results)

    # Settings operations
    def get_setting(self, key: str, default: Optional[str] = None) -> Optional[str]:
        """Get a setting value."""
        with self.get_session() as session:
            setting = session.get(Setting, key)
            return setting.value if setting else default

    def set_setting(self, key: str, value: str) -> None:
        """Set a setting value."""
        with self.get_session() as session:
            setting = session.get(Setting, key)
            if setting:
                setting.value = value
            else:
                setting = Setting(key=key, value=value)
                session.add(setting)
            session.commit()
