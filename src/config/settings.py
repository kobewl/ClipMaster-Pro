from pathlib import Path
import os

class Settings:
    """应用程序设置"""
    
    # 应用程序名称
    APP_NAME = "剪贴板助手"
    
    # 窗口设置
    WINDOW_WIDTH = 400
    WINDOW_HEIGHT = 600
    
    # 历史记录设置
    MAX_HISTORY = 100  # 添加最大历史记录数量
    
    # 目录设置
    BASE_DIR = Path(os.path.expanduser("~")) / ".clipboard_assistant"
    DATA_DIR = BASE_DIR / "data"
    LOG_DIR = BASE_DIR / "logs"
    
    # 文件设置
    HISTORY_FILE = DATA_DIR / "history.json"
    
    @classmethod
    def ensure_directories(cls) -> None:
        """确保必要的目录存在"""
        cls.DATA_DIR.mkdir(parents=True, exist_ok=True)
        cls.LOG_DIR.mkdir(parents=True, exist_ok=True) 