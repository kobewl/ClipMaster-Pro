from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QLabel,
    QPushButton, QCheckBox, QSpinBox, QTabWidget,
    QWidget, QFormLayout, QLineEdit, QGroupBox,
    QDialogButtonBox, QMessageBox, QComboBox,
    QSlider
)
from PyQt6.QtCore import Qt, pyqtSignal, QTimer, QEvent
from PyQt6.QtGui import QKeySequence

from config.settings import Settings
from utils.logger import logger
from utils.startup import StartupManager


class SettingsDialog(QDialog):
    """Settings dialog with General, Hotkeys, Advanced, and AI tabs."""

    settingsChanged = pyqtSignal()
    aiSettingsChanged = pyqtSignal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("设置")
        self.setMinimumWidth(480)
        self.setMinimumHeight(520)
        self._capturing_for = None  # Initialize capture state
        self._init_ui()
        self._load_settings()
        # Install event filter on the dialog itself for key capture
        self.installEventFilter(self)

    def _init_ui(self):
        layout = QVBoxLayout(self)
        layout.setSpacing(16)
        layout.setContentsMargins(20, 20, 20, 20)

        tab_widget = QTabWidget()
        tab_widget.setDocumentMode(True)

        tab_widget.addTab(self._create_general_tab(), "常规")
        tab_widget.addTab(self._create_hotkeys_tab(), "热键")
        tab_widget.addTab(self._create_ai_tab(), "AI 预测")
        tab_widget.addTab(self._create_advanced_tab(), "高级")

        layout.addWidget(tab_widget)

        button_box = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok
            | QDialogButtonBox.StandardButton.Cancel
        )
        button_box.accepted.connect(self._save_settings)
        button_box.rejected.connect(self.reject)
        layout.addWidget(button_box)

    # ── General Tab ──────────────────────────────────────────────────

    def _create_general_tab(self) -> QWidget:
        tab = QWidget()
        layout = QVBoxLayout(tab)
        layout.setSpacing(16)

        appearance_group = QGroupBox("🎨 外观")
        appearance_layout = QVBoxLayout(appearance_group)
        self.dark_mode_checkbox = QCheckBox("启用暗色模式")
        self.dark_mode_checkbox.setToolTip("切换应用程序的主题颜色")
        appearance_layout.addWidget(self.dark_mode_checkbox)
        layout.addWidget(appearance_group)

        startup_group = QGroupBox("🚀 启动")
        startup_layout = QVBoxLayout(startup_group)
        self.startup_checkbox = QCheckBox("开机自启动")
        self.startup_checkbox.setToolTip("系统启动时自动运行程序")
        startup_layout.addWidget(self.startup_checkbox)
        self.minimize_to_tray_checkbox = QCheckBox("启动时最小化到托盘")
        self.minimize_to_tray_checkbox.setToolTip("程序启动时不显示主窗口")
        startup_layout.addWidget(self.minimize_to_tray_checkbox)
        layout.addWidget(startup_group)

        history_group = QGroupBox("📋 历史记录")
        history_layout = QFormLayout(history_group)
        self.max_history_spinbox = QSpinBox()
        self.max_history_spinbox.setRange(50, 5000)
        self.max_history_spinbox.setSingleStep(50)
        self.max_history_spinbox.setToolTip("设置最多保存多少条历史记录（建议 500-2000）")
        history_layout.addRow("最大历史记录数:", self.max_history_spinbox)
        layout.addWidget(history_group)

        layout.addStretch()
        return tab

    # ── Hotkeys Tab ──────────────────────────────────────────────────

    def _create_hotkeys_tab(self) -> QWidget:
        tab = QWidget()
        layout = QFormLayout(tab)
        layout.setSpacing(16)

        info_label = QLabel("点击按钮后按下快捷键组合来设置")
        info_label.setStyleSheet("color: #6B7280; font-style: italic;")
        layout.addRow(info_label)

        # Show window hotkey
        show_layout = QHBoxLayout()
        self.show_window_hotkey = QLineEdit()
        self.show_window_hotkey.setReadOnly(True)
        self.show_window_hotkey.setMaximumWidth(150)
        show_layout.addWidget(self.show_window_hotkey)
        self.show_window_btn = QPushButton("设置热键")
        self.show_window_btn.setMaximumWidth(100)
        self.show_window_btn.clicked.connect(lambda: self._start_hotkey_capture("show_window"))
        show_layout.addWidget(self.show_window_btn)
        show_layout.addStretch()
        layout.addRow("显示主窗口:", show_layout)

        # Clear history hotkey
        clear_layout = QHBoxLayout()
        self.clear_history_hotkey = QLineEdit()
        self.clear_history_hotkey.setReadOnly(True)
        self.clear_history_hotkey.setMaximumWidth(150)
        clear_layout.addWidget(self.clear_history_hotkey)
        self.clear_history_btn = QPushButton("设置热键")
        self.clear_history_btn.setMaximumWidth(100)
        self.clear_history_btn.clicked.connect(lambda: self._start_hotkey_capture("clear_history"))
        clear_layout.addWidget(self.clear_history_btn)
        clear_layout.addStretch()
        layout.addRow("清空历史记录:", clear_layout)

        # Search hotkey
        search_layout = QHBoxLayout()
        self.search_hotkey = QLineEdit()
        self.search_hotkey.setReadOnly(True)
        self.search_hotkey.setMaximumWidth(150)
        search_layout.addWidget(self.search_hotkey)
        self.search_btn = QPushButton("设置热键")
        self.search_btn.setMaximumWidth(100)
        self.search_btn.clicked.connect(lambda: self._start_hotkey_capture("search"))
        search_layout.addWidget(self.search_btn)
        search_layout.addStretch()
        layout.addRow("聚焦搜索:", search_layout)

        # Capture status label
        self.capture_status_label = QLabel("")
        self.capture_status_label.setStyleSheet("color: #F59E0B; font-size: 12px; margin-top: 8px;")
        layout.addRow(self.capture_status_label)

        note_label = QLabel("💡 提示: 更改热键后立即生效，无需重启")
        note_label.setStyleSheet("color: #10B981; font-size: 12px; margin-top: 16px;")
        layout.addRow(note_label)

        return tab

    def _start_hotkey_capture(self, hotkey_type: str):
        """Start capturing a hotkey for the specified type."""
        self._capturing_for = hotkey_type
        self.capture_status_label.setText("请按下快捷键组合... (按 Esc 取消)")

        # Update button text
        if hotkey_type == "show_window":
            self.show_window_btn.setText("捕获中...")
        elif hotkey_type == "clear_history":
            self.clear_history_btn.setText("捕获中...")
        elif hotkey_type == "search":
            self.search_btn.setText("捕获中...")

    def _stop_hotkey_capture(self):
        """Stop capturing hotkey."""
        self._capturing_for = None
        self.capture_status_label.setText("")
        self.show_window_btn.setText("设置热键")
        self.clear_history_btn.setText("设置热键")
        self.search_btn.setText("设置热键")

    # ── AI Prediction Tab ────────────────────────────────────────────

    def _create_ai_tab(self) -> QWidget:
        tab = QWidget()
        layout = QVBoxLayout(tab)
        layout.setSpacing(12)

        # --- Enable / description ---
        enable_group = QGroupBox("🤖 AI 智能预测")
        eg_layout = QVBoxLayout(enable_group)

        self.ai_enabled_cb = QCheckBox("启用 AI 智能预测")
        eg_layout.addWidget(self.ai_enabled_cb)

        desc = QLabel(
            "AI 将学习你的剪贴板记录，在你打字时预测并建议内容。\n"
            "按 Tab 键接受建议，按 Esc 键忽略。"
        )
        desc.setWordWrap(True)
        desc.setStyleSheet("color: #6B7280; font-size: 12px;")
        eg_layout.addWidget(desc)
        layout.addWidget(enable_group)

        # --- Provider / API configuration ---
        api_group = QGroupBox("🔗 Provider & API")
        api_layout = QFormLayout(api_group)

        self.ai_provider_combo = QComboBox()
        from services.ai_service import PROVIDERS
        self._provider_keys = list(PROVIDERS.keys())
        for key in self._provider_keys:
            self.ai_provider_combo.addItem(PROVIDERS[key]["display"], key)
        self.ai_provider_combo.currentIndexChanged.connect(self._on_provider_changed)
        api_layout.addRow("Provider:", self.ai_provider_combo)

        self.ai_api_url = QLineEdit()
        self.ai_api_url.setPlaceholderText("https://api.openai.com/v1")
        self._api_url_label = QLabel("API URL:")
        api_layout.addRow(self._api_url_label, self.ai_api_url)

        self.ai_api_key = QLineEdit()
        self.ai_api_key.setEchoMode(QLineEdit.EchoMode.Password)
        self.ai_api_key.setPlaceholderText("sk-...")
        self._api_key_label = QLabel("API Key:")
        api_layout.addRow(self._api_key_label, self.ai_api_key)

        self.ai_model = QLineEdit()
        self.ai_model.setPlaceholderText("gpt-3.5-turbo")
        api_layout.addRow("Model:", self.ai_model)

        self._pkg_status = QLabel("")
        self._pkg_status.setStyleSheet("font-size: 12px;")
        api_layout.addRow(self._pkg_status)

        layout.addWidget(api_group)

        # --- Prediction tuning ---
        tune_group = QGroupBox("⚡ 预测参数")
        tune_layout = QFormLayout(tune_group)

        self.ai_trigger_delay = QSpinBox()
        self.ai_trigger_delay.setRange(300, 5000)
        self.ai_trigger_delay.setSingleStep(100)
        self.ai_trigger_delay.setSuffix(" ms")
        self.ai_trigger_delay.setToolTip("停止打字多久后触发预测请求")
        tune_layout.addRow("触发延迟:", self.ai_trigger_delay)

        self.ai_max_tokens = QSpinBox()
        self.ai_max_tokens.setRange(10, 500)
        self.ai_max_tokens.setSingleStep(10)
        tune_layout.addRow("最大 Token:", self.ai_max_tokens)

        layout.addWidget(tune_group)

        # --- Test button ---
        btn_row = QHBoxLayout()
        self.test_ai_btn = QPushButton("🔍 测试连接")
        self.test_ai_btn.setFixedHeight(34)
        self.test_ai_btn.clicked.connect(self._test_ai_connection)
        btn_row.addWidget(self.test_ai_btn)
        btn_row.addStretch()
        layout.addLayout(btn_row)

        layout.addStretch()
        return tab

    def _on_provider_changed(self, index: int):
        from services.ai_service import PROVIDERS, check_provider_available, _langchain_available

        key = self.ai_provider_combo.currentData()
        cfg = PROVIDERS.get(key, {})

        # Toggle visibility of URL / Key fields
        self._api_url_label.setVisible(cfg.get("needs_url", True))
        self.ai_api_url.setVisible(cfg.get("needs_url", True))
        if cfg.get("url_hint"):
            self.ai_api_url.setPlaceholderText(cfg["url_hint"])

        self._api_key_label.setVisible(cfg.get("needs_key", True))
        self.ai_api_key.setVisible(cfg.get("needs_key", True))

        self.ai_model.setPlaceholderText(cfg.get("default_model", ""))

        # Check package availability
        ok, hint = check_provider_available(key)
        if ok:
            if cfg.get("direct_ok") and not _langchain_available():
                self._pkg_status.setText("✅ Ready (direct HTTP mode)")
            else:
                self._pkg_status.setText("✅ Ready")
            self._pkg_status.setStyleSheet("color: #0F7B0F; font-size: 12px;")
        else:
            self._pkg_status.setText(f"⚠️ Package missing — run: {hint}")
            self._pkg_status.setStyleSheet("color: #C42B1C; font-size: 12px;")

    def _test_ai_connection(self):
        """Save current AI fields into a temp config, create service, test."""
        self.test_ai_btn.setEnabled(False)
        self.test_ai_btn.setText("Testing…")

        # Temporarily save so AIService can read them
        self._persist_ai_fields()

        from services.ai_service import AIService
        svc = AIService()
        svc.test_result.connect(self._on_test_result)
        svc.test_connection()
        self._test_svc = svc  # prevent GC

    def _on_test_result(self, ok: bool, msg: str):
        self.test_ai_btn.setEnabled(True)
        self.test_ai_btn.setText("🔍 测试连接")
        if ok:
            QMessageBox.information(self, "连接测试", f"✅ {msg}")
        else:
            QMessageBox.warning(self, "连接测试", f"❌ {msg}")

    def _persist_ai_fields(self):
        """Write AI-related fields into Settings (used by test & save)."""
        ai = Settings.get("ai", {})
        ai["enabled"] = self.ai_enabled_cb.isChecked()
        ai["provider"] = self.ai_provider_combo.currentData()
        ai["api_url"] = self.ai_api_url.text().strip()
        ai["api_key"] = self.ai_api_key.text().strip()
        ai["model"] = self.ai_model.text().strip() or self.ai_model.placeholderText()
        ai["trigger_delay"] = self.ai_trigger_delay.value()
        ai["max_tokens"] = self.ai_max_tokens.value()
        Settings.set("ai", ai)

    # ── Advanced Tab ─────────────────────────────────────────────────

    def _create_advanced_tab(self) -> QWidget:
        tab = QWidget()
        layout = QVBoxLayout(tab)
        layout.setSpacing(16)

        data_group = QGroupBox("💾 数据管理")
        data_layout = QFormLayout(data_group)
        retention_layout = QHBoxLayout()
        self.retention_days_spinbox = QSpinBox()
        self.retention_days_spinbox.setRange(0, 365)
        self.retention_days_spinbox.setSingleStep(1)
        self.retention_days_spinbox.setSuffix(" 天")
        self.retention_days_spinbox.setToolTip("设置历史记录的保留天数，0表示永久保留")
        retention_layout.addWidget(self.retention_days_spinbox)
        retention_note = QLabel("(0 = 永久保留)")
        retention_note.setStyleSheet("color: #9CA3AF; font-size: 12px;")
        retention_layout.addWidget(retention_note)
        retention_layout.addStretch()
        data_layout.addRow("历史记录保留:", retention_layout)
        cleanup_note = QLabel("超过保留天数的非收藏记录将被自动清理")
        cleanup_note.setStyleSheet("color: #6B7280; font-size: 12px;")
        data_layout.addRow(cleanup_note)
        layout.addWidget(data_group)

        perf_group = QGroupBox("⚡ 性能")
        perf_layout = QFormLayout(perf_group)
        self.display_limit_spinbox = QSpinBox()
        self.display_limit_spinbox.setRange(50, 500)
        self.display_limit_spinbox.setSingleStep(10)
        self.display_limit_spinbox.setSuffix(" 条")
        self.display_limit_spinbox.setToolTip("限制列表一次显示的记录数量，提高性能")
        perf_layout.addRow("列表显示限制:", self.display_limit_spinbox)
        layout.addWidget(perf_group)

        layout.addStretch()
        return tab

    # ── Load / Save ──────────────────────────────────────────────────

    def _load_settings(self):
        try:
            self.dark_mode_checkbox.setChecked(Settings.get("dark_mode", False))
            self.startup_checkbox.setChecked(Settings.get("startup", True))
            self.minimize_to_tray_checkbox.setChecked(Settings.get("minimize_to_tray", False))
            self.max_history_spinbox.setValue(Settings.get("max_history", 1000))

            hotkeys = Settings.get("hotkeys", {})
            self.show_window_hotkey.setText(hotkeys.get("show_window", "Ctrl+O"))
            self.clear_history_hotkey.setText(hotkeys.get("clear_history", "Ctrl+Shift+C"))
            self.search_hotkey.setText(hotkeys.get("search", "Ctrl+F"))

            self.retention_days_spinbox.setValue(Settings.get("retention_days", 30))
            self.display_limit_spinbox.setValue(Settings.get("display_limit", 100))

            # AI settings
            ai = Settings.get("ai", {})
            self.ai_enabled_cb.setChecked(ai.get("enabled", False))

            provider = ai.get("provider", "openai")
            idx = self._provider_keys.index(provider) if provider in self._provider_keys else 0
            self.ai_provider_combo.setCurrentIndex(idx)
            self._on_provider_changed(idx)

            self.ai_api_url.setText(ai.get("api_url", ""))
            self.ai_api_key.setText(ai.get("api_key", ""))
            self.ai_model.setText(ai.get("model", ""))
            self.ai_trigger_delay.setValue(ai.get("trigger_delay", 800))
            self.ai_max_tokens.setValue(ai.get("max_tokens", 100))
        except Exception as e:
            logger.error(f"加载设置时发生错误: {str(e)}")

    def _save_settings(self):
        try:
            Settings.set("dark_mode", self.dark_mode_checkbox.isChecked())

            startup_changed = Settings.get("startup", True) != self.startup_checkbox.isChecked()
            if startup_changed:
                success = StartupManager.set_startup(self.startup_checkbox.isChecked())
                if not success:
                    QMessageBox.warning(self, "设置开机自启动失败",
                                        "无法设置开机自启动，请检查系统权限。")

            Settings.set("startup", self.startup_checkbox.isChecked())
            Settings.set("minimize_to_tray", self.minimize_to_tray_checkbox.isChecked())
            Settings.set("max_history", self.max_history_spinbox.value())

            hotkeys = Settings.get("hotkeys", {})
            hotkeys["show_window"] = self.show_window_hotkey.text() or "Ctrl+O"
            hotkeys["clear_history"] = self.clear_history_hotkey.text() or "Ctrl+Shift+C"
            hotkeys["search"] = self.search_hotkey.text() or "Ctrl+F"
            Settings.set("hotkeys", hotkeys)

            Settings.set("retention_days", self.retention_days_spinbox.value())
            Settings.set("display_limit", self.display_limit_spinbox.value())

            # AI settings
            self._persist_ai_fields()

            self.settingsChanged.emit()
            self.aiSettingsChanged.emit()
            self.accept()
        except Exception as e:
            logger.error(f"保存设置时发生错误: {str(e)}")
            QMessageBox.critical(self, "保存设置失败",
                                 f"保存设置时发生错误: {str(e)}")

    # ── Hotkey capture ───────────────────────────────────────────────

    def eventFilter(self, obj, event):
        """Filter events for hotkey capture."""
        # Check if we're in capture mode and this is a key press on the dialog
        if self._capturing_for and event.type() == QEvent.Type.KeyPress:
            key_event = event
            key = key_event.key()

            # Escape cancels capture
            if key == Qt.Key.Key_Escape:
                self._stop_hotkey_capture()
                return True

            # Ignore modifier-only keys
            if key in (Qt.Key.Key_Control, Qt.Key.Key_Shift,
                       Qt.Key.Key_Alt, Qt.Key.Key_Meta):
                return True

            # Check for modifiers
            modifiers = key_event.modifiers()
            has_modifier = bool(modifiers & (Qt.KeyboardModifier.ControlModifier |
                                             Qt.KeyboardModifier.AltModifier |
                                             Qt.KeyboardModifier.ShiftModifier |
                                             Qt.KeyboardModifier.MetaModifier))

            if not has_modifier:
                self.capture_status_label.setText("热键必须包含 Ctrl、Alt、Shift 或 Win 键")
                QTimer.singleShot(2000, lambda: self.capture_status_label.setText("请按下快捷键组合... (按 Esc 取消)"))
                return True

            # Get the hotkey string
            try:
                key_string = self._get_key_string(key_event)
                if key_string and "+" in key_string:
                    # Set the captured hotkey
                    if self._capturing_for == "show_window":
                        self.show_window_hotkey.setText(key_string)
                    elif self._capturing_for == "clear_history":
                        self.clear_history_hotkey.setText(key_string)
                    elif self._capturing_for == "search":
                        self.search_hotkey.setText(key_string)

                    self.capture_status_label.setText(f"已捕获: {key_string}")
                    logger.info(f"Captured hotkey: {key_string} for {self._capturing_for}")
                else:
                    self.capture_status_label.setText("无法识别的按键组合")
            except Exception as e:
                logger.error(f"Error capturing hotkey: {e}")
                self.capture_status_label.setText("捕获失败")

            self._stop_hotkey_capture()
            return True

        return super().eventFilter(obj, event)

    def _get_key_string(self, event) -> str:
        """Convert key event to hotkey string like 'Ctrl+Shift+A'."""
        parts = []

        # Add modifiers
        if event.modifiers() & Qt.KeyboardModifier.ControlModifier:
            parts.append("Ctrl")
        if event.modifiers() & Qt.KeyboardModifier.AltModifier:
            parts.append("Alt")
        if event.modifiers() & Qt.KeyboardModifier.ShiftModifier:
            parts.append("Shift")
        if event.modifiers() & Qt.KeyboardModifier.MetaModifier:
            parts.append("Meta")

        # Add key name
        key = event.key()
        key_map = {
            Qt.Key.Key_A: "A", Qt.Key.Key_B: "B", Qt.Key.Key_C: "C",
            Qt.Key.Key_D: "D", Qt.Key.Key_E: "E", Qt.Key.Key_F: "F",
            Qt.Key.Key_G: "G", Qt.Key.Key_H: "H", Qt.Key.Key_I: "I",
            Qt.Key.Key_J: "J", Qt.Key.Key_K: "K", Qt.Key.Key_L: "L",
            Qt.Key.Key_M: "M", Qt.Key.Key_N: "N", Qt.Key.Key_O: "O",
            Qt.Key.Key_P: "P", Qt.Key.Key_Q: "Q", Qt.Key.Key_R: "R",
            Qt.Key.Key_S: "S", Qt.Key.Key_T: "T", Qt.Key.Key_U: "U",
            Qt.Key.Key_V: "V", Qt.Key.Key_W: "W", Qt.Key.Key_X: "X",
            Qt.Key.Key_Y: "Y", Qt.Key.Key_Z: "Z",
            Qt.Key.Key_0: "0", Qt.Key.Key_1: "1", Qt.Key.Key_2: "2",
            Qt.Key.Key_3: "3", Qt.Key.Key_4: "4", Qt.Key.Key_5: "5",
            Qt.Key.Key_6: "6", Qt.Key.Key_7: "7", Qt.Key.Key_8: "8",
            Qt.Key.Key_9: "9",
            Qt.Key.Key_F1: "F1", Qt.Key.Key_F2: "F2", Qt.Key.Key_F3: "F3",
            Qt.Key.Key_F4: "F4", Qt.Key.Key_F5: "F5", Qt.Key.Key_F6: "F6",
            Qt.Key.Key_F7: "F7", Qt.Key.Key_F8: "F8", Qt.Key.Key_F9: "F9",
            Qt.Key.Key_F10: "F10", Qt.Key.Key_F11: "F11", Qt.Key.Key_F12: "F12",
            Qt.Key.Key_Space: "Space",
            Qt.Key.Key_Return: "Return",
            Qt.Key.Key_Enter: "Enter",
            Qt.Key.Key_Tab: "Tab",
            Qt.Key.Key_Escape: "Escape",
            Qt.Key.Key_Backspace: "Backspace",
            Qt.Key.Key_Delete: "Delete",
            Qt.Key.Key_Insert: "Insert",
            Qt.Key.Key_Home: "Home",
            Qt.Key.Key_End: "End",
            Qt.Key.Key_PageUp: "PageUp",
            Qt.Key.Key_PageDown: "PageDown",
            Qt.Key.Key_Up: "Up",
            Qt.Key.Key_Down: "Down",
            Qt.Key.Key_Left: "Left",
            Qt.Key.Key_Right: "Right",
            Qt.Key.Key_Comma: ",",
            Qt.Key.Key_Period: ".",
            Qt.Key.Key_Slash: "/",
            Qt.Key.Key_Semicolon: ";",
            Qt.Key.Key_Apostrophe: "'",
            Qt.Key.Key_BracketLeft: "[",
            Qt.Key.Key_BracketRight: "]",
            Qt.Key.Key_Backslash: "\\",
            Qt.Key.Key_Minus: "-",
            Qt.Key.Key_Equal: "=",
            Qt.Key.Key_QuoteLeft: "`",  # Backtick/Grave
            Qt.Key.Key_AsciiTilde: "~",
            Qt.Key.Key_Plus: "+",
            Qt.Key.Key_Asterisk: "*",
        }

        if key in key_map:
            parts.append(key_map[key])
        else:
            # Try to get key from QKeySequence
            seq = QKeySequence(key)
            txt = seq.toString()
            if txt and txt not in parts:
                parts.append(txt)

        return "+".join(parts) if len(parts) > 1 else ""
