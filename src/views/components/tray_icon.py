from PyQt6.QtWidgets import QSystemTrayIcon, QMenu, QStyle, QApplication
from PyQt6.QtCore import pyqtSignal
from config.settings import Settings

class TrayIcon(QSystemTrayIcon):
    """系统托盘图标组件"""
    
    showWindowRequested = pyqtSignal()
    quitRequested = pyqtSignal()
    
    def __init__(self, parent=None):
        super().__init__(parent)
        self._init_ui()
    
    def _init_ui(self):
        """初始化UI"""
        # 设置图标 - 从应用程序获取样式
        app_style = QApplication.style()
        self.setIcon(app_style.standardIcon(QStyle.StandardPixmap.SP_DialogSaveButton))
        
        # 创建菜单
        menu = QMenu()
        show_action = menu.addAction("显示主窗口")
        show_action.triggered.connect(self.showWindowRequested.emit)
        menu.addSeparator()
        quit_action = menu.addAction("退出")
        quit_action.triggered.connect(self.quitRequested.emit)
        
        self.setContextMenu(menu)
        self.activated.connect(self._handle_activation)
        
        # 显示欢迎消息
        self.show()
        self.showMessage(
            Settings.APP_NAME,
            "程序已在后台运行\n使用 Ctrl+O 打开主窗口",
            QSystemTrayIcon.MessageIcon.Information,
            2000
        )
    
    def _handle_activation(self, reason):
        """处理托盘图标激活"""
        if reason == QSystemTrayIcon.ActivationReason.DoubleClick:
            self.showWindowRequested.emit() 