"""Modern polished dark theme for GalleryBrowser.

Design principles:
  - VS Code / Figma-quality dark UI
  - Layered surfaces: bg < surface < elevated < overlay
  - Accent blue (#2196F3) with tonal variations
  - Subtle depth via 1px borders and soft shadows
  - Consistent 4px / 8px spacing grid
  - Smooth hover transitions where QSS allows
  - Rounded corners (6-8px) for cards, 4px for inputs
"""

# ── Colour tokens ──────────────────────────────────────────────────
# Background layers (darkest to lightest)
_BG = "#0f0f0f"  # App canvas
_SURFACE = "#181818"  # Panels, panes
_SURFACE_2 = "#1e1e1e"  # Elevated cards, toolbar
_SURFACE_3 = "#252525"  # Menus, dropdowns, popovers
_SURFACE_4 = "#2c2c2c"  # Hover / raised elements

# Borders
_BORDER = "#2a2a2a"  # Subtle structural lines
_BORDER_HOVER = "#3a3a3a"  # Borders on hover
_BORDER_FOCUS = "#2196F3"  # Focus ring

# Text
_TEXT_PRIMARY = "#e8e8e8"
_TEXT_SECONDARY = "#a0a0a0"
_TEXT_MUTED = "#666666"
_TEXT_ON_ACCENT = "#ffffff"

# Accent palette
_ACCENT = "#2196F3"  # Primary accent
_ACCENT_HOVER = "#42A5F5"
_ACCENT_PRESSED = "#1976D2"
_ACCENT_DIM = "#0d47a1"  # Selected backgrounds
_ACCENT_SURFACE = "#1a3a5c"  # Subtle accent tint for selections

# Semantic
_SUCCESS = "#4CAF50"
_WARNING = "#FF9800"
_ERROR = "#F44336"
_GOLD = "#E2B340"  # Folder icons, favourites

# Status bar
_STATUS_BG = "#1565C0"


# ── Helpers ─────────────────────────────────────────────────────────
def _color_tokens() -> dict:
    """Return colour tokens dict for use in format strings."""
    return {k: v for k, v in globals().items() if k.startswith("_") and isinstance(v, str)}


# ── Main stylesheet ────────────────────────────────────────────────
DARK_THEME = f"""

/* ============================================================
   GLOBAL
   ============================================================ */
* {{
    outline: none;
}}

QMainWindow, QWidget {{
    background-color: {_BG};
    color: {_TEXT_PRIMARY};
    font-family: 'Inter', 'Segoe UI', 'SF Pro Text', 'Helvetica Neue', sans-serif;
    font-size: 13px;
    selection-background-color: {_ACCENT};
    selection-color: {_TEXT_ON_ACCENT};
}}

/* ============================================================
   MENU BAR
   ============================================================ */
QMenuBar {{
    background-color: {_SURFACE_2};
    border-bottom: 1px solid {_BORDER};
    padding: 2px 4px;
    spacing: 2px;
}}

QMenuBar::item {{
    background-color: transparent;
    padding: 6px 10px;
    border-radius: 6px;
    color: {_TEXT_SECONDARY};
}}

QMenuBar::item:selected {{
    background-color: {_SURFACE_4};
    color: {_TEXT_PRIMARY};
}}

QMenuBar::item:pressed {{
    background-color: {_ACCENT_SURFACE};
    color: {_TEXT_PRIMARY};
}}

QMenu {{
    background-color: {_SURFACE_3};
    border: 1px solid {_BORDER};
    border-radius: 8px;
    padding: 6px;
}}

QMenu::item {{
    padding: 6px 28px 6px 12px;
    border-radius: 4px;
    color: {_TEXT_PRIMARY};
}}

QMenu::item:selected {{
    background-color: {_ACCENT};
    color: {_TEXT_ON_ACCENT};
}}

QMenu::item:disabled {{
    color: {_TEXT_MUTED};
}}

QMenu::separator {{
    height: 1px;
    background-color: {_BORDER};
    margin: 6px 8px;
}}

QMenu::icon {{
    padding-left: 8px;
}}

/* ============================================================
   TOOLBAR
   ============================================================ */
QToolBar {{
    background-color: {_SURFACE_2};
    border: none;
    border-bottom: 1px solid {_BORDER};
    padding: 4px 8px;
    spacing: 2px;
}}

QToolBar::separator {{
    width: 1px;
    background-color: {_BORDER};
    margin: 4px 6px;
}}

QToolButton {{
    background-color: transparent;
    border: 1px solid transparent;
    border-radius: 6px;
    padding: 6px;
    color: {_TEXT_SECONDARY};
}}

QToolButton:hover {{
    background-color: {_SURFACE_4};
    border: 1px solid {_BORDER};
    color: {_TEXT_PRIMARY};
}}

QToolButton:pressed {{
    background-color: {_ACCENT_SURFACE};
    border: 1px solid {_ACCENT_DIM};
}}

QToolButton:checked {{
    background-color: {_ACCENT_SURFACE};
    border: 1px solid {_ACCENT};
    color: {_ACCENT_HOVER};
}}

QToolButton::menu-indicator {{
    image: none;
}}

/* ============================================================
   SPLITTERS
   ============================================================ */
QSplitter::handle {{
    background-color: {_BORDER};
}}

QSplitter::handle:horizontal {{
    width: 1px;
}}

QSplitter::handle:vertical {{
    height: 1px;
}}

QSplitter::handle:hover {{
    background-color: {_ACCENT};
}}

/* ============================================================
   BREADCRUMB BAR
   ============================================================ */
QWidget#breadcrumbBar {{
    background-color: {_SURFACE};
    border-bottom: 1px solid {_BORDER};
}}

QLabel#breadcrumbCaption {{
    color: {_TEXT_SECONDARY};
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
}}

QScrollArea#breadcrumbScrollArea {{
    background: transparent;
    border: none;
}}

QWidget#breadcrumbContent {{
    background-color: {_SURFACE_2};
    border: 1px solid {_BORDER};
    border-radius: 8px;
}}

QPushButton#breadcrumbSegment {{
    background-color: transparent;
    border: 1px solid transparent;
    border-radius: 6px;
    color: {_TEXT_PRIMARY};
    padding: 4px 10px;
    font-size: 13px;
    font-weight: 500;
}}

QPushButton#breadcrumbSegment:hover {{
    background-color: {_SURFACE_4};
    border: 1px solid {_BORDER_HOVER};
}}

QPushButton#breadcrumbSegment:pressed {{
    background-color: {_ACCENT_SURFACE};
    border: 1px solid {_ACCENT_DIM};
}}

QLabel#breadcrumbCurrent {{
    background-color: transparent;
    color: {_TEXT_PRIMARY};
    padding: 4px 10px;
    font-size: 13px;
    font-weight: 600;
}}

QLabel#breadcrumbSeparator {{
    color: {_TEXT_MUTED};
    padding: 0 4px;
}}

/* ============================================================
   TREE WIDGET  (folder pane)
   ============================================================ */
QTreeWidget {{
    background-color: {_SURFACE};
    border: none;
    outline: none;
    padding: 4px;
}}

QTreeWidget::item {{
    padding: 3px 6px;
    border-radius: 6px;
    margin: 1px 4px;
    color: {_TEXT_PRIMARY};
}}

QTreeWidget::item:selected {{
    background-color: {_ACCENT_SURFACE};
    color: {_TEXT_PRIMARY};
}}

QTreeWidget::item:hover:!selected {{
    background-color: {_SURFACE_4};
}}

QTreeWidget::item:selected:hover {{
    background-color: {_ACCENT_SURFACE};
}}

/* ============================================================
   HEADER VIEW  (table headers)
   ============================================================ */
QHeaderView::section {{
    background-color: {_SURFACE_2};
    color: {_TEXT_SECONDARY};
    padding: 6px 12px;
    border: none;
    border-bottom: 1px solid {_BORDER};
    font-weight: 600;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
}}

QHeaderView::section:hover {{
    background-color: {_SURFACE_3};
    color: {_TEXT_PRIMARY};
}}

/* ============================================================
   SCROLL AREAS & BARS
   ============================================================ */
QScrollArea {{
    border: none;
    background-color: {_SURFACE};
}}

QScrollBar:vertical {{
    background-color: transparent;
    width: 8px;
    border-radius: 4px;
    margin: 2px;
}}

QScrollBar::handle:vertical {{
    background-color: {_SURFACE_4};
    min-height: 32px;
    border-radius: 4px;
}}

QScrollBar::handle:vertical:hover {{
    background-color: {_TEXT_MUTED};
}}

QScrollBar::add-line:vertical,
QScrollBar::sub-line:vertical {{
    height: 0px;
}}

QScrollBar::add-page:vertical,
QScrollBar::sub-page:vertical {{
    background: transparent;
}}

QScrollBar:horizontal {{
    background-color: transparent;
    height: 8px;
    border-radius: 4px;
    margin: 2px;
}}

QScrollBar::handle:horizontal {{
    background-color: {_SURFACE_4};
    min-width: 32px;
    border-radius: 4px;
}}

QScrollBar::handle:horizontal:hover {{
    background-color: {_TEXT_MUTED};
}}

QScrollBar::add-line:horizontal,
QScrollBar::sub-line:horizontal {{
    width: 0px;
}}

QScrollBar::add-page:horizontal,
QScrollBar::sub-page:horizontal {{
    background: transparent;
}}

/* ============================================================
   STATUS BAR
   ============================================================ */
QStatusBar {{
    background-color: {_STATUS_BG};
    color: {_TEXT_ON_ACCENT};
    padding: 0px 12px;
    font-size: 11px;
    font-weight: 500;
    min-height: 24px;
    max-height: 24px;
}}

QStatusBar::item {{
    border: none;
}}

QStatusBar QLabel {{
    color: {_TEXT_ON_ACCENT};
    background: transparent;
    padding: 0px;
}}

/* ============================================================
   LABELS
   ============================================================ */
QLabel {{
    color: {_TEXT_PRIMARY};
    padding: 0px;
    background: transparent;
}}

/* ============================================================
   LINE EDIT
   ============================================================ */
QLineEdit {{
    background-color: {_SURFACE_3};
    border: 1px solid {_BORDER};
    border-radius: 6px;
    padding: 6px 10px;
    color: {_TEXT_PRIMARY};
    selection-background-color: {_ACCENT};
    font-size: 13px;
}}

QLineEdit:hover {{
    border: 1px solid {_BORDER_HOVER};
}}

QLineEdit:focus {{
    border: 1px solid {_ACCENT};
    background-color: {_SURFACE_2};
}}

QLineEdit::placeholder {{
    color: {_TEXT_MUTED};
}}

/* ============================================================
   COMBO BOX
   ============================================================ */
QComboBox {{
    background-color: {_SURFACE_3};
    border: 1px solid {_BORDER};
    border-radius: 6px;
    padding: 5px 10px;
    color: {_TEXT_PRIMARY};
    min-width: 70px;
}}

QComboBox:hover {{
    border: 1px solid {_BORDER_HOVER};
    background-color: {_SURFACE_4};
}}

QComboBox:focus {{
    border: 1px solid {_ACCENT};
}}

QComboBox::drop-down {{
    border: none;
    width: 20px;
    padding-right: 4px;
}}

QComboBox QAbstractItemView {{
    background-color: {_SURFACE_3};
    border: 1px solid {_BORDER};
    border-radius: 6px;
    padding: 4px;
    selection-background-color: {_ACCENT};
    selection-color: {_TEXT_ON_ACCENT};
    outline: none;
}}

QComboBox QAbstractItemView::item {{
    padding: 6px 10px;
    border-radius: 4px;
    min-height: 24px;
}}

QComboBox QAbstractItemView::item:hover {{
    background-color: {_SURFACE_4};
}}

/* ============================================================
   SPIN BOX
   ============================================================ */
QSpinBox {{
    background-color: {_SURFACE_3};
    border: 1px solid {_BORDER};
    border-radius: 6px;
    padding: 5px 10px;
    color: {_TEXT_PRIMARY};
}}

QSpinBox:hover {{
    border: 1px solid {_BORDER_HOVER};
}}

QSpinBox:focus {{
    border: 1px solid {_ACCENT};
}}

QSpinBox::up-button, QSpinBox::down-button {{
    background-color: transparent;
    border: none;
    width: 16px;
}}

/* ============================================================
   SLIDERS
   ============================================================ */
QSlider::groove:horizontal {{
    height: 4px;
    background-color: {_BORDER};
    border-radius: 2px;
}}

QSlider::handle:horizontal {{
    background-color: {_ACCENT};
    width: 14px;
    height: 14px;
    margin: -5px 0;
    border-radius: 7px;
    border: 2px solid {_SURFACE_2};
}}

QSlider::handle:horizontal:hover {{
    background-color: {_ACCENT_HOVER};
    width: 16px;
    height: 16px;
    margin: -6px 0;
    border-radius: 8px;
}}

QSlider::sub-page:horizontal {{
    background-color: {_ACCENT};
    border-radius: 2px;
}}

/* ============================================================
   TAB WIDGET
   ============================================================ */
QTabWidget::pane {{
    border: none;
    background-color: {_SURFACE};
    border-top: 1px solid {_BORDER};
}}

QTabBar {{
    background-color: transparent;
}}

QTabBar::tab {{
    background-color: transparent;
    color: {_TEXT_SECONDARY};
    padding: 8px 16px;
    margin-right: 0px;
    border-bottom: 2px solid transparent;
    font-size: 12px;
    font-weight: 500;
}}

QTabBar::tab:selected {{
    color: {_TEXT_PRIMARY};
    border-bottom: 2px solid {_ACCENT};
}}

QTabBar::tab:hover:!selected {{
    color: {_TEXT_PRIMARY};
    background-color: {_SURFACE_4};
}}

/* ============================================================
   PUSH BUTTON
   ============================================================ */
QPushButton {{
    background-color: {_ACCENT};
    border: none;
    border-radius: 6px;
    padding: 7px 16px;
    color: {_TEXT_ON_ACCENT};
    font-weight: 600;
    font-size: 12px;
}}

QPushButton:hover {{
    background-color: {_ACCENT_HOVER};
}}

QPushButton:pressed {{
    background-color: {_ACCENT_PRESSED};
}}

QPushButton:disabled {{
    background-color: {_SURFACE_3};
    color: {_TEXT_MUTED};
    border: 1px solid {_BORDER};
}}

/* Secondary button class (via object name or inline override) */
QPushButton[flat="true"] {{
    background-color: transparent;
    border: 1px solid {_BORDER};
    color: {_TEXT_PRIMARY};
}}

QPushButton[flat="true"]:hover {{
    background-color: {_SURFACE_4};
    border: 1px solid {_BORDER_HOVER};
}}

/* ============================================================
   LIST WIDGET
   ============================================================ */
QListWidget {{
    background-color: {_SURFACE};
    border: none;
    outline: none;
    padding: 4px;
}}

QListWidget::item {{
    padding: 6px 10px;
    border-radius: 6px;
    margin: 1px 2px;
    color: {_TEXT_PRIMARY};
}}

QListWidget::item:selected {{
    background-color: {_ACCENT_SURFACE};
}}

QListWidget::item:hover:!selected {{
    background-color: {_SURFACE_4};
}}

/* ============================================================
   TABLE WIDGET
   ============================================================ */
QTableWidget {{
    background-color: {_SURFACE};
    border: none;
    gridline-color: {_BORDER};
    outline: none;
}}

QTableWidget::item {{
    padding: 6px 8px;
    color: {_TEXT_PRIMARY};
    border-bottom: 1px solid {_BORDER};
}}

QTableWidget::item:selected {{
    background-color: {_ACCENT_SURFACE};
    color: {_TEXT_PRIMARY};
}}

QTableWidget::item:hover:!selected {{
    background-color: {_SURFACE_4};
}}

/* ============================================================
   PROGRESS BAR
   ============================================================ */
QProgressBar {{
    border: none;
    background-color: {_SURFACE_3};
    border-radius: 4px;
    text-align: center;
    color: {_TEXT_ON_ACCENT};
    font-size: 11px;
}}

QProgressBar::chunk {{
    background-color: {_ACCENT};
    border-radius: 4px;
}}

/* ============================================================
   GROUP BOX
   ============================================================ */
QGroupBox {{
    background-color: {_SURFACE};
    border: 1px solid {_BORDER};
    border-radius: 8px;
    margin-top: 16px;
    padding: 16px 12px 12px 12px;
    font-weight: 600;
}}

QGroupBox::title {{
    subcontrol-origin: margin;
    left: 12px;
    padding: 0 8px;
    color: {_TEXT_SECONDARY};
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
}}

/* ============================================================
   CHECK BOX
   ============================================================ */
QCheckBox {{
    spacing: 8px;
    padding: 4px 0;
    color: {_TEXT_PRIMARY};
}}

QCheckBox::indicator {{
    width: 18px;
    height: 18px;
    border-radius: 4px;
    border: 2px solid {_TEXT_MUTED};
    background-color: transparent;
}}

QCheckBox::indicator:hover {{
    border: 2px solid {_TEXT_SECONDARY};
}}

QCheckBox::indicator:checked {{
    background-color: {_ACCENT};
    border: 2px solid {_ACCENT};
}}

QCheckBox::indicator:checked:hover {{
    background-color: {_ACCENT_HOVER};
    border: 2px solid {_ACCENT_HOVER};
}}

/* ============================================================
   RADIO BUTTON
   ============================================================ */
QRadioButton {{
    spacing: 8px;
    padding: 4px 0;
    color: {_TEXT_PRIMARY};
}}

QRadioButton::indicator {{
    width: 18px;
    height: 18px;
    border-radius: 9px;
    border: 2px solid {_TEXT_MUTED};
    background-color: transparent;
}}

QRadioButton::indicator:hover {{
    border: 2px solid {_TEXT_SECONDARY};
}}

QRadioButton::indicator:checked {{
    background-color: {_ACCENT};
    border: 2px solid {_ACCENT};
}}

/* ============================================================
   TOOLTIP
   ============================================================ */
QToolTip {{
    background-color: {_SURFACE_3};
    border: 1px solid {_BORDER};
    border-radius: 6px;
    padding: 6px 10px;
    color: {_TEXT_PRIMARY};
    font-size: 12px;
}}

/* ============================================================
   DIALOG
   ============================================================ */
QDialog {{
    background-color: {_SURFACE};
}}

/* ============================================================
   INPUT DIALOG / MESSAGE BOX
   ============================================================ */
QInputDialog, QMessageBox {{
    background-color: {_SURFACE};
}}
"""


def apply_theme(app):
    """Apply the polished dark theme to the application.

    Args:
        app: QApplication instance
    """
    import os
    import tempfile

    # Generate branch arrow SVG icons for the tree widget.
    # When QSS styles QTreeWidget::branch, Qt drops the native arrows
    # and requires explicit images via the stylesheet.
    _arrow_color = _TEXT_SECONDARY  # muted arrow colour

    _arrow_right_svg = (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8" viewBox="0 0 12 12">'
        f'<path d="M4 2 L9 6 L4 10" fill="none" stroke="{_arrow_color}" stroke-width="1.8" '
        f'stroke-linecap="round" stroke-linejoin="round"/></svg>'
    )
    _arrow_down_svg = (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8" viewBox="0 0 12 12">'
        f'<path d="M2 4 L6 9 L10 4" fill="none" stroke="{_arrow_color}" stroke-width="1.8" '
        f'stroke-linecap="round" stroke-linejoin="round"/></svg>'
    )

    arrow_dir = os.path.join(tempfile.gettempdir(), "gallerybrowser_arrows")
    os.makedirs(arrow_dir, exist_ok=True)

    arrow_right_path = os.path.join(arrow_dir, "arrow_right.svg")
    arrow_down_path = os.path.join(arrow_dir, "arrow_down.svg")

    with open(arrow_right_path, "w") as f:
        f.write(_arrow_right_svg)
    with open(arrow_down_path, "w") as f:
        f.write(_arrow_down_svg)

    # Build branch stylesheet snippet with the generated arrow images
    branch_css = f"""
QTreeWidget::branch {{
    background-color: transparent;
}}

QTreeWidget::branch:has-children:!has-siblings:closed,
QTreeWidget::branch:closed:has-children:has-siblings {{
    image: url({arrow_right_path});
}}

QTreeWidget::branch:open:has-children:!has-siblings,
QTreeWidget::branch:open:has-children:has-siblings {{
    image: url({arrow_down_path});
}}
"""

    app.setStyleSheet(DARK_THEME + branch_css)

    # Set application-wide palette for better native widget integration
    from PyQt6.QtGui import QColor, QPalette

    palette = app.palette()
    palette.setColor(QPalette.ColorRole.Window, QColor(_BG))
    palette.setColor(QPalette.ColorRole.WindowText, QColor(_TEXT_PRIMARY))
    palette.setColor(QPalette.ColorRole.Base, QColor(_SURFACE))
    palette.setColor(QPalette.ColorRole.AlternateBase, QColor(_SURFACE_2))
    palette.setColor(QPalette.ColorRole.Text, QColor(_TEXT_PRIMARY))
    palette.setColor(QPalette.ColorRole.Button, QColor(_SURFACE_2))
    palette.setColor(QPalette.ColorRole.ButtonText, QColor(_TEXT_PRIMARY))
    palette.setColor(QPalette.ColorRole.Highlight, QColor(_ACCENT))
    palette.setColor(QPalette.ColorRole.HighlightedText, QColor(_TEXT_ON_ACCENT))
    palette.setColor(QPalette.ColorRole.ToolTipBase, QColor(_SURFACE_3))
    palette.setColor(QPalette.ColorRole.ToolTipText, QColor(_TEXT_PRIMARY))
    palette.setColor(QPalette.ColorRole.PlaceholderText, QColor(_TEXT_MUTED))
    palette.setColor(QPalette.ColorRole.Link, QColor(_ACCENT))
    palette.setColor(QPalette.ColorRole.LinkVisited, QColor(_ACCENT_DIM))

    # Disabled colours
    palette.setColor(
        QPalette.ColorGroup.Disabled, QPalette.ColorRole.WindowText, QColor(_TEXT_MUTED)
    )
    palette.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.Text, QColor(_TEXT_MUTED))
    palette.setColor(
        QPalette.ColorGroup.Disabled, QPalette.ColorRole.ButtonText, QColor(_TEXT_MUTED)
    )

    app.setPalette(palette)
