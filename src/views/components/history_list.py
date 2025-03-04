from PyQt6.QtWidgets import (
    QListWidget, QAbstractItemView, QMenu, 
    QApplication, QMessageBox, QTextBrowser
)
from PyQt6.QtCore import pyqtSignal, Qt
from PyQt6.QtGui import QAction
from models.clipboard_item import ClipboardItem
from utils.logger import logger

class HistoryList(QListWidget):
    """历史记录列表组件"""
    
    itemCopied = pyqtSignal(str)
    itemDeleted = pyqtSignal(int)
    
    def __init__(self, parent=None):
        super().__init__(parent)
        self._init_ui()
        self._items = []  # 存储 ClipboardItem 对象
    
    def _init_ui(self):
        """初始化UI"""
        self.setObjectName("historyList")
        self.setVerticalScrollMode(QAbstractItemView.ScrollMode.ScrollPerPixel)
        self.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.customContextMenuRequested.connect(self._show_context_menu)
        self.itemClicked.connect(self._handle_item_click)
        self.itemDoubleClicked.connect(self._handle_item_double_click)
    
    def update_items(self, items: list[ClipboardItem]):
        """更新列表项"""
        self.clear()
        self._items = items
        for item in items:
            # 显示时间和内容预览
            time_str = item.timestamp.strftime("%H:%M:%S")
            preview = item.content[:50] + ('...' if len(item.content) > 50 else '')
            self.addItem(f"{time_str} | {preview}")
    
    def filter_items(self, text: str):
        """过滤列表项"""
        for i in range(self.count()):
            item = self.item(i)
            # 在完整内容中搜索，而不仅仅是显示的预览
            if 0 <= i < len(self._items):
                content = self._items[i].content.lower()
                item.setHidden(text.lower() not in content)
    
    def _handle_item_click(self, item):
        """处理项目点击"""
        if item:
            # 获取原始内容
            index = self.row(item)
            if 0 <= index < len(self._items):
                content = self._items[index].content
                self.itemCopied.emit(content)
    
    def _handle_item_double_click(self, item):
        """处理项目双击"""
        if item:
            # 获取原始内容
            index = self.row(item)
            if 0 <= index < len(self._items):
                content = self._items[index].content
                self._show_full_content(content)
    
    def _show_context_menu(self, position):
        """显示上下文菜单"""
        try:
            # 获取当前项
            item = self.itemAt(position)
            if not item:
                return
                
            index = self.row(item)
            if index < 0 or index >= len(self._items):
                return
                
            # 创建菜单
            menu = QMenu(self)
            
            # 复制
            copy_action = QAction("复制", self)
            copy_action.triggered.connect(lambda: self._handle_item_click(item))
            menu.addAction(copy_action)
            
            # 查看完整内容
            view_action = QAction("查看完整内容", self)
            view_action.triggered.connect(lambda: self._show_full_content(self._items[index].content))
            menu.addAction(view_action)
            
            # 删除
            delete_action = QAction("删除", self)
            delete_action.triggered.connect(lambda: self._delete_item(index))
            menu.addAction(delete_action)
            
            # 显示菜单
            menu.exec(self.mapToGlobal(position))
            
        except Exception as e:
            logger.error(f"显示上下文菜单时发生错误: {str(e)}")
    
    def _show_full_content(self, content: str):
        """显示完整内容"""
        try:
            msg_box = QMessageBox()
            msg_box.setWindowTitle("剪贴板内容")
            msg_box.setText(content)
            msg_box.setStandardButtons(QMessageBox.StandardButton.Ok)
            msg_box.setDefaultButton(QMessageBox.StandardButton.Ok)
            
            # 设置可复制文本
            text_browser = msg_box.findChild(QTextBrowser)
            if text_browser:
                text_browser.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse | 
                                                   Qt.TextInteractionFlag.TextSelectableByKeyboard)
            
            msg_box.exec()
        except Exception as e:
            logger.error(f"显示完整内容时发生错误: {str(e)}")
    
    def _delete_item(self, index: int):
        """删除项目"""
        try:
            if 0 <= index < len(self._items):
                self.takeItem(index)
                self.itemDeleted.emit(index)
                logger.debug(f"已删除索引为 {index} 的项目")
        except Exception as e:
            logger.error(f"删除项目时发生错误: {str(e)}") 