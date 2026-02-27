from PyQt6.QtWidgets import (
    QListWidget, QAbstractItemView, QMenu, 
    QApplication, QMessageBox, QTextEdit,
    QWidget, QHBoxLayout, QVBoxLayout, QLabel,
    QPushButton, QListWidgetItem
)
from PyQt6.QtCore import pyqtSignal, Qt, QSize, QTimer
from PyQt6.QtGui import QAction, QFont, QIcon, QColor

from models.clipboard_item import ClipboardItem, ContentType
from utils.logger import logger


class HistoryListItem(QWidget):
    """自定义历史列表项"""
    
    def __init__(self, item: ClipboardItem, parent=None):
        super().__init__(parent)
        self.item_data = item
        self._init_ui()
    
    def _init_ui(self):
        """初始化UI"""
        layout = QHBoxLayout(self)
        layout.setContentsMargins(12, 8, 12, 8)
        layout.setSpacing(12)
        
        # 图标
        self.icon_label = QLabel(self.item_data.get_icon())
        self.icon_label.setStyleSheet("font-size: 18px;")
        layout.addWidget(self.icon_label)
        
        # 内容区域
        content_layout = QVBoxLayout()
        content_layout.setSpacing(4)
        
        # 预览文本
        preview = self.item_data.preview_text(60)
        self.text_label = QLabel(preview)
        self.text_label.setWordWrap(False)
        font = QFont("Segoe UI", 10)
        font.setWeight(QFont.Weight.Medium)
        self.text_label.setFont(font)
        content_layout.addWidget(self.text_label)
        
        # 时间戳
        time_str = self.item_data.timestamp.strftime("%m-%d %H:%M")
        self.time_label = QLabel(time_str)
        self.time_label.setStyleSheet("color: #9CA3AF; font-size: 11px;")
        content_layout.addWidget(self.time_label)
        
        layout.addLayout(content_layout, 1)
        
        # 收藏按钮
        self.fav_button = QPushButton("⭐" if self.item_data.is_favorite else "☆")
        self.fav_button.setObjectName("iconButton")
        self.fav_button.setFixedSize(32, 32)
        self.fav_button.setStyleSheet("border: none; background: transparent; font-size: 16px;")
        layout.addWidget(self.fav_button)
        
        # 根据收藏状态调整样式
        self._update_favorite_style()
    
    def _update_favorite_style(self):
        """更新收藏样式"""
        if self.item_data.is_favorite:
            self.setStyleSheet("background-color: rgba(254, 243, 199, 0.5);")
        else:
            self.setStyleSheet("")
    
    def update_item(self, item: ClipboardItem):
        """更新显示"""
        self.item_data = item
        self.icon_label.setText(item.get_icon())
        self.text_label.setText(item.preview_text(60))
        self.time_label.setText(item.timestamp.strftime("%m-%d %H:%M"))
        self.fav_button.setText("⭐" if item.is_favorite else "☆")
        self._update_favorite_style()


class HistoryList(QListWidget):
    """优化的历史记录列表组件"""
    
    itemCopied = pyqtSignal(object)  # ClipboardItem
    itemDeleted = pyqtSignal(str)    # content_hash
    favoriteToggled = pyqtSignal(str)  # content_hash
    
    def __init__(self, parent=None):
        super().__init__(parent)
        self._items = []  # ClipboardItem 列表
        self._item_widgets = {}  # content_hash -> widget
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
    
    def _on_scroll(self):
        """滚动时延迟加载"""
        self._load_timer.stop()
        self._load_timer.start(50)
    
    def _load_visible_items(self):
        """加载可见项（虚拟滚动优化）"""
        # 可以在这里实现更复杂的虚拟滚动逻辑
        pass
    
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
            list_item.setSizeHint(QSize(self.width() - 20, 70))
            
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
    
    def filter_items(self, text: str, favorites_only: bool = False):
        """过滤列表项（支持模糊搜索）"""
        text = text.lower().strip()
        
        for i in range(self.count()):
            list_item = self.item(i)
            content_hash = list_item.data(Qt.ItemDataRole.UserRole)
            
            # 找到对应的 ClipboardItem
            item = None
            for it in self._items:
                if it.content_hash == content_hash:
                    item = it
                    break
            
            if item is None:
                list_item.setHidden(True)
                continue
            
            # 检查收藏过滤
            if favorites_only and not item.is_favorite:
                list_item.setHidden(True)
                continue
            
            # 检查搜索文本
            if text:
                # 搜索内容、标签
                searchable = item.content.lower()
                if item.tags:
                    searchable += ' ' + ' '.join(item.tags).lower()
                
                # 模糊匹配
                match = text in searchable
                list_item.setHidden(not match)
            else:
                list_item.setHidden(False)
    
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