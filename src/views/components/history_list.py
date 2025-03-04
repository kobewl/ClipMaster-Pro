from PyQt6.QtWidgets import QListWidget, QAbstractItemView
from PyQt6.QtCore import pyqtSignal
from models.clipboard_item import ClipboardItem

class HistoryList(QListWidget):
    """历史记录列表组件"""
    
    itemCopied = pyqtSignal(str)
    
    def __init__(self, parent=None):
        super().__init__(parent)
        self._init_ui()
        self._items = []  # 存储 ClipboardItem 对象
    
    def _init_ui(self):
        """初始化UI"""
        self.setObjectName("historyList")
        self.setVerticalScrollMode(QAbstractItemView.ScrollMode.ScrollPerPixel)
        self.itemClicked.connect(self._handle_item_click)
    
    def update_items(self, items: list[ClipboardItem]):
        """更新列表项"""
        self.clear()
        self._items = items
        for item in items:
            self.addItem(item.display_text())
    
    def filter_items(self, text: str):
        """过滤列表项"""
        for i in range(self.count()):
            item = self.item(i)
            item.setHidden(text.lower() not in item.text().lower())
    
    def _handle_item_click(self, item):
        """处理项目点击"""
        if item:
            # 获取原始内容
            index = self.row(item)
            if 0 <= index < len(self._items):
                content = self._items[index].content
                self.itemCopied.emit(content) 