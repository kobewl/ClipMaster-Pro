from PyQt6.QtWidgets import QSystemTrayIcon, QMenu, QStyle, QApplication, QAction
from PyQt6.QtCore import pyqtSignal
from config.settings import Settings

class TrayIcon(QSystemTrayIcon):
    """系统托盘图标组件"""
    
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
        # 设置图标 - 从应用程序获取样式
        app_style = QApplication.style()
        self.setIcon(app_style.standardIcon(QStyle.StandardPixmap.SP_DialogSaveButton))
        
        # 创建菜单
        menu = QMenu()
        
        # 显示主窗口
        show_action = QAction("显示主窗口", menu)
        show_action.triggered.connect(self.showWindowRequested.emit)
        menu.addAction(show_action)
        
        # 清空历史记录
        clear_action = QAction("清空历史记录", menu)
        clear_action.triggered.connect(self.clearHistoryRequested.emit)
        menu.addAction(clear_action)
        
        # 数据管理
        data_action = QAction("数据管理", menu)
        data_action.triggered.connect(self.dataManagementRequested.emit)
        menu.addAction(data_action)
        
        # 切换主题
        theme_action = QAction("切换主题", menu)
        theme_action.triggered.connect(self.toggleThemeRequested.emit)
        menu.addAction(theme_action)
        
        # 设置
        settings_action = QAction("设置", menu)
        settings_action.triggered.connect(self.settingsRequested.emit)
        menu.addAction(settings_action)
        
        # 分隔线
        menu.addSeparator()
        
        # 关于信息
        about_action = QAction(f"关于 {Settings.APP_NAME} v{Settings.APP_VERSION}", menu)
        about_action.triggered.connect(self._show_about)
        menu.addAction(about_action)
        
        # 分隔线
        menu.addSeparator()
        
        # 退出
        quit_action = QAction("退出", menu)
        quit_action.triggered.connect(self.quitRequested.emit)
        menu.addAction(quit_action)
        
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
    
    def _show_about(self):
        """显示关于信息"""
        self.showMessage(
            f"关于 {Settings.APP_NAME}",
            f"版本: v{Settings.APP_VERSION}\n"
            f"一个简单高效的剪贴板管理工具\n"
            f"使用 PyQt6 构建",
            QSystemTrayIcon.MessageIcon.Information,
            5000
        ) 