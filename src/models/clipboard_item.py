from datetime import datetime
from dataclasses import dataclass
from typing import Dict

@dataclass
class ClipboardItem:
    """剪贴板项目"""
    content: str
    timestamp: datetime
    
    @classmethod
    def from_dict(cls, data: Dict) -> 'ClipboardItem':
        """从字典创建实例"""
        return cls(
            content=data['content'],
            timestamp=datetime.fromisoformat(data['timestamp'])
        )
    
    def to_dict(self) -> Dict:
        """转换为字典"""
        return {
            'content': self.content,
            'timestamp': self.timestamp.isoformat()
        }
    
    def display_text(self) -> str:
        """显示文本"""
        return self.content  # 直接返回内容，不包含时间戳 