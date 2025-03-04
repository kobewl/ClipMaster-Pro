# import json
# import logging
# import os
# import sys
# import time
# from datetime import datetime
# from typing import Optional

# import keyboard
# from PyQt6.QtCore import (Qt, QTimer, QEvent, QRect)
# from PyQt6.QtGui import QColor, QCursor, QMoveEvent
# from PyQt6.QtWidgets import (QApplication, QMainWindow, QListWidget,
#                              QVBoxLayout, QWidget, QSystemTrayIcon, QMenu,
#                              QPushButton, QHBoxLayout, QLabel, QLineEdit,
#                              QMessageBox, QStyle, QListWidgetItem, QInputDialog, QAbstractItemView)
# from PyQt6.QtWidgets import QGraphicsDropShadowEffect


# # 设置日志
# def setup_logging():
#     """初始化日志设置"""
#     log_dir = "logs"
#     if not os.path.exists(log_dir):
#         os.makedirs(log_dir)
    
#     log_file = os.path.join(log_dir, f"clipboard_{datetime.now().strftime('%Y%m%d')}.log")
#     logging.basicConfig(
#         level=logging.INFO,
#         format='%(asctime)s - %(levelname)s - %(message)s',
#         handlers=[
#             logging.FileHandler(log_file, encoding='utf-8'),
#             logging.StreamHandler()
#         ]
#     )

# class ClipboardItem:
#     """剪贴板项目类"""
#     def __init__(self, text: str, category: str = "默认", timestamp: Optional[datetime] = None):
#         self.full_text = text
#         self.category = category
#         self.timestamp = timestamp or datetime.now()

#     def to_dict(self) -> dict:
#         """转换为字典格式"""
#         return {
#             'text': self.full_text,
#             'category': self.category,
#             'timestamp': self.timestamp.isoformat()
#         }

#     @classmethod
#     def from_dict(cls, data: dict) -> 'ClipboardItem':
#         """从字典创建实例"""
#         text = str(data.get('text', ''))
#         category = str(data.get('category', '默认'))
#         timestamp_str = data.get('timestamp')
#         timestamp = datetime.fromisoformat(timestamp_str) if timestamp_str else datetime.now()
#         return cls(text=text, category=category, timestamp=timestamp)

#     def display_text(self):
#         """生成显示文本"""
#         display_text = self.full_text[:60] + ('...' if len(self.full_text) > 60 else '')
#         time_str = self.timestamp.strftime("%H:%M:%S")
#         return f"{time_str} | {display_text}"

# class ClipboardAssistant(QMainWindow):
#     def __init__(self):
#         """初始化应用"""
#         super().__init__()
#         self.setWindowTitle('剪贴板助手')
        
#         # 初始化基本变量
#         self.clipboard = QApplication.clipboard()
#         self.clipboard_history = []
#         self.last_text = ""
#         self.is_top = False
#         self._resize_start = False
#         self._drag_pos = None
        
#         # 初始化UI（在加载历史记录之前）
#         self.init_ui()
        
#         # 设置托盘图标
#         self.setup_tray_icon()
        
#         # 设置剪贴板监控
#         self.setup_clipboard_monitoring()
        
#         # 设置自动保存
#         self.setup_autosave()
        
#         # 设置快捷键
#         self.setup_shortcuts()
        
#         # 设置右键菜单
#         self.setup_context_menu()
        
#         # 加载历史记录（在UI初始化之后）
#         self.load_history()
        
#         # 居中显示窗口
#         self.center_window()
        
#         # 显示窗口
#         self.show()
        
#         # 显示托盘通知
#         self.tray_icon.showMessage(
#             "剪贴板助手",
#             "程序已在后台运行\n使用 Ctrl+O 打开主窗口",
#             QSystemTrayIcon.MessageIcon.Information,
#             2000
#         )

#     def init_ui(self):
#         """初始化UI"""
#         try:
#             # 设置窗口基本属性
#             self.setGeometry(100, 100, 500, 600)
#             self.setMinimumWidth(400)
#             self.setMinimumHeight(300)
            
#             # 设置窗口标志
#             self.setWindowFlags(
#                 Qt.WindowType.FramelessWindowHint |
#                 Qt.WindowType.Tool
#             )
            
#             # 初始化调整大小的变量
#             self._resize_area = None
#             self._start_pos = None
#             self._start_geometry = None
            
#             # 创建主窗口部件
#             main_widget = QWidget()
#             main_widget.setObjectName("mainWidget")
#             self.setCentralWidget(main_widget)
            
#             # 设置整体样式
#             self.setStyleSheet("""
#                 #mainWidget {
#                     background-color: white;
#                     border: 1px solid #e0e0e0;
#                     border-radius: 12px;
#                 }
                
#                 QLineEdit {
#                     padding: 10px 16px;
#                     border: 1px solid #eaeaea;
#                     border-radius: 20px;
#                     background-color: #f8f9fa;
#                     font-size: 13px;
#                     color: #333;
#                 }
                
#                 QLineEdit:focus {
#                     border-color: #2196f3;
#                     background-color: white;
#                 }
                
#                 QListWidget {
#                     border: 1px solid #eaeaea;
#                     background-color: white;
#                     border-radius: 8px;
#                 }
                
#                 QListWidget::item {
#                     padding: 10px 12px;
#                     border-radius: 6px;
#                     margin: 2px 4px;
#                     color: #333;
#                 }
                
#                 QListWidget::item:hover {
#                     background-color: #f5f5f5;
#                 }
                
#                 QListWidget::item:selected {
#                     background-color: #e3f2fd;
#                     color: #1976d2;
#                     font-weight: 500;
#                 }
                
#                 QPushButton {
#                     padding: 8px 16px;
#                     border-radius: 16px;
#                     font-size: 13px;
#                     font-weight: 500;
#                     background-color: #f5f5f5;
#                     border: none;
#                     color: #424242;
#                 }
                
#                 QPushButton:hover {
#                     background-color: #e3f2fd;
#                     color: #1976d2;
#                 }
                
#                 #closeBtn {
#                     min-width: 24px;
#                     min-height: 24px;
#                     padding: 4px;
#                     font-size: 18px;
#                     color: #757575;
#                     background: transparent;
#                     border-radius: 12px;
#                 }
                
#                 #closeBtn:hover {
#                     background-color: #ffebee;
#                     color: #f44336;
#                 }
                
#                 #titleBar {
#                     background-color: transparent;
#                     padding: 8px;
#                     border-bottom: 1px solid #f0f0f0;
#                 }
                
#                 #statusLabel {
#                     color: #666;
#                     font-size: 12px;
#                     padding: 4px 8px;
#                     border-radius: 10px;
#                     background-color: #f5f5f5;
#                 }
                
#                 #statusLabel[status="ready"] {
#                     background-color: #e8f5e9;
#                     color: #2e7d32;
#                 }
                
#                 #statusLabel[status="copied"] {
#                     background-color: #e3f2fd;
#                     color: #1976d2;
#                 }
                
#                 #clearBtn {
#                     background-color: #fafafa;
#                     color: #757575;
#                 }
                
#                 #clearBtn:hover {
#                     background-color: #ffebee;
#                     color: #d32f2f;
#                 }
                
#                 #topButton {
#                     padding: 8px;
#                     border-radius: 16px;
#                     font-size: 13px;
#                     background-color: #f5f5f5;
#                     border: none;
#                     color: #424242;
#                     margin-right: 8px;
#                 }
                
#                 #topButton:checked {
#                     background-color: #e3f2fd;
#                     color: #1976d2;
#                 }
#             """)
            
#             # 创建主布局
#             layout = QVBoxLayout(main_widget)
#             layout.setContentsMargins(16, 12, 16, 16)
#             layout.setSpacing(12)
            
#             # 顶部工具栏
#             top_bar = QWidget()
#             top_bar.setObjectName("titleBar")
#             top_layout = QHBoxLayout(top_bar)
#             top_layout.setContentsMargins(4, 0, 4, 8)
            
#             # 标题
#             title_label = QLabel("📋 剪贴板助手")
#             title_label.setStyleSheet("""
#                 font-size: 15px;
#                 font-weight: bold;
#                 color: #2196f3;
#             """)
#             top_layout.addWidget(title_label)
#             top_layout.addStretch()
            
#             # 状态标签
#             self.status_label = QLabel("就绪")
#             self.status_label.setObjectName("statusLabel")
#             self.status_label.setProperty("status", "ready")
#             top_layout.addWidget(self.status_label)
            
#             # 关闭按钮
#             close_btn = QPushButton("×")
#             close_btn.setObjectName("closeBtn")
#             close_btn.clicked.connect(self.hide)
#             top_layout.addWidget(close_btn)
            
#             layout.addWidget(top_bar)
            
#             # 搜索框和置顶按钮
#             search_layout = QHBoxLayout()
#             search_layout.setSpacing(8)
#             search_layout.setContentsMargins(8, 8, 8, 8)
            
#             # 搜索框
#             self.search_input = QLineEdit()
#             self.search_input.setPlaceholderText("🔍 搜索...")
#             self.search_input.setFixedWidth(180)
#             self.search_input.textChanged.connect(self.filter_history)
#             search_layout.addWidget(self.search_input)
            
#             # 置顶按钮
#             self.top_button = QPushButton("📌置顶")
#             self.top_button.setObjectName("topButton")
#             self.top_button.setCheckable(True)
#             self.top_button.setFixedSize(52, 52)
#             self.top_button.clicked.connect(self.toggle_top_window)
#             search_layout.addWidget(self.top_button)
            
#             # 更新置顶按钮样式
#             self.top_button.setStyleSheet("""
#                 #topButton {
#                     border-radius: 18px;
#                     font-size: 14px;
#                     background-color: #f5f5f5;
#                     border: none;
#                     padding: 0;
#                     margin: 0;
#                 }
                
#                 #topButton:checked {
#                     background-color: #1976d2;
#                     color: white;
#                 }
                
#                 #topButton:hover {
#                     background-color: #eeeeee;
#                 }
#             """)
            
#             search_layout.addStretch()
#             layout.addLayout(search_layout)
            
#             # 初始化分类按钮列表
#             self.category_buttons = []
            
#             # 创建分类布局
#             category_layout = QHBoxLayout()
#             category_layout.setSpacing(8)
#             category_layout.setContentsMargins(8, 0, 8, 0)
            
#             # 创建默认分类按钮
#             default_btn = QPushButton("默认")
#             default_btn.setCheckable(True)
#             default_btn.setStyleSheet("""
#                 QPushButton {
#                     padding: 8px 16px;
#                     background-color: #f5f5f5;
#                     color: #424242;
#                     border: 2px solid #e0e0e0;
#                     border-radius: 18px;
#                     font-size: 13px;
#                     min-height: 36px;
#                 }
#                 QPushButton:hover {
#                     background-color: #e3f2fd;
#                     border-color: #90caf9;
#                     color: #1976d2;
#                 }
#                 QPushButton:checked {
#                     background-color: #1976d2;
#                     color: white;
#                     border-color: #1565c0;
#                 }
#             """)
#             default_btn.clicked.connect(lambda: self.toggle_category_filter(default_btn))
#             category_layout.addWidget(default_btn)
#             self.category_buttons.append(default_btn)
            
#             # 添加弹性空间
#             category_layout.addStretch()
            
#             # 将分类布局添加到主布局
#             layout.addLayout(category_layout)
            
#             # 历史记录列表
#             self.history_list = QListWidget()
#             self.history_list.setVerticalScrollMode(QAbstractItemView.ScrollMode.ScrollPerPixel)
#             self.history_list.itemClicked.connect(lambda item: self.copy_selected_item(item))
#             self.history_list.setSelectionMode(QAbstractItemView.SelectionMode.SingleSelection)
#             self.history_list.setStyleSheet("""
#                 QScrollBar:vertical {
#                     border: none;
#                     background: #f5f5f5;
#                     width: 8px;
#                     border-radius: 4px;
#                 }
#                 QScrollBar::handle:vertical {
#                     background: #bdbdbd;
#                     border-radius: 4px;
#                 }
#                 QScrollBar::handle:vertical:hover {
#                     background: #9e9e9e;
#                 }
#             """)
#             layout.addWidget(self.history_list)
            
#             # 底部状态栏
#             bottom_layout = QHBoxLayout()
#             bottom_layout.setSpacing(8)
            
#             clear_btn = QPushButton("🗑️ 清空历史")
#             clear_btn.setObjectName("clearBtn")
#             clear_btn.clicked.connect(self.clear_history)
#             bottom_layout.addStretch()
#             bottom_layout.addWidget(clear_btn)
            
#             layout.addLayout(bottom_layout)
            
#             # 添加窗口阴影
#             shadow = QGraphicsDropShadowEffect(self)
#             shadow.setBlurRadius(20)
#             shadow.setColor(QColor(0, 0, 0, 30))
#             shadow.setOffset(0, 2)
#             main_widget.setGraphicsEffect(shadow)
            
#             # 更新按钮样式
#             self.setStyleSheet(self.styleSheet() + """
#                 QLineEdit {
#                     padding: 6px 12px;
#                     border: 1px solid #e0e0e0;
#                     border-radius: 16px;
#                     background-color: #f8f9fa;
#                     font-size: 13px;
#                     color: #333;
#                 }
                
#                 #topButton {
#                     padding: 6px 12px;
#                     border-radius: 16px;
#                     font-size: 13px;
#                     background-color: #f5f5f5;
#                     border: none;
#                     color: #424242;
#                 }
                
#                 #topButton:checked {
#                     background-color: #e3f2fd;
#                     color: #1976d2;
#                 }
#             """)
            
#             logging.info("UI初始化完成")
#         except Exception as e:
#             logging.error(f"初始化UI时发生错误: {str(e)}")

#     def init_components(self):
#         """初始化所有UI组件"""
#         try:
#             # 顶部工具栏
#             top_bar = QWidget()
#             top_layout = QHBoxLayout(top_bar)
#             top_layout.setContentsMargins(0, 0, 0, 0)
            
#             # 标题
#             title_label = QLabel("📋 剪贴板历史")
#             title_label.setStyleSheet("""
#                 font-size: 16px;
#                 font-weight: bold;
#                 color: #2196f3;
#             """)
#             top_layout.addWidget(title_label)
            
#             # 关闭按钮
#             close_btn = QPushButton("✕")
#             close_btn.setFixedSize(30, 30)
#             close_btn.setStyleSheet("""
#                 QPushButton {
#                     background-color: transparent;
#                     border-radius: 15px;
#                     font-size: 16px;
#                     color: #757575;
#                     padding: 0;
#                 }
#                 QPushButton:hover {
#                     background-color: #ffebee;
#                     color: #f44336;
#                 }
#             """)
#             close_btn.clicked.connect(self.hide)
#             top_layout.addWidget(close_btn)
            
#             self.main_layout.addWidget(top_bar)
            
#             # 搜索框
#             search_widget = QWidget()
#             search_layout = QHBoxLayout(search_widget)
#             search_layout.setContentsMargins(0, 0, 0, 0)
            
#             self.search_input = QLineEdit()
#             self.search_input.setPlaceholderText("🔍 搜索剪贴板历史...")
#             self.search_input.textChanged.connect(self.filter_history)
#             search_layout.addWidget(self.search_input)
            
#             self.top_button = QPushButton("📌 置顶")
#             self.top_button.setFixedWidth(80)
#             self.top_button.clicked.connect(self.toggle_top_window)
#             search_layout.addWidget(self.top_button)
            
#             self.main_layout.addWidget(search_widget)
            
#             # 历史记录列表
#             self.history_list = QListWidget()
#             self.history_list.setVerticalScrollMode(QAbstractItemView.ScrollMode.ScrollPerPixel)
#             self.history_list.itemClicked.connect(self.copy_selected_item)
#             self.main_layout.addWidget(self.history_list)
            
#             # 底部状态栏
#             bottom_widget = QWidget()
#             bottom_layout = QHBoxLayout(bottom_widget)
#             bottom_layout.setContentsMargins(0, 0, 0, 0)
            
#             self.status_label = QLabel()
#             self.status_label.setStyleSheet("color: #757575; font-size: 12px;")
#             bottom_layout.addWidget(self.status_label)
            
#             clear_btn = QPushButton("🗑️ 清空历史")
#             clear_btn.setStyleSheet("""
#                 QPushButton {
#                     background-color: #f5f5f5;
#                     color: #757575;
#                     border: 1px solid #e0e0e0;
#                 }
#                 QPushButton:hover {
#                     background-color: #fbe9e7;
#                     color: #d32f2f;
#                     border-color: #ffccbc;
#                 }
#             """)
#             clear_btn.clicked.connect(self.clear_history)
#             bottom_layout.addWidget(clear_btn)
            
#             self.main_layout.addWidget(bottom_widget)
            
#             # 设置右键菜单
#             self.setup_context_menu()
            
#             logging.info("UI组件初始化完成")
#         except Exception as e:
#             logging.error(f"初始化UI组件时发生错误: {str(e)}")

#     def setup_history_list(self):
#         """初始化历史记录列表"""
#         self.history_list = QListWidget()
#         self.history_list.setVerticalScrollMode(QAbstractItemView.ScrollMode.ScrollPerPixel)
#         self.history_list.itemClicked.connect(self.copy_selected_item)
#         self.layout().addWidget(self.history_list)

#     def setup_search_bar(self):
#         """初始化搜索栏"""
#         search_layout = QHBoxLayout()
        
#         self.search_input = QLineEdit()
#         self.search_input.setPlaceholderText("🔍 搜索剪贴板历史...")
#         self.search_input.textChanged.connect(self.filter_history)
#         search_layout.addWidget(self.search_input)
        
#         self.top_button = QPushButton("📌 置顶")
#         self.top_button.setFixedWidth(80)
#         self.top_button.clicked.connect(self.toggle_top_window)
#         search_layout.addWidget(self.top_button)
        
#         self.layout().addLayout(search_layout)

#     def setup_bottom_bar(self):
#         """初始化底部状态栏"""
#         bottom_bar = QWidget()
#         bottom_layout = QHBoxLayout(bottom_bar)
#         bottom_layout.setContentsMargins(0, 0, 0, 0)
        
#         self.status_label = QLabel()
#         bottom_layout.addWidget(self.status_label)
        
#         clear_btn = QPushButton("🗑️ 清空历史")
#         clear_btn.clicked.connect(self.clear_history)
#         bottom_layout.addWidget(clear_btn)
        
#         self.layout().addWidget(bottom_bar)

#     def setup_autosave(self):
#         """设置自动保存"""
#         self.autosave_timer = QTimer(self)
#         self.autosave_timer.timeout.connect(self.save_history)
#         self.autosave_timer.start(60000)  # 每分钟自动保存

#     def setup_title_bar(self):
#         """设置自定义标题栏"""
#         title_bar = QWidget()
#         title_bar.setFixedHeight(36)
#         title_bar.setObjectName("titleBar")
        
#         # 标题栏样式
#         title_bar.setStyleSheet("""
#             #titleBar {
#                 background-color: #2196f3;
#                 border-top-left-radius: 8px;
#                 border-top-right-radius: 8px;
#             }
            
#             #titleLabel {
#                 color: white;
#                 font-size: 14px;
#                 font-weight: bold;
#             }
            
#             #closeBtn {
#                 min-width: 36px;
#                 min-height: 36px;
#                 font-family: "Segoe UI";
#                 font-size: 16px;
#                 color: white;
#                 border: none;
#                 border-top-right-radius: 8px;
#                 background: transparent;
#             }
            
#             #closeBtn:hover {
#                 background-color: #e81123;
#             }
            
#             #closeBtn:pressed {
#                 background-color: #d10f1f;
#             }
#         """)
        
#         # 标题栏布局
#         layout = QHBoxLayout(title_bar)
#         layout.setContentsMargins(10, 0, 0, 0)
#         layout.setSpacing(0)
        
#         # 图标和标题
#         icon_label = QLabel("📋")
#         icon_label.setStyleSheet("color: white; font-size: 16px;")
#         layout.addWidget(icon_label)
        
#         title_label = QLabel(" 剪贴板助手")
#         title_label.setObjectName("titleLabel")
#         layout.addWidget(title_label)
        
#         layout.addStretch()
        
#         # 关闭按钮
#         close_btn = QPushButton("×")
#         close_btn.setObjectName("closeBtn")
#         close_btn.clicked.connect(self.hide)  # 直接隐藏窗口而不是退出程序
#         layout.addWidget(close_btn)
        
#         # 添加到主布局
#         self.layout().addWidget(title_bar)
        
#         # 保存标题栏引用并设置事件过滤器
#         self._title_bar = title_bar
#         title_bar.installEventFilter(self)

#     def eventFilter(self, obj, event):
#         """事件过滤器，用于处理标题栏拖动"""
#         if obj == self._title_bar:
#             if event.type() == QEvent.Type.MouseButtonPress:
#                 if event.button() == Qt.MouseButton.LeftButton:
#                     self._drag_pos = event.globalPosition().toPoint() - self.frameGeometry().topLeft()
#                     return True
#             elif event.type() == QEvent.Type.MouseMove:
#                 if event.buttons() & Qt.MouseButton.LeftButton:
#                     self.move(event.globalPosition().toPoint() - self._drag_pos)
#                     return True
#         return super().eventFilter(obj, event)

#     def mousePressEvent(self, event):
#         """处理鼠标按下事件"""
#         if event.button() == Qt.MouseButton.LeftButton:
#             # 直接使用 QPoint，不需要再调用 toPoint()
#             pos = event.position().toPoint()
            
#             # 检查是否在控件区域内
#             if (self.search_input.geometry().contains(pos) or 
#                 self.top_button.geometry().contains(pos) or 
#                 self.history_list.geometry().contains(pos)):  # pos 已经是 QPoint 了
#                 super().mousePressEvent(event)
#                 return
            
#             rect = self.rect()
#             edge_size = 6
            
#             # 检测鼠标位置在哪个边缘
#             if pos.x() <= edge_size:  # 左边
#                 if pos.y() <= edge_size:
#                     self._resize_area = 'top-left'
#                 elif pos.y() >= rect.height() - edge_size:
#                     self._resize_area = 'bottom-left'
#                 else:
#                     self._resize_area = 'left'
#             elif pos.x() >= rect.width() - edge_size:  # 右边
#                 if pos.y() <= edge_size:
#                     self._resize_area = 'top-right'
#                 elif pos.y() >= rect.height() - edge_size:
#                     self._resize_area = 'bottom-right'
#                 else:
#                     self._resize_area = 'right'
#             elif pos.y() <= edge_size:  # 上边
#                 self._resize_area = 'top'
#             elif pos.y() >= rect.height() - edge_size:  # 下边
#                 self._resize_area = 'bottom'
#             else:
#                 self._resize_area = None
#                 self._drag_pos = event.globalPosition().toPoint() - self.frameGeometry().topLeft()
            
#             self._start_pos = event.globalPosition().toPoint()
#             self._start_geometry = self.geometry()

#     def mouseMoveEvent(self, event):
#         """处理鼠标移动事件"""
#         if event.buttons() & Qt.MouseButton.LeftButton:
#             if self._resize_area:
#                 # 计算位置差异
#                 diff = event.globalPosition().toPoint() - self._start_pos
#                 new_geometry = QRect(self._start_geometry)
                
#                 # 根据不同的调整区域更新几何形状
#                 if 'left' in self._resize_area:
#                     new_geometry.setLeft(new_geometry.left() + diff.x())
#                 if 'right' in self._resize_area:
#                     new_geometry.setRight(new_geometry.right() + diff.x())
#                 if 'top' in self._resize_area:
#                     new_geometry.setTop(new_geometry.top() + diff.y())
#                 if 'bottom' in self._resize_area:
#                     new_geometry.setBottom(new_geometry.bottom() + diff.y())
                
#                 # 确保窗口不会小于最小尺寸
#                 if (new_geometry.width() >= self.minimumWidth() and 
#                     new_geometry.height() >= self.minimumHeight()):
#                     self.setGeometry(new_geometry)
#             elif hasattr(self, '_drag_pos'):
#                 self.move(event.globalPosition().toPoint() - self._drag_pos)

#     def mouseReleaseEvent(self, event):
#         """处理鼠标释放事件"""
#         self._resize_area = None
#         self._start_pos = None
#         self._start_geometry = None

#     def changeCursor(self, event):
#         """根据鼠标位置改变光标形状"""
#         try:
#             # 获取鼠标位置
#             if isinstance(event, QMoveEvent):
#                 pos = self.mapFromGlobal(QCursor.pos())
#             else:
#                 pos = event.position().toPoint()
            
#             rect = self.rect()
#             edge_size = 6
            
#             # 获取所有交互区域
#             search_rect = self.search_input.geometry()
#             top_button_rect = self.top_button.geometry()
#             list_rect = self.history_list.geometry()
            
#             # 检查是否在控件区域内
#             if (search_rect.contains(pos) or 
#                 top_button_rect.contains(pos) or 
#                 list_rect.contains(pos)):
#                 self.setCursor(Qt.CursorShape.ArrowCursor)
#                 return
            
#             # 边缘区域检测
#             is_left = pos.x() <= edge_size
#             is_right = pos.x() >= rect.width() - edge_size
#             is_top = pos.y() <= edge_size
#             is_bottom = pos.y() >= rect.height() - edge_size
            
#             # 设置对应的光标
#             if is_left and is_top:
#                 self.setCursor(Qt.CursorShape.SizeFDiagCursor)
#             elif is_left and is_bottom:
#                 self.setCursor(Qt.CursorShape.SizeBDiagCursor)
#             elif is_right and is_top:
#                 self.setCursor(Qt.CursorShape.SizeBDiagCursor)
#             elif is_right and is_bottom:
#                 self.setCursor(Qt.CursorShape.SizeFDiagCursor)
#             elif is_left or is_right:
#                 self.setCursor(Qt.CursorShape.SizeHorCursor)
#             elif is_top or is_bottom:
#                 self.setCursor(Qt.CursorShape.SizeVerCursor)
#             else:
#                 self.setCursor(Qt.CursorShape.ArrowCursor)
#         except Exception as e:
#             logging.error(f"更改光标时发生错误: {str(e)}")
#             self.setCursor(Qt.CursorShape.ArrowCursor)

#     def enterEvent(self, event):
#         """鼠标进入窗口事件"""
#         self.changeCursor(event)

#     def leaveEvent(self, event):
#         """鼠标离开窗口事件"""
#         self.setCursor(Qt.CursorShape.ArrowCursor)

#     def moveEvent(self, event):
#         """处理窗口移动事件"""
#         self.changeCursor(event)

#     def setup_tray_icon(self):
#         """设置系统托盘图标"""
#         try:
#             self.tray_icon = QSystemTrayIcon(self)
            
#             # 使用系统内置的标准图标
#             icon = self.style().standardIcon(QStyle.StandardPixmap.SP_ComputerIcon)
#             self.tray_icon.setIcon(icon)
#             self.setWindowIcon(icon)
            
#             # 创建托盘菜单
#             tray_menu = QMenu()
#             show_action = tray_menu.addAction('显示')
#             show_action.triggered.connect(self.toggle_window)
#             quit_action = tray_menu.addAction('退出')
#             quit_action.triggered.connect(self.quit_application)
            
#             # 设置托盘菜单
#             self.tray_icon.setContextMenu(tray_menu)
#             self.tray_icon.activated.connect(self.tray_icon_activated)
#             self.tray_icon.show()
            
#             logging.info("托盘图标设置成功")
#         except Exception as e:
#             logging.error(f"设置托盘图标时发生错误: {str(e)}")

#     def tray_icon_activated(self, reason):
#         """处理托盘图标点击事件"""
#         if reason == QSystemTrayIcon.ActivationReason.Trigger:
#             self.toggle_window()

#     def setup_clipboard_monitoring(self):
#         """设置剪贴板监控"""
#         self.clipboard_timer = QTimer(self)
#         self.clipboard_timer.timeout.connect(self.check_clipboard)
#         self.clipboard_timer.start(1000)  # 每秒检查一次

#     def check_clipboard(self):
#         """检查剪贴板内容"""
#         try:
#             # 如果正在进行手动复制操作，跳过检查
#             if hasattr(self, '_manual_copy') and self._manual_copy:
#                 return
            
#             # 获取剪贴板内容，设置超时机制
#             start_time = time.time()
#             while time.time() - start_time < 0.5:  # 500ms 超时
#                 try:
#                     current_text = self.clipboard.text().strip()
#                     break
#                 except Exception:
#                     time.sleep(0.1)
#             else:
#                 logging.warning("获取剪贴板内容超时")
#                 return

#             if not current_text or current_text == self.last_text:
#                 return
            
#             self.last_text = current_text
#             new_item = ClipboardItem(current_text)
            
#             if not any(item.full_text == current_text for item in self.clipboard_history):
#                 self.clipboard_history.insert(0, new_item)
                
#                 # 使用 QTimer 确保在主线程中更新 UI
#                 QTimer.singleShot(0, lambda: self.update_list_safely(new_item))
                
#                 if len(self.clipboard_history) > 50:
#                     self.clipboard_history.pop()
#                     QTimer.singleShot(0, lambda: self.remove_last_item_safely())
                
#                 # 使用延迟保存避免频繁写入
#                 self.schedule_save()
                
#         except Exception as e:
#             logging.error(f"检查剪贴板时发生错误: {str(e)}")
#             self.restart_clipboard_monitor()

#     def update_list_safely(self, item):
#         """安全地更新列表"""
#         try:
#             self.history_list.insertItem(0, QListWidgetItem(item.display_text()))
#         except Exception as e:
#             logging.error(f"更新列表时发生错误: {str(e)}")

#     def remove_last_item_safely(self):
#         """安全地移除最后一项"""
#         try:
#             self.history_list.takeItem(self.history_list.count() - 1)
#         except Exception as e:
#             logging.error(f"移除列表项时发生错误: {str(e)}")

#     def restart_clipboard_monitor(self):
#         """重启剪贴板监控"""
#         try:
#             self.clipboard_timer.stop()
#             time.sleep(0.5)
#             self.clipboard_timer.start()
#             logging.info("剪贴板监控已重启")
#         except Exception as e:
#             logging.error(f"重启剪贴板监控时发生错误: {str(e)}")

#     def schedule_save(self):
#         """计划保存操作"""
#         if hasattr(self, '_save_timer'):
#             self._save_timer.stop()
#         else:
#             self._save_timer = QTimer(self)
#             self._save_timer.setSingleShot(True)
#             self._save_timer.timeout.connect(self.save_history)
        
#         self._save_timer.start(5000)  # 5秒后保存

#     def clear_history(self):
#         """清空历史记录"""
#         reply = QMessageBox.question(self, '确认', '确定要清空所有历史记录吗？',
#                                    QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No)
#         if reply == QMessageBox.StandardButton.Yes:
#             self.clipboard_history.clear()
#             self.history_list.clear()
#             self.save_history()
#             self.status_label.setText("历史记录已清空")

#     def show_window(self):
#         """显示窗口"""
#         try:
#             # 更新历史记录列表
#             self.update_history_list()
            
#             # 显示窗口
#             self.show()
#             self.activateWindow()
#             self.raise_()
            
#             # 将窗口移动到屏幕中央
#             screen = QApplication.primaryScreen().geometry()
#             self.move(
#                 (screen.width() - self.width()) // 2,
#                 (screen.height() - self.height()) // 2
#             )
            
#             logging.info("窗口已显示")
#         except Exception as e:
#             logging.error(f"显示窗口时发生错误: {str(e)}")

#     def toggle_window(self):
#         """切换窗口显示状态"""
#         try:
#             if self.isVisible():
#                 self.hide()
#             else:
#                 # 确保窗口在当前鼠标所在的屏幕上显示
#                 cursor_pos = QCursor.pos()
#                 screen = QApplication.screenAt(cursor_pos)
#                 if screen:
#                     screen_geometry = screen.geometry()
#                     x = cursor_pos.x() - self.width() // 2
#                     y = cursor_pos.y() - 20  # 稍微偏移，避免遮挡鼠标
                    
#                     # 确保窗口不会超出屏幕边界
#                     x = max(screen_geometry.left(), min(x, screen_geometry.right() - self.width()))
#                     y = max(screen_geometry.top(), min(y, screen_geometry.bottom() - self.height()))
                    
#                     self.move(x, y)
                
#                 self.show()
#                 self.activateWindow()
#                 self.raise_()
#         except Exception as e:
#             logging.error(f"切换窗口显示状态时发生错误: {str(e)}")

#     def _show_window_safely(self):
#         """安全地显示窗口"""
#         try:
#             self.show()
#             self.activateWindow()
#             self.raise_()
            
#             # 将窗口移动到屏幕中央
#             screen = QApplication.primaryScreen().geometry()
#             self.move(
#                 (screen.width() - self.width()) // 2,
#                 (screen.height() - self.height()) // 2
#             )
#             logging.info("窗口已显示")
#         except Exception as e:
#             logging.error(f"显示窗口时发生错误: {str(e)}")

#     def update_history_list(self):
#         """更新历史记录列表"""
#         self.history_list.clear()
#         for item in self.clipboard_history:
#             self.history_list.addItem(QListWidgetItem(item.display_text()))

#     def save_history(self):
#         """保存历史记录"""
#         history_data = [item.to_dict() for item in self.clipboard_history]
#         with open('clipboard_history.json', 'w', encoding='utf-8') as f:
#             json.dump(history_data, f, ensure_ascii=False, indent=2)

#     def filter_history(self, text):
#         """搜索过滤"""
#         self.apply_filters()

#     def copy_selected_item(self, item):
#         """复制选中的项目"""
#         if not item:
#             return
#         try:
#             self._manual_copy = True
#             text = item.text()
#             if '] ' in text:
#                 text = text.split('] ', 1)[1]
            
#             for history_item in self.clipboard_history:
#                 if history_item.display_text() == text:
#                     self.clipboard_timer.stop()
#                     self.clipboard.setText(history_item.full_text)
#                     self.update_status(f"已复制: {history_item.full_text[:30]}...", "copied")
#                     QTimer.singleShot(2000, lambda: self.update_status("就绪", "ready"))  # 2秒后恢复就绪状态
#                     QTimer.singleShot(500, self._reset_clipboard_monitor)
#                     break
#         except Exception as e:
#             logging.error(f"复制选中项目时发生错误: {str(e)}")
#             self._reset_clipboard_monitor()

#     def _reset_clipboard_monitor(self):
#         """重置剪贴板监控"""
#         try:
#             self._manual_copy = False
#             self.clipboard_timer.start()
#         except Exception as e:
#             logging.error(f"重置剪贴板监控时发生错误: {str(e)}")

#     def quit_application(self):
#         """安全退出应用"""
#         try:
#             reply = QMessageBox.question(
#                 self,
#                 '确认退出',
#                 '确定要退出剪贴板助手吗？',
#                 QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
#                 QMessageBox.StandardButton.No
#             )
            
#             if reply == QMessageBox.StandardButton.Yes:
#                 logging.info("开始退出程序...")
                
#                 # 移除全局热键
#                 try:
#                     keyboard.unhook_all()
#                     logging.info("已移除全局热键")
#                 except Exception as e:
#                     logging.error(f"移除热键时发生错误: {str(e)}")

#                 # 停止所有定时器
#                 for timer_name in ['clipboard_timer', 'autosave_timer', 'hotkey_check_timer']:
#                     if hasattr(self, timer_name):
#                         timer = getattr(self, timer_name)
#                         timer.stop()
                
#                 # 保存历史记录
#                 self.save_history()
                
#                 # 隐藏托盘图标
#                 if hasattr(self, 'tray_icon'):
#                     self.tray_icon.hide()
                
#                 logging.info("程序正常退出")
#                 QApplication.quit()
#         except Exception as e:
#             logging.error(f"退出时发生错误: {str(e)}")
#             sys.exit(1)

#     def load_history(self):
#         """加载历史记录"""
#         if os.path.exists('clipboard_history.json'):
#             with open('clipboard_history.json', 'r', encoding='utf-8') as f:
#                 data = json.load(f)
#                 if isinstance(data, list):
#                     self.clipboard_history = [ClipboardItem.from_dict(item) for item in data]
#                     self.update_history_list()

#     def closeEvent(self, event):
#         """处理窗口关闭事件"""
#         try:
#             event.ignore()  # 忽略关闭事件
#             self.hide()     # 隐藏窗口而不是关闭
#             logging.info("窗口已隐藏")
#         except Exception as e:
#             logging.error(f"关闭窗口时发生错误: {str(e)}")

#     def setup_shortcuts(self):
#         """设置快捷键"""
#         try:
#             # 移除所有已存在的热键
#             keyboard.unhook_all()
            
#             # 使用组合键方式注册
#             keyboard.add_hotkey('ctrl+o', self.toggle_window, suppress=True)
#             logging.info("全局快捷键 Ctrl+O 注册成功")
            
#             # 添加热键状态检查定时器
#             self.hotkey_check_timer = QTimer(self)
#             self.hotkey_check_timer.timeout.connect(self.check_hotkey_status)
#             self.hotkey_check_timer.start(30000)  # 每30秒检查一次
#         except Exception as e:
#             logging.error(f"设置快捷键时发生错误: {str(e)}")

#     def check_hotkey_status(self):
#         """检查热键状态"""
#         try:
#             # 重新注册热键
#             keyboard.unhook_all()
#             keyboard.add_hotkey('ctrl+o', self.toggle_window, suppress=True)
#             logging.info("热键状态已刷新")
#         except Exception as e:
#             logging.error(f"检查热键状态时发生错误: {str(e)}")

#     def eventFilter(self, obj, event):
#         """事件过滤器，用于处理全局快捷键"""
#         try:
#             if event.type() == QEvent.Type.KeyPress:
#                 if (event.key() == Qt.Key.Key_O and 
#                     event.modifiers() == Qt.KeyboardModifier.ControlModifier):
#                     self.toggle_window()
#                     return True
#         except Exception as e:
#             logging.error(f"处理快捷键事件时发生错误: {str(e)}")
#         return super().eventFilter(obj, event)

#     def toggle_top_window(self):
#         """切换窗口置顶状态"""
#         try:
#             self.is_top = not self.is_top
            
#             # 保存当前窗口位置
#             current_pos = self.pos()
            
#             if self.is_top:
#                 self.setWindowFlags(self.windowFlags() | Qt.WindowType.WindowStaysOnTopHint)
#                 self.top_button.setStyleSheet(self.top_button.styleSheet() + """
#                     #topButton:checked {
#                         background-color: #1976d2;
#                         color: white;
#                     }
#                 """)
#             else:
#                 self.setWindowFlags(self.windowFlags() & ~Qt.WindowType.WindowStaysOnTopHint)
#                 self.top_button.setStyleSheet(self.top_button.styleSheet().replace("""
#                     #topButton:checked {
#                         background-color: #1976d2;
#                         color: white;
#                     }
#                 """, ""))
            
#             # 恢复窗口位置并显示
#             self.move(current_pos)
#             self.show()
#             self.activateWindow()
            
#             logging.info(f"窗口置顶状态已切换: {'置顶' if self.is_top else '取消置顶'}")
#         except Exception as e:
#             logging.error(f"切换窗口置顶状态时发生错误: {str(e)}")

#     def show_add_category_dialog(self):
#         """显示添加分类对话框"""
#         category, ok = QInputDialog.getText(
#             self, 
#             '新建分类', 
#             '请输入分类名称:',
#             QLineEdit.EchoMode.Normal,
#             ""
#         )
#         if ok and category.strip():
#             self.add_category(category.strip())

#     def add_category(self, category):
#         """添加新分类"""
#         if category == "默认" or any(btn.text() == category for btn in self.category_buttons):
#             return
        
#         # 创建新的分类按钮
#         btn = QPushButton(category)
#         btn.setCheckable(True)
#         btn.setStyleSheet("""
#             QPushButton {
#                 padding: 8px 16px;
#                 background-color: #f5f5f5;
#                 color: #424242;
#                 border: 2px solid #e0e0e0;
#                 border-radius: 18px;
#                 font-size: 13px;
#                 min-height: 36px;
#             }
#             QPushButton:hover {
#                 background-color: #e3f2fd;
#                 border-color: #90caf9;
#                 color: #1976d2;
#             }
#             QPushButton:checked {
#                 background-color: #1976d2;
#                 color: white;
#                 border-color: #1565c0;
#             }
#             QPushButton:pressed {
#                 background-color: #1565c0;
#                 border-color: #0d47a1;
#             }
#         """)
#         btn.clicked.connect(lambda: self.toggle_category_filter(btn))
        
#         # 在最后（弹性空间）之前插入
#         self.category_inner_layout.insertWidget(
#             self.category_inner_layout.count() - 1, 
#             btn
#         )
#         self.category_buttons.append(btn)

#     def toggle_category_filter(self, button):
#         """切换分类筛选"""
#         try:
#             # 更新按钮状态
#             was_checked = button.isChecked()
            
#             # 如果是取消选中，则显示所有内容
#             if was_checked:
#                 button.setChecked(False)
#             else:
#                 # 如果是选中，则先取消其他按钮的选中状态
#                 for btn in self.category_buttons:
#                     btn.setChecked(False)
#                 button.setChecked(True)
            
#             self.apply_filters()
#         except Exception as e:
#             logging.error(f"切换分类过滤时发生错误: {str(e)}")

#     def apply_filters(self):
#         """应用所有过滤器"""
#         self.history_list.clear()
#         search_text = self.search_input.text().lower()
        
#         # 获取选中的分类
#         selected_category = next((btn.text() for btn in self.category_buttons if btn.isChecked()), None)
        
#         for item in self.clipboard_history:
#             # 检查分类过滤
#             category_match = (not selected_category) or (item.category == selected_category)
#             # 检查搜索文本
#             text_match = not search_text or search_text in item.full_text.lower()
            
#             if category_match and text_match:
#                 list_item = QListWidgetItem()
#                 display_text = item.display_text()
#                 if item.category != "默认":
#                     display_text = f"[{item.category}] {display_text}"
#                 list_item.setText(display_text)
                
#                 if item.category != "默认":
#                     list_item.setBackground(QColor("#e3f2fd"))
#                     list_item.setToolTip(f"分类: {item.category}")
                
#                 self.history_list.addItem(list_item)

#     def set_item_category(self, item):
#         """设置项目分类"""
#         if not item:
#             return
        
#         try:
#             # 找到对应的历史记录项
#             text = item.text()
#             if '] ' in text:  # 如果有分类标记，去掉它
#                 text = text.split('] ', 1)[1]
            
#             for history_item in self.clipboard_history:
#                 if history_item.display_text() == text:
#                     # 获取所有现有分类
#                     categories = [btn.text() for btn in self.category_buttons]
                    
#                     # 显示分类选择对话框
#                     category, ok = QInputDialog.getItem(
#                         self,
#                         '设置分类',
#                         '选择分类:',
#                         categories,
#                         current=categories.index(history_item.category) if history_item.category in categories else 0,
#                         editable=True
#                     )
                    
#                     if ok and category:
#                         # 如果是新分类，添加到分类列表
#                         if category not in categories:
#                             self.add_category(category)
                        
#                         # 更新项目分类
#                         history_item.category = category
#                         self.save_history()
#                         self.apply_filters()
#                     break
#         except Exception as e:
#             logging.error(f"设置分类时发生错误: {str(e)}")

#     def setup_context_menu(self):
#         """设置右键菜单"""
#         self.history_list.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
#         self.history_list.customContextMenuRequested.connect(self.show_context_menu)

#     def show_context_menu(self, position):
#         """显示右键菜单"""
#         item = self.history_list.itemAt(position)
#         if not item:
#             return
        
#         menu = QMenu()
#         copy_action = menu.addAction("复制")
        
#         # 获取当前项的分类
#         current_category = None
#         for history_item in self.clipboard_history:
#             if history_item.display_text() == item.text():
#                 current_category = history_item.category
#                 break
        
#         # 添加分类相关菜单项
#         category_menu = menu.addMenu("分类")
#         set_category_action = category_menu.addAction("设置分类")
#         if current_category and current_category != "默认":
#             remove_category_action = category_menu.addAction("取消分类")
#         else:
#             remove_category_action = None
        
#         delete_action = menu.addAction("删除")
        
#         action = menu.exec(self.history_list.viewport().mapToGlobal(position))
        
#         if action == copy_action:
#             self.copy_selected_item(item)
#         elif action == set_category_action:
#             self.set_item_category(item)
#         elif action == delete_action:
#             self.delete_item(item)
#         elif remove_category_action and action == remove_category_action:
#             self.remove_item_category(item)

#     def remove_item_category(self, item):
#         """取消项目的分类"""
#         if not item:
#             return
        
#         try:
#             # 找到对应的历史记录项
#             for history_item in self.clipboard_history:
#                 if history_item.display_text() == item.text():
#                     # 设置为默认分类
#                     history_item.category = "默认"
                    
#                     # 保存更改
#                     self.save_history()
                    
#                     # 更新显示
#                     self.apply_filters()
#                     break
#         except Exception as e:
#             logging.error(f"取消分类时发生错误: {str(e)}")

#     def center_window(self):
#         """将窗口居中显示"""
#         try:
#             # 获取屏幕几何信息
#             screen = QApplication.primaryScreen().geometry()
            
#             # 获取窗口几何信息
#             window_geometry = self.geometry()
            
#             # 计算居中位置
#             x = (screen.width() - window_geometry.width()) // 2
#             y = (screen.height() - window_geometry.height()) // 2
            
#             # 移动窗口
#             self.move(x, y)
#             logging.info("窗口已居中显示")
#         except Exception as e:
#             logging.error(f"居中显示窗口时发生错误: {str(e)}")

#     def update_status(self, text="就绪", status="ready"):
#         """更新状态标签"""
#         self.status_label.setText(text)
#         self.status_label.setProperty("status", status)
#         self.status_label.style().unpolish(self.status_label)
#         self.status_label.style().polish(self.status_label)

# if __name__ == '__main__':
#     try:
#         # 设置日志
#         setup_logging()
#         logging.info("程序开始启动...")
        
#         # 创建应用实例
#         app = QApplication(sys.argv)
#         app.setQuitOnLastWindowClosed(False)
        
#         # 创建主窗口
#         clipboard_assistant = ClipboardAssistant()
        
#         # 启动应用
#         sys.exit(app.exec())
#     except Exception as e:
#         logging.error(f"程序启动失败: {str(e)}")
#         logging.exception("详细错误信息:")
#         sys.exit(1) 