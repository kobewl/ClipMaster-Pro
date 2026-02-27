from datetime import datetime
from dataclasses import dataclass, field
from typing import Dict, Optional, Literal
from enum import Enum
import hashlib

class ContentType(Enum):
    """内容类型"""
    TEXT = "text"
    IMAGE = "image"
    FILE = "file"
    HTML = "html"
    RTF = "rtf"

@dataclass
class ClipboardItem:
    """剪贴板项目"""
    content: str
    timestamp: datetime
    content_type: ContentType = ContentType.TEXT
    content_hash: str = ""
    is_favorite: bool = False
    tags: list = field(default_factory=list)
    metadata: Dict = field(default_factory=dict)
    
    def __post_init__(self):
        """初始化后计算哈希"""
        if not self.content_hash and self.content:
            self.content_hash = self._calculate_hash()
    
    def _calculate_hash(self) -> str:
        """计算内容哈希（用于快速去重）"""
        return hashlib.md5(self.content.encode('utf-8')).hexdigest()[:16]
    
    @classmethod
    def from_dict(cls, data: Dict) -> 'ClipboardItem':
        """从字典创建实例"""
        timestamp = data.get('timestamp')
        if isinstance(timestamp, str):
            timestamp = datetime.fromisoformat(timestamp)
        else:
            timestamp = datetime.now()
        
        # 处理旧格式数据
        content_type_str = data.get('content_type', 'text')
        try:
            content_type = ContentType(content_type_str)
        except ValueError:
            content_type = ContentType.TEXT
        
        return cls(
            content=data.get('content', ''),
            timestamp=timestamp,
            content_type=content_type,
            content_hash=data.get('content_hash', ''),
            is_favorite=data.get('is_favorite', False),
            tags=data.get('tags', []),
            metadata=data.get('metadata', {})
        )
    
    def to_dict(self) -> Dict:
        """转换为字典"""
        return {
            'content': self.content,
            'timestamp': self.timestamp.isoformat(),
            'content_type': self.content_type.value,
            'content_hash': self.content_hash,
            'is_favorite': self.is_favorite,
            'tags': self.tags,
            'metadata': self.metadata
        }
    
    def display_text(self) -> str:
        """显示文本"""
        if self.content_type == ContentType.TEXT:
            return self.content
        elif self.content_type == ContentType.IMAGE:
            return f"[图片] {self.metadata.get('width', '?')}x{self.metadata.get('height', '?')}"
        elif self.content_type == ContentType.FILE:
            files = self.metadata.get('files', [])
            if len(files) == 1:
                return f"[文件] {files[0]}"
            else:
                return f"[文件] {len(files)} 个文件"
        elif self.content_type == ContentType.HTML:
            return "[HTML 内容]"
        elif self.content_type == ContentType.RTF:
            return "[RTF 内容]"
        return self.content
    
    def preview_text(self, max_length: int = 50) -> str:
        """获取预览文本"""
        text = self.display_text()
        if len(text) > max_length:
            return text[:max_length] + '...'
        return text
    
    def get_icon(self) -> str:
        """获取内容类型对应的图标"""
        icons = {
            ContentType.TEXT: "📝",
            ContentType.IMAGE: "🖼️",
            ContentType.FILE: "📎",
            ContentType.HTML: "🌐",
            ContentType.RTF: "📄"
        }
        if self.is_favorite:
            return "⭐"
        return icons.get(self.content_type, "📋")