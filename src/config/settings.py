from pathlib import Path
import os
import json

class Settings:
    """应用程序设置"""
    
    # 应用程序名称
    APP_NAME = "剪贴板助手"
    APP_VERSION = "1.1.0"
    
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
    CONFIG_FILE = DATA_DIR / "config.json"
    
    # 默认配置
    DEFAULT_CONFIG = {
        "dark_mode": False,
        "startup": True,
        "auto_save_interval": 60,  # 秒
        "max_history": 100,
        "hotkeys": {
            "show_window": "Ctrl+O",
            "clear_history": "Ctrl+Shift+C"
        }
    }
    
    # 当前配置
    _config = None
    
    @classmethod
    def ensure_directories(cls) -> None:
        """确保必要的目录存在"""
        cls.DATA_DIR.mkdir(parents=True, exist_ok=True)
        cls.LOG_DIR.mkdir(parents=True, exist_ok=True)
    
    @classmethod
    def load_config(cls) -> dict:
        """加载配置"""
        if cls._config is not None:
            return cls._config
            
        cls.ensure_directories()
        
        if not cls.CONFIG_FILE.exists():
            cls._config = cls.DEFAULT_CONFIG.copy()
            cls.save_config()
            return cls._config
            
        try:
            cls._config = json.loads(cls.CONFIG_FILE.read_text(encoding='utf-8'))
            # 确保所有默认配置项都存在
            for key, value in cls.DEFAULT_CONFIG.items():
                if key not in cls._config:
                    cls._config[key] = value
            return cls._config
        except Exception as e:
            print(f"加载配置时出错: {str(e)}")
            cls._config = cls.DEFAULT_CONFIG.copy()
            return cls._config
    
    @classmethod
    def save_config(cls) -> None:
        """保存配置"""
        if cls._config is None:
            cls._config = cls.DEFAULT_CONFIG.copy()
            
        try:
            cls.CONFIG_FILE.write_text(
                json.dumps(cls._config, ensure_ascii=False, indent=2),
                encoding='utf-8'
            )
        except Exception as e:
            print(f"保存配置时出错: {str(e)}")
    
    @classmethod
    def get(cls, key, default=None):
        """获取配置项"""
        config = cls.load_config()
        return config.get(key, default)
    
    @classmethod
    def set(cls, key, value):
        """设置配置项"""
        config = cls.load_config()
        config[key] = value
        cls.save_config()
        return value 