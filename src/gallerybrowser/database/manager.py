"""Database manager for GalleryBrowser."""

import threading
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional

from appdirs import user_cache_dir
from sqlalchemy import create_engine, select, text
from sqlalchemy.orm import Session, sessionmaker

from .models import Base, Collection, File, Setting, Tag, Thumbnail


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
            # Use default cache location
            cache_dir = Path(user_cache_dir("gallerybrowser", "NickPittas"))
            cache_dir.mkdir(parents=True, exist_ok=True)
            db_path = str(cache_dir / "database.sqlite")

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

    def get_file_by_path(self, path: str) -> Optional[File]:
        """Get a file by its path."""
        with self.get_session() as session:
            result = session.scalar(select(File).where(File.path == path))
            return self._expunge_one(session, result)

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
