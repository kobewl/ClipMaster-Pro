import json
from typing import List, Optional
from datetime import datetime
from models.clipboard_item import ClipboardItem
from config.settings import Settings
from utils.logger import logger

class ClipboardService:
    """剪贴板服务类"""
    
    def __init__(self):
        self._history: List[ClipboardItem] = []
        # 确保目录和文件存在
        self._ensure_files()
        self._load_history()
    
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
            self._history = []
            
            for item in data:
                try:
                    # 处理旧格式数据
                    if isinstance(item, str):
                        self._history.append(ClipboardItem(
                            content=item,
                            timestamp=datetime.now()
                        ))
                    else:
                        self._history.append(ClipboardItem.from_dict(item))
                except Exception as e:
                    logger.warning(f"跳过无效的历史记录项: {str(e)}")
                    continue
            
            logger.info(f"已加载 {len(self._history)} 条历史记录")
            
        except Exception as e:
            logger.error(f"加载历史记录时发生错误: {str(e)}")
            self._history = []
    
    def save_history(self) -> None:
        """保存历史记录到文件"""
        try:
            data = [item.to_dict() for item in self._history]
            Settings.HISTORY_FILE.write_text(
                json.dumps(data, ensure_ascii=False, indent=2),
                encoding='utf-8'
            )
            logger.debug(f"已保存 {len(self._history)} 条历史记录")
        except Exception as e:
            logger.error(f"保存历史记录时发生错误: {str(e)}")
    
    def add_item(self, content: str) -> None:
        """添加新的历史记录"""
        try:
            # 创建新项目
            new_item = ClipboardItem(
                content=content,
                timestamp=datetime.now()
            )
            
            # 检查是否已存在相同内容
            for item in self._history:
                if item.content == content:
                    self._history.remove(item)
                    break
            
            # 添加到开头
            self._history.insert(0, new_item)
            
            # 限制历史记录数量
            if len(self._history) > Settings.MAX_HISTORY:
                self._history = self._history[:Settings.MAX_HISTORY]
            
            # 保存到文件
            self.save_history()
            
        except Exception as e:
            logger.error(f"添加历史记录时发生错误: {str(e)}")
    
    def clear_history(self) -> None:
        """清空历史记录"""
        try:
            self._history.clear()
            self.save_history()
        except Exception as e:
            logger.error(f"清空历史记录时发生错误: {str(e)}")
    
    def get_history(self) -> List[ClipboardItem]:
        """获取历史记录"""
        return self._history.copy() 