from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QLabel, 
    QPushButton, QFileDialog, QMessageBox, QGroupBox,
    QProgressBar, QTextEdit
)
from PyQt6.QtCore import Qt, pyqtSignal, QThread, pyqtSignal as Signal

from controllers.clipboard_controller import ClipboardController
from utils.logger import logger


class ExportWorker(QThread):
    """导出工作线程"""
    finished = Signal(bool, str)
    progress = Signal(int)
    
    def __init__(self, controller, file_path):
        super().__init__()
        self.controller = controller
        self.file_path = file_path
    
    def run(self):
        try:
            success = self.controller.export_history(self.file_path)
            self.finished.emit(success, self.file_path)
        except Exception as e:
            self.finished.emit(False, str(e))


class ImportWorker(QThread):
    """导入工作线程"""
    finished = Signal(bool, str)
    progress = Signal(int)
    
    def __init__(self, controller, file_path):
        super().__init__()
        self.controller = controller
        self.file_path = file_path
    
    def run(self):
        try:
            success = self.controller.import_history(self.file_path)
            self.finished.emit(success, self.file_path)
        except Exception as e:
            self.finished.emit(False, str(e))


class DataDialog(QDialog):
    """优化的数据管理对话框"""
    
    dataChanged = pyqtSignal()
    
    def __init__(self, clipboard_controller: ClipboardController, parent=None):
        super().__init__(parent)
        self.clipboard_controller = clipboard_controller
        self.export_worker = None
        self.import_worker = None
        self._init_ui()
        self._update_stats()
    
    def _init_ui(self):
        """初始化UI"""
        self.setWindowTitle("数据管理")
        self.setMinimumWidth(500)
        self.setMinimumHeight(400)
        
        layout = QVBoxLayout(self)
        layout.setSpacing(16)
        layout.setContentsMargins(20, 20, 20, 20)
        
        # 标题
        title_label = QLabel("📊 剪贴板数据管理")
        title_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        title_label.setStyleSheet("font-size: 20px; font-weight: bold; margin-bottom: 8px;")
        layout.addWidget(title_label)
        
        # 统计信息组
        stats_group = QGroupBox("📈 统计信息")
        stats_layout = QVBoxLayout(stats_group)
        
        self.stats_label = QLabel("加载中...")
        self.stats_label.setStyleSheet("font-size: 14px; line-height: 1.6;")
        stats_layout.addWidget(self.stats_label)
        
        layout.addWidget(stats_group)
        
        # 导出组
        export_group = QGroupBox("📤 导出数据")
        export_layout = QVBoxLayout(export_group)
        
        export_info = QLabel("将历史记录导出为 JSON 文件，可用于备份或迁移")
        export_info.setStyleSheet("color: #6B7280; font-size: 12px;")
        export_info.setWordWrap(True)
        export_layout.addWidget(export_info)
        
        export_button_layout = QHBoxLayout()
        
        self.export_button = QPushButton("📤 导出历史记录")
        self.export_button.setObjectName("primaryButton")
        self.export_button.clicked.connect(self._export_history)
        export_button_layout.addWidget(self.export_button)
        
        export_button_layout.addStretch()
        export_layout.addLayout(export_button_layout)
        
        layout.addWidget(export_group)
        
        # 导入组
        import_group = QGroupBox("📥 导入数据")
        import_layout = QVBoxLayout(import_group)
        
        import_info = QLabel("从 JSON 文件导入历史记录。导入时可选择合并或覆盖现有数据")
        import_info.setStyleSheet("color: #6B7280; font-size: 12px;")
        import_info.setWordWrap(True)
        import_layout.addWidget(import_info)
        
        import_button_layout = QHBoxLayout()
        
        self.import_button = QPushButton("📥 导入历史记录")
        self.import_button.setObjectName("secondaryButton")
        self.import_button.clicked.connect(self._import_history)
        import_button_layout.addWidget(self.import_button)
        
        import_button_layout.addStretch()
        import_layout.addLayout(import_button_layout)
        
        # 警告标签
        warning_label = QLabel("⚠️ 警告: 导入操作可能会覆盖现有数据，建议先导出备份")
        warning_label.setStyleSheet("color: #DC2626; font-size: 12px; margin-top: 8px;")
        warning_label.setWordWrap(True)
        import_layout.addWidget(warning_label)
        
        layout.addWidget(import_group)
        
        # 进度条
        self.progress_bar = QProgressBar()
        self.progress_bar.setVisible(False)
        layout.addWidget(self.progress_bar)
        
        # 日志文本框
        self.log_text = QTextEdit()
        self.log_text.setVisible(False)
        self.log_text.setMaximumHeight(100)
        self.log_text.setReadOnly(True)
        layout.addWidget(self.log_text)
        
        # 关闭按钮
        close_button = QPushButton("关闭")
        close_button.clicked.connect(self.accept)
        layout.addWidget(close_button)
    
    def _update_stats(self):
        """更新统计信息"""
        try:
            count = self.clipboard_controller.get_count()
            self.stats_label.setText(
                f"📋 当前历史记录总数: <b>{count}</b> 条\n"
                f"💾 数据存储在本地数据库中\n"
                f"🔒 数据仅保存在您的设备上"
            )
        except Exception as e:
            logger.error(f"更新统计信息时发生错误: {str(e)}")
            self.stats_label.setText("无法获取统计信息")
    
    def _export_history(self):
        """导出历史记录"""
        try:
            file_path, _ = QFileDialog.getSaveFileName(
                self,
                "导出历史记录",
                "clipboard_backup.json",
                "JSON文件 (*.json);;所有文件 (*.*)"
            )
            
            if not file_path:
                return
            
            if not file_path.endswith('.json'):
                file_path += '.json'
            
            # 显示进度条
            self.progress_bar.setVisible(True)
            self.progress_bar.setRange(0, 0)  # 无限进度
            self.export_button.setEnabled(False)
            
            # 在工作线程中执行导出
            self.export_worker = ExportWorker(self.clipboard_controller, file_path)
            self.export_worker.finished.connect(self._on_export_finished)
            self.export_worker.start()
            
        except Exception as e:
            logger.error(f"导出历史记录时发生错误: {str(e)}")
            QMessageBox.critical(
                self,
                "导出失败",
                f"导出历史记录时发生错误: {str(e)}"
            )
            self.progress_bar.setVisible(False)
            self.export_button.setEnabled(True)
    
    def _on_export_finished(self, success: bool, message: str):
        """导出完成回调"""
        self.progress_bar.setVisible(False)
        self.export_button.setEnabled(True)
        
        if success:
            QMessageBox.information(
                self,
                "导出成功",
                f"历史记录已成功导出到:\n{message}"
            )
        else:
            QMessageBox.critical(
                self,
                "导出失败",
                f"导出历史记录时发生错误:\n{message}"
            )
    
    def _import_history(self):
        """导入历史记录"""
        try:
            # 确认导入
            reply = QMessageBox.question(
                self,
                "确认导入",
                "导入操作将会覆盖当前的历史记录，确定要继续吗？\n\n"
                "建议先导出当前数据作为备份。",
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No | QMessageBox.StandardButton.Cancel,
                QMessageBox.StandardButton.No
            )
            
            if reply != QMessageBox.StandardButton.Yes:
                return
            
            file_path, _ = QFileDialog.getOpenFileName(
                self,
                "导入历史记录",
                "",
                "JSON文件 (*.json);;所有文件 (*.*)"
            )
            
            if not file_path:
                return
            
            # 显示进度条
            self.progress_bar.setVisible(True)
            self.progress_bar.setRange(0, 0)
            self.import_button.setEnabled(False)
            
            # 在工作线程中执行导入
            self.import_worker = ImportWorker(self.clipboard_controller, file_path)
            self.import_worker.finished.connect(self._on_import_finished)
            self.import_worker.start()
            
        except Exception as e:
            logger.error(f"导入历史记录时发生错误: {str(e)}")
            QMessageBox.critical(
                self,
                "导入失败",
                f"导入历史记录时发生错误: {str(e)}"
            )
            self.progress_bar.setVisible(False)
            self.import_button.setEnabled(True)
    
    def _on_import_finished(self, success: bool, message: str):
        """导入完成回调"""
        self.progress_bar.setVisible(False)
        self.import_button.setEnabled(True)
        
        if success:
            self._update_stats()
            self.dataChanged.emit()
            QMessageBox.information(
                self,
                "导入成功",
                f"历史记录已成功导入！\n{message}"
            )
        else:
            QMessageBox.critical(
                self,
                "导入失败",
                f"导入历史记录时发生错误:\n{message}"
            )