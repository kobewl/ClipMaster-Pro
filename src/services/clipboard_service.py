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
        # 追踪所有线程创建的连接，确保程序退出时能全部关闭
        self._all_connections: list[sqlite3.Connection] = []
        self._connections_lock = threading.Lock()
        self._init_db()

    def _get_connection(self) -> sqlite3.Connection:
        """获取线程本地连接"""
        if not hasattr(self._local, 'connection') or self._local.connection is None:
            conn = sqlite3.connect(self.db_path, check_same_thread=False)
            conn.row_factory = sqlite3.Row
            self._local.connection = conn
            with self._connections_lock:
                self._all_connections.append(conn)
        return self._local.connection

    def close(self):
        """关闭当前线程的数据库连接（向后兼容）"""
        if hasattr(self._local, 'connection') and self._local.connection is not None:
            try:
                self._local.connection.close()
            except Exception:
                pass
            self._local.connection = None

    def close_all(self):
        """关闭所有线程的数据库连接，防止资源泄漏"""
        with self._connections_lock:
            for conn in list(self._all_connections):
                try:
                    conn.close()
                except Exception:
                    pass
            self._all_connections.clear()
        self._local.connection = None

    def _init_db(self):
        """初始化数据库"""
        conn = self._get_connection()
        cursor = conn.cursor()

        # 性能优化：启用 WAL（写前日志）模式，提升并发读写性能
        try:
            cursor.execute('PRAGMA journal_mode=WAL')
            cursor.execute('PRAGMA synchronous=NORMAL')
            cursor.execute('PRAGMA temp_store=MEMORY')
            cursor.execute('PRAGMA cache_size=-8000')  # 约 8MB 页缓存
        except Exception as e:
            logger.warning(f"启用 WAL 模式时发生警告: {e}")

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
                compressed INTEGER DEFAULT 0,
                search_text TEXT DEFAULT ''
            )
        ''')

        # 兼容旧库：检查并增加 search_text 列
        self._ensure_search_text_column(cursor)

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

    # 搜索文本最大长度（截断超长内容，避免数据库膨胀）
    # 8000 字符可覆盖绝大多数剪贴板场景，又不至于让单条记录过大
    _SEARCH_TEXT_MAX_LEN = 8000

    def _ensure_search_text_column(self, cursor) -> None:
        """旧库迁移：补齐 search_text 列并填充数据。"""
        try:
            cursor.execute("PRAGMA table_info(clipboard_history)")
            cols = {row[1] for row in cursor.fetchall()}
            if 'search_text' not in cols:
                logger.info("迁移：添加 search_text 列")
                cursor.execute("ALTER TABLE clipboard_history ADD COLUMN search_text TEXT DEFAULT ''")

            # 为 search_text 创建索引（LIKE 搜索时 SQLite 可走 B-tree 前缀匹配）
            cursor.execute(
                'CREATE INDEX IF NOT EXISTS idx_search_text ON clipboard_history(search_text)'
            )

            # 检查是否需要回填（search_text 为空但 content 非空的旧数据）
            cursor.execute(
                "SELECT COUNT(*) FROM clipboard_history "
                "WHERE (search_text IS NULL OR search_text = '') "
                "AND content_type IN ('text', 'html')"
            )
            backfill_count = cursor.fetchone()[0]
            if backfill_count > 0:
                logger.info(f"迁移：回填 {backfill_count} 条历史记录的搜索文本")
                self._backfill_search_text(cursor)
        except Exception as e:
            logger.error(f"迁移 search_text 列时发生错误: {e}")

    def _backfill_search_text(self, cursor) -> None:
        """为旧数据回填 search_text 列。"""
        cursor.execute(
            "SELECT id, content, compressed, content_type FROM clipboard_history "
            "WHERE (search_text IS NULL OR search_text = '')"
        )
        rows = cursor.fetchall()
        for row in rows:
            try:
                content = row['content']
                if row['compressed']:
                    content = gzip.decompress(content).decode('utf-8', errors='ignore')
                search_text = self._derive_search_text(content, row['content_type'])
                cursor.execute(
                    "UPDATE clipboard_history SET search_text = ? WHERE id = ?",
                    (search_text, row['id'])
                )
            except Exception as e:
                logger.debug(f"回填 search_text 失败 (id={row['id']}): {e}")

    @classmethod
    def _derive_search_text(cls, content, content_type: str) -> str:
        """从原始内容派生可搜索文本（截断至最大长度）。"""
        if not content:
            return ''
        if not isinstance(content, str):
            return ''
        # 图片和文件类型用 content（路径/文件列表）作为可搜索文本
        text = content
        if len(text) > cls._SEARCH_TEXT_MAX_LEN:
            text = text[:cls._SEARCH_TEXT_MAX_LEN]
        return text

    
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

            # 在压缩前提取可搜索文本（保证压缩内容也能搜到）
            search_text = self._derive_search_text(
                item.content, item.content_type.value
            )

            # 压缩大文本内容
            content = item.content
            compressed = 0
            if len(content) > 1000:
                content = gzip.compress(content.encode('utf-8'))
                compressed = 1

            cursor.execute('''
                INSERT INTO clipboard_history
                (content_hash, content, content_type, timestamp, is_favorite, tags, metadata, compressed, search_text)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ''', (
                item.content_hash,
                content if compressed == 0 else content,
                item.content_type.value,
                item.timestamp.timestamp(),
                1 if item.is_favorite else 0,
                json.dumps(item.tags),
                json.dumps(item.metadata),
                compressed,
                search_text,
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
                # 使用 search_text 列搜索（压缩内容也能搜到，因为它存的是解压后的文本）
                query += ' AND search_text LIKE ?'
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
    
    def get_item_by_hash(self, content_hash: str) -> Optional[ClipboardItem]:
        """根据 content_hash 获取单个项目"""
        try:
            conn = self._get_connection()
            cursor = conn.cursor()
            cursor.execute('SELECT * FROM clipboard_history WHERE content_hash = ?', (content_hash,))
            row = cursor.fetchone()
            if row:
                content = row['content']
                if row['compressed']:
                    content = gzip.decompress(content).decode('utf-8')
                return ClipboardItem(
                    content=content,
                    timestamp=datetime.fromtimestamp(row['timestamp']),
                    content_type=ContentType(row['content_type']),
                    content_hash=row['content_hash'],
                    is_favorite=bool(row['is_favorite']),
                    tags=json.loads(row['tags']),
                    metadata=json.loads(row['metadata'])
                )
            return None
        except Exception as e:
            logger.error(f"根据哈希获取项目时发生错误: {str(e)}")
            return None

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

    def delete_items(self, content_hashes: list[str]) -> int:
        """批量删除项目，返回删除数量"""
        if not content_hashes:
            return 0
        try:
            conn = self._get_connection()
            cursor = conn.cursor()
            placeholders = ','.join('?' * len(content_hashes))
            cursor.execute(
                f'DELETE FROM clipboard_history WHERE content_hash IN ({placeholders})',
                content_hashes
            )
            conn.commit()
            return cursor.rowcount
        except Exception as e:
            logger.error(f"批量删除项目时发生错误: {str(e)}")
            return 0

    def get_image_paths_by_hashes(self, content_hashes: list[str]) -> list[str]:
        """一次性获取多个 hash 对应的图片文件路径（避免 N+1 查询）"""
        if not content_hashes:
            return []
        try:
            conn = self._get_connection()
            cursor = conn.cursor()
            placeholders = ','.join('?' * len(content_hashes))
            cursor.execute(
                f"SELECT content, compressed FROM clipboard_history "
                f"WHERE content_hash IN ({placeholders}) AND content_type = 'image'",
                content_hashes
            )
            paths = []
            for row in cursor.fetchall():
                content = row['content']
                if row['compressed']:
                    try:
                        content = gzip.decompress(content).decode('utf-8')
                    except Exception:
                        continue
                if content:
                    paths.append(content)
            return paths
        except Exception as e:
            logger.error(f"批量获取图片路径时发生错误: {str(e)}")
            return []
    
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
    
    def _delete_image_file(self, content: str, content_type: str = None) -> None:
        """删除本地图片文件（当记录被删除时清理磁盘）。"""
        try:
            if content_type == 'image' or (content and not content.startswith("data:image")):
                if os.path.exists(content):
                    os.remove(content)
                    logger.debug(f"已删除图片文件: {content}")
        except Exception as e:
            logger.warning(f"删除图片文件失败: {e}")

    def _enforce_max_limit(self):
        """强制执行最大记录数限制"""
        try:
            count = self.db.get_count()
            if count > self.max_history:
                # 获取需要删除的项目
                excess = count - self.max_history
                conn = self.db._get_connection()
                cursor = conn.cursor()

                # 先查出要删除的项目，清理图片文件
                cursor.execute('''
                    SELECT content, content_type FROM clipboard_history
                    WHERE id IN (
                        SELECT id FROM clipboard_history
                        WHERE is_favorite = 0
                        ORDER BY timestamp ASC
                        LIMIT ?
                    )
                ''', (excess,))
                rows = cursor.fetchall()
                for row in rows:
                    self._delete_image_file(row['content'], row['content_type'])

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
            # 先获取要删除的图片项目，清理文件
            items = self.db.get_items(limit=10000, favorites_only=False)
            for item in items:
                if item.content_type == ContentType.IMAGE:
                    self._delete_image_file(item.content, 'image')

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
            # 先获取项目信息，如有图片则清理文件
            item = self.db.get_item_by_hash(content_hash)
            if item and item.content_type == ContentType.IMAGE:
                self._delete_image_file(item.content, 'image')

            result = self.db.delete_item(content_hash)
            if result:
                self.history_changed.emit()
                logger.debug(f"已删除项目: {content_hash}")
            return result
        except Exception as e:
            logger.error(f"删除项目时发生错误: {str(e)}")
            return False

    def delete_items(self, content_hashes: list[str]) -> int:
        """批量删除项目，只触发一次 history_changed"""
        if not content_hashes:
            return 0
        try:
            # 一次性获取所有要删除的图片路径，避免 N+1 查询
            image_paths = self.db.get_image_paths_by_hashes(content_hashes)
            for path in image_paths:
                self._delete_image_file(path, 'image')

            deleted = self.db.delete_items(content_hashes)
            if deleted > 0:
                self.history_changed.emit()
                logger.info(f"已批量删除 {deleted} 个项目")
            return deleted
        except Exception as e:
            logger.error(f"批量删除项目时发生错误: {str(e)}")
            return 0
    
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