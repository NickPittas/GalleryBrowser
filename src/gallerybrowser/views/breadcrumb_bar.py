"""Breadcrumb path bar for modern file-browser style navigation."""

from __future__ import annotations

from pathlib import Path

from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtGui import QFontMetrics
from PyQt6.QtWidgets import (
    QFrame,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QScrollArea,
    QSizePolicy,
    QWidget,
)


class BreadcrumbBar(QWidget):
    """Compact breadcrumb widget for filesystem navigation."""

    path_selected = pyqtSignal(str)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.current_path: str | None = None
        self.current_label = "No folder selected"
        self.segment_buttons: list[QPushButton] = []
        self.current_segments: list[tuple[str, str | None, bool]] = []
        self._setup_ui()
        self.set_label("No folder selected")

    def _setup_ui(self) -> None:
        self.setObjectName("breadcrumbBar")
        layout = QHBoxLayout(self)
        layout.setContentsMargins(12, 8, 12, 8)
        layout.setSpacing(8)

        self.caption_label = QLabel("Path")
        self.caption_label.setObjectName("breadcrumbCaption")
        layout.addWidget(self.caption_label, 0, Qt.AlignmentFlag.AlignVCenter)

        self.scroll_area = QScrollArea()
        self.scroll_area.setObjectName("breadcrumbScrollArea")
        self.scroll_area.setWidgetResizable(True)
        self.scroll_area.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.scroll_area.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.scroll_area.setFrameShape(QFrame.Shape.NoFrame)
        self.scroll_area.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self.scroll_area.setFixedHeight(36)

        self._content = QWidget()
        self._content.setObjectName("breadcrumbContent")
        self._content_layout = QHBoxLayout(self._content)
        self._content_layout.setContentsMargins(8, 4, 8, 4)
        self._content_layout.setSpacing(2)
        self._content_layout.addStretch(1)

        self.scroll_area.setWidget(self._content)
        layout.addWidget(self.scroll_area, 1)

    def set_label(self, label: str) -> None:
        """Show a single non-clickable label."""
        self.current_path = None
        self.current_label = label
        self._render_segments([(label, None, False)])

    def set_path(self, path: str | None) -> None:
        """Render a filesystem path as clickable breadcrumb segments."""
        if not path:
            self.set_label("No folder selected")
            return

        resolved = str(Path(path).resolve())
        self.current_path = resolved
        self.current_label = resolved
        self._render_segments(self._build_path_segments(Path(resolved)))
        self.scroll_area.horizontalScrollBar().setValue(
            self.scroll_area.horizontalScrollBar().maximum()
        )

    def _build_path_segments(self, path: Path) -> list[tuple[str, str | None, bool]]:
        parts = path.parts
        if not parts:
            return [(str(path), None, False)]

        segments: list[tuple[str, str | None, bool]] = []
        if path.is_absolute():
            segments.append((parts[0], parts[0], True))
            current = Path(parts[0])
            remaining_parts = parts[1:]
        else:
            current = Path(parts[0])
            segments.append((parts[0], str(current), True))
            remaining_parts = parts[1:]

        for part in remaining_parts:
            current = current / part
            segments.append((part, str(current), True))

        return segments

    def _render_segments(self, segments: list[tuple[str, str | None, bool]]) -> None:
        self.current_segments = list(segments)
        self.segment_buttons = []

        while self._content_layout.count():
            item = self._content_layout.takeAt(0)
            widget = item.widget()
            if widget is not None:
                widget.deleteLater()

        for index, (label, path, clickable) in enumerate(segments):
            if index > 0:
                sep = QLabel("›")
                sep.setObjectName("breadcrumbSeparator")
                sep.setAlignment(Qt.AlignmentFlag.AlignCenter)
                self._content_layout.addWidget(sep)

            if clickable and path:
                button = QPushButton(self._elide_label(label))
                button.setObjectName("breadcrumbSegment")
                button.setFlat(True)
                button.setCursor(Qt.CursorShape.PointingHandCursor)
                button.setToolTip(path)
                button.clicked.connect(lambda _checked=False, target=path: self.path_selected.emit(target))
                self._content_layout.addWidget(button)
                self.segment_buttons.append(button)
            else:
                chip = QLabel(self._elide_label(label))
                chip.setObjectName("breadcrumbCurrent")
                chip.setToolTip(label)
                self._content_layout.addWidget(chip)

        self._content_layout.addStretch(1)

    def _elide_label(self, label: str) -> str:
        metrics = QFontMetrics(self.font())
        return metrics.elidedText(label, Qt.TextElideMode.ElideMiddle, 220)
