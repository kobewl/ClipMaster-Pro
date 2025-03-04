from PyQt6.QtWidgets import (
    QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
    QPushButton, QLabel, QFrame, QApplication
)
from PyQt6.QtCore import Qt, QPoint, QRect
from PyQt6.QtGui import QMouseEvent, QMoveEvent, QScreen
from PyQt6.QtGui import QShortcut, QKeySequence

from views.components.search_bar import SearchBar
from views.components.history_list import HistoryList
from views.components.tray_icon import TrayIcon
from views.components.settings_dialog import SettingsDialog
from views.components.data_dialog import DataDialog
from views.styles.main_style import MainStyle, DarkStyle, StyleManager
from controllers.clipboard_controller import ClipboardController
from controllers.hotkey_controller import HotkeyController
from config.settings import Settings
from utils.logger import logger

class MainWindow(QMainWindow):
    """主窗口类"""
    
    def __init__(self, clipboard_controller: ClipboardController):
        super().__init__()
        self.clipboard_controller = clipboard_controller
        self.hotkey_controller = HotkeyController(self)
        
        # 初始化窗口拖动相关变量
        self._resize_area = None
        self._start_pos = None
        self._start_geometry = None
        self._is_moving = False
        self.is_top = False
        
        # 加载主题设置
        self.is_dark_mode = Settings.get("dark_mode", False)
        
        self._init_window()
        self._init_ui()
        self._init_connections()
        self._init_hotkeys()
        
        # 窗口居中显示
        self.center_window()
    
    def _init_window(self):
        """初始化窗口属性"""
        self.setWindowTitle(Settings.APP_NAME)
        self.setFixedWidth(Settings.WINDOW_WIDTH)  # 固定宽度
        self.setFixedHeight(Settings.WINDOW_HEIGHT)  # 固定高度
        self.setWindowFlags(Qt.WindowType.FramelessWindowHint | Qt.WindowType.Tool)
    
    def center_window(self):
        """将窗口居中显示"""
        try:
            # 获取主屏幕
            screen = QApplication.primaryScreen().geometry()
            
            # 计算居中位置
            x = (screen.width() - self.width()) // 2
            y = (screen.height() - self.height()) // 2
            
            # 移动窗口
            self.move(x, y)
            logger.info("窗口已居中显示")
        except Exception as e:
            logger.error(f"窗口居中显示失败: {str(e)}")
    
    def _init_ui(self):
        """初始化UI"""
        # 创建主部件和布局
        main_widget = QWidget()
        main_widget.setObjectName("mainWidget")
        self.setCentralWidget(main_widget)
        layout = QVBoxLayout(main_widget)
        layout.setSpacing(8)
        layout.setContentsMargins(12, 12, 12, 12)
        
        # 标题栏
        title_layout = self._create_title_bar()
        layout.addLayout(title_layout)
        
        # 分隔线
        separator = QFrame()
        separator.setFrameShape(QFrame.Shape.HLine)
        separator.setFrameShadow(QFrame.Shadow.Sunken)
        separator.setObjectName("separator")
        layout.addWidget(separator)
        
        # 搜索栏和置顶按钮
        toolbar = self._create_toolbar()
        layout.addLayout(toolbar)
        
        # 历史记录列表
        self.history_list = HistoryList()
        layout.addWidget(self.history_list)
        
        # 设置样式
        self._apply_theme()
        
        # 创建系统托盘图标
        self.tray_icon = TrayIcon(self)
    
    def _create_title_bar(self) -> QHBoxLayout:
        """创建标题栏"""
        layout = QHBoxLayout()
        layout.setSpacing(8)
        layout.setContentsMargins(8, 4, 8, 4)
        
        # 标题标签
        title_label = QLabel(f"📋 {Settings.APP_NAME}")
        title_label.setObjectName("titleLabel")
        layout.addWidget(title_label)
        
        # 添加弹性空间
        layout.addStretch()
        
        # 数据管理按钮
        self.data_button = QPushButton("📊")
        self.data_button.setObjectName("dataButton")
        self.data_button.setFixedSize(32, 32)
        self.data_button.setToolTip("数据管理")
        layout.addWidget(self.data_button)
        
        # 设置按钮
        self.settings_button = QPushButton("⚙️")
        self.settings_button.setObjectName("settingsButton")
        self.settings_button.setFixedSize(32, 32)
        self.settings_button.setToolTip("设置")
        layout.addWidget(self.settings_button)
        
        # 主题切换按钮
        self.theme_button = QPushButton("🌓")
        self.theme_button.setObjectName("themeButton")
        self.theme_button.setFixedSize(32, 32)
        self.theme_button.setToolTip("切换主题")
        layout.addWidget(self.theme_button)
        
        # 关闭按钮
        close_button = QPushButton("×")
        close_button.setObjectName("closeButton")
        close_button.setFixedSize(32, 32)
        close_button.clicked.connect(self.hide)  # 点击时隐藏窗口
        layout.addWidget(close_button)
        
        return layout
    
    def _create_toolbar(self) -> QHBoxLayout:
        """创建工具栏"""
        layout = QHBoxLayout()
        layout.setSpacing(8)
        layout.setContentsMargins(0, 0, 0, 0)
        
        # 搜索栏
        self.search_bar = SearchBar()
        layout.addWidget(self.search_bar)
        
        # 置顶按钮
        self.top_button = QPushButton("📌")
        self.top_button.setObjectName("topButton")
        self.top_button.setCheckable(True)
        self.top_button.setFixedSize(32, 32)
        self.top_button.setToolTip("窗口置顶")
        layout.addWidget(self.top_button)
        
        layout.addStretch()
        return layout
    
    def _init_connections(self):
        """初始化信号连接"""
        try:
            # 剪贴板控制器连接
            self.clipboard_controller.history_updated.connect(self._update_history)
            
            # 搜索栏连接
            self.search_bar.textChanged.connect(self.history_list.filter_items)
            
            # 历史记录列表连接
            self.history_list.itemCopied.connect(self._handle_item_copy)
            self.history_list.itemDeleted.connect(self._handle_item_delete)
            
            # 置顶按钮连接
            self.top_button.clicked.connect(self.toggle_top_window)
            
            # 主题切换按钮连接
            self.theme_button.clicked.connect(self.toggle_theme)
            
            # 设置按钮连接
            self.settings_button.clicked.connect(self.show_settings)
            
            # 数据管理按钮连接
            self.data_button.clicked.connect(self.show_data_dialog)
            
            # 托盘图标连接
            self.tray_icon.showWindowRequested.connect(self.show_and_activate)
            self.tray_icon.clearHistoryRequested.connect(self.clipboard_controller.clear_history)
            self.tray_icon.toggleThemeRequested.connect(self.toggle_theme)
            self.tray_icon.settingsRequested.connect(self.show_settings)
            self.tray_icon.dataManagementRequested.connect(self.show_data_dialog)
            self.tray_icon.quitRequested.connect(self.quit_application)
            
            # 初始加载历史记录
            self._update_history()
            
        except Exception as e:
            logger.error(f"初始化信号连接时发生错误: {str(e)}")
    
    def _init_hotkeys(self):
        """初始化热键"""
        try:
            # 注册显示窗口的快捷键
            show_key = Settings.get("hotkeys", {}).get("show_window", "Ctrl+O")
            self.show_shortcut = QShortcut(QKeySequence(show_key), self)
            self.show_shortcut.activated.connect(self.show_and_activate)
            self.show_shortcut.setContext(Qt.ShortcutContext.ApplicationShortcut)
            logger.info(f"已注册 {show_key} 快捷键")
            
            # 注册清空历史的快捷键
            clear_key = Settings.get("hotkeys", {}).get("clear_history", "Ctrl+Shift+C")
            self.clear_shortcut = QShortcut(QKeySequence(clear_key), self)
            self.clear_shortcut.activated.connect(self.clipboard_controller.clear_history)
            self.clear_shortcut.setContext(Qt.ShortcutContext.ApplicationShortcut)
            logger.info(f"已注册 {clear_key} 快捷键")
        except Exception as e:
            logger.error(f"注册快捷键时发生错误: {str(e)}")
    
    def _update_history(self):
        """更新历史记录列表"""
        try:
            history = self.clipboard_controller.get_history()
            self.history_list.update_items(history)
            logger.debug(f"已更新历史记录列表，共 {len(history)} 项")
        except Exception as e:
            logger.error(f"更新历史记录列表时发生错误: {str(e)}")
    
    def _handle_item_copy(self, text: str):
        """处理项目复制"""
        self.clipboard_controller.copy_text(text)
    
    def _handle_item_delete(self, index: int):
        """处理项目删除"""
        self.clipboard_controller.delete_item(index)
    
    def show_and_activate(self):
        """显示并激活窗口"""
        try:
            if self.isMinimized():
                self.showNormal()
            else:
                self.show()
            self.activateWindow()
            self.raise_()
            logger.info("窗口已显示并激活")
        except Exception as e:
            logger.error(f"显示窗口时发生错误: {str(e)}")
    
    def toggle_top_window(self):
        """切换窗口置顶状态"""
        self.is_top = not self.is_top
        current_pos = self.pos()
        
        if self.is_top:
            self.setWindowFlags(self.windowFlags() | Qt.WindowType.WindowStaysOnTopHint)
        else:
            self.setWindowFlags(self.windowFlags() & ~Qt.WindowType.WindowStaysOnTopHint)
        
        self.move(current_pos)
        self.show()
        self.activateWindow()
    
    def toggle_theme(self):
        """切换主题"""
        self.is_dark_mode = not self.is_dark_mode
        Settings.set("dark_mode", self.is_dark_mode)
        self._apply_theme()
        logger.info(f"已切换到{'暗色' if self.is_dark_mode else '亮色'}主题")
    
    def _apply_theme(self):
        """应用主题"""
        style = StyleManager.get_style(self.is_dark_mode)
        self.setStyleSheet(style)
    
    def quit_application(self):
        """退出应用程序"""
        try:
            # 保存历史记录
            self.clipboard_controller.service.save_history()
            # 隐藏托盘图标
            self.tray_icon.hide()
            # 退出应用
            app = QApplication.instance()
            if app:
                app.quit()
        except Exception as e:
            logger.error(f"退出应用程序时发生错误: {str(e)}")
            # 强制退出
            import sys
            sys.exit(0)
    
    def show_settings(self):
        """显示设置对话框"""
        try:
            settings_dialog = SettingsDialog(self)
            settings_dialog.settingsChanged.connect(self._apply_settings)
            settings_dialog.exec()
        except Exception as e:
            logger.error(f"显示设置对话框时发生错误: {str(e)}")
    
    def show_data_dialog(self):
        """显示数据管理对话框"""
        try:
            data_dialog = DataDialog(self.clipboard_controller, self)
            data_dialog.dataChanged.connect(self._update_history)
            data_dialog.exec()
        except Exception as e:
            logger.error(f"显示数据管理对话框时发生错误: {str(e)}")
    
    def _apply_settings(self):
        """应用设置"""
        try:
            # 应用主题
            self.is_dark_mode = Settings.get("dark_mode", False)
            self._apply_theme()
            
            # 更新最大历史记录数
            max_history = Settings.get("max_history", 100)
            self.clipboard_controller.service.set_max_history(max_history)
            
            # 更新自动保存间隔
            auto_save_interval = Settings.get("auto_save_interval", 60)
            self.clipboard_controller.service.update_auto_save_interval(auto_save_interval)
            
            # 更新历史记录保留天数
            retention_days = Settings.get("retention_days", 10)
            self.clipboard_controller.service.set_retention_days(retention_days)
            
            logger.info("已应用新设置")
        except Exception as e:
            logger.error(f"应用设置时发生错误: {str(e)}")
    
    # 窗口拖动和调整大小相关方法
    def mousePressEvent(self, event: QMouseEvent):
        """处理鼠标按下事件"""
        if event.button() == Qt.MouseButton.LeftButton:
            # 保存鼠标按下时的全局位置
            self._start_pos = event.globalPosition().toPoint()
            self._is_moving = True
    
    def mouseMoveEvent(self, event: QMouseEvent):
        """处理鼠标移动事件"""
        # 处理窗口拖动
        if self._is_moving and self._start_pos:
            # 计算移动的距离
            delta = event.globalPosition().toPoint() - self._start_pos
            # 更新窗口位置
            self.move(self.x() + delta.x(), self.y() + delta.y())
            # 更新起始位置
            self._start_pos = event.globalPosition().toPoint()
    
    def mouseReleaseEvent(self, event: QMouseEvent):
        """处理鼠标释放事件"""
        if event.button() == Qt.MouseButton.LeftButton:
            self._is_moving = False
            self._start_pos = None
    
    def leaveEvent(self, event):
        """处理鼠标离开事件"""
        self._is_moving = False
        self._start_pos = None 