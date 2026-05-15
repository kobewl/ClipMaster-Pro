from PyQt6.QtWidgets import (
   QListWidget, QAbstractItemView, QMenu,
   QApplication, QMessageBox, QTextEdit,
   QWidget, QHBoxLayout, QVBoxLayout, QLabel,
    QPushButton, QListWidgetItem, QSizePolicy
)
from PyQt6.QtCore import pyqtSignal, Qt, QSize, QTimer, QEvent
from PyQt6.QtGui import QAction, QFont, QIcon, QColor, QPixmap, QImage

from models.clipboard_item import ClipboardItem, ContentType
from utils.logger import logger


class HistoryListItem(QWidget):
    """自定义历史列表项"""
    
    # 亮色主题颜色
    _LIGHT = {
        "border": "#D8E3EE",
        "selected_border": "#0A84FF",
        "selected_bg": "rgba(10, 132, 255, 0.12)",
        "favorite_bg": "rgba(255, 204, 102, 0.14)",
        "card_bg": "rgba(255, 255, 255, 0.75)",
        "hover_bg": "rgba(248, 251, 255, 0.95)",
        "text": "#152033",
        "meta": "#7D8EA3",
        "source": "#0A84FF",
    }
    # 暗色主题颜色
    _DARK = {
        "border": "#29415B",
        "selected_border": "#4CC2FF",
        "selected_bg": "rgba(76, 194, 255, 0.14)",
        "favorite_bg": "rgba(255, 190, 92, 0.16)",
        "card_bg": "rgba(24, 33, 47, 0.76)",
        "hover_bg": "rgba(32, 51, 71, 0.92)",
        "text": "#F5F9FF",
        "meta": "#9FB3C8",
        "source": "#4CC2FF",
    }
    
    # 图片缩略图尺寸
    _THUMB_W = 80
    _THUMB_H = 60

    def __init__(self, item: ClipboardItem, parent=None):
        super().__init__(parent)
        self.item_data = item
        self._selected = False
        self._thumbnail: QPixmap | None = None  # 缩略图缓存
        self._thumbnail_loading = False
        self._list_widget = None  # 由 HistoryList 设置，用于事件转发
        # 允许 QWidget 渲染 stylesheet 中的 background 和 border
        self.setAttribute(Qt.WidgetAttribute.WA_StyledBackground, True)
        self._init_ui()

    def _init_ui(self):
        """初始化UI"""
        layout = QHBoxLayout(self)
        layout.setContentsMargins(14, 12, 14, 12)
        layout.setSpacing(12)

        # 左侧：图标或缩略图
        self.icon_label = QLabel()
        self.icon_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(self.icon_label)

        # 内容区域
        content_layout = QVBoxLayout()
        content_layout.setSpacing(4)
        content_layout.setContentsMargins(0, 0, 0, 0)

        self.text_label = QLabel()
        self.text_label.setWordWrap(False)
        self.text_label.setStyleSheet("border: none; background: transparent;")
        self.text_label.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Preferred)
        font = QFont()
        font.setPointSize(12)
        font.setWeight(QFont.Weight.Medium)
        self.text_label.setFont(font)
        content_layout.addWidget(self.text_label)

        # 时间和来源在一行
        info_layout = QHBoxLayout()
        info_layout.setSpacing(10)
        info_layout.setContentsMargins(0, 0, 0, 0)

        self.time_label = QLabel()
        info_layout.addWidget(self.time_label)

        self.source_label = QLabel()
        info_layout.addWidget(self.source_label)
        info_layout.addStretch()

        content_layout.addLayout(info_layout)

        layout.addLayout(content_layout, 1)

        # 收藏按钮
        self.fav_button = QPushButton()
        self.fav_button.setObjectName("iconButton")
        self.fav_button.setFixedSize(36, 36)
        self.fav_button.setStyleSheet("border: none; background: transparent; font-size: 18px;")
        layout.addWidget(self.fav_button)

        self._refresh_display()
        self._update_style()

    # ── 缩略图生成 ────────────────────────────────────────────
    def _make_thumbnail(self) -> QPixmap | None:
        """加载图片并缩放为缩略图，结果缓存。支持文件路径（新数据）和 Base64（旧数据）。"""
        if self._thumbnail is not None:
            return self._thumbnail
        try:
            content = self.item_data.content
            # 优先从文件路径加载（新数据）
            if content and not content.startswith('data:image'):
                img = QImage()
                if img.load(content) and not img.isNull():
                    pix = QPixmap.fromImage(img)
                    self._thumbnail = pix.scaled(
                        self._THUMB_W, self._THUMB_H,
                        Qt.AspectRatioMode.KeepAspectRatio,
                        Qt.TransformationMode.SmoothTransformation,
                    )
                    return self._thumbnail
            # 兼容旧数据（Base64）
            elif content.startswith('data:image'):
                import base64 as _b64
                raw = _b64.b64decode(content.split(',', 1)[1])
                img = QImage()
                if img.loadFromData(raw) and not img.isNull():
                    pix = QPixmap.fromImage(img)
                    self._thumbnail = pix.scaled(
                        self._THUMB_W, self._THUMB_H,
                        Qt.AspectRatioMode.KeepAspectRatio,
                        Qt.TransformationMode.SmoothTransformation,
                    )
                    return self._thumbnail
        except Exception as e:
            logger.warning(f"生成缩略图失败: {e}")
        return None

    def _load_thumbnail(self):
        """异步加载缩略图（由 QTimer 调用，不阻塞 UI）。"""
        try:
            thumb = self._make_thumbnail()
            if thumb and self.icon_label:
                self.icon_label.setPixmap(thumb)
                self.icon_label.setText("")
                self.icon_label.setStyleSheet(
                    "border: none; background: transparent; border-radius: 4px;"
                )
        except Exception as e:
            logger.warning(f"异步加载缩略图失败: {e}")
        finally:
            self._thumbnail_loading = False

    # ── 内容刷新 ──────────────────────────────────────────────
    def _refresh_display(self):
        """刷新图标/缩略图、文字、时间、收藏按钮"""
        item = self.item_data

        if item.content_type == ContentType.IMAGE:
            # 图片：先显示占位符，再异步加载缩略图
            self.icon_label.setFixedSize(self._THUMB_W, self._THUMB_H)
            self.icon_label.setPixmap(QPixmap())
            self.icon_label.setText("🖼️")
            self.icon_label.setStyleSheet(
                "font-size: 20px; border: none; background: transparent;"
            )
            if not self._thumbnail_loading:
                self._thumbnail_loading = True
                QTimer.singleShot(0, self._load_thumbnail)
        else:
            # 其他类型：emoji 图标
            self.icon_label.setFixedSize(36, 36)
            self.icon_label.setPixmap(QPixmap())
            self.icon_label.setText(item.get_icon())
            self.icon_label.setStyleSheet("font-size: 22px; border: none; background: transparent;")

        self.text_label.setText(item.preview_text(80))
        self.time_label.setText(item.timestamp.strftime("%m-%d %H:%M"))

        # 显示来源
        source_display = item.get_source_display()
        if source_display and source_display != "未知来源":
            self.source_label.setText(f"📍 {source_display}")
            self.source_label.setToolTip(item.get_source_tooltip())
        else:
            self.source_label.setText("")

        self.fav_button.setText("⭐" if item.is_favorite else "☆")
    
    def _update_style(self):
        """根据选中状态和收藏状态更新边框与背景"""
        from views.styles.main_style import StyleManager
        c = self._DARK if StyleManager.is_dark_mode() else self._LIGHT

        self.text_label.setStyleSheet(
            f"color: {c['text']}; border: none; background: transparent;"
        )
        self.time_label.setStyleSheet(
            f"color: {c['meta']}; font-size: 13px; border: none; background: transparent;"
        )
        self.source_label.setStyleSheet(
            f"color: {c['source']}; font-size: 12px; border: none; background: transparent;"
        )
        self.fav_button.setStyleSheet(
            f"border: none; background: transparent; font-size: 18px; color: {c['source']};"
        )
        
        if self._selected:
            self.setStyleSheet(
                f"HistoryListItem {{"
                f"background-color: {c['selected_bg']};"
                f"border: 1px solid {c['selected_border']};"
                f"border-radius: 12px;"
                f"}}"
            )
        elif self.item_data.is_favorite:
            self.setStyleSheet(
                f"HistoryListItem {{"
                f"background-color: {c['favorite_bg']};"
                f"border: 1px solid {c['border']};"
                f"border-radius: 12px;"
                f"}}"
                f"HistoryListItem:hover {{"
                f"background-color: {c['hover_bg']};"
                f"}}"
            )
        else:
            self.setStyleSheet(
                f"HistoryListItem {{"
                f"background-color: {c['card_bg']};"
                f"border: 1px solid {c['border']};"
                f"border-radius: 12px;"
                f"}}"
                f"HistoryListItem:hover {{"
                f"background-color: {c['hover_bg']};"
                f"}}"
            )
    
    def mousePressEvent(self, event):
        """重写鼠标按下事件，手动处理 QListWidget 的多选逻辑。
        setItemWidget 后 widget 会吃掉鼠标事件，导致 QListWidget 无法处理 Ctrl/Shift 多选。
        """
        if event.button() == Qt.MouseButton.LeftButton:
            # 收藏按钮区域由按钮自己处理
            if self.fav_button.geometry().contains(event.pos()):
                super().mousePressEvent(event)
                return
            lw = self._list_widget
            if lw:
                for i in range(lw.count()):
                    li = lw.item(i)
                    if lw.itemWidget(li) == self:
                        modifiers = QApplication.keyboardModifiers()
                        if modifiers == Qt.KeyboardModifier.ControlModifier:
                            li.setSelected(not li.isSelected())
                        elif modifiers == Qt.KeyboardModifier.ShiftModifier:
                            current = lw.currentRow()
                            if current >= 0:
                                start, end = sorted([current, i])
                                for j in range(start, end + 1):
                                    lw.item(j).setSelected(True)
                        else:
                            lw.clearSelection()
                            li.setSelected(True)
                            lw.setCurrentItem(li)
                        break
        super().mousePressEvent(event)

    def set_selected(self, selected: bool):
        """由 HistoryList 调用，更新选中状态样式"""
        if self._selected != selected:
            self._selected = selected
            self._update_style()
    
    def update_item(self, item: ClipboardItem):
        """更新显示"""
        self.item_data = item
        self._thumbnail = None  # 清除旧缩略图缓存
        self._refresh_display()
        self._update_style()


class HistoryList(QListWidget):
    """优化的历史记录列表组件"""
    
    itemCopied = pyqtSignal(object)  # ClipboardItem
    itemDeleted = pyqtSignal(str)    # content_hash
    favoriteToggled = pyqtSignal(str)  # content_hash
    itemsBatchDeleted = pyqtSignal(list)  # list[content_hash]
    
    def __init__(self, parent=None):
        super().__init__(parent)
        self._items = []  # ClipboardItem 列表
        self._item_widgets = {}  # content_hash -> widget
        self._items_by_hash = {}  # content_hash -> ClipboardItem (O(1) 查找)
        self._prev_selected_row = -1  # 上一次选中的行号
        self._init_ui()
        
        # 延迟加载定时器
        self._load_timer = QTimer(self)
        self._load_timer.setSingleShot(True)
        self._load_timer.timeout.connect(self._load_visible_items)
    
    def _init_ui(self):
        """初始化UI"""
        self.setObjectName("historyList")
        self.setVerticalScrollMode(QAbstractItemView.ScrollMode.ScrollPerPixel)
        self.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        self.setSelectionMode(QAbstractItemView.SelectionMode.ExtendedSelection)
        self.setSpacing(4)

        # 连接信号
        self.customContextMenuRequested.connect(self._show_context_menu)
        self.itemClicked.connect(self._handle_item_click)
        self.itemDoubleClicked.connect(self._handle_item_double_click)
        self.verticalScrollBar().valueChanged.connect(self._on_scroll)
        self.currentRowChanged.connect(self._on_selection_changed)
    
    def _on_scroll(self):
        """滚动时延迟加载"""
        self._load_timer.stop()
        self._load_timer.start(50)
    
    def _load_visible_items(self):
        """加载可见项（虚拟滚动优化）"""
        # 可以在这里实现更复杂的虚拟滚动逻辑
        pass

    def _item_size_hint(self, item: ClipboardItem) -> QSize:
        item_height = 90 if item.content_type == ContentType.IMAGE else 82
        item_width = max(self.viewport().width() - 8, 0)
        return QSize(item_width, item_height)

    def _refresh_item_sizes(self):
        for row, item in enumerate(self._items):
            list_item = self.item(row)
            if list_item:
                list_item.setSizeHint(self._item_size_hint(item))
    
    def _full_refresh(self, items: list[ClipboardItem]):
        """全量刷新：清空后重建所有项（用于首次加载或大量变化）。"""
        current_hash = None
        current = self.currentItem()
        if current:
            current_hash = current.data(Qt.ItemDataRole.UserRole)

        self.clear()
        self._items = items
        self._item_widgets.clear()
        self._items_by_hash = {it.content_hash: it for it in items}

        for item in items:
            list_item = QListWidgetItem()
            list_item.setData(Qt.ItemDataRole.UserRole, item.content_hash)
            list_item.setSizeHint(self._item_size_hint(item))
            self.addItem(list_item)

            widget = HistoryListItem(item)
            widget._list_widget = self
            widget.fav_button.clicked.connect(
                lambda checked, h=item.content_hash: self.favoriteToggled.emit(h)
            )
            self.setItemWidget(list_item, widget)
            self._item_widgets[item.content_hash] = widget

        # 恢复选中
        if current_hash and current_hash in self._item_widgets:
            for row in range(self.count()):
                if self.item(row).data(Qt.ItemDataRole.UserRole) == current_hash:
                    self.setCurrentRow(row)
                    break
        elif len(items) > 0:
            self.setCurrentRow(0)

        QTimer.singleShot(0, self._refresh_item_sizes)

    def _insert_at_top(self, item: ClipboardItem):
        """在列表顶部插入一项（最高效的增量更新）。"""
        self._items_by_hash[item.content_hash] = item
        list_item = QListWidgetItem()
        list_item.setData(Qt.ItemDataRole.UserRole, item.content_hash)
        list_item.setSizeHint(self._item_size_hint(item))
        self.insertItem(0, list_item)

        widget = HistoryListItem(item)
        widget._list_widget = self
        widget.fav_button.clicked.connect(
            lambda checked, h=item.content_hash: self.favoriteToggled.emit(h)
        )
        self.setItemWidget(list_item, widget)
        self._item_widgets[item.content_hash] = widget
        self.setCurrentRow(0)

    def update_items(self, items: list[ClipboardItem]):
        """智能增量更新：优先检测常见场景（顶部插入），避免全量重建。"""
        if not self._items:
            self._full_refresh(items)
            return

        new_hashes = [it.content_hash for it in items]
        old_hashes = [it.content_hash for it in self._items]

        # 场景 A：顺序完全一致，仅数据状态变化（如收藏切换）
        if new_hashes == old_hashes:
            self._items = items
            self._items_by_hash = {it.content_hash: it for it in items}
            for item in items:
                widget = self._item_widgets.get(item.content_hash)
                if widget:
                    widget.update_item(item)
            return

        # 场景 B：顶部新增一项（最常见：复制新内容）
        if len(items) == len(self._items) + 1 and old_hashes == new_hashes[1:]:
            self._insert_at_top(items[0])
            self._items = items
            return

        # 场景 C：其他变化，回退到全量刷新
        self._full_refresh(items)
    
    def filter_items(self, text: str, favorites_only: bool = False):
        """过滤列表项（支持模糊搜索，O(1) 查找）。"""
        text = text.lower().strip()

        for i in range(self.count()):
            list_item = self.item(i)
            content_hash = list_item.data(Qt.ItemDataRole.UserRole)
            item = self._items_by_hash.get(content_hash)

            if item is None:
                list_item.setHidden(True)
                continue

            if favorites_only and not item.is_favorite:
                list_item.setHidden(True)
                continue

            if text:
                searchable = item.content.lower()
                if item.tags:
                    searchable += ' ' + ' '.join(item.tags).lower()
                list_item.setHidden(text not in searchable)
            else:
                list_item.setHidden(False)
    
    def _on_selection_changed(self, current_row: int):
        """当选中行变化时，只更新旧选中项和新选中项的样式"""
        prev = self._prev_selected_row
        if prev == current_row:
            return
        
        for row in (prev, current_row):
            if 0 <= row < self.count():
                list_item = self.item(row)
                if list_item:
                    content_hash = list_item.data(Qt.ItemDataRole.UserRole)
                    widget = self._item_widgets.get(content_hash)
                    if widget:
                        widget.set_selected(row == current_row)
        
        self._prev_selected_row = current_row
    
    def _handle_item_click(self, list_item: QListWidgetItem):
        """处理项目点击 (仅选中，不做复制)"""
        pass
    
    def _handle_item_double_click(self, list_item: QListWidgetItem):
        """处理项目双击 (触发复制)"""
        content_hash = list_item.data(Qt.ItemDataRole.UserRole)
        item = self._find_item_by_hash(content_hash)
        if item:
            self.itemCopied.emit(item)
    
    def _find_item_by_hash(self, content_hash: str) -> ClipboardItem | None:
        """根据哈希查找项目（O(1) 字典查找）。"""
        return self._items_by_hash.get(content_hash)
    
    def _show_context_menu(self, position):
        """显示上下文菜单"""
        try:
            list_item = self.itemAt(position)
            if not list_item:
                return
            
            content_hash = list_item.data(Qt.ItemDataRole.UserRole)
            item = self._find_item_by_hash(content_hash)
            if not item:
                return
            
            menu = QMenu(self)
            dark = False
            try:
                from views.styles.main_style import StyleManager
                dark = StyleManager.is_dark_mode()
            except Exception:
                dark = False
            if dark:
                menu.setStyleSheet("""
                    QMenu {
                        background-color: #18212F;
                        border: 1px solid #29415B;
                        border-radius: 10px;
                        padding: 8px;
                        color: white;
                    }
                    QMenu::item {
                        padding: 8px 24px;
                        border-radius: 7px;
                    }
                    QMenu::item:selected {
                        background-color: #203347;
                        color: #4CC2FF;
                    }
                """)
            else:
                menu.setStyleSheet("""
                    QMenu {
                        background-color: white;
                        border: 1px solid #D8E3EE;
                        border-radius: 10px;
                        padding: 8px;
                    }
                    QMenu::item {
                        padding: 8px 24px;
                        border-radius: 7px;
                    }
                    QMenu::item:selected {
                        background-color: #EAF4FF;
                        color: #0A84FF;
                    }
                """)
            
            # 复制
            copy_action = QAction("📋 复制", self)
            copy_action.triggered.connect(lambda: self.itemCopied.emit(item))
            menu.addAction(copy_action)
            
            # 查看完整内容
            view_action = QAction("👁️ 查看完整内容", self)
            view_action.triggered.connect(lambda: self._show_full_content(item))
            menu.addAction(view_action)
            
            menu.addSeparator()
            
            # 收藏/取消收藏
            fav_text = "⭐ 取消收藏" if item.is_favorite else "☆ 收藏"
            fav_action = QAction(fav_text, self)
            fav_action.triggered.connect(lambda: self.favoriteToggled.emit(content_hash))
            menu.addAction(fav_action)
            
            menu.addSeparator()

            # 批量删除（当多选时显示）
            selected_items = self.selectedItems()
            if len(selected_items) > 1:
                batch_action = QAction(f"🗑️ 删除选中的 {len(selected_items)} 项", self)
                batch_action.triggered.connect(self._delete_selected_items)
                menu.addAction(batch_action)

            # 单条删除
            delete_action = QAction("🗑️ 删除", self)
            delete_action.triggered.connect(lambda: self.itemDeleted.emit(content_hash))
            menu.addAction(delete_action)

            menu.exec(self.mapToGlobal(position))

        except Exception as e:
            logger.error(f"显示上下文菜单时发生错误: {str(e)}")

    def _delete_selected_items(self):
        """删除所有选中的项"""
        hashes = []
        for list_item in self.selectedItems():
            h = list_item.data(Qt.ItemDataRole.UserRole)
            if h:
                hashes.append(h)
        if hashes:
            self.itemsBatchDeleted.emit(hashes)
    
    def _show_full_content(self, item: ClipboardItem):
        """显示完整内容"""
        try:
            from PyQt6.QtWidgets import QDialog, QVBoxLayout

            dialog = QDialog(self)
            dialog.setWindowTitle("剪贴板内容")
            dialog.setMinimumSize(520, 400)
            layout = QVBoxLayout(dialog)

            if item.content_type == ContentType.IMAGE:
                # 图片：显示实际图片 + 尺寸信息
                img_label = QLabel()
                img_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
                pix = QPixmap(item.content)
                if not pix.isNull():
                    # 缩放以适应窗口，但保持清晰度
                    scaled = pix.scaled(
                        480, 360,
                        Qt.AspectRatioMode.KeepAspectRatio,
                        Qt.TransformationMode.SmoothTransformation,
                    )
                    img_label.setPixmap(scaled)
                else:
                    img_label.setText("图片加载失败")
                info = QLabel(f"{pix.width()} × {pix.height()} 像素 | 路径: {item.content}")
                info.setStyleSheet("color: #7D8EA3; font-size: 11px;")
                info.setWordWrap(True)
                layout.addWidget(img_label)
                layout.addWidget(info)
            elif item.content_type == ContentType.FILE:
                files = item.metadata.get('files', [])
                text_label = QLabel("文件列表:\n" + "\n".join(files))
                text_label.setWordWrap(True)
                layout.addWidget(text_label)
            else:
                text_edit = QTextEdit()
                text_edit.setPlainText(item.content)
                text_edit.setReadOnly(True)
                text_edit.setMinimumSize(500, 300)
                layout.addWidget(text_edit)

            dialog.exec()

        except Exception as e:
            logger.error(f"显示完整内容时发生错误: {str(e)}")
    
    def update_item_favorite(self, content_hash: str, is_favorite: bool):
        """更新项目的收藏状态显示"""
        widget = self._item_widgets.get(content_hash)
        if widget:
            widget.item_data.is_favorite = is_favorite
            widget.update_item(widget.item_data)

    def resizeEvent(self, event):
        super().resizeEvent(event)
        self._refresh_item_sizes()

    def showEvent(self, event):
        super().showEvent(event)
        QTimer.singleShot(0, self._refresh_item_sizes)
