import json
from typing import List, Optional
from datetime import datetime, timedelta
from PyQt6.QtCore import QObject, QTimer
import os

from models.clipboard_item import ClipboardItem
from config.settings import Settings
from utils.logger import logger

class ClipboardService(QObject):
    """剪贴板服务类"""
    
    def __init__(self):
        super().__init__()
        self.history_file = os.path.join(Settings.DATA_DIR, "history.json")
        self.history: List[ClipboardItem] = []
        self.max_history = Settings.get("max_history", 100)
        self.retention_days = Settings.get("retention_days", 10)  # 默认保留10天
        self._modified = False
        
        # 确保目录和文件存在
        self._ensure_files()
        self._load_history()
        
        # 清理过期记录
        self._clean_expired_items()
        
        # 设置自动保存定时器
        self._setup_auto_save()
    
    def _setup_auto_save(self):
        """设置自动保存定时器"""
        self._auto_save_timer = QTimer(self)
        self._auto_save_timer.timeout.connect(self._auto_save)
        
        # 获取自动保存间隔（秒）
        interval = Settings.get("auto_save_interval", 60)
        self._auto_save_timer.setInterval(interval * 1000)  # 转换为毫秒
        self._auto_save_timer.start()
        
        logger.info(f"已设置自动保存定时器，间隔 {interval} 秒")
    
    def _auto_save(self):
        """自动保存历史记录"""
        if self._modified:
            self.save_history()
            self._modified = False
            logger.debug("已自动保存历史记录")
    
    def _ensure_files(self) -> None:
        """确保必要的文件和目录存在"""
        try:
            # 确保目录存在
            Settings.ensure_directories()
            
            # 如果历史文件不存在，创建一个空的历史文件
            if not Settings.HISTORY_FILE.exists():
                Settings.HISTORY_FILE.write_text(
                    json.dumps([], ensure_ascii=False, indent=2),
                    encoding='utf-8'
                )
                logger.info("已创建新的历史记录文件")
        except Exception as e:
            logger.error(f"确保文件存在时发生错误: {str(e)}")
    
    def _load_history(self) -> None:
        """从文件加载历史记录"""
        try:
            data = json.loads(Settings.HISTORY_FILE.read_text(encoding='utf-8'))
            self.history = []
            
            for item in data:
                try:
                    # 处理旧格式数据
                    if isinstance(item, str):
                        self.history.append(ClipboardItem(
                            content=item,
                            timestamp=datetime.now()
                        ))
                    else:
                        self.history.append(ClipboardItem.from_dict(item))
                except Exception as e:
                    logger.warning(f"跳过无效的历史记录项: {str(e)}")
                    continue
            
            logger.info(f"已加载 {len(self.history)} 条历史记录")
            
        except Exception as e:
            logger.error(f"加载历史记录时发生错误: {str(e)}")
            self.history = []
    
    def save_history(self) -> None:
        """保存历史记录到文件"""
        try:
            data = [item.to_dict() for item in self.history]
            Settings.HISTORY_FILE.write_text(
                json.dumps(data, ensure_ascii=False, indent=2),
                encoding='utf-8'
            )
            logger.debug(f"已保存 {len(self.history)} 条历史记录")
            self._modified = False
        except Exception as e:
            logger.error(f"保存历史记录时发生错误: {str(e)}")
    
    def _clean_expired_items(self) -> None:
        """清理过期的历史记录"""
        try:
            if self.retention_days <= 0:  # 如果设置为0或负数，表示不自动清理
                return
                
            now = datetime.now()
            cutoff_date = now - timedelta(days=self.retention_days)
            
            original_count = len(self.history)
            self.history = [item for item in self.history if item.timestamp > cutoff_date]
            
            removed_count = original_count - len(self.history)
            if removed_count > 0:
                logger.info(f"已清理 {removed_count} 条过期历史记录（超过 {self.retention_days} 天）")
                self._modified = True
                self.save_history()  # 保存更改
        except Exception as e:
            logger.error(f"清理过期历史记录时发生错误: {str(e)}")
    
    def add_item(self, content: str) -> None:
        """添加项目到历史记录"""
        try:
            # 如果内容为空，则忽略
            if not content or content.isspace():
                return
                
            # 创建新项目
            new_item = ClipboardItem(content=content, timestamp=datetime.now())
            
            # 检查是否已存在相同内容的项目
            for i, item in enumerate(self.history):
                if item.content == content:
                    # 如果存在，则删除旧项目
                    self.history.pop(i)
                    break
            
            # 添加新项目到列表开头
            self.history.insert(0, new_item)
            
            # 如果超过最大数量，则删除最旧的项目
            while len(self.history) > self.max_history:
                self.history.pop()
                
            # 清理过期记录
            self._clean_expired_items()
                
            # 标记为已修改
            self._modified = True
            
            logger.debug(f"已添加新项目到历史记录: {content[:30]}...")
        except Exception as e:
            logger.error(f"添加项目到历史记录时发生错误: {str(e)}")
    
    def clear_history(self) -> None:
        """清空历史记录"""
        try:
            self.history.clear()
            self._modified = True
            self.save_history()  # 立即保存
        except Exception as e:
            logger.error(f"清空历史记录时发生错误: {str(e)}")
    
    def delete_item(self, index: int) -> None:
        """删除指定索引的历史记录项"""
        try:
            if 0 <= index < len(self.history):
                del self.history[index]
                self._modified = True
                logger.debug(f"已从服务中删除索引为 {index} 的历史记录项")
        except Exception as e:
            logger.error(f"删除历史记录项时发生错误: {str(e)}")
    
    def get_history(self) -> List[ClipboardItem]:
        """获取历史记录"""
        return self.history.copy()
    
    def set_max_history(self, max_history: int) -> None:
        """设置最大历史记录数"""
        try:
            self.max_history = max_history
            
            # 如果当前历史记录数超过新的最大值，则裁剪
            if len(self.history) > self.max_history:
                self.history = self.history[:self.max_history]
                self._modified = True
                
            logger.info(f"已设置最大历史记录数为 {max_history}")
        except Exception as e:
            logger.error(f"设置最大历史记录数时发生错误: {str(e)}")
    
    def update_auto_save_interval(self, interval: int) -> None:
        """更新自动保存间隔"""
        try:
            if self._auto_save_timer:
                self._auto_save_timer.setInterval(interval * 1000)  # 转换为毫秒
                logger.info(f"已更新自动保存间隔为 {interval} 秒")
        except Exception as e:
            logger.error(f"更新自动保存间隔时发生错误: {str(e)}")
    
    def export_history(self, file_path: str) -> bool:
        """导出历史记录到文件"""
        try:
            data = [item.to_dict() for item in self.history]
            with open(file_path, 'w', encoding='utf-8') as f:
                json.dump(data, f, ensure_ascii=False, indent=2)
            logger.info(f"已导出 {len(self.history)} 条历史记录到 {file_path}")
            return True
        except Exception as e:
            logger.error(f"导出历史记录时发生错误: {str(e)}")
            return False
    
    def import_history(self, file_path: str) -> bool:
        """从文件导入历史记录"""
        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                data = json.load(f)
            
            # 清空当前历史记录
            self.history.clear()
            
            # 导入新的历史记录
            for item in data:
                try:
                    self.history.append(ClipboardItem.from_dict(item))
                except Exception as e:
                    logger.warning(f"跳过无效的历史记录项: {str(e)}")
                    continue
            
            # 标记为已修改
            self._modified = True
            
            # 立即保存
            self.save_history()
            
            logger.info(f"已从 {file_path} 导入 {len(self.history)} 条历史记录")
            return True
        except Exception as e:
            logger.error(f"导入历史记录时发生错误: {str(e)}")
            return False
    
    def set_retention_days(self, days: int) -> None:
        """设置历史记录保留天数"""
        try:
            if days < 0:
                days = 0  # 不允许负值
                
            self.retention_days = days
            Settings.set("retention_days", days)
            logger.info(f"历史记录保留天数已设置为 {days} 天")
            
            # 立即清理过期记录
            self._clean_expired_items()
        except Exception as e:
            logger.error(f"设置历史记录保留天数时发生错误: {str(e)}")
    
    def __del__(self):
        """析构函数，确保在对象销毁前保存历史记录"""
        try:
            if self._modified:
                self.save_history()
        except:
            pass 