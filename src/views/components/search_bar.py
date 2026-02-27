from PyQt6.QtWidgets import QWidget, QHBoxLayout, QLineEdit, QLabel
from PyQt6.QtCore import pyqtSignal, Qt
from PyQt6.QtGui import QKeyEvent


class SearchBar(QWidget):
    """优化的搜索栏组件"""
    
    textChanged = pyqtSignal(str)
    keyNavigationRequested = pyqtSignal(QKeyEvent)
    
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setObjectName("searchContainer")
        self._init_ui()
    
    def _init_ui(self):
        """初始化UI"""
        layout = QHBoxLayout(self)
        layout.setContentsMargins(12, 0, 12, 0)
        layout.setSpacing(8)
        
        # 搜索图标
        search_icon = QLabel("🔍")
        search_icon.setStyleSheet("font-size: 14px; color: #9CA3AF;")
        layout.addWidget(search_icon)
        
        # 搜索输入框
        self.search_input = CustomLineEdit()
        self.search_input.setPlaceholderText("搜索剪贴板内容... (Ctrl+F)")
        self.search_input.textChanged.connect(self.textChanged.emit)
        self.search_input.keyPressed.connect(self.keyNavigationRequested.emit)
        layout.addWidget(self.search_input)
        
        # 清除按钮（当输入框有内容时显示）
        self.clear_button = QLabel("✕")
        self.clear_button.setStyleSheet("""
            font-size: 12px; 
            color: #9CA3AF; 
            padding: 4px;
            border-radius: 4px;
        """)
        self.clear_button.setCursor(Qt.CursorShape.PointingHandCursor)
        self.clear_button.mousePressEvent = lambda e: self.clear()
        self.clear_button.hide()
        layout.addWidget(self.clear_button)
        
        # 设置布局属性
        layout.setAlignment(Qt.AlignmentFlag.AlignVCenter)
        
        # 连接文本变化以显示/隐藏清除按钮
        self.search_input.textChanged.connect(self._on_text_changed)
    
    def _on_text_changed(self, text: str):
        """处理文本变化"""
        if text:
            self.clear_button.show()
        else:
            self.clear_button.hide()
    
    def text(self) -> str:
        """获取搜索文本"""
        return self.search_input.text()
    
    def clear(self):
        """清空搜索框"""
        self.search_input.clear()
        self.clear_button.hide()
    
    def focus_search(self):
        """聚焦搜索框"""
        self.search_input.setFocus()
        self.search_input.selectAll()

class CustomLineEdit(QLineEdit):
    """自定义带按键拦截的输入框"""
    keyPressed = pyqtSignal(QKeyEvent)

    def keyPressEvent(self, event: QKeyEvent):
        key = event.key()
        
        up_keys = (Qt.Key.Key_Up, getattr(Qt.Key.Key_Up, 'value', 0x01000013))
        down_keys = (Qt.Key.Key_Down, getattr(Qt.Key.Key_Down, 'value', 0x01000015))
        enter_keys = (
            Qt.Key.Key_Return, getattr(Qt.Key.Key_Return, 'value', 0x01000004),
            Qt.Key.Key_Enter, getattr(Qt.Key.Key_Enter, 'value', 0x01000005)
        )
        
        if key in up_keys or key in down_keys or key in enter_keys:
            self.keyPressed.emit(event)
            return
            
        super().keyPressEvent(event)