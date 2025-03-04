from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QLabel, 
    QPushButton, QFileDialog, QMessageBox
)
from PyQt6.QtCore import Qt, pyqtSignal

from controllers.clipboard_controller import ClipboardController
from utils.logger import logger

class DataDialog(QDialog):
    """数据管理对话框"""
    
    dataChanged = pyqtSignal()
    
    def __init__(self, clipboard_controller: ClipboardController, parent=None):
        super().__init__(parent)
        self.clipboard_controller = clipboard_controller
        self._init_ui()
    
    def _init_ui(self):
        """初始化UI"""
        self.setWindowTitle("数据管理")
        self.setMinimumWidth(400)
        
        # 主布局
        layout = QVBoxLayout(self)
        
        # 标题
        title_label = QLabel("剪贴板历史记录数据管理")
        title_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        title_label.setStyleSheet("font-size: 16px; font-weight: bold; margin-bottom: 16px;")
        layout.addWidget(title_label)
        
        # 导出按钮
        export_button = QPushButton("导出历史记录")
        export_button.setToolTip("将剪贴板历史记录导出到文件")
        export_button.clicked.connect(self._export_history)
        layout.addWidget(export_button)
        
        # 导入按钮
        import_button = QPushButton("导入历史记录")
        import_button.setToolTip("从文件导入剪贴板历史记录")
        import_button.clicked.connect(self._import_history)
        layout.addWidget(import_button)
        
        # 警告标签
        warning_label = QLabel("注意: 导入操作将会覆盖当前的历史记录")
        warning_label.setStyleSheet("color: #d32f2f; font-style: italic; margin-top: 8px;")
        warning_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(warning_label)
        
        # 关闭按钮
        close_button = QPushButton("关闭")
        close_button.clicked.connect(self.accept)
        layout.addWidget(close_button)
    
    def _export_history(self):
        """导出历史记录"""
        try:
            # 获取保存文件路径
            file_path, _ = QFileDialog.getSaveFileName(
                self,
                "导出历史记录",
                "",
                "JSON文件 (*.json)"
            )
            
            if not file_path:
                return
                
            # 如果没有扩展名，添加.json扩展名
            if not file_path.endswith('.json'):
                file_path += '.json'
                
            # 导出历史记录
            success = self.clipboard_controller.export_history(file_path)
            
            if success:
                QMessageBox.information(
                    self,
                    "导出成功",
                    f"历史记录已成功导出到:\n{file_path}"
                )
            else:
                QMessageBox.warning(
                    self,
                    "导出失败",
                    "导出历史记录时发生错误，请检查文件权限。"
                )
                
        except Exception as e:
            logger.error(f"导出历史记录时发生错误: {str(e)}")
            QMessageBox.critical(
                self,
                "导出失败",
                f"导出历史记录时发生错误: {str(e)}"
            )
    
    def _import_history(self):
        """导入历史记录"""
        try:
            # 确认导入
            reply = QMessageBox.question(
                self,
                "确认导入",
                "导入操作将会覆盖当前的历史记录，确定要继续吗？",
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
                QMessageBox.StandardButton.No
            )
            
            if reply != QMessageBox.StandardButton.Yes:
                return
                
            # 获取导入文件路径
            file_path, _ = QFileDialog.getOpenFileName(
                self,
                "导入历史记录",
                "",
                "JSON文件 (*.json)"
            )
            
            if not file_path:
                return
                
            # 导入历史记录
            success = self.clipboard_controller.import_history(file_path)
            
            if success:
                QMessageBox.information(
                    self,
                    "导入成功",
                    f"历史记录已成功从以下位置导入:\n{file_path}"
                )
                # 发送数据已更改信号
                self.dataChanged.emit()
            else:
                QMessageBox.warning(
                    self,
                    "导入失败",
                    "导入历史记录时发生错误，请检查文件格式。"
                )
                
        except Exception as e:
            logger.error(f"导入历史记录时发生错误: {str(e)}")
            QMessageBox.critical(
                self,
                "导入失败",
                f"导入历史记录时发生错误: {str(e)}"
            ) 