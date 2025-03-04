from PyQt6.QtWidgets import QWidget, QHBoxLayout, QLineEdit, QLabel
from PyQt6.QtCore import pyqtSignal, Qt

class SearchBar(QWidget):
    """搜索栏组件"""
    
    textChanged = pyqtSignal(str)
    
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setObjectName("searchContainer")
        self._init_ui()
    
    def _init_ui(self):
        """初始化UI"""
        layout = QHBoxLayout(self)
        layout.setContentsMargins(8, 0, 8, 0)
        layout.setSpacing(4)
        
        # 搜索图标
        search_icon = QLabel("🔍")
        search_icon.setStyleSheet("color: #666;")
        layout.addWidget(search_icon)
        
        # 搜索输入框
        self.search_input = QLineEdit()
        self.search_input.setPlaceholderText("搜索剪贴板内容...")
        self.search_input.setMinimumWidth(200)
        self.search_input.textChanged.connect(self.textChanged.emit)
        layout.addWidget(self.search_input)
        
        # 设置布局属性
        layout.setAlignment(Qt.AlignmentFlag.AlignVCenter)
    
    def text(self) -> str:
        """获取搜索文本"""
        return self.search_input.text()
    
    def clear(self):
        """清空搜索框"""
        self.search_input.clear() 