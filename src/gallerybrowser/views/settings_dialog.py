"""Settings dialog for application preferences."""

from PyQt6.QtCore import Qt
from PyQt6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QDialog,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QSpinBox,
    QTabWidget,
    QVBoxLayout,
    QWidget,
)

from gallerybrowser.config import Config


class SettingsDialog(QDialog):
    """Dialog for application settings."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Settings")
        self.setMinimumSize(500, 400)
        self.setup_ui()
        self.load_settings()

    def setup_ui(self):
        """Set up the UI."""
        layout = QVBoxLayout(self)

        # Tab widget
        tabs = QTabWidget()

        # General tab
        general_tab = QWidget()
        general_layout = QVBoxLayout(general_tab)
        general_layout.setAlignment(Qt.AlignmentFlag.AlignTop)

        # Thumbnail size
        thumb_layout = QHBoxLayout()
        thumb_layout.addWidget(QLabel("Default thumbnail size:"))
        self.thumb_size_combo = QComboBox()
        self.thumb_size_combo.addItems(["64", "128", "256", "512"])
        thumb_layout.addWidget(self.thumb_size_combo)
        thumb_layout.addStretch()
        general_layout.addLayout(thumb_layout)

        # Show hidden files
        self.show_hidden_check = QCheckBox("Show hidden files")
        general_layout.addWidget(self.show_hidden_check)

        # Video preview
        self.video_preview_check = QCheckBox("Enable video preview")
        self.video_preview_check.setChecked(True)
        general_layout.addWidget(self.video_preview_check)

        general_layout.addStretch()
        tabs.addTab(general_tab, "General")

        # Performance tab
        perf_tab = QWidget()
        perf_layout = QVBoxLayout(perf_tab)
        perf_layout.setAlignment(Qt.AlignmentFlag.AlignTop)

        # Thread count
        thread_layout = QHBoxLayout()
        thread_layout.addWidget(QLabel("Thumbnail threads:"))
        self.thread_spin = QSpinBox()
        self.thread_spin.setRange(1, 8)
        self.thread_spin.setValue(2)
        thread_layout.addWidget(self.thread_spin)
        thread_layout.addStretch()
        perf_layout.addLayout(thread_layout)

        # Cache size
        cache_layout = QHBoxLayout()
        cache_layout.addWidget(QLabel("Max cache size (MB):"))
        self.cache_spin = QSpinBox()
        self.cache_spin.setRange(100, 10000)
        self.cache_spin.setValue(1000)
        self.cache_spin.setSingleStep(100)
        cache_layout.addWidget(self.cache_spin)
        cache_layout.addStretch()
        perf_layout.addLayout(cache_layout)

        # RAM preview budget
        ram_layout = QHBoxLayout()
        ram_layout.addWidget(QLabel("RAM preview budget (MB):"))
        self.ram_preview_spin = QSpinBox()
        self.ram_preview_spin.setRange(256, 32768)
        self.ram_preview_spin.setValue(2048)
        self.ram_preview_spin.setSingleStep(256)
        self.ram_preview_spin.setToolTip(
            "Maximum RAM used for caching decoded sequence frames.\n"
            "At 1920x1080 (~6 MB/frame), 2048 MB holds ~330 frames."
        )
        ram_layout.addWidget(self.ram_preview_spin)
        ram_layout.addStretch()
        perf_layout.addLayout(ram_layout)

        perf_layout.addStretch()
        tabs.addTab(perf_tab, "Performance")

        layout.addWidget(tabs)

        # Buttons
        button_layout = QHBoxLayout()
        button_layout.addStretch()
        cancel_btn = QPushButton("Cancel")
        cancel_btn.clicked.connect(self.reject)
        button_layout.addWidget(cancel_btn)
        save_btn = QPushButton("Save")
        save_btn.setDefault(True)
        save_btn.clicked.connect(self.on_save)
        button_layout.addWidget(save_btn)
        layout.addLayout(button_layout)

    def load_settings(self):
        """Load current settings from disk."""
        settings = Config.load_settings()
        # Thumbnail size
        size_str = str(settings.get("thumbnail_size", 128))
        idx = self.thumb_size_combo.findText(size_str)
        if idx >= 0:
            self.thumb_size_combo.setCurrentIndex(idx)
        # Checkboxes
        self.show_hidden_check.setChecked(settings.get("show_hidden", False))
        self.video_preview_check.setChecked(settings.get("video_preview", True))
        # Performance
        self.thread_spin.setValue(settings.get("thumbnail_threads", 2))
        self.cache_spin.setValue(settings.get("max_cache_mb", 1000))
        self.ram_preview_spin.setValue(settings.get("ram_preview_mb", 2048))

    def on_save(self):
        """Save settings to disk."""
        settings = Config.load_settings()
        settings.update({
            "thumbnail_size": int(self.thumb_size_combo.currentText()),
            "show_hidden": self.show_hidden_check.isChecked(),
            "video_preview": self.video_preview_check.isChecked(),
            "thumbnail_threads": self.thread_spin.value(),
            "max_cache_mb": self.cache_spin.value(),
            "ram_preview_mb": self.ram_preview_spin.value(),
        })
        Config.save_settings(settings)
        self.accept()

    def get_settings(self):
        """Get settings as dictionary."""
        return {
            "thumbnail_size": int(self.thumb_size_combo.currentText()),
            "show_hidden": self.show_hidden_check.isChecked(),
            "video_preview": self.video_preview_check.isChecked(),
            "thumbnail_threads": self.thread_spin.value(),
            "max_cache_mb": self.cache_spin.value(),
            "ram_preview_mb": self.ram_preview_spin.value(),
        }
