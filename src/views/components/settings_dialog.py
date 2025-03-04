from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QLabel, 
    QPushButton, QCheckBox, QSpinBox, QTabWidget,
    QWidget, QFormLayout, QLineEdit, QGroupBox,
    QDialogButtonBox, QMessageBox
)
from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtGui import QKeySequence

from config.settings import Settings
from utils.logger import logger
from utils.startup import StartupManager

class SettingsDialog(QDialog):
    """设置对话框"""
    
    settingsChanged = pyqtSignal()
    
    def __init__(self, parent=None):
        super().__init__(parent)
        self._init_ui()
        self._load_settings()
    
    def _init_ui(self):
        """初始化UI"""
        self.setWindowTitle("设置")
        self.setMinimumWidth(400)
        self.setMinimumHeight(300)
        
        # 主布局
        layout = QVBoxLayout(self)
        
        # 创建标签页
        tab_widget = QTabWidget()
        
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
        
        # 外观设置
        appearance_group = QGroupBox("外观")
        appearance_layout = QVBoxLayout(appearance_group)
        
        # 暗色模式
        self.dark_mode_checkbox = QCheckBox("暗色模式")
        self.dark_mode_checkbox.setToolTip("启用暗色主题")
        appearance_layout.addWidget(self.dark_mode_checkbox)
        
        layout.addWidget(appearance_group)
        
        # 启动设置
        startup_group = QGroupBox("启动")
        startup_layout = QVBoxLayout(startup_group)
        
        # 开机自启动
        self.startup_checkbox = QCheckBox("开机自启动")
        self.startup_checkbox.setToolTip("系统启动时自动运行程序")
        startup_layout.addWidget(self.startup_checkbox)
        
        layout.addWidget(startup_group)
        
        # 历史记录设置
        history_group = QGroupBox("历史记录")
        history_layout = QFormLayout(history_group)
        
        # 最大历史记录数
        self.max_history_spinbox = QSpinBox()
        self.max_history_spinbox.setRange(10, 1000)
        self.max_history_spinbox.setSingleStep(10)
        self.max_history_spinbox.setToolTip("设置要保存的最大历史记录数量")
        history_layout.addRow("最大历史记录数:", self.max_history_spinbox)
        
        layout.addWidget(history_group)
        
        # 添加弹性空间
        layout.addStretch()
        
        return tab
    
    def _create_hotkeys_tab(self) -> QWidget:
        """创建热键设置标签页"""
        tab = QWidget()
        layout = QFormLayout(tab)
        
        # 显示窗口热键
        self.show_window_hotkey = QLineEdit()
        self.show_window_hotkey.setPlaceholderText("点击此处，然后按下快捷键")
        self.show_window_hotkey.setToolTip("设置显示主窗口的快捷键")
        layout.addRow("显示主窗口:", self.show_window_hotkey)
        
        # 清空历史记录热键
        self.clear_history_hotkey = QLineEdit()
        self.clear_history_hotkey.setPlaceholderText("点击此处，然后按下快捷键")
        self.clear_history_hotkey.setToolTip("设置清空历史记录的快捷键")
        layout.addRow("清空历史记录:", self.clear_history_hotkey)
        
        # 热键说明
        note_label = QLabel("注意: 更改热键后需要重启应用程序才能生效")
        note_label.setStyleSheet("color: #666; font-style: italic;")
        layout.addRow("", note_label)
        
        return tab
    
    def _create_advanced_tab(self) -> QWidget:
        """创建高级设置标签页"""
        tab = QWidget()
        layout = QFormLayout(tab)
        
        # 自动保存间隔
        self.auto_save_spinbox = QSpinBox()
        self.auto_save_spinbox.setRange(10, 600)
        self.auto_save_spinbox.setSingleStep(10)
        self.auto_save_spinbox.setSuffix(" 秒")
        self.auto_save_spinbox.setToolTip("设置自动保存历史记录的时间间隔")
        layout.addRow("自动保存间隔:", self.auto_save_spinbox)
        
        # 历史记录保留天数
        self.retention_days_spinbox = QSpinBox()
        self.retention_days_spinbox.setRange(0, 365)
        self.retention_days_spinbox.setSingleStep(1)
        self.retention_days_spinbox.setSuffix(" 天")
        self.retention_days_spinbox.setToolTip("设置历史记录的保留天数，0表示永久保留")
        layout.addRow("历史记录保留:", self.retention_days_spinbox)
        
        # 保留天数说明
        note_label = QLabel("注意: 超过保留天数的历史记录将被自动删除，设置为0表示永久保留")
        note_label.setStyleSheet("color: #666; font-style: italic;")
        layout.addRow("", note_label)
        
        return tab
    
    def _load_settings(self):
        """加载设置"""
        try:
            # 常规设置
            self.dark_mode_checkbox.setChecked(Settings.get("dark_mode", False))
            self.startup_checkbox.setChecked(Settings.get("startup", True))
            self.max_history_spinbox.setValue(Settings.get("max_history", 100))
            
            # 热键设置
            hotkeys = Settings.get("hotkeys", {})
            self.show_window_hotkey.setText(hotkeys.get("show_window", "Ctrl+O"))
            self.clear_history_hotkey.setText(hotkeys.get("clear_history", "Ctrl+Shift+C"))
            
            # 高级设置
            self.auto_save_spinbox.setValue(Settings.get("auto_save_interval", 60))
            self.retention_days_spinbox.setValue(Settings.get("retention_days", 10))
            
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
            Settings.set("max_history", self.max_history_spinbox.value())
            
            # 热键设置
            hotkeys = Settings.get("hotkeys", {})
            hotkeys["show_window"] = self.show_window_hotkey.text()
            hotkeys["clear_history"] = self.clear_history_hotkey.text()
            Settings.set("hotkeys", hotkeys)
            
            # 高级设置
            Settings.set("auto_save_interval", self.auto_save_spinbox.value())
            Settings.set("retention_days", self.retention_days_spinbox.value())
            
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
        # 检查当前焦点是否在热键输入框上
        focused_widget = self.focusWidget()
        if isinstance(focused_widget, QLineEdit) and focused_widget in [self.show_window_hotkey, self.clear_history_hotkey]:
            # 创建键序列
            key_sequence = QKeySequence(event.key() | int(event.modifiers()))
            # 设置文本
            focused_widget.setText(key_sequence.toString())
            return
            
        # 默认处理
        super().keyPressEvent(event) 