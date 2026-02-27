import keyboard
from PyQt6.QtCore import QObject, pyqtSignal
from utils.logger import logger

class HotkeyController(QObject):
    """全局热键控制器"""
    
    # 定义信号以安全地向主线程派发回调
    on_hotkey_triggered = pyqtSignal(str)
    
    def __init__(self, parent=None):
        super().__init__(parent)
        self.shortcuts = {}
        
        # 将接收到的信号与其对应的实际回调关联起来
        self.callbacks = {}
        self.on_hotkey_triggered.connect(self._handle_signal)
    
    def _convert_key_sequence(self, qkey_sequence: str) -> str:
        """将 QKeySequence (如 Ctrl+O) 转换为 keyboard (如 ctrl+o)"""
        if not qkey_sequence:
            return ""
        return qkey_sequence.lower().replace("ctrl", "ctrl").replace("shift", "shift").replace("alt", "alt")
    
    def register_shortcut(self, key_sequence: str, callback) -> bool:
        """注册全局热键"""
        try:
            if not key_sequence:
                return False
                
            kb_sequence = self._convert_key_sequence(key_sequence)
            
            if kb_sequence in self.shortcuts:
                self.unregister_shortcut(key_sequence)
            
            # 采用 lambda 表达式发出绑定了原本按键字符的信号
            hook = keyboard.add_hotkey(kb_sequence, lambda: self.on_hotkey_triggered.emit(key_sequence), suppress=False)
            
            self.shortcuts[kb_sequence] = hook
            self.callbacks[key_sequence] = callback
            
            logger.info(f"已注册全局热键: {key_sequence} -> {kb_sequence}")
            return True
            
        except Exception as e:
            logger.error(f"注册全局热键时发生错误: {str(e)}")
            return False
    
    def unregister_shortcut(self, key_sequence: str) -> bool:
        """注销热键"""
        try:
            if not key_sequence:
                return False
                
            kb_sequence = self._convert_key_sequence(key_sequence)
            
            if kb_sequence in self.shortcuts:
                # 移除热键
                keyboard.remove_hotkey(self.shortcuts[kb_sequence])
                del self.shortcuts[kb_sequence]
                
                if key_sequence in self.callbacks:
                    del self.callbacks[key_sequence]
                
                logger.info(f"已注销旧热键: {key_sequence}")
                return True
            return False
        except Exception as e:
            logger.error(f"注销热键时发生错误: {str(e)}")
            return False 

    def unregister_all(self):
        """注销所有已注册的热键"""
        for kb_sequence, hook in list(self.shortcuts.items()):
            try:
                keyboard.remove_hotkey(hook)
            except Exception as e:
                logger.warning(f"注销热键 {kb_sequence} 时出错: {e}")
        self.shortcuts.clear()
        self.callbacks.clear()

    def _handle_signal(self, key_sequence: str):
        """由于全局热键在非主线程触发，在这里接管回主线程"""
        callback = self.callbacks.get(key_sequence)
        if callback:
            callback()