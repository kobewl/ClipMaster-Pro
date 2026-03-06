from PyQt6.QtWidgets import (
   QListWidget, QAbstractItemView, QMenu,
   QApplication, QMessageBox, QTextEdit,
   QWidget, QHBoxLayout, QVBoxLayout, QLabel,
    QPushButton, QListWidgetItem
)
from PyQt6.QtCore import pyqtSignal, Qt, QSize, QTimer
from PyQt6.QtGui import QAction, QFont, QIcon, QColor, QPixmap, QImage

from models.clipboard_item import ClipboardItem, ContentType
from utils.logger import logger


class HistoryListItem(QWidget):
    """自定义历史列表项"""
    
    # 亮色主题颜色
    _LIGHT = {
        "border": "#93C5FD",
        "selected_border": "#FF8C00",
        "selected_bg": "#FFF3E0",
        "favorite_bg": "rgba(254, 243, 199, 0.5)",
    }
    # 暗色主题颜色
    _DARK = {
        "border": "#1E3A8A",
        "selected_border": "#FF8C00",
        "selected_bg": "#4A3525",
        "favorite_bg": "rgba(74, 53, 37, 0.5)",
    }
    
    # 图片缩略图尺寸
    _THUMB_W = 60
    _THUMB_H = 44

    def __init__(self, item: ClipboardItem, parent=None):
        super().__init__(parent)
        self.item_data = item
        self._selected = False
        self._thumbnail: QPixmap | None = None  # 缩略图缓存
        # 允许 QWidget 渲染 stylesheet 中的 background 和 border
        self.setAttribute(Qt.WidgetAttribute.WA_StyledBackground, True)
        self._init_ui()

    def _init_ui(self):
        """初始化UI"""
        layout = QHBoxLayout(self)
        layout.setContentsMargins(10, 8, 10, 8)
        layout.setSpacing(10)

        # 左侧：图标或缩略图
        self.icon_label = QLabel()
        self.icon_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(self.icon_label)

        # 内容区域
        content_layout = QVBoxLayout()
        content_layout.setSpacing(3)
        content_layout.setContentsMargins(0, 0, 0, 0)

        self.text_label = QLabel()
        self.text_label.setWordWrap(False)
        self.text_label.setStyleSheet("border: none; background: transparent;")
        font = QFont("Segoe UI", 10)
        font.setWeight(QFont.Weight.Medium)
        self.text_label.setFont(font)
        content_layout.addWidget(self.text_label)

        # 时间和来源在一行
        info_layout = QHBoxLayout()
        info_layout.setSpacing(8)
        info_layout.setContentsMargins(0, 0, 0, 0)

        self.time_label = QLabel()
        self.time_label.setStyleSheet(
            "color: #9CA3AF; font-size: 11px; border: none; background: transparent;"
        )
        info_layout.addWidget(self.time_label)

        self.source_label = QLabel()
        self.source_label.setStyleSheet(
            "color: #60A5FA; font-size: 10px; border: none; background: transparent;"
        )
        info_layout.addWidget(self.source_label)
        info_layout.addStretch()

        content_layout.addLayout(info_layout)

        layout.addLayout(content_layout, 1)

        # 收藏按钮
        self.fav_button = QPushButton()
        self.fav_button.setObjectName("iconButton")
        self.fav_button.setFixedSize(32, 32)
        self.fav_button.setStyleSheet("border: none; background: transparent; font-size: 16px;")
        layout.addWidget(self.fav_button)

        self._refresh_display()
        self._update_style()

    # ── 缩略图生成 ────────────────────────────────────────────
    def _make_thumbnail(self) -> QPixmap | None:
        """将 base64 图片内容解码并缩放为缩略图，结果缓存"""
        if self._thumbnail is not None:
            return self._thumbnail
        try:
            content = self.item_data.content
            if content.startswith('data:image'):
                import base64 as _b64
                raw = _b64.b64decode(content.split(',', 1)[1])
                img = QImage()
                if img.loadFromData(raw) and not img.isNull():
                    pix = QPixmap.fromImage(img)
                    self._thumbnail = pix.scaled(
                        self._THUMB_W, self._THUMB_H,
                        Qt.AspectRatioMode.KeepAspectRatio,
                        Qt.TransformationMode.SmoothTransformation,
                    )
                    return self._thumbnail
        except Exception as e:
            logger.warning(f"生成缩略图失败: {e}")
        return None

    # ── 内容刷新 ──────────────────────────────────────────────
    def _refresh_display(self):
        """刷新图标/缩略图、文字、时间、收藏按钮"""
        item = self.item_data

        if item.content_type == ContentType.IMAGE:
            # 图片：显示缩略图
            self.icon_label.setFixedSize(self._THUMB_W, self._THUMB_H)
            thumb = self._make_thumbnail()
            if thumb:
                self.icon_label.setPixmap(thumb)
                self.icon_label.setText("")
                self.icon_label.setStyleSheet(
                    "border: none; background: transparent; border-radius: 4px;"
                )
            else:
                self.icon_label.setPixmap(QPixmap())
                self.icon_label.setText("🖼️")
                self.icon_label.setStyleSheet(
                    "font-size: 20px; border: none; background: transparent;"
                )
        else:
            # 其他类型：emoji 图标
            self.icon_label.setFixedSize(28, 28)
            self.icon_label.setPixmap(QPixmap())
            self.icon_label.setText(item.get_icon())
            self.icon_label.setStyleSheet("font-size: 18px; border: none; background: transparent;")

        self.text_label.setText(item.preview_text(60))
        self.time_label.setText(item.timestamp.strftime("%m-%d %H:%M"))

        # 显示来源
        source_display = item.get_source_display()
        if source_display and source_display != "未知来源":
            self.source_label.setText(f"📍 {source_display}")
            self.source_label.setToolTip(item.get_source_tooltip())
        else:
            self.source_label.setText("")

        self.fav_button.setText("⭐" if item.is_favorite else "☆")
    
    def _update_style(self):
        """根据选中状态和收藏状态更新边框与背景"""
        from views.styles.main_style import StyleManager
        c = self._DARK if StyleManager.is_dark_mode() else self._LIGHT
        
        if self._selected:
            self.setStyleSheet(
                f"HistoryListItem {{"
                f"background-color: {c['selected_bg']};"
                f"border: 2px solid {c['selected_border']};"
                f"border-radius: 6px;"
                f"}}"
            )
        elif self.item_data.is_favorite:
            self.setStyleSheet(
                f"HistoryListItem {{"
                f"background-color: {c['favorite_bg']};"
                f"border: 2px solid {c['border']};"
                f"border-radius: 6px;"
                f"}}"
            )
        else:
            self.setStyleSheet(
                f"HistoryListItem {{"
                f"background-color: transparent;"
                f"border: 2px solid {c['border']};"
                f"border-radius: 6px;"
                f"}}"
            )
    
    def set_selected(self, selected: bool):
        """由 HistoryList 调用，更新选中状态样式"""
        if self._selected != selected:
            self._selected = selected
            self._update_style()
    
    def update_item(self, item: ClipboardItem):
        """更新显示"""
        self.item_data = item
        self._thumbnail = None  # 清除旧缩略图缓存
        self._refresh_display()
        self._update_style()


class HistoryList(QListWidget):
    """优化的历史记录列表组件"""
    
    itemCopied = pyqtSignal(object)  # ClipboardItem
    itemDeleted = pyqtSignal(str)    # content_hash
    favoriteToggled = pyqtSignal(str)  # content_hash
    
    def __init__(self, parent=None):
        super().__init__(parent)
        self._items = []  # ClipboardItem 列表
        self._item_widgets = {}  # content_hash -> widget
        self._prev_selected_row = -1  # 上一次选中的行号
        self._init_ui()
        
        # 延迟加载定时器
        self._load_timer = QTimer(self)
        self._load_timer.setSingleShot(True)
        self._load_timer.timeout.connect(self._load_visible_items)
    
    def _init_ui(self):
        """初始化UI"""
        self.setObjectName("historyList")
        self.setVerticalScrollMode(QAbstractItemView.ScrollMode.ScrollPerPixel)
        self.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.setSpacing(4)
        
        # 连接信号
        self.customContextMenuRequested.connect(self._show_context_menu)
        self.itemClicked.connect(self._handle_item_click)
        self.itemDoubleClicked.connect(self._handle_item_double_click)
        self.verticalScrollBar().valueChanged.connect(self._on_scroll)
        self.currentRowChanged.connect(self._on_selection_changed)
    
    def _on_scroll(self):
        """滚动时延迟加载"""
        self._load_timer.stop()
        self._load_timer.start(50)
    
    def _load_visible_items(self):
        """加载可见项（虚拟滚动优化）"""
        # 可以在这里实现更复杂的虚拟滚动逻辑
        pass

    def _item_size_hint(self, item: ClipboardItem) -> QSize:
        item_height = 76 if item.content_type == ContentType.IMAGE else 70
        item_width = max(self.viewport().width() - 8, 0)
        return QSize(item_width, item_height)

    def _refresh_item_sizes(self):
        for row, item in enumerate(self._items):
            list_item = self.item(row)
            if list_item:
                list_item.setSizeHint(self._item_size_hint(item))
    
    def update_items(self, items: list[ClipboardItem]):
        """更新列表项（使用增量更新）"""
        # 保存当前选中项
        current_row = self.currentRow()
        
        # 清空列表
        self.clear()
        self._items = items
        self._item_widgets.clear()
        
        # 批量添加项目
        for i, item in enumerate(items):
            list_item = QListWidgetItem()
            list_item.setData(Qt.ItemDataRole.UserRole, item.content_hash)
            list_item.setSizeHint(self._item_size_hint(item))
            
            self.addItem(list_item)
            
            # 创建自定义部件
            widget = HistoryListItem(item)
            widget.fav_button.clicked.connect(
                lambda checked, h=item.content_hash: self.favoriteToggled.emit(h)
            )
            self.setItemWidget(list_item, widget)
            self._item_widgets[item.content_hash] = widget
        
        # 恢复选中或默认选中第一项
        if current_row >= 0 and current_row < len(items):
            self.setCurrentRow(current_row)
        elif len(items) > 0:
            self.setCurrentRow(0)

        QTimer.singleShot(0, self._refresh_item_sizes)
    
    def filter_items(self, text: str, favorites_only: bool = False):
        """过滤列表项（支持模糊搜索）"""
        text = text.lower().strip()
        
        items_by_hash = {it.content_hash: it for it in self._items}
        
        for i in range(self.count()):
            list_item = self.item(i)
            content_hash = list_item.data(Qt.ItemDataRole.UserRole)
            item = items_by_hash.get(content_hash)
            
            if item is None:
                list_item.setHidden(True)
                continue
            
            if favorites_only and not item.is_favorite:
                list_item.setHidden(True)
                continue
            
            if text:
                searchable = item.content.lower()
                if item.tags:
                    searchable += ' ' + ' '.join(item.tags).lower()
                list_item.setHidden(text not in searchable)
            else:
                list_item.setHidden(False)
    
    def _on_selection_changed(self, current_row: int):
        """当选中行变化时，只更新旧选中项和新选中项的样式"""
        prev = self._prev_selected_row
        if prev == current_row:
            return
        
        for row in (prev, current_row):
            if 0 <= row < self.count():
                list_item = self.item(row)
                if list_item:
                    content_hash = list_item.data(Qt.ItemDataRole.UserRole)
                    widget = self._item_widgets.get(content_hash)
                    if widget:
                        widget.set_selected(row == current_row)
        
        self._prev_selected_row = current_row
    
    def _handle_item_click(self, list_item: QListWidgetItem):
        """处理项目点击 (仅选中，不做复制)"""
        pass
    
    def _handle_item_double_click(self, list_item: QListWidgetItem):
        """处理项目双击 (触发复制)"""
        content_hash = list_item.data(Qt.ItemDataRole.UserRole)
        item = self._find_item_by_hash(content_hash)
        if item:
            self.itemCopied.emit(item)
    
    def _find_item_by_hash(self, content_hash: str) -> ClipboardItem:
        """根据哈希查找项目"""
        for item in self._items:
            if item.content_hash == content_hash:
                return item
        return None
    
    def _show_context_menu(self, position):
        """显示上下文菜单"""
        try:
            list_item = self.itemAt(position)
            if not list_item:
                return
            
            content_hash = list_item.data(Qt.ItemDataRole.UserRole)
            item = self._find_item_by_hash(content_hash)
            if not item:
                return
            
            menu = QMenu(self)
            menu.setStyleSheet("""
                QMenu {
                    background-color: white;
                    border: 1px solid #E5E7EB;
                    border-radius: 8px;
                    padding: 8px;
                }
                QMenu::item {
                    padding: 8px 24px;
                    border-radius: 6px;
                }
                QMenu::item:selected {
                    background-color: #EEF2FF;
                    color: #4F46E5;
                }
            """)
            
            # 复制
            copy_action = QAction("📋 复制", self)
            copy_action.triggered.connect(lambda: self.itemCopied.emit(item))
            menu.addAction(copy_action)
            
            # 查看完整内容
            view_action = QAction("👁️ 查看完整内容", self)
            view_action.triggered.connect(lambda: self._show_full_content(item))
            menu.addAction(view_action)
            
            menu.addSeparator()
            
            # 收藏/取消收藏
            fav_text = "⭐ 取消收藏" if item.is_favorite else "☆ 收藏"
            fav_action = QAction(fav_text, self)
            fav_action.triggered.connect(lambda: self.favoriteToggled.emit(content_hash))
            menu.addAction(fav_action)
            
            menu.addSeparator()
            
            # 删除
            delete_action = QAction("🗑️ 删除", self)
            delete_action.triggered.connect(lambda: self.itemDeleted.emit(content_hash))
            menu.addAction(delete_action)
            
            menu.exec(self.mapToGlobal(position))
            
        except Exception as e:
            logger.error(f"显示上下文菜单时发生错误: {str(e)}")
    
    def _show_full_content(self, item: ClipboardItem):
        """显示完整内容"""
        try:
            dialog = QMessageBox(self)
            dialog.setWindowTitle("剪贴板内容")
            
            # 根据内容类型显示
            if item.content_type == ContentType.IMAGE:
                dialog.setText("[图片内容]\n" + item.preview_text(500))
            elif item.content_type == ContentType.FILE:
                files = item.metadata.get('files', [])
                dialog.setText("文件列表:\n" + "\n".join(files))
            else:
                # 使用文本编辑框显示
                text_edit = QTextEdit()
                text_edit.setPlainText(item.content)
                text_edit.setReadOnly(True)
                text_edit.setMinimumSize(500, 300)
                
                dialog.layout().addWidget(text_edit, 0, 0, 1, dialog.layout().columnCount())
                dialog.setText("")
            
            dialog.setStandardButtons(QMessageBox.StandardButton.Ok)
            dialog.exec()
            
        except Exception as e:
            logger.error(f"显示完整内容时发生错误: {str(e)}")
    
    def update_item_favorite(self, content_hash: str, is_favorite: bool):
        """更新项目的收藏状态显示"""
        widget = self._item_widgets.get(content_hash)
        if widget:
            widget.item_data.is_favorite = is_favorite
            widget.update_item(widget.item_data)

    def resizeEvent(self, event):
        super().resizeEvent(event)
        self._refresh_item_sizes()

    def showEvent(self, event):
        super().showEvent(event)
        QTimer.singleShot(0, self._refresh_item_sizes)
