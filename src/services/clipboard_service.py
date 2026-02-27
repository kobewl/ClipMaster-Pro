import json
import gzip
import sqlite3
import threading
from typing import List, Optional, Callable
from datetime import datetime, timedelta
from PyQt6.QtCore import QObject, QTimer, pyqtSignal
from PyQt6.QtGui import QImage, QPixmap
from PyQt6.QtWidgets import QApplication
import os
import base64
import io

from models.clipboard_item import ClipboardItem, ContentType
from config.settings import Settings
from utils.logger import logger


class DatabaseManager:
    """SQLite数据库管理器 - 高效存储大量数据"""
    
    def __init__(self, db_path: str):
        self.db_path = db_path
        self._local = threading.local()
        self._init_db()
    
    def _get_connection(self) -> sqlite3.Connection:
        """获取线程本地连接"""
        if not hasattr(self._local, 'connection') or self._local.connection is None:
            self._local.connection = sqlite3.connect(self.db_path, check_same_thread=False)
            self._local.connection.row_factory = sqlite3.Row
        return self._local.connection

    def close(self):
        """关闭当前线程的数据库连接"""
        if hasattr(self._local, 'connection') and self._local.connection is not None:
            try:
                self._local.connection.close()
            except Exception:
                pass
            self._local.connection = None
    
    def _init_db(self):
        """初始化数据库"""
        conn = self._get_connection()
        cursor = conn.cursor()
        
        # 创建历史记录表
        cursor.execute('''
            CREATE TABLE IF NOT EXISTS clipboard_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content_hash TEXT UNIQUE NOT NULL,
                content TEXT NOT NULL,
                content_type TEXT DEFAULT 'text',
                timestamp REAL NOT NULL,
                is_favorite INTEGER DEFAULT 0,
                tags TEXT DEFAULT '[]',
                metadata TEXT DEFAULT '{}',
                compressed INTEGER DEFAULT 0
            )
        ''')
        
        # 创建索引
        cursor.execute('''
            CREATE INDEX IF NOT EXISTS idx_timestamp ON clipboard_history(timestamp)
        ''')
        cursor.execute('''
            CREATE INDEX IF NOT EXISTS idx_content_hash ON clipboard_history(content_hash)
        ''')
        cursor.execute('''
            CREATE INDEX IF NOT EXISTS idx_favorite ON clipboard_history(is_favorite)
        ''')
        
        conn.commit()
    
    def add_item(self, item: ClipboardItem) -> bool:
        """添加项目"""
        try:
            conn = self._get_connection()
            cursor = conn.cursor()
            
            # 检查是否已存在
            cursor.execute(
                'SELECT id FROM clipboard_history WHERE content_hash = ?',
                (item.content_hash,)
            )
            if cursor.fetchone():
                # 更新时间和移动到最前（通过删除再插入）
                cursor.execute(
                    'DELETE FROM clipboard_history WHERE content_hash = ?',
                    (item.content_hash,)
                )
            
            # 压缩大文本内容
            content = item.content
            compressed = 0
            if len(content) > 1000:
                content = gzip.compress(content.encode('utf-8'))
                compressed = 1
            
            cursor.execute('''
                INSERT INTO clipboard_history 
                (content_hash, content, content_type, timestamp, is_favorite, tags, metadata, compressed)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ''', (
                item.content_hash,
                content if compressed == 0 else content,
                item.content_type.value,
                item.timestamp.timestamp(),
                1 if item.is_favorite else 0,
                json.dumps(item.tags),
                json.dumps(item.metadata),
                compressed
            ))
            
            conn.commit()
            return True
        except Exception as e:
            logger.error(f"添加项目到数据库时发生错误: {str(e)}")
            return False
    
    def get_items(self, limit: int = 100, offset: int = 0, 
                  search_text: str = None, favorites_only: bool = False) -> List[ClipboardItem]:
        """获取项目列表"""
        try:
            conn = self._get_connection()
            cursor = conn.cursor()
            
            query = 'SELECT * FROM clipboard_history WHERE 1=1'
            params = []
            
            if favorites_only:
                query += ' AND is_favorite = 1'
            
            if search_text:
                query += ' AND content LIKE ?'
                params.append(f'%{search_text}%')
            
            query += ' ORDER BY is_favorite DESC, timestamp DESC LIMIT ? OFFSET ?'
            params.extend([limit, offset])
            
            cursor.execute(query, params)
            rows = cursor.fetchall()
            
            items = []
            for row in rows:
                content = row['content']
                if row['compressed']:
                    content = gzip.decompress(content).decode('utf-8')
                
                items.append(ClipboardItem(
                    content=content,
                    timestamp=datetime.fromtimestamp(row['timestamp']),
                    content_type=ContentType(row['content_type']),
                    content_hash=row['content_hash'],
                    is_favorite=bool(row['is_favorite']),
                    tags=json.loads(row['tags']),
                    metadata=json.loads(row['metadata'])
                ))
            
            return items
        except Exception as e:
            logger.error(f"从数据库获取项目时发生错误: {str(e)}")
            return []
    
    def delete_item(self, content_hash: str) -> bool:
        """删除项目"""
        try:
            conn = self._get_connection()
            cursor = conn.cursor()
            cursor.execute('DELETE FROM clipboard_history WHERE content_hash = ?', (content_hash,))
            conn.commit()
            return cursor.rowcount > 0
        except Exception as e:
            logger.error(f"删除项目时发生错误: {str(e)}")
            return False
    
    def clear_history(self, keep_favorites: bool = True) -> bool:
        """清空历史记录"""
        try:
            conn = self._get_connection()
            cursor = conn.cursor()
            
            if keep_favorites:
                cursor.execute('DELETE FROM clipboard_history WHERE is_favorite = 0')
            else:
                cursor.execute('DELETE FROM clipboard_history')
            
            conn.commit()
            return True
        except Exception as e:
            logger.error(f"清空历史记录时发生错误: {str(e)}")
            return False
    
    def toggle_favorite(self, content_hash: str) -> bool:
        """切换收藏状态"""
        try:
            conn = self._get_connection()
            cursor = conn.cursor()
            cursor.execute('''
                UPDATE clipboard_history 
                SET is_favorite = CASE WHEN is_favorite = 1 THEN 0 ELSE 1 END
                WHERE content_hash = ?
            ''', (content_hash,))
            conn.commit()
            return cursor.rowcount > 0
        except Exception as e:
            logger.error(f"切换收藏状态时发生错误: {str(e)}")
            return False
    
    def clean_expired(self, days: int) -> int:
        """清理过期记录"""
        if days <= 0:
            return 0
        
        try:
            conn = self._get_connection()
            cursor = conn.cursor()
            
            cutoff_time = (datetime.now() - timedelta(days=days)).timestamp()
            
            cursor.execute('''
                DELETE FROM clipboard_history 
                WHERE timestamp < ? AND is_favorite = 0
            ''', (cutoff_time,))
            
            conn.commit()
            return cursor.rowcount
        except Exception as e:
            logger.error(f"清理过期记录时发生错误: {str(e)}")
            return 0
    
    def get_count(self) -> int:
        """获取记录总数"""
        try:
            conn = self._get_connection()
            cursor = conn.cursor()
            cursor.execute('SELECT COUNT(*) FROM clipboard_history')
            return cursor.fetchone()[0]
        except Exception as e:
            logger.error(f"获取记录数时发生错误: {str(e)}")
            return 0
    
    def export_to_json(self, file_path: str) -> bool:
        """导出到JSON文件"""
        try:
            items = self.get_items(limit=10000)
            data = [item.to_dict() for item in items]
            
            with open(file_path, 'w', encoding='utf-8') as f:
                json.dump(data, f, ensure_ascii=False, indent=2)
            
            return True
        except Exception as e:
            logger.error(f"导出到JSON时发生错误: {str(e)}")
            return False
    
    def import_from_json(self, file_path: str) -> bool:
        """从JSON文件导入"""
        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                data = json.load(f)
            
            for item_data in data:
                try:
                    item = ClipboardItem.from_dict(item_data)
                    self.add_item(item)
                except Exception as e:
                    logger.warning(f"跳过无效项目: {str(e)}")
                    continue
            
            return True
        except Exception as e:
            logger.error(f"从JSON导入时发生错误: {str(e)}")
            return False


class ClipboardService(QObject):
    """优化的剪贴板服务类"""
    
    history_changed = pyqtSignal()
    item_added = pyqtSignal(object)  # ClipboardItem
    
    def __init__(self):
        super().__init__()
        self.db = DatabaseManager(str(Settings.DATA_DIR / "clipboard.db"))
        self.max_history = Settings.get("max_history", 1000)
        self.retention_days = Settings.get("retention_days", 30)
        self._last_content_hash = None
        self._batch_timer = None
        self._pending_items = []
        
        # 清理过期记录
        self._clean_expired_items()
        
        # 设置自动清理定时器
        self._setup_cleanup_timer()
    
    def _setup_cleanup_timer(self):
        """设置清理定时器"""
        self._cleanup_timer = QTimer(self)
        self._cleanup_timer.timeout.connect(self._clean_expired_items)
        self._cleanup_timer.start(3600000)  # 每小时检查一次
    
    def _clean_expired_items(self):
        """清理过期记录"""
        if self.retention_days > 0:
            removed = self.db.clean_expired(self.retention_days)
            if removed > 0:
                logger.info(f"已清理 {removed} 条过期历史记录")
                self.history_changed.emit()
    
    def add_item(self, content: str, content_type: ContentType = ContentType.TEXT,
                 metadata: dict = None) -> bool:
        """添加项目到历史记录"""
        try:
            # 如果内容为空，则忽略
            if not content or (isinstance(content, str) and content.isspace()):
                return False
            
            # 创建新项目
            new_item = ClipboardItem(
                content=content,
                timestamp=datetime.now(),
                content_type=content_type,
                metadata=metadata or {}
            )
            
            # 检查是否与上次内容相同（避免重复）
            if new_item.content_hash == self._last_content_hash:
                return False
            
            self._last_content_hash = new_item.content_hash
            
            # 添加到数据库
            if self.db.add_item(new_item):
                # 检查并限制总数
                self._enforce_max_limit()
                
                self.item_added.emit(new_item)
                self.history_changed.emit()
                
                logger.debug(f"已添加新项目: {new_item.preview_text(30)}")
                return True
            
            return False
            
        except Exception as e:
            logger.error(f"添加项目到历史记录时发生错误: {str(e)}")
            return False
    
    def _enforce_max_limit(self):
        """强制执行最大记录数限制"""
        try:
            count = self.db.get_count()
            if count > self.max_history:
                # 获取需要删除的项目
                excess = count - self.max_history
                conn = self.db._get_connection()
                cursor = conn.cursor()
                
                # 删除最旧的非收藏项目
                cursor.execute('''
                    DELETE FROM clipboard_history 
                    WHERE id IN (
                        SELECT id FROM clipboard_history 
                        WHERE is_favorite = 0 
                        ORDER BY timestamp ASC 
                        LIMIT ?
                    )
                ''', (excess,))
                
                conn.commit()
                logger.info(f"已删除 {cursor.rowcount} 条旧记录以限制总数")
        except Exception as e:
            logger.error(f"强制执行最大限制时发生错误: {str(e)}")
    
    def clear_history(self, keep_favorites: bool = True) -> bool:
        """清空历史记录"""
        try:
            result = self.db.clear_history(keep_favorites)
            if result:
                self.history_changed.emit()
                logger.info(f"已清空历史记录{'(保留收藏)' if keep_favorites else ''}")
            return result
        except Exception as e:
            logger.error(f"清空历史记录时发生错误: {str(e)}")
            return False
    
    def delete_item(self, content_hash: str) -> bool:
        """删除指定项目"""
        try:
            result = self.db.delete_item(content_hash)
            if result:
                self.history_changed.emit()
                logger.debug(f"已删除项目: {content_hash}")
            return result
        except Exception as e:
            logger.error(f"删除项目时发生错误: {str(e)}")
            return False
    
    def toggle_favorite(self, content_hash: str) -> bool:
        """切换收藏状态"""
        try:
            result = self.db.toggle_favorite(content_hash)
            if result:
                self.history_changed.emit()
                logger.debug(f"已切换收藏状态: {content_hash}")
            return result
        except Exception as e:
            logger.error(f"切换收藏状态时发生错误: {str(e)}")
            return False
    
    def get_history(self, limit: int = 100, offset: int = 0,
                   search_text: str = None, favorites_only: bool = False) -> List[ClipboardItem]:
        """获取历史记录"""
        return self.db.get_items(limit, offset, search_text, favorites_only)
    
    def get_count(self) -> int:
        """获取记录总数"""
        return self.db.get_count()
    
    def set_max_history(self, max_history: int):
        """设置最大历史记录数"""
        self.max_history = max(max_history, 10)
        Settings.set("max_history", self.max_history)
        self._enforce_max_limit()
        logger.info(f"已设置最大历史记录数为 {self.max_history}")
    
    def set_retention_days(self, days: int):
        """设置历史记录保留天数"""
        self.retention_days = max(0, days)
        Settings.set("retention_days", self.retention_days)
        
        if self.retention_days > 0:
            self._clean_expired_items()
        
        logger.info(f"历史记录保留天数已设置为 {self.retention_days} 天")
    
    def export_history(self, file_path: str) -> bool:
        """导出历史记录到文件"""
        try:
            return self.db.export_to_json(file_path)
        except Exception as e:
            logger.error(f"导出历史记录时发生错误: {str(e)}")
            return False
    
    def import_history(self, file_path: str) -> bool:
        """从文件导入历史记录"""
        try:
            result = self.db.import_from_json(file_path)
            if result:
                self._enforce_max_limit()
                self.history_changed.emit()
                logger.info(f"已从 {file_path} 导入历史记录")
            return result
        except Exception as e:
            logger.error(f"导入历史记录时发生错误: {str(e)}")
            return False