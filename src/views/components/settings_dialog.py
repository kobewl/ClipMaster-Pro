from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QLabel, 
    QPushButton, QCheckBox, QSpinBox, QTabWidget,
    QWidget, QFormLayout, QLineEdit, QGroupBox,
    QDialogButtonBox, QMessageBox, QComboBox,
    QSlider
)
from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtGui import QKeySequence

from config.settings import Settings
from utils.logger import logger
from utils.startup import StartupManager


class SettingsDialog(QDialog):
    """优化的设置对话框"""
    
    settingsChanged = pyqtSignal()
    
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("设置")
        self.setMinimumWidth(450)
        self.setMinimumHeight(400)
        self._init_ui()
        self._load_settings()
    
    def _init_ui(self):
        """初始化UI"""
        layout = QVBoxLayout(self)
        layout.setSpacing(16)
        layout.setContentsMargins(20, 20, 20, 20)
        
        # 创建标签页
        tab_widget = QTabWidget()
        tab_widget.setDocumentMode(True)
        
        # 常规设置标签页
        general_tab = self._create_general_tab()
        tab_widget.addTab(general_tab, "常规")
        
        # 热键设置标签页
        hotkeys_tab = self._create_hotkeys_tab()
        tab_widget.addTab(hotkeys_tab, "热键")
        
        # 高级设置标签页
        advanced_tab = self._create_advanced_tab()
        tab_widget.addTab(advanced_tab, "高级")
        
        layout.addWidget(tab_widget)
        
        # 按钮
        button_box = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok | 
            QDialogButtonBox.StandardButton.Cancel
        )
        button_box.accepted.connect(self._save_settings)
        button_box.rejected.connect(self.reject)
        layout.addWidget(button_box)
    
    def _create_general_tab(self) -> QWidget:
        """创建常规设置标签页"""
        tab = QWidget()
        layout = QVBoxLayout(tab)
        layout.setSpacing(16)
        
        # 外观设置组
        appearance_group = QGroupBox("🎨 外观")
        appearance_layout = QVBoxLayout(appearance_group)
        
        self.dark_mode_checkbox = QCheckBox("启用暗色模式")
        self.dark_mode_checkbox.setToolTip("切换应用程序的主题颜色")
        appearance_layout.addWidget(self.dark_mode_checkbox)
        
        layout.addWidget(appearance_group)
        
        # 启动设置组
        startup_group = QGroupBox("🚀 启动")
        startup_layout = QVBoxLayout(startup_group)
        
        self.startup_checkbox = QCheckBox("开机自启动")
        self.startup_checkbox.setToolTip("系统启动时自动运行程序")
        startup_layout.addWidget(self.startup_checkbox)
        
        self.minimize_to_tray_checkbox = QCheckBox("启动时最小化到托盘")
        self.minimize_to_tray_checkbox.setToolTip("程序启动时不显示主窗口")
        startup_layout.addWidget(self.minimize_to_tray_checkbox)
        
        layout.addWidget(startup_group)
        
        # 历史记录设置组
        history_group = QGroupBox("📋 历史记录")
        history_layout = QFormLayout(history_group)
        
        self.max_history_spinbox = QSpinBox()
        self.max_history_spinbox.setRange(50, 5000)
        self.max_history_spinbox.setSingleStep(50)
        self.max_history_spinbox.setToolTip("设置最多保存多少条历史记录（建议 500-2000）")
        history_layout.addRow("最大历史记录数:", self.max_history_spinbox)
        
        layout.addWidget(history_group)
        
        # 添加弹性空间
        layout.addStretch()
        
        return tab
    
    def _create_hotkeys_tab(self) -> QWidget:
        """创建热键设置标签页"""
        tab = QWidget()
        layout = QFormLayout(tab)
        layout.setSpacing(16)
        
        # 说明标签
        info_label = QLabel("点击输入框后按下快捷键组合即可设置")
        info_label.setStyleSheet("color: #6B7280; font-style: italic;")
        layout.addRow(info_label)
        
        # 显示窗口热键
        self.show_window_hotkey = QLineEdit()
        self.show_window_hotkey.setPlaceholderText("点击此处，然后按下快捷键")
        self.show_window_hotkey.setToolTip("设置显示/隐藏主窗口的快捷键")
        layout.addRow("显示主窗口:", self.show_window_hotkey)
        
        # 清空历史记录热键
        self.clear_history_hotkey = QLineEdit()
        self.clear_history_hotkey.setPlaceholderText("点击此处，然后按下快捷键")
        self.clear_history_hotkey.setToolTip("设置清空历史记录的快捷键")
        layout.addRow("清空历史记录:", self.clear_history_hotkey)
        
        # 搜索热键
        self.search_hotkey = QLineEdit()
        self.search_hotkey.setPlaceholderText("点击此处，然后按下快捷键")
        self.search_hotkey.setToolTip("设置聚焦搜索框的快捷键")
        layout.addRow("聚焦搜索:", self.search_hotkey)
        
        # 热键说明
        note_label = QLabel("💡 提示: 更改热键后需要重启应用程序才能生效")
        note_label.setStyleSheet("color: #F59E0B; font-size: 12px; margin-top: 16px;")
        layout.addRow(note_label)
        
        return tab
    
    def _create_advanced_tab(self) -> QWidget:
        """创建高级设置标签页"""
        tab = QWidget()
        layout = QVBoxLayout(tab)
        layout.setSpacing(16)
        
        # 数据管理组
        data_group = QGroupBox("💾 数据管理")
        data_layout = QFormLayout(data_group)
        
        # 历史记录保留天数
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
        
        # 自动清理说明
        cleanup_note = QLabel("超过保留天数的非收藏记录将被自动清理")
        cleanup_note.setStyleSheet("color: #6B7280; font-size: 12px;")
        data_layout.addRow(cleanup_note)
        
        layout.addWidget(data_group)
        
        # 性能设置组
        perf_group = QGroupBox("⚡ 性能")
        perf_layout = QFormLayout(perf_group)
        
        # 列表显示数量限制
        self.display_limit_spinbox = QSpinBox()
        self.display_limit_spinbox.setRange(50, 500)
        self.display_limit_spinbox.setSingleStep(10)
        self.display_limit_spinbox.setSuffix(" 条")
        self.display_limit_spinbox.setToolTip("限制列表一次显示的记录数量，提高性能")
        perf_layout.addRow("列表显示限制:", self.display_limit_spinbox)
        
        layout.addWidget(perf_group)
        
        # 添加弹性空间
        layout.addStretch()
        
        return tab
    
    def _load_settings(self):
        """加载设置"""
        try:
            # 常规设置
            self.dark_mode_checkbox.setChecked(Settings.get("dark_mode", False))
            self.startup_checkbox.setChecked(Settings.get("startup", True))
            self.minimize_to_tray_checkbox.setChecked(Settings.get("minimize_to_tray", False))
            self.max_history_spinbox.setValue(Settings.get("max_history", 1000))
            
            # 热键设置
            hotkeys = Settings.get("hotkeys", {})
            self.show_window_hotkey.setText(hotkeys.get("show_window", "Ctrl+O"))
            self.clear_history_hotkey.setText(hotkeys.get("clear_history", "Ctrl+Shift+C"))
            self.search_hotkey.setText(hotkeys.get("search", "Ctrl+F"))
            
            # 高级设置
            self.retention_days_spinbox.setValue(Settings.get("retention_days", 30))
            self.display_limit_spinbox.setValue(Settings.get("display_limit", 100))
            
        except Exception as e:
            logger.error(f"加载设置时发生错误: {str(e)}")
    
    def _save_settings(self):
        """保存设置"""
        try:
            # 常规设置
            Settings.set("dark_mode", self.dark_mode_checkbox.isChecked())
            
            # 设置开机自启动
            startup_changed = Settings.get("startup", True) != self.startup_checkbox.isChecked()
            if startup_changed:
                success = StartupManager.set_startup(self.startup_checkbox.isChecked())
                if not success:
                    QMessageBox.warning(
                        self,
                        "设置开机自启动失败",
                        "无法设置开机自启动，请检查系统权限。"
                    )
            
            Settings.set("startup", self.startup_checkbox.isChecked())
            Settings.set("minimize_to_tray", self.minimize_to_tray_checkbox.isChecked())
            Settings.set("max_history", self.max_history_spinbox.value())
            
            # 热键设置
            hotkeys = Settings.get("hotkeys", {})
            hotkeys["show_window"] = self.show_window_hotkey.text() or "Ctrl+O"
            hotkeys["clear_history"] = self.clear_history_hotkey.text() or "Ctrl+Shift+C"
            hotkeys["search"] = self.search_hotkey.text() or "Ctrl+F"
            Settings.set("hotkeys", hotkeys)
            
            # 高级设置
            Settings.set("retention_days", self.retention_days_spinbox.value())
            Settings.set("display_limit", self.display_limit_spinbox.value())
            
            # 发送设置已更改信号
            self.settingsChanged.emit()
            
            # 关闭对话框
            self.accept()
            
        except Exception as e:
            logger.error(f"保存设置时发生错误: {str(e)}")
            QMessageBox.critical(
                self,
                "保存设置失败",
                f"保存设置时发生错误: {str(e)}"
            )
    
    def keyPressEvent(self, event):
        """处理键盘事件，用于捕获热键"""
        focused_widget = self.focusWidget()
        hotkey_inputs = [
            self.show_window_hotkey,
            self.clear_history_hotkey,
            self.search_hotkey
        ]
        
        if isinstance(focused_widget, QLineEdit) and focused_widget in hotkey_inputs:
            # 忽略单独的修饰键
            if event.key() in (Qt.Key.Key_Control, Qt.Key.Key_Shift, 
                              Qt.Key.Key_Alt, Qt.Key.Key_Meta):
                return
            
            # 创建键序列
            key_sequence = QKeySequence(event.key() | event.modifiers().value)
            key_string = key_sequence.toString()
            
            if key_string:
                focused_widget.setText(key_string)
            return
        
        super().keyPressEvent(event)