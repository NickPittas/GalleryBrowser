"""Batch rename dialog with chainable operations and live preview.

All operations (Find & Replace, Insert / Remove, Numbering, Case,
Extension) can be enabled simultaneously via checkboxes.  They are
applied in top-to-bottom order, and the live two-column preview table
updates instantly on every change.
"""

import os
import re
from pathlib import Path
from typing import List, Tuple

from PyQt6.QtCore import Qt
from PyQt6.QtGui import QColor, QFont
from PyQt6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QDialog,
    QFrame,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QLineEdit,
    QPushButton,
    QScrollArea,
    QSpinBox,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
    QWidget,
)

# ── Colour constants (match the app theme) ──────────────────────────
_BG = "#0f0f0f"
_SURFACE = "#181818"
_SURFACE_2 = "#1e1e1e"
_SURFACE_3 = "#252525"
_SURFACE_4 = "#2c2c2c"
_BORDER = "#2a2a2a"
_BORDER_HOVER = "#3a3a3a"
_TEXT_PRIMARY = "#e8e8e8"
_TEXT_SECONDARY = "#a0a0a0"
_TEXT_MUTED = "#666666"
_ACCENT = "#2196F3"
_ACCENT_SURFACE = "#1a3a5c"
_ERROR = "#F44336"
_SUCCESS = "#4CAF50"


# ── Helper: collapsible operation panel ─────────────────────────────


class _OperationPanel(QFrame):
    """A titled panel with an enable checkbox and a content area."""

    def __init__(self, title: str, parent=None):
        super().__init__(parent)
        self.setFrameShape(QFrame.Shape.StyledPanel)
        self.setStyleSheet(
            f"""
            _OperationPanel {{
                background-color: {_SURFACE_2};
                border: 1px solid {_BORDER};
                border-radius: 6px;
            }}
            """
        )
        outer = QVBoxLayout(self)
        outer.setContentsMargins(8, 6, 8, 6)
        outer.setSpacing(4)

        # Header row: checkbox + title
        header = QHBoxLayout()
        self.enable_cb = QCheckBox(title)
        font = QFont()
        font.setBold(True)
        self.enable_cb.setFont(font)
        self.enable_cb.setStyleSheet(f"color: {_TEXT_PRIMARY};")
        header.addWidget(self.enable_cb)
        header.addStretch()
        outer.addLayout(header)

        # Content widget (shown/hidden with checkbox)
        self.content = QWidget()
        self.content_layout = QVBoxLayout(self.content)
        self.content_layout.setContentsMargins(20, 2, 0, 2)
        self.content_layout.setSpacing(4)
        self.content.setEnabled(False)
        self.content.setStyleSheet(f"color: {_TEXT_MUTED};")
        outer.addWidget(self.content)

        self.enable_cb.toggled.connect(self._on_toggle)

    def _on_toggle(self, checked: bool):
        self.content.setEnabled(checked)
        if checked:
            self.content.setStyleSheet("")
        else:
            self.content.setStyleSheet(f"color: {_TEXT_MUTED};")

    @property
    def enabled(self) -> bool:
        return self.enable_cb.isChecked()


class BatchRenameDialog(QDialog):
    """Dialog for batch renaming files with chainable operations."""

    def __init__(self, file_paths: List[str], parent=None):
        super().__init__(parent)
        self.file_paths = sorted(file_paths)
        self.new_names: List[str] = []
        # Cache creation times once
        self._ctimes: dict = {}
        for fp in self.file_paths:
            try:
                self._ctimes[fp] = os.stat(fp).st_mtime
            except OSError:
                self._ctimes[fp] = 0.0
        self._setup_ui()
        self._connect_signals()
        self._update_preview()

    # ── public API ──────────────────────────────────────────────────
    def get_renames(self) -> List[Tuple[str, str]]:
        """Return ``[(old_path, new_name), ...]``."""
        return list(zip(self.file_paths, self.new_names))

    # ── UI construction ─────────────────────────────────────────────
    def _setup_ui(self):
        self.setWindowTitle(f"Batch Rename  —  {len(self.file_paths)} files")
        self.setMinimumSize(820, 600)
        self.resize(860, 660)

        root = QVBoxLayout(self)
        root.setContentsMargins(12, 12, 12, 12)
        root.setSpacing(6)

        # ── Scope selector ──────────────────────────────────────────
        scope_row = QHBoxLayout()
        scope_row.addWidget(self._label("Apply to:"))
        self.scope_combo = QComboBox()
        self.scope_combo.addItems(["Name only", "Extension only", "Name + Extension"])
        self.scope_combo.setCurrentIndex(0)
        self.scope_combo.setFixedWidth(170)
        scope_row.addWidget(self.scope_combo)
        scope_row.addStretch()
        root.addLayout(scope_row)

        # ── Operations (scrollable) ─────────────────────────────────
        ops_scroll = QScrollArea()
        ops_scroll.setWidgetResizable(True)
        ops_scroll.setFrameShape(QFrame.Shape.NoFrame)
        ops_scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        ops_scroll.setMaximumHeight(280)
        ops_container = QWidget()
        ops_layout = QVBoxLayout(ops_container)
        ops_layout.setContentsMargins(0, 0, 0, 0)
        ops_layout.setSpacing(4)

        self._build_find_replace_panel(ops_layout)
        self._build_insert_remove_panel(ops_layout)
        self._build_numbering_panel(ops_layout)
        self._build_case_panel(ops_layout)
        self._build_extension_panel(ops_layout)

        ops_layout.addStretch()
        ops_scroll.setWidget(ops_container)
        root.addWidget(ops_scroll)

        # ── Preview table ───────────────────────────────────────────
        preview_label = QLabel("Preview")
        preview_label.setStyleSheet(f"color: {_TEXT_SECONDARY}; font-weight: bold;")
        root.addWidget(preview_label)

        self.table = QTableWidget(len(self.file_paths), 2)
        self.table.setHorizontalHeaderLabels(["Original Name", "New Name"])
        self.table.horizontalHeader().setStretchLastSection(True)
        self.table.horizontalHeader().setSectionResizeMode(0, QHeaderView.ResizeMode.Stretch)
        self.table.horizontalHeader().setSectionResizeMode(1, QHeaderView.ResizeMode.Stretch)
        self.table.verticalHeader().setDefaultSectionSize(24)
        self.table.verticalHeader().hide()
        self.table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        self.table.setSelectionMode(QTableWidget.SelectionMode.NoSelection)
        self.table.setFocusPolicy(Qt.FocusPolicy.NoFocus)
        self.table.setStyleSheet(
            f"""
            QTableWidget {{
                background-color: {_SURFACE};
                border: 1px solid {_BORDER};
                border-radius: 4px;
                gridline-color: {_BORDER};
            }}
            QTableWidget::item {{
                padding: 2px 6px;
                color: {_TEXT_PRIMARY};
            }}
            QHeaderView::section {{
                background-color: {_SURFACE_2};
                color: {_TEXT_SECONDARY};
                border: none;
                border-bottom: 1px solid {_BORDER};
                padding: 4px 6px;
                font-weight: bold;
            }}
            """
        )
        root.addWidget(self.table, stretch=1)

        # ── Bottom bar ──────────────────────────────────────────────
        bottom = QHBoxLayout()
        self.status_label = QLabel("")
        self.status_label.setStyleSheet(f"color: {_TEXT_MUTED};")
        bottom.addWidget(self.status_label)
        bottom.addStretch()

        cancel_btn = QPushButton("Cancel")
        cancel_btn.clicked.connect(self.reject)
        bottom.addWidget(cancel_btn)

        self.rename_btn = QPushButton("Rename")
        self.rename_btn.setDefault(True)
        self.rename_btn.clicked.connect(self._on_accept)
        self.rename_btn.setStyleSheet(
            f"""
            QPushButton {{
                background-color: {_ACCENT};
                color: #ffffff;
                border: none;
                padding: 6px 20px;
                border-radius: 4px;
                font-weight: bold;
            }}
            QPushButton:hover {{ background-color: #42A5F5; }}
            QPushButton:pressed {{ background-color: #1976D2; }}
            QPushButton:disabled {{ background-color: {_SURFACE_4}; color: {_TEXT_MUTED}; }}
            """
        )
        bottom.addWidget(self.rename_btn)
        root.addLayout(bottom)

    # ── Helpers ──────────────────────────────────────────────────────

    @staticmethod
    def _label(text: str) -> QLabel:
        lbl = QLabel(text)
        lbl.setFixedWidth(80)
        return lbl

    @staticmethod
    def _small_label(text: str) -> QLabel:
        return QLabel(text)

    # ── Panel builders ──────────────────────────────────────────────

    def _build_find_replace_panel(self, parent_layout: QVBoxLayout):
        self.pnl_fr = _OperationPanel("Find && Replace")
        cl = self.pnl_fr.content_layout

        row1 = QHBoxLayout()
        row1.addWidget(self._small_label("Find:"))
        self.fr_find = QLineEdit()
        self.fr_find.setPlaceholderText("Text to find")
        row1.addWidget(self.fr_find)
        cl.addLayout(row1)

        row2 = QHBoxLayout()
        row2.addWidget(self._small_label("Replace:"))
        self.fr_replace = QLineEdit()
        self.fr_replace.setPlaceholderText("Replacement (empty = delete)")
        row2.addWidget(self.fr_replace)
        cl.addLayout(row2)

        opts = QHBoxLayout()
        self.fr_case = QCheckBox("Case sensitive")
        self.fr_case.setChecked(True)
        opts.addWidget(self.fr_case)
        self.fr_regex = QCheckBox("Regex")
        opts.addWidget(self.fr_regex)
        opts.addStretch()
        cl.addLayout(opts)

        parent_layout.addWidget(self.pnl_fr)

    def _build_insert_remove_panel(self, parent_layout: QVBoxLayout):
        self.pnl_ir = _OperationPanel("Insert / Remove")
        cl = self.pnl_ir.content_layout

        mode_row = QHBoxLayout()
        mode_row.addWidget(self._small_label("Mode:"))
        self.ir_mode = QComboBox()
        self.ir_mode.addItems(["Insert text", "Remove characters"])
        self.ir_mode.setFixedWidth(180)
        mode_row.addWidget(self.ir_mode)
        mode_row.addStretch()
        cl.addLayout(mode_row)

        # Insert fields
        self.ir_insert_widget = QWidget()
        il = QHBoxLayout(self.ir_insert_widget)
        il.setContentsMargins(0, 0, 0, 0)
        il.addWidget(self._small_label("Text:"))
        self.ir_text = QLineEdit()
        self.ir_text.setPlaceholderText("Text to insert")
        il.addWidget(self.ir_text)
        il.addWidget(self._small_label("at pos:"))
        self.ir_insert_pos = QSpinBox()
        self.ir_insert_pos.setRange(0, 999)
        self.ir_insert_pos.setValue(0)
        self.ir_insert_pos.setFixedWidth(70)
        self.ir_insert_pos.setToolTip("0 = beginning (prefix)")
        il.addWidget(self.ir_insert_pos)
        cl.addWidget(self.ir_insert_widget)

        # Remove fields
        self.ir_remove_widget = QWidget()
        rl = QHBoxLayout(self.ir_remove_widget)
        rl.setContentsMargins(0, 0, 0, 0)
        rl.addWidget(self._small_label("From pos:"))
        self.ir_remove_pos = QSpinBox()
        self.ir_remove_pos.setRange(0, 999)
        self.ir_remove_pos.setValue(0)
        self.ir_remove_pos.setFixedWidth(70)
        rl.addWidget(self.ir_remove_pos)
        rl.addWidget(self._small_label("remove"))
        self.ir_remove_count = QSpinBox()
        self.ir_remove_count.setRange(1, 999)
        self.ir_remove_count.setValue(1)
        self.ir_remove_count.setFixedWidth(70)
        rl.addWidget(self.ir_remove_count)
        rl.addWidget(self._small_label("chars"))
        rl.addStretch()
        cl.addWidget(self.ir_remove_widget)
        self.ir_remove_widget.hide()

        self.ir_mode.currentIndexChanged.connect(self._toggle_insert_remove)

        parent_layout.addWidget(self.pnl_ir)

    def _build_numbering_panel(self, parent_layout: QVBoxLayout):
        self.pnl_num = _OperationPanel("Numbering")
        cl = self.pnl_num.content_layout

        # Row 1: position + sort order
        row1 = QHBoxLayout()
        row1.addWidget(self._small_label("Position:"))
        self.num_position = QComboBox()
        self.num_position.addItems(["Prefix", "Suffix", "Insert at position"])
        self.num_position.setFixedWidth(160)
        row1.addWidget(self.num_position)

        self.num_insert_pos = QSpinBox()
        self.num_insert_pos.setRange(0, 999)
        self.num_insert_pos.setFixedWidth(65)
        self.num_insert_pos.setVisible(False)
        row1.addWidget(self.num_insert_pos)

        row1.addSpacing(12)
        row1.addWidget(self._small_label("Order:"))
        self.num_sort = QComboBox()
        self.num_sort.addItems(
            [
                "Current order",
                "By name (A-Z)",
                "By date modified (oldest first)",
                "By date modified (newest first)",
            ]
        )
        self.num_sort.setFixedWidth(220)
        self.num_sort.setToolTip("Order in which numbers are assigned")
        row1.addWidget(self.num_sort)
        row1.addStretch()
        cl.addLayout(row1)

        self.num_position.currentIndexChanged.connect(
            lambda idx: self.num_insert_pos.setVisible(idx == 2)
        )

        # Row 2: start / step / padding
        row2 = QHBoxLayout()
        row2.addWidget(self._small_label("Start:"))
        self.num_start = QSpinBox()
        self.num_start.setRange(0, 99999)
        self.num_start.setValue(1)
        self.num_start.setFixedWidth(75)
        row2.addWidget(self.num_start)

        row2.addWidget(self._small_label("Step:"))
        self.num_step = QSpinBox()
        self.num_step.setRange(1, 1000)
        self.num_step.setValue(1)
        self.num_step.setFixedWidth(65)
        row2.addWidget(self.num_step)

        row2.addWidget(self._small_label("Padding:"))
        self.num_padding = QSpinBox()
        self.num_padding.setRange(1, 10)
        self.num_padding.setValue(3)
        self.num_padding.setFixedWidth(55)
        self.num_padding.setToolTip("Zero-fill width  (3 = 001, 002 ...)")
        row2.addWidget(self.num_padding)

        row2.addWidget(self._small_label("Sep:"))
        self.num_separator = QLineEdit("_")
        self.num_separator.setFixedWidth(50)
        self.num_separator.setToolTip("Text between number and name")
        row2.addWidget(self.num_separator)
        row2.addStretch()
        cl.addLayout(row2)

        # Row 3: replace name
        self.num_replace_name = QCheckBox(
            "Replace original name entirely (number + extension only)"
        )
        cl.addWidget(self.num_replace_name)

        parent_layout.addWidget(self.pnl_num)

    def _build_case_panel(self, parent_layout: QVBoxLayout):
        self.pnl_case = _OperationPanel("Case")
        cl = self.pnl_case.content_layout

        row = QHBoxLayout()
        row.addWidget(self._small_label("Convert to:"))
        self.case_mode = QComboBox()
        self.case_mode.addItems(
            [
                "UPPERCASE",
                "lowercase",
                "Title Case",
                "Sentence case",
            ]
        )
        self.case_mode.setFixedWidth(160)
        row.addWidget(self.case_mode)
        row.addStretch()
        cl.addLayout(row)

        parent_layout.addWidget(self.pnl_case)

    def _build_extension_panel(self, parent_layout: QVBoxLayout):
        self.pnl_ext = _OperationPanel("Extension")
        cl = self.pnl_ext.content_layout

        row = QHBoxLayout()
        row.addWidget(self._small_label("Mode:"))
        self.ext_mode = QComboBox()
        self.ext_mode.addItems(["Change to", "Remove extension", "Add extension"])
        self.ext_mode.setFixedWidth(160)
        row.addWidget(self.ext_mode)

        self.ext_value = QLineEdit()
        self.ext_value.setPlaceholderText("e.g.  jpg")
        self.ext_value.setFixedWidth(120)
        row.addWidget(self.ext_value)
        row.addStretch()
        cl.addLayout(row)

        self.ext_mode.currentIndexChanged.connect(lambda idx: self.ext_value.setVisible(idx != 1))

        parent_layout.addWidget(self.pnl_ext)

    # ── Signal wiring ───────────────────────────────────────────────

    def _connect_signals(self):
        """Wire every input widget to ``_update_preview``."""
        u = self._update_preview

        self.scope_combo.currentIndexChanged.connect(u)

        # Panel enable checkboxes
        for pnl in (self.pnl_fr, self.pnl_ir, self.pnl_num, self.pnl_case, self.pnl_ext):
            pnl.enable_cb.toggled.connect(u)

        # Find & Replace
        self.fr_find.textChanged.connect(u)
        self.fr_replace.textChanged.connect(u)
        self.fr_case.stateChanged.connect(u)
        self.fr_regex.stateChanged.connect(u)

        # Insert / Remove
        self.ir_mode.currentIndexChanged.connect(u)
        self.ir_text.textChanged.connect(u)
        self.ir_insert_pos.valueChanged.connect(u)
        self.ir_remove_pos.valueChanged.connect(u)
        self.ir_remove_count.valueChanged.connect(u)

        # Numbering
        self.num_position.currentIndexChanged.connect(u)
        self.num_insert_pos.valueChanged.connect(u)
        self.num_start.valueChanged.connect(u)
        self.num_step.valueChanged.connect(u)
        self.num_padding.valueChanged.connect(u)
        self.num_separator.textChanged.connect(u)
        self.num_replace_name.stateChanged.connect(u)
        self.num_sort.currentIndexChanged.connect(u)

        # Case
        self.case_mode.currentIndexChanged.connect(u)

        # Extension
        self.ext_mode.currentIndexChanged.connect(u)
        self.ext_value.textChanged.connect(u)

    # ── Toggle helpers ──────────────────────────────────────────────

    def _toggle_insert_remove(self, idx: int):
        self.ir_insert_widget.setVisible(idx == 0)
        self.ir_remove_widget.setVisible(idx == 1)

    # ── Core rename logic ───────────────────────────────────────────

    @staticmethod
    def _split_name(filename: str) -> Tuple[str, str]:
        """Split filename into (stem, extension-with-dot)."""
        p = Path(filename)
        return p.stem, p.suffix

    def _apply_scope(self, stem: str, ext: str, transform) -> Tuple[str, str]:
        """Apply *transform* to the part selected by the scope combo."""
        scope = self.scope_combo.currentIndex()
        if scope == 0:  # Name only
            return transform(stem), ext
        elif scope == 1:  # Extension only
            new_ext = transform(ext.lstrip("."))
            return stem, ("." + new_ext) if new_ext else ""
        else:  # Name + Extension
            combined = stem + ext
            result = transform(combined)
            p = Path(result)
            return p.stem, p.suffix

    def _numbering_order(self) -> List[int]:
        """Return indices into ``self.file_paths`` in the chosen sort order.

        The returned list maps *numbering rank* -> *file_paths index*.
        So if the user picks "By date modified (oldest first)", the file with
        the oldest mtime gets rank 0, next-oldest gets rank 1, etc.
        """
        n = len(self.file_paths)
        sort_mode = self.num_sort.currentIndex()

        if sort_mode == 0:  # current order (alphabetical)
            return list(range(n))
        elif sort_mode == 1:  # by name A-Z
            indices = list(range(n))
            indices.sort(key=lambda i: Path(self.file_paths[i]).name.lower())
            return indices
        elif sort_mode == 2:  # date modified oldest first
            indices = list(range(n))
            indices.sort(key=lambda i: self._ctimes.get(self.file_paths[i], 0))
            return indices
        elif sort_mode == 3:  # date modified newest first
            indices = list(range(n))
            indices.sort(key=lambda i: self._ctimes.get(self.file_paths[i], 0), reverse=True)
            return indices
        return list(range(n))

    def _compute_new_name(self, filename: str, index: int, num_rank: int) -> str:
        """Compute the new name by chaining all enabled operations.

        *index* is the position in the file list.
        *num_rank* is the numbering rank (may differ from index when sorted).
        """
        stem, ext = self._split_name(filename)

        # 1) Find & Replace
        if self.pnl_fr.enabled:
            stem, ext = self._apply_find_replace(stem, ext)

        # 2) Insert / Remove
        if self.pnl_ir.enabled:
            stem, ext = self._apply_insert_remove(stem, ext)

        # 3) Numbering
        if self.pnl_num.enabled:
            stem, ext = self._apply_numbering(stem, ext, num_rank)

        # 4) Case
        if self.pnl_case.enabled:
            stem, ext = self._apply_case(stem, ext)

        # 5) Extension
        if self.pnl_ext.enabled:
            stem, ext = self._apply_extension(stem, ext)

        return stem + ext

    # ── Per-operation transforms ────────────────────────────────────

    def _apply_find_replace(self, stem: str, ext: str) -> Tuple[str, str]:
        find = self.fr_find.text()
        if not find:
            return stem, ext

        replace = self.fr_replace.text()
        use_regex = self.fr_regex.isChecked()
        case_sensitive = self.fr_case.isChecked()

        def transform(s):
            try:
                if use_regex:
                    flags = 0 if case_sensitive else re.IGNORECASE
                    return re.sub(find, replace, s, flags=flags)
                else:
                    if case_sensitive:
                        return s.replace(find, replace)
                    else:
                        pattern = re.escape(find)
                        return re.sub(pattern, replace, s, flags=re.IGNORECASE)
            except re.error:
                return s

        return self._apply_scope(stem, ext, transform)

    def _apply_insert_remove(self, stem: str, ext: str) -> Tuple[str, str]:
        mode = self.ir_mode.currentIndex()

        if mode == 0:  # Insert
            text = self.ir_text.text()
            if not text:
                return stem, ext
            pos = self.ir_insert_pos.value()

            def transform(s):
                p = min(pos, len(s))
                return s[:p] + text + s[p:]
        else:  # Remove
            pos = self.ir_remove_pos.value()
            count = self.ir_remove_count.value()

            def transform(s):
                return s[:pos] + s[pos + count :]

        return self._apply_scope(stem, ext, transform)

    def _apply_numbering(self, stem: str, ext: str, rank: int) -> Tuple[str, str]:
        start = self.num_start.value()
        step = self.num_step.value()
        padding = self.num_padding.value()
        sep = self.num_separator.text()
        pos_mode = self.num_position.currentIndex()
        replace_name = self.num_replace_name.isChecked()

        num = start + rank * step
        num_str = str(num).zfill(padding)

        if replace_name:
            return num_str, ext

        if pos_mode == 0:  # Prefix
            return num_str + sep + stem, ext
        elif pos_mode == 1:  # Suffix
            return stem + sep + num_str, ext
        else:  # Insert at position
            ins_pos = self.num_insert_pos.value()
            p = min(ins_pos, len(stem))
            return stem[:p] + num_str + stem[p:], ext

    def _apply_case(self, stem: str, ext: str) -> Tuple[str, str]:
        transforms = {
            0: str.upper,
            1: str.lower,
            2: str.title,
            3: lambda s: s[:1].upper() + s[1:].lower() if s else s,
        }
        transform = transforms.get(self.case_mode.currentIndex(), lambda s: s)
        return self._apply_scope(stem, ext, transform)

    def _apply_extension(self, stem: str, ext: str) -> Tuple[str, str]:
        mode = self.ext_mode.currentIndex()
        if mode == 0:  # Change to
            new_ext = self.ext_value.text().strip().lstrip(".")
            return stem, ("." + new_ext) if new_ext else ""
        elif mode == 1:  # Remove
            return stem, ""
        elif mode == 2:  # Add
            add_ext = self.ext_value.text().strip().lstrip(".")
            return stem, ext + ("." + add_ext if add_ext else "")
        return stem, ext

    # ── Preview ─────────────────────────────────────────────────────

    def _update_preview(self):
        """Recompute all new names and refresh the table."""
        self.new_names = []
        errors = 0
        seen_names: dict = {}

        # Build numbering rank map:  file_paths index  ->  rank
        rank_map: dict = {}
        if self.pnl_num.enabled:
            ordered_indices = self._numbering_order()
            for rank, file_idx in enumerate(ordered_indices):
                rank_map[file_idx] = rank

        for i, file_path in enumerate(self.file_paths):
            old_name = Path(file_path).name
            num_rank = rank_map.get(i, i)
            new_name = self._compute_new_name(old_name, i, num_rank)

            # Error detection
            is_error = False
            error_reason = ""
            if not new_name or "/" in new_name or "\0" in new_name:
                is_error = True
                error_reason = "invalid"
            elif new_name in seen_names:
                is_error = True
                error_reason = f"duplicate of row {seen_names[new_name] + 1}"

            if not is_error:
                seen_names[new_name] = i

            self.new_names.append(new_name)

            # Original name column
            old_item = QTableWidgetItem(old_name)
            old_item.setForeground(QColor(_TEXT_SECONDARY))
            self.table.setItem(i, 0, old_item)

            # New name column
            display = new_name if not is_error else f"{new_name}  ({error_reason})"
            new_item = QTableWidgetItem(display)
            if is_error:
                new_item.setForeground(QColor(_ERROR))
                errors += 1
            elif new_name != old_name:
                new_item.setForeground(QColor(_SUCCESS))
            else:
                new_item.setForeground(QColor(_TEXT_MUTED))
            self.table.setItem(i, 1, new_item)

        # Status
        changed = sum(
            1 for i, fp in enumerate(self.file_paths) if self.new_names[i] != Path(fp).name
        )
        if errors:
            self.status_label.setText(f"{errors} error(s)  —  rename disabled")
            self.status_label.setStyleSheet(f"color: {_ERROR};")
            self.rename_btn.setEnabled(False)
        elif changed == 0:
            self.status_label.setText("No changes")
            self.status_label.setStyleSheet(f"color: {_TEXT_MUTED};")
            self.rename_btn.setEnabled(False)
        else:
            self.status_label.setText(f"{changed} of {len(self.file_paths)} files will be renamed")
            self.status_label.setStyleSheet(f"color: {_TEXT_SECONDARY};")
            self.rename_btn.setEnabled(True)

    # ── Accept ──────────────────────────────────────────────────────

    def _on_accept(self):
        if self.rename_btn.isEnabled() and len(self.new_names) == len(self.file_paths):
            self.accept()
