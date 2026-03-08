from PyQt6.QtWidgets import QSystemTrayIcon, QMenu, QStyle, QApplication
from PyQt6.QtCore import pyqtSignal
from PyQt6.QtGui import QIcon, QPixmap, QPainter, QColor, QFont, QAction
from config.settings import Settings
from utils.logger import logger


class TrayIcon(QSystemTrayIcon):
    """优化的系统托盘图标组件"""
    
    showWindowRequested = pyqtSignal()
    quitRequested = pyqtSignal()
    clearHistoryRequested = pyqtSignal()
    toggleThemeRequested = pyqtSignal()
    settingsRequested = pyqtSignal()
    dataManagementRequested = pyqtSignal()
    
    def __init__(self, parent=None):
        super().__init__(parent)
        self._init_ui()
    
    def _init_ui(self):
        """初始化UI"""
        # 创建自定义图标
        self.setIcon(self._create_icon())
        
        # 创建菜单
        menu = QMenu()
        menu.setStyleSheet("""
            QMenu {
                background-color: white;
                border: 1px solid #E5E7EB;
                border-radius: 8px;
                padding: 8px;
            }
            QMenu::item {
                padding: 10px 24px;
                border-radius: 6px;
                font-size: 13px;
            }
            QMenu::item:selected {
                background-color: #EEF2FF;
                color: #4F46E5;
            }
            QMenu::separator {
                height: 1px;
                background-color: #E5E7EB;
                margin: 8px 0;
            }
        """)
        
        # 显示主窗口
        show_action = QAction("📋 显示主窗口", menu)
        show_action.triggered.connect(self.showWindowRequested.emit)
        menu.addAction(show_action)
        
        menu.addSeparator()
        
        # 清空历史记录
        clear_action = QAction("🗑️ 清空历史记录", menu)
        clear_action.triggered.connect(self.clearHistoryRequested.emit)
        menu.addAction(clear_action)
        
        # 数据管理
        data_action = QAction("📊 数据管理", menu)
        data_action.triggered.connect(self.dataManagementRequested.emit)
        menu.addAction(data_action)
        
        menu.addSeparator()
        
        # 切换主题
        theme_action = QAction("🌙 切换主题", menu)
        theme_action.triggered.connect(self.toggleThemeRequested.emit)
        menu.addAction(theme_action)
        
        # 设置
        settings_action = QAction("⚙️ 设置", menu)
        settings_action.triggered.connect(self.settingsRequested.emit)
        menu.addAction(settings_action)
        
        menu.addSeparator()
        
        # 关于信息
        about_action = QAction(f"ℹ️ 关于 {Settings.APP_NAME}", menu)
        about_action.triggered.connect(self._show_about)
        menu.addAction(about_action)
        
        menu.addSeparator()
        
        # 退出
        quit_action = QAction("✕ 退出", menu)
        quit_action.triggered.connect(self.quitRequested.emit)
        menu.addAction(quit_action)
        
        self.setContextMenu(menu)
        self.activated.connect(self._handle_activation)
        
        # 显示托盘图标
        self.show()
        
        # 显示欢迎消息（延迟显示，避免启动时干扰）
        from PyQt6.QtCore import QTimer
        QTimer.singleShot(1000, self._show_welcome)
    
    def _create_icon(self) -> QIcon:
        """创建自定义托盘图标"""
        # 创建一个简单的剪贴板图标
        pixmap = QPixmap(64, 64)
        pixmap.fill(QColor("transparent"))
        
        painter = QPainter(pixmap)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        
        # 绘制背景圆角矩形
        painter.setBrush(QColor("#4F46E5"))
        painter.setPen(QColor("#4F46E5"))
        painter.drawRoundedRect(4, 4, 56, 56, 12, 12)
        
        # 绘制剪贴板图标
        painter.setPen(QColor("white"))
        painter.setBrush(QColor("white"))
        
        # 绘制剪贴板形状
        painter.drawRoundedRect(18, 16, 28, 32, 4, 4)
        painter.setBrush(QColor("#4F46E5"))
        painter.drawRoundedRect(24, 12, 16, 8, 2, 2)
        
        # 绘制线条表示文本
        painter.setPen(QColor("#4F46E5"))
        painter.drawLine(22, 26, 42, 26)
        painter.drawLine(22, 32, 38, 32)
        painter.drawLine(22, 38, 40, 38)
        
        painter.end()
        
        return QIcon(pixmap)
    
    def _show_welcome(self):
        """显示欢迎消息"""
        try:
            hotkey = Settings.get("hotkeys", {}).get(
                "show_window", Settings.DEFAULT_HOTKEYS["show_window"]
            )
            self.showMessage(
                f"✅ {Settings.APP_NAME} 已启动",
                f"程序已在后台运行\n使用 {hotkey} 快速打开主窗口",
                QSystemTrayIcon.MessageIcon.Information,
                3000
            )
        except Exception as e:
            logger.error(f"显示欢迎消息时发生错误: {str(e)}")
    
    def _handle_activation(self, reason):
        """处理托盘图标激活"""
        if reason == QSystemTrayIcon.ActivationReason.DoubleClick:
            self.showWindowRequested.emit()
    
    def _show_about(self):
        """显示关于信息"""
        self.showMessage(
            f"ℹ️ 关于 {Settings.APP_NAME}",
            f"版本: v{Settings.APP_VERSION}\n"
            f"一个高效、现代的剪贴板管理工具\n"
            f"支持文本、图片、文件等多种格式\n"
            f"使用 PyQt6 构建",
            QSystemTrayIcon.MessageIcon.Information,
            5000
        )
    
    def show_notification(self, title: str, message: str, duration: int = 2000):
        """显示通知"""
        self.showMessage(
            title,
            message,
            QSystemTrayIcon.MessageIcon.Information,
            duration
        )
