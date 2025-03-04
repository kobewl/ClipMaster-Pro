from PyQt6.QtCore import QObject
from PyQt6.QtGui import QKeySequence, QShortcut
from PyQt6.QtWidgets import QApplication
from utils.logger import logger

class HotkeyController(QObject):
    """热键控制器"""
    
    def __init__(self, parent=None):
        super().__init__(parent)
        self.shortcuts = {}
        self.app = QApplication.instance()
    
    def register_shortcut(self, key_sequence: str, callback) -> bool:
        """注册全局热键"""
        try:
            if key_sequence in self.shortcuts:
                return False
            
            # 创建全局快捷键
            shortcut = QShortcut(QKeySequence(key_sequence), self.parent())
            shortcut.setContext(Qt.ShortcutContext.ApplicationShortcut)  # 设置为应用程序级快捷键
            shortcut.activated.connect(callback)
            self.shortcuts[key_sequence] = shortcut
            
            logger.info(f"已注册热键: {key_sequence}")
            return True
            
        except Exception as e:
            logger.error(f"注册热键时发生错误: {str(e)}")
            return False
    
    def unregister_shortcut(self, key_sequence: str) -> bool:
        """注销热键"""
        try:
            if key_sequence not in self.shortcuts:
                return False
                
            shortcut = self.shortcuts.pop(key_sequence)
            shortcut.setEnabled(False)
            shortcut.deleteLater()
            
            logger.info(f"已注销热键: {key_sequence}")
            return True
        except Exception as e:
            logger.error(f"注销热键时发生错误: {str(e)}")
            return False 