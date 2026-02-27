from PyQt6.QtCore import QObject, pyqtSignal, QTimer
from PyQt6.QtGui import QClipboard
from PyQt6.QtCore import Qt
from PyQt6.QtWidgets import QApplication

from models.clipboard_item import ClipboardItem, ContentType
from services.clipboard_service import ClipboardService
from utils.logger import logger


class ClipboardController(QObject):
    """优化的剪贴板控制器"""
    
    history_updated = pyqtSignal()
    item_added = pyqtSignal(object)  # ClipboardItem
    
    def __init__(self, clipboard: QClipboard):
        super().__init__()
        self.clipboard = clipboard
        self.service = ClipboardService()
        self._manual_copy = False
        self._last_clipboard_data = None
        self._debounce_timer = QTimer(self)
        self._debounce_timer.setSingleShot(True)
        self._debounce_timer.timeout.connect(self._process_clipboard_change)
        
        # 连接剪贴板信号
        self.clipboard.dataChanged.connect(self._on_clipboard_changed)
        
        # 初始化时发送一次更新信号
        self.history_updated.emit()
        
        # 连接服务信号
        self.service.item_added.connect(self.item_added.emit)
        self.service.history_changed.connect(self.history_updated.emit)
    
    def _on_clipboard_changed(self):
        """剪贴板变化时的处理（防抖）"""
        if self._manual_copy:
            self._manual_copy = False
            return
        
        # 使用防抖避免频繁处理
        self._debounce_timer.stop()
        self._debounce_timer.start(100)  # 100ms 防抖
    
    def _process_clipboard_change(self):
        """处理剪贴板变化"""
        try:
            # 检查是否有文本
            mime_data = self.clipboard.mimeData()
            
            if mime_data.hasImage():
                # 处理图片
                self._handle_image_clipboard()
            elif mime_data.hasUrls():
                # 处理文件
                self._handle_file_clipboard(mime_data.urls())
            elif mime_data.hasHtml():
                # 处理HTML
                html = mime_data.html()
                text = mime_data.text() or html
                self.service.add_item(
                    content=text,
                    content_type=ContentType.HTML,
                    metadata={'html': html}
                )
            elif mime_data.hasText():
                # 处理纯文本
                text = mime_data.text()
                if text and text.strip():
                    self.service.add_item(text, ContentType.TEXT)
                    logger.debug(f"剪贴板文本已更新: {text[:50]}...")
                    
        except Exception as e:
            logger.error(f"处理剪贴板变化时发生错误: {str(e)}")
    
    def _handle_image_clipboard(self):
        """处理图片剪贴板内容"""
        try:
            image = self.clipboard.image()
            if not image.isNull():
                # 将图片转换为base64存储
                buffer = QApplication.clipboard().pixmap().toImage()
                
                # 压缩图片
                scaled_image = buffer.scaled(800, 600, Qt.AspectRatioMode.KeepAspectRatio, 
                                            Qt.TransformationMode.SmoothTransformation)
                
                # 转换为字节
                byte_array = QByteArray()
                buffer_stream = QBuffer(byte_array)
                buffer_stream.open(QBuffer.OpenModeFlag.WriteOnly)
                scaled_image.save(buffer_stream, "JPEG", 85)
                
                base64_data = base64.b64encode(byte_array.data()).decode('utf-8')
                
                self.service.add_item(
                    content=f"data:image/jpeg;base64,{base64_data}",
                    content_type=ContentType.IMAGE,
                    metadata={
                        'width': image.width(),
                        'height': image.height(),
                        'format': 'jpeg'
                    }
                )
                logger.debug(f"剪贴板图片已更新: {image.width()}x{image.height()}")
        except Exception as e:
            logger.error(f"处理图片剪贴板时发生错误: {str(e)}")
    
    def _handle_file_clipboard(self, urls):
        """处理文件剪贴板内容"""
        try:
            file_paths = [url.toLocalFile() for url in urls if url.isLocalFile()]
            if file_paths:
                self.service.add_item(
                    content="\n".join(file_paths),
                    content_type=ContentType.FILE,
                    metadata={'files': file_paths, 'count': len(file_paths)}
                )
                logger.debug(f"剪贴板文件已更新: {len(file_paths)} 个文件")
        except Exception as e:
            logger.error(f"处理文件剪贴板时发生错误: {str(e)}")
    
    def copy_text(self, text: str) -> None:
        """复制文本到剪贴板"""
        try:
            self._manual_copy = True
            self.clipboard.setText(text)
            logger.info(f"已复制文本: {text[:50]}...")
        except Exception as e:
            logger.error(f"复制文本时发生错误: {str(e)}")
    
    def copy_item(self, item: ClipboardItem) -> None:
        """复制项目到剪贴板"""
        try:
            self._manual_copy = True
            
            if item.content_type == ContentType.IMAGE:
                # 处理图片复制
                try:
                    if item.content.startswith('data:image'):
                        import base64
                        base64_data = item.content.split(',')[1]
                        image_data = base64.b64decode(base64_data)
                        image = QImage()
                        image.loadFromData(image_data)
                        self.clipboard.setImage(image)
                    else:
                        self.clipboard.setText(item.content)
                except Exception as e:
                    logger.error(f"复制图片时发生错误: {str(e)}")
                    self.clipboard.setText(item.content)
            elif item.content_type == ContentType.FILE:
                # 处理文件复制
                from PyQt6.QtCore import QUrl, QMimeData
                mime_data = QMimeData()
                urls = [QUrl.fromLocalFile(f) for f in item.metadata.get('files', [])]
                mime_data.setUrls(urls)
                self.clipboard.setMimeData(mime_data)
            else:
                self.clipboard.setText(item.content)
            
            logger.info(f"已复制项目: {item.preview_text(30)}")
        except Exception as e:
            logger.error(f"复制项目时发生错误: {str(e)}")
    
    def clear_history(self, keep_favorites: bool = True) -> None:
        """清空历史记录"""
        try:
            self.service.clear_history(keep_favorites)
            logger.info("已清空历史记录")
        except Exception as e:
            logger.error(f"清空历史记录时发生错误: {str(e)}")
    
    def delete_item(self, content_hash: str) -> None:
        """删除指定项目"""
        try:
            self.service.delete_item(content_hash)
            logger.info(f"已删除项目: {content_hash}")
        except Exception as e:
            logger.error(f"删除项目时发生错误: {str(e)}")
    
    def toggle_favorite(self, content_hash: str) -> None:
        """切换收藏状态"""
        try:
            self.service.toggle_favorite(content_hash)
        except Exception as e:
            logger.error(f"切换收藏状态时发生错误: {str(e)}")
    
    def get_history(self, limit: int = 100, offset: int = 0,
                   search_text: str = None, favorites_only: bool = False):
        """获取历史记录"""
        return self.service.get_history(limit, offset, search_text, favorites_only)
    
    def get_count(self) -> int:
        """获取记录总数"""
        return self.service.get_count()
    
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
            return self.service.import_history(file_path)
        except Exception as e:
            logger.error(f"导入历史记录时发生错误: {str(e)}")
            return False
    
    def update_settings(self, max_history: int = None, retention_days: int = None):
        """更新设置"""
        if max_history is not None:
            self.service.set_max_history(max_history)
        if retention_days is not None:
            self.service.set_retention_days(retention_days)


# 导入需要的Qt类
from PyQt6.QtCore import QByteArray, QBuffer, QUrl
from PyQt6.QtGui import QImage
import base64