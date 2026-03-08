from PyQt6.QtWidgets import (
    QMainWindow, QWidget, QVBoxLayout, QHBoxLayout,
    QPushButton, QLabel, QFrame, QApplication, QSizePolicy
)
from PyQt6.QtCore import Qt, QPoint, QTimer
from PyQt6.QtGui import QColor, QMouseEvent, QPainter, QPen, QScreen, QShortcut, QKeySequence
import platform

try:
    import keyboard
    _KEYBOARD_IMPORT_ERROR = None
except ImportError as e:
    keyboard = None
    _KEYBOARD_IMPORT_ERROR = e

if platform.system() == "Darwin":
    try:
        import objc
        from AppKit import NSFloatingWindowLevel, NSNormalWindowLevel
        _MAC_NATIVE_WINDOW_LEVEL_AVAILABLE = True
    except ImportError:
        objc = None
        NSFloatingWindowLevel = None
        NSNormalWindowLevel = None
        _MAC_NATIVE_WINDOW_LEVEL_AVAILABLE = False
else:
    objc = None
    NSFloatingWindowLevel = None
    NSNormalWindowLevel = None
    _MAC_NATIVE_WINDOW_LEVEL_AVAILABLE = False

from views.components.search_bar import SearchBar
from views.components.history_list import HistoryList
from views.components.tray_icon import TrayIcon
from views.components.settings_dialog import SettingsDialog
from views.components.data_dialog import DataDialog
from views.styles.main_style import StyleManager
from controllers.clipboard_controller import ClipboardController
from controllers.hotkey_controller import HotkeyController
from services.prediction_engine import PredictionEngine
from config.settings import Settings
from utils.logger import logger


class BorderFrame(QFrame):
    """Draw the outer rounded border explicitly to avoid delayed QSS rendering."""

    def paintEvent(self, event):
        super().paintEvent(event)

        colors = StyleManager.get_colors()
        border_color = QColor(colors.get("border", "#A0A0A0"))

        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.setPen(QPen(border_color, 2))
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawRoundedRect(self.rect().adjusted(1, 1, -1, -1), 8, 8)


class BorderOverlay(QWidget):
    """Top-most overlay that keeps the outer border visible after child repaints."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents, True)
        self.setAttribute(Qt.WidgetAttribute.WA_NoSystemBackground, True)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground, True)

    def paintEvent(self, event):
        colors = StyleManager.get_colors()
        border_color = QColor(colors.get("border", "#A0A0A0"))

        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        painter.setPen(QPen(border_color, 2))
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawRoundedRect(self.rect().adjusted(1, 1, -1, -1), 8, 8)


class MainWindow(QMainWindow):
    """优化的主窗口类"""
    
    def __init__(self, clipboard_controller: ClipboardController):
        super().__init__()
        self.clipboard_controller = clipboard_controller
        self.hotkey_controller = HotkeyController(self)
        self.prediction_engine = PredictionEngine(
            clipboard_controller.service, parent=self
        )
        
        # 窗口拖动相关变量
        self._start_pos = None
        self._is_moving = False
        self.is_top = False
        self.show_favorites_only = False
        self._platform = platform.system()
        
        # 加载主题设置
        self.is_dark_mode = Settings.get("dark_mode", False)
        
        self._init_window()
        self._init_ui()
        self._init_connections()
        self._init_hotkeys()
        
        # 防抖定时器：合并短时间内的多次 _update_history 请求
        self._update_debounce = QTimer(self)
        self._update_debounce.setSingleShot(True)
        self._update_debounce.setInterval(150)
        self._update_debounce.timeout.connect(self._do_update_history)
        
        # 搜索防抖定时器
        self._search_debounce = QTimer(self)
        self._search_debounce.setSingleShot(True)
        self._search_debounce.setInterval(300)
        self._search_debounce.timeout.connect(self._do_update_history)
        
        # 窗口居中显示
        self.center_window()
        
        # 延迟加载历史记录
        QTimer.singleShot(100, self._do_update_history)

        # 延迟强制刷新样式，确保边框正确显示
        # 多个时间点刷新，确保样式完全应用
        QTimer.singleShot(50, self._force_style_refresh)
        QTimer.singleShot(200, self._force_style_refresh)
        QTimer.singleShot(500, self._force_style_refresh)
    
    def _init_window(self):
        """初始化窗口属性"""
        self.setWindowTitle(Settings.APP_NAME)
        self.setFixedSize(Settings.WINDOW_WIDTH, Settings.WINDOW_HEIGHT)
        self._apply_window_flags()
        
        # 设置窗口阴影效果（使用样式）
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground, False)

    def _build_window_flags(self):
        flags = Qt.WindowType.FramelessWindowHint
        if self._platform == "Darwin":
            flags |= Qt.WindowType.Window
        else:
            flags |= Qt.WindowType.Tool
        if self.is_top:
            flags |= Qt.WindowType.WindowStaysOnTopHint
        return flags

    def _apply_window_flags(self):
        was_visible = self.isVisible()
        current_pos = self.pos()
        self.setWindowFlags(self._build_window_flags())
        self.move(current_pos)
        if was_visible:
            self.show()
            self.raise_()
            self.activateWindow()
        self._apply_native_topmost_state()

    def _apply_native_topmost_state(self):
        if self._platform != "Darwin" or not _MAC_NATIVE_WINDOW_LEVEL_AVAILABLE:
            return
        try:
            if not self.winId():
                return
            native_view = objc.objc_object(c_void_p=int(self.winId()))
            native_window = native_view.window()
            if native_window is None:
                return
            level = NSFloatingWindowLevel if self.is_top else NSNormalWindowLevel
            native_window.setLevel_(level)
            logger.info(f"macOS 原生窗口层级已设置: {'floating' if self.is_top else 'normal'}")
        except Exception as e:
            logger.error(f"设置 macOS 原生窗口层级失败: {e}")
    
    def center_window(self):
        """将窗口居中显示"""
        try:
            screen = QApplication.primaryScreen().geometry()
            x = (screen.width() - self.width()) // 2
            y = (screen.height() - self.height()) // 2
            self.move(x, y)
            logger.info("窗口已居中显示")
        except Exception as e:
            logger.error(f"窗口居中显示失败: {str(e)}")
    
    def _init_ui(self):
        """初始化UI"""
        # 创建主部件
        main_widget = QWidget()
        main_widget.setObjectName("mainWidget")
        self.setCentralWidget(main_widget)

        # 主布局 - 无边距
        layout = QVBoxLayout(main_widget)
        layout.setSpacing(0)
        layout.setContentsMargins(0, 0, 0, 0)

        # 边框容器 - 使用QFrame实现可靠边框渲染
        self.border_frame = BorderFrame()
        self.border_frame.setObjectName("borderFrame")
        # 关键：启用样式背景，否则样式表的background-color不会生效
        self.border_frame.setAttribute(Qt.WidgetAttribute.WA_StyledBackground, True)
        # 不设置FrameShape，让样式表完全控制边框
        border_layout = QVBoxLayout(self.border_frame)
        border_layout.setSpacing(0)
        border_layout.setContentsMargins(2, 2, 2, 2)  # 边框内边距

        # 内容容器
        content_widget = QWidget()
        content_widget.setObjectName("contentWidget")
        content_layout = QVBoxLayout(content_widget)
        content_layout.setSpacing(0)
        content_layout.setContentsMargins(0, 0, 0, 0)
        
        # 标题栏
        title_bar = self._create_title_bar()
        content_layout.addWidget(title_bar)
        
        # 工具栏（搜索和过滤）
        toolbar = self._create_toolbar()
        content_layout.addWidget(toolbar)
        
        # 历史记录列表
        self.history_list = HistoryList()
        content_layout.addWidget(self.history_list, 1)
        
        # 状态栏
        self.status_bar = self._create_status_bar()
        content_layout.addWidget(self.status_bar)
        
        border_layout.addWidget(content_widget)
        layout.addWidget(self.border_frame)

        self._border_overlay = BorderOverlay(self)
        self._sync_border_overlay()
         
        # 设置样式
        self._apply_theme()
        
        # 创建系统托盘图标
        self.tray_icon = TrayIcon(self)
    
    def _create_title_bar(self) -> QWidget:
        """创建标题栏"""
        title_bar = QWidget()
        title_bar.setObjectName("titleBar")
        title_bar.setFixedHeight(48)
        
        layout = QHBoxLayout(title_bar)
        layout.setSpacing(12)
        layout.setContentsMargins(16, 0, 16, 0)
        
        # 标题标签
        title_label = QLabel(f"📋 {Settings.APP_NAME}")
        title_label.setObjectName("titleLabel")
        layout.addWidget(title_label)
        
        layout.addStretch()
        
        # 置顶按钮
        self.top_button = QPushButton("📌")
        self.top_button.setObjectName("iconButton")
        self.top_button.setCheckable(True)
        self.top_button.setFixedSize(32, 32)
        self.top_button.setToolTip("窗口置顶")
        layout.addWidget(self.top_button)
        
        # 主题切换按钮
        self.theme_button = QPushButton("🌙" if not self.is_dark_mode else "☀️")
        self.theme_button.setObjectName("iconButton")
        self.theme_button.setFixedSize(32, 32)
        self.theme_button.setToolTip("切换主题")
        layout.addWidget(self.theme_button)
        
        # 设置按钮
        self.settings_button = QPushButton("⚙️")
        self.settings_button.setObjectName("iconButton")
        self.settings_button.setFixedSize(32, 32)
        self.settings_button.setToolTip("设置")
        layout.addWidget(self.settings_button)
        
        # 关闭按钮
        close_button = QPushButton("✕")
        close_button.setObjectName("closeButton")
        close_button.setFixedSize(32, 32)
        close_button.clicked.connect(self.hide)
        layout.addWidget(close_button)
        
        return title_bar
    
    def _create_toolbar(self) -> QWidget:
        """创建工具栏"""
        toolbar = QWidget()
        toolbar.setObjectName("toolbar")
        toolbar.setFixedHeight(48)
        
        layout = QHBoxLayout(toolbar)
        layout.setSpacing(12)
        layout.setContentsMargins(16, 8, 16, 8)
        
        # 搜索栏
        self.search_bar = SearchBar()
        layout.addWidget(self.search_bar, 1)
        
        # 收藏筛选按钮
        self.fav_filter_button = QPushButton("⭐ 收藏")
        self.fav_filter_button.setObjectName("filterButton")
        self.fav_filter_button.setCheckable(True)
        self.fav_filter_button.setFixedHeight(32)
        self.fav_filter_button.setToolTip("只显示收藏的项目")
        layout.addWidget(self.fav_filter_button)
        
        # 数据管理按钮
        self.data_button = QPushButton("📊")
        self.data_button.setObjectName("iconButton")
        self.data_button.setFixedSize(32, 32)
        self.data_button.setToolTip("数据管理")
        layout.addWidget(self.data_button)
        
        return toolbar
    
    def _create_status_bar(self) -> QLabel:
        """创建状态栏"""
        status_bar = QLabel("就绪")
        status_bar.setObjectName("statusBar")
        status_bar.setFixedHeight(32)
        return status_bar
    
    def _init_connections(self):
        """初始化信号连接"""
        try:
            # 剪贴板控制器连接
            self.clipboard_controller.history_updated.connect(self._update_history)
            self.clipboard_controller.item_added.connect(self._on_item_added)
            
            # 搜索栏连接
            self.search_bar.textChanged.connect(self._on_search_changed)
            self.search_bar.keyNavigationRequested.connect(self._handle_search_key_navigation)
            
            # 收藏筛选连接
            self.fav_filter_button.clicked.connect(self._on_fav_filter_changed)
            
            # 历史记录列表连接
            self.history_list.itemCopied.connect(self._handle_item_copy)
            self.history_list.itemDeleted.connect(self._handle_item_delete)
            self.history_list.favoriteToggled.connect(self._handle_favorite_toggle)
            
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
            self.tray_icon.clearHistoryRequested.connect(
                lambda: self.clipboard_controller.clear_history(keep_favorites=True)
            )
            self.tray_icon.toggleThemeRequested.connect(self.toggle_theme)
            self.tray_icon.settingsRequested.connect(self.show_settings)
            self.tray_icon.dataManagementRequested.connect(self.show_data_dialog)
            self.tray_icon.quitRequested.connect(self.quit_application)
            
        except Exception as e:
            logger.error(f"初始化信号连接时发生错误: {str(e)}")
    
    def _init_hotkeys(self):
        """初始化全局热键"""
        try:
            hotkeys = Settings.get("hotkeys", {})

            # 记录当前热键，用于后续配置更改时注销
            self.current_show_key = hotkeys.get("show_window", Settings.DEFAULT_HOTKEYS["show_window"])
            self.current_clear_key = hotkeys.get("clear_history", Settings.DEFAULT_HOTKEYS["clear_history"])
            self.current_search_key = hotkeys.get("search", Settings.DEFAULT_HOTKEYS["search"])

            # 保存回调引用，防止被垃圾回收
            self._clear_history_callback = lambda: self.clipboard_controller.clear_history(keep_favorites=True)

            # 注册显示窗口的快捷键
            result1 = self.hotkey_controller.register_shortcut(self.current_show_key, self.toggle_window_visibility)
            logger.info(f"热键注册: 显示窗口 [{self.current_show_key}] = {'成功' if result1 else '失败'}")

            # 注册清空历史的快捷键
            result2 = self.hotkey_controller.register_shortcut(self.current_clear_key, self._clear_history_callback)
            logger.info(f"热键注册: 清空历史 [{self.current_clear_key}] = {'成功' if result2 else '失败'}")

            # 注册搜索快捷键
            result3 = self.hotkey_controller.register_shortcut(self.current_search_key, self.show_and_focus_search)
            logger.info(f"热键注册: 搜索 [{self.current_search_key}] = {'成功' if result3 else '失败'}")

            if self._platform == "Darwin":
                logger.info("macOS 平台：全局热键依赖 pynput 与辅助功能权限")
            elif self._platform != "Windows":
                logger.info("非 Windows 平台：全局热键依赖额外后端支持")

            # 注册ESC关闭窗口 (局部热键即可)
            self.esc_shortcut = QShortcut(QKeySequence("Esc"), self)
            self.esc_shortcut.activated.connect(self.hide)

        except Exception as e:
            logger.error(f"注册全局热键时发生错误: {str(e)}")
    
    def _update_history(self):
        """请求更新历史记录列表（防抖，合并短时间内的多次调用）"""
        self._update_debounce.stop()
        self._update_debounce.start()

    def _do_update_history(self):
        """实际执行历史记录列表更新"""
        try:
            search_text = self.search_bar.text()
            history = self.clipboard_controller.get_history(
                limit=100,
                search_text=search_text if search_text else None,
                favorites_only=self.show_favorites_only
            )
            self.history_list.update_items(history)
            
            count = self.clipboard_controller.get_count()
            self.status_bar.setText(f"共 {count} 条记录")
            
            logger.debug(f"已更新历史记录列表，共 {len(history)} 项")
        except Exception as e:
            logger.error(f"更新历史记录列表时发生错误: {str(e)}")
    
    def _on_item_added(self, item):
        """新项目添加时的处理"""
        # 如果不在搜索模式，刷新列表
        if not self.search_bar.text() and not self.show_favorites_only:
            self._update_history()
        
        # 更新状态栏
        count = self.clipboard_controller.get_count()
        self.status_bar.setText(f"共 {count} 条记录 | 刚刚添加新内容")
    
    def _on_search_changed(self, text: str):
        """搜索文本变化（使用独立的300ms防抖，避免每次按键都查询数据库）"""
        self._search_debounce.stop()
        self._search_debounce.start()
    
    def _on_fav_filter_changed(self, checked: bool):
        """收藏筛选变化"""
        self.show_favorites_only = checked
        self._update_history()
    
    def _handle_item_copy(self, item):
        """处理项目复制与自动粘贴"""
        self.clipboard_controller.copy_item(item)
        self.status_bar.setText(f"已复制: {item.preview_text(30)}")
        
        # 隐藏窗口
        self.hide()

        if keyboard is None:
            logger.info(f"自动粘贴不可用: {_KEYBOARD_IMPORT_ERROR}")
            return

        paste_hotkey = "ctrl+v" if self._platform == "Windows" else "command+v"

        def _send_paste():
            try:
                keyboard.send(paste_hotkey)
            except Exception as e:
                logger.error(f"自动粘贴失败: {e}")

        QTimer.singleShot(300, _send_paste)
    
    def _handle_item_delete(self, content_hash: str):
        """处理项目删除"""
        self.clipboard_controller.delete_item(content_hash)
        self._update_history()
        self.status_bar.setText("已删除项目")
    
    def _handle_favorite_toggle(self, content_hash: str):
        """处理收藏切换"""
        self.clipboard_controller.toggle_favorite(content_hash)
        self._update_history()
    
    def toggle_window_visibility(self):
        """切换窗口的显示和隐藏"""
        if self.isVisible() and self.isActiveWindow():
            self.hide()
            logger.info("响应热键已隐藏窗口")
        else:
            self.show_and_activate()
            
    def _handle_search_key_navigation(self, event):
        """处理来自搜索框的按键导航"""
        count = self.history_list.count()
        if count == 0:
            return
            
        current = self.history_list.currentRow()
        key = event.key()
        
        up_keys = (Qt.Key.Key_Up, getattr(Qt.Key.Key_Up, 'value', 0x01000013))
        down_keys = (Qt.Key.Key_Down, getattr(Qt.Key.Key_Down, 'value', 0x01000015))
        enter_keys = (
            Qt.Key.Key_Return, getattr(Qt.Key.Key_Return, 'value', 0x01000004),
            Qt.Key.Key_Enter, getattr(Qt.Key.Key_Enter, 'value', 0x01000005)
        )
        
        if key in up_keys:
            next_row = current - 1 if current > 0 else count - 1
            self.history_list.setCurrentRow(next_row)
            self.history_list.scrollToItem(self.history_list.item(next_row))
        elif key in down_keys:
            next_row = current + 1 if current < count - 1 else 0
            self.history_list.setCurrentRow(next_row)
            self.history_list.scrollToItem(self.history_list.item(next_row))
        elif key in enter_keys:
            if current >= 0:
                list_item = self.history_list.item(current)
                if list_item:
                    content_hash = list_item.data(Qt.ItemDataRole.UserRole)
                    item = self.history_list._find_item_by_hash(content_hash)
                    if item:
                        self.history_list.itemCopied.emit(item)
                    
    def show_and_activate(self):
        """显示并激活窗口"""
        try:
            if self.isMinimized():
                self.showNormal()
            self.show()
            self._apply_native_topmost_state()
            self.activateWindow()
            self.raise_()
            self.search_bar.focus_search()
            
            # 默认选中第一项
            if self.history_list.count() > 0:
                self.history_list.setCurrentRow(0)

            # 强制刷新样式，确保边框正确显示
            QTimer.singleShot(10, self._force_style_refresh)

            logger.info("窗口已显示并激活")
        except Exception as e:
            logger.error(f"显示窗口时发生错误: {str(e)}")
            
    def show_and_focus_search(self):
        """显示窗口并聚焦搜索框"""
        self.show_and_activate()
        self.search_bar.focus_search()
    
    def toggle_top_window(self):
        """切换窗口置顶状态"""
        self.is_top = not self.is_top

        self.top_button.setChecked(self.is_top)
        self._apply_window_flags()

        if self.is_top:
            self.status_bar.setText("窗口已置顶")
        else:
            self.status_bar.setText("窗口已取消置顶")
    
    def toggle_theme(self):
        """切换主题"""
        self.is_dark_mode = not self.is_dark_mode
        Settings.set("dark_mode", self.is_dark_mode)
        self._apply_theme()
        
        # 更新主题按钮图标
        self.theme_button.setText("☀️" if self.is_dark_mode else "🌙")
        
        logger.info(f"已切换到{'暗色' if self.is_dark_mode else '亮色'}主题")
        self.status_bar.setText(f"已切换到{'暗色' if self.is_dark_mode else '亮色'}主题")
    
    def _apply_theme(self):
        """应用主题"""
        style = StyleManager.get_style(self.is_dark_mode)
        self.setStyleSheet(style)

        # 强制样式立即更新，避免边框延迟显示
        self._force_style_refresh()

    def _force_style_refresh(self):
        """强制刷新样式，确保边框正确渲染"""
        # 刷新主窗口
        self.style().unpolish(self)
        self.style().polish(self)
        self.update()

        # 刷新主部件
        main_widget = self.centralWidget()
        if main_widget:
            main_widget.style().unpolish(main_widget)
            main_widget.style().polish(main_widget)
            main_widget.update()

            # 确保布局边距正确
            layout = main_widget.layout()
            if layout:
                layout.invalidate()
                layout.activate()

        # 刷新边框容器 - 关键：确保边框样式正确应用
        if hasattr(self, 'border_frame') and self.border_frame:
            self.border_frame.style().unpolish(self.border_frame)
            self.border_frame.style().polish(self.border_frame)
            self.border_frame.update()

        self._sync_border_overlay()

        # 递归刷新所有子部件
        self._recursive_style_refresh(self)

    def _sync_border_overlay(self):
        if hasattr(self, "_border_overlay") and self._border_overlay:
            self._border_overlay.setGeometry(self.rect())
            self._border_overlay.raise_()
            self._border_overlay.update()

    def _recursive_style_refresh(self, widget):
        """递归刷新所有子部件的样式"""
        for child in widget.findChildren(QWidget):
            child.style().unpolish(child)
            child.style().polish(child)
            child.update()
    
    def quit_application(self):
        """退出应用程序，清理所有资源"""
        try:
            self.prediction_engine.stop()
            self.hotkey_controller.unregister_all()
            self.clipboard_controller.service.db.close()
            self.tray_icon.hide()
            app = QApplication.instance()
            if app:
                app.quit()
        except Exception as e:
            logger.error(f"退出应用程序时发生错误: {str(e)}")
            import sys
            sys.exit(0)
    
    def show_settings(self):
        """显示设置对话框"""
        try:
            settings_dialog = SettingsDialog(self)
            settings_dialog.settingsChanged.connect(self._apply_settings)
            settings_dialog.aiSettingsChanged.connect(self._apply_ai_settings)
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

            # 更新热键
            hotkeys = Settings.get("hotkeys", {})
            new_show_key = hotkeys.get("show_window", Settings.DEFAULT_HOTKEYS["show_window"])
            new_clear_key = hotkeys.get("clear_history", Settings.DEFAULT_HOTKEYS["clear_history"])
            new_search_key = hotkeys.get("search", Settings.DEFAULT_HOTKEYS["search"])

            # 更新清空历史回调引用
            self._clear_history_callback = lambda: self.clipboard_controller.clear_history(keep_favorites=True)

            # 注销并重新注册有变化的热键
            if hasattr(self, 'current_show_key') and self.current_show_key != new_show_key:
                self.hotkey_controller.unregister_shortcut(self.current_show_key)
                result = self.hotkey_controller.register_shortcut(new_show_key, self.toggle_window_visibility)
                logger.info(f"热键更新: 显示窗口 [{new_show_key}] = {'成功' if result else '失败'}")
            elif not hasattr(self, 'current_show_key'):
                result = self.hotkey_controller.register_shortcut(new_show_key, self.toggle_window_visibility)
                logger.info(f"热键注册: 显示窗口 [{new_show_key}] = {'成功' if result else '失败'}")

            if hasattr(self, 'current_clear_key') and self.current_clear_key != new_clear_key:
                self.hotkey_controller.unregister_shortcut(self.current_clear_key)
                result = self.hotkey_controller.register_shortcut(new_clear_key, self._clear_history_callback)
                logger.info(f"热键更新: 清空历史 [{new_clear_key}] = {'成功' if result else '失败'}")
            elif not hasattr(self, 'current_clear_key'):
                result = self.hotkey_controller.register_shortcut(new_clear_key, self._clear_history_callback)
                logger.info(f"热键注册: 清空历史 [{new_clear_key}] = {'成功' if result else '失败'}")

            if hasattr(self, 'current_search_key') and self.current_search_key != new_search_key:
                self.hotkey_controller.unregister_shortcut(self.current_search_key)
                result = self.hotkey_controller.register_shortcut(new_search_key, self.show_and_focus_search)
                logger.info(f"热键更新: 搜索 [{new_search_key}] = {'成功' if result else '失败'}")
            elif not hasattr(self, 'current_search_key'):
                result = self.hotkey_controller.register_shortcut(new_search_key, self.show_and_focus_search)
                logger.info(f"热键注册: 搜索 [{new_search_key}] = {'成功' if result else '失败'}")

            # 更新当前热键记录
            self.current_show_key = new_show_key
            self.current_clear_key = new_clear_key
            self.current_search_key = new_search_key

            # 更新设置
            max_history = Settings.get("max_history", 1000)
            retention_days = Settings.get("retention_days", 30)
            self.clipboard_controller.update_settings(max_history, retention_days)

            self._update_history()
            logger.info("已应用新设置")
            self.status_bar.setText("设置已更新")
        except Exception as e:
            logger.error(f"应用设置时发生错误: {str(e)}")
    
    def _apply_ai_settings(self):
        """Reload prediction engine after AI settings change."""
        try:
            self.prediction_engine.reload_settings()
            ai = Settings.get("ai", {})
            if ai.get("enabled") and self.prediction_engine.is_available():
                self.status_bar.setText("AI 智能预测已启用")
            elif ai.get("enabled"):
                self.status_bar.setText("AI 智能预测不可用：缺少键盘监听能力")
            else:
                self.status_bar.setText("AI 智能预测已关闭")
        except Exception as e:
            logger.error(f"应用AI设置时发生错误: {str(e)}")
    
    # 窗口拖动相关方法
    def mousePressEvent(self, event: QMouseEvent):
        """处理鼠标按下事件"""
        if event.button() == Qt.MouseButton.LeftButton:
            # 检查是否在标题栏区域
            if event.pos().y() < 56:
                self._start_pos = event.globalPosition().toPoint()
                self._is_moving = True
    
    def mouseMoveEvent(self, event: QMouseEvent):
        """处理鼠标移动事件"""
        if self._is_moving and self._start_pos:
            delta = event.globalPosition().toPoint() - self._start_pos
            self.move(self.x() + delta.x(), self.y() + delta.y())
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

    def resizeEvent(self, event):
        super().resizeEvent(event)
        self._sync_border_overlay()

    def showEvent(self, event):
        super().showEvent(event)
        QTimer.singleShot(0, self._sync_border_overlay)
