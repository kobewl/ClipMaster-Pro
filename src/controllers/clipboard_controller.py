from PyQt6.QtCore import QObject, pyqtSignal
from PyQt6.QtGui import QClipboard
from services.clipboard_service import ClipboardService
from utils.logger import logger

class ClipboardController(QObject):
    """剪贴板控制器"""
    
    history_updated = pyqtSignal()  # 历史记录更新信号
    
    def __init__(self, clipboard: QClipboard):
        super().__init__()
        self.clipboard = clipboard
        self.service = ClipboardService()
        self._manual_copy = False
        
        # 连接剪贴板信号
        self.clipboard.dataChanged.connect(self._handle_clipboard_change)
        
        # 初始化时发送一次更新信号
        self.history_updated.emit()
    
    def _handle_clipboard_change(self) -> None:
        """处理剪贴板变化"""
        try:
            if self._manual_copy:
                self._manual_copy = False
                return
                
            text = self.clipboard.text()
            if text and text.strip():
                self.service.add_item(text)
                self.history_updated.emit()
                logger.debug(f"剪贴板内容已更新: {text[:30]}...")
        except Exception as e:
            logger.error(f"处理剪贴板变化时发生错误: {str(e)}")
    
    def copy_text(self, text: str) -> None:
        """复制文本到剪贴板"""
        try:
            self._manual_copy = True
            self.clipboard.setText(text)
            logger.info(f"已复制文本: {text[:30]}...")
        except Exception as e:
            logger.error(f"复制文本时发生错误: {str(e)}")
    
    def clear_history(self) -> None:
        """清空历史记录"""
        try:
            self.service.clear_history()
            self.history_updated.emit()
            logger.info("已清空历史记录")
        except Exception as e:
            logger.error(f"清空历史记录时发生错误: {str(e)}")
    
    def delete_item(self, index: int) -> None:
        """删除指定索引的历史记录项"""
        try:
            self.service.delete_item(index)
            self.history_updated.emit()
            logger.info(f"已删除索引为 {index} 的历史记录项")
        except Exception as e:
            logger.error(f"删除历史记录项时发生错误: {str(e)}")
    
    def get_history(self):
        """获取历史记录"""
        return self.service.get_history()
    
    def export_history(self, file_path: str) -> bool:
        """导出历史记录到文件"""
        try:
            return self.service.export_history(file_path)
        except Exception as e:
            logger.error(f"导出历史记录时发生错误: {str(e)}")
            return False
    
    def import_history(self, file_path: str) -> bool:
        """从文件导入历史记录"""
        try:
            success = self.service.import_history(file_path)
            if success:
                self.history_updated.emit()
            return success
        except Exception as e:
            logger.error(f"导入历史记录时发生错误: {str(e)}")
            return False 