from pathlib import Path
import os
import json
import threading

class Settings:
    """应用程序设置 - 线程安全版本"""
    
    # 应用程序信息
    APP_NAME = "ClipMaster Pro"
    APP_VERSION = "2.0.0"
    
    # 窗口设置
    WINDOW_WIDTH = 420
    WINDOW_HEIGHT = 640
    
    # 目录设置
    BASE_DIR = Path(os.path.expanduser("~")) / ".clipmaster_pro"
    DATA_DIR = BASE_DIR / "data"
    LOG_DIR = BASE_DIR / "logs"
    CACHE_DIR = BASE_DIR / "cache"
    
    # 文件设置
    HISTORY_FILE = DATA_DIR / "history.json"
    CONFIG_FILE = DATA_DIR / "config.json"
    
    # 默认配置
    DEFAULT_CONFIG = {
        "dark_mode": False,
        "startup": True,
        "minimize_to_tray": False,
        "max_history": 1000,
        "retention_days": 30,
        "display_limit": 100,
        "auto_save_interval": 60,
        "hotkeys": {
            "show_window": "Ctrl+O",
            "clear_history": "Ctrl+Shift+C",
            "search": "Ctrl+F"
        },
        "ui": {
            "window_opacity": 1.0,
            "animation_enabled": True,
            "show_preview": True
        }
    }
    
    # 线程锁
    _lock = threading.RLock()
    _config = None
    _initialized = False
    
    @classmethod
    def ensure_directories(cls) -> None:
        """确保必要的目录存在"""
        with cls._lock:
            cls.DATA_DIR.mkdir(parents=True, exist_ok=True)
            cls.LOG_DIR.mkdir(parents=True, exist_ok=True)
            cls.CACHE_DIR.mkdir(parents=True, exist_ok=True)
    
    @classmethod
    def load_config(cls) -> dict:
        """加载配置（线程安全）"""
        with cls._lock:
            if cls._config is not None:
                return cls._config.copy()
            
            cls.ensure_directories()
            
            if not cls.CONFIG_FILE.exists():
                cls._config = cls.DEFAULT_CONFIG.copy()
                cls.save_config()
                return cls._config.copy()
            
            try:
                with open(cls.CONFIG_FILE, 'r', encoding='utf-8') as f:
                    cls._config = json.load(f)
                
                # 确保所有默认配置项都存在
                for key, value in cls.DEFAULT_CONFIG.items():
                    if key not in cls._config:
                        cls._config[key] = value
                    elif isinstance(value, dict) and isinstance(cls._config.get(key), dict):
                        # 合并嵌套字典
                        for sub_key, sub_value in value.items():
                            if sub_key not in cls._config[key]:
                                cls._config[key][sub_key] = sub_value
                
                return cls._config.copy()
                
            except Exception as e:
                print(f"加载配置时出错: {str(e)}")
                cls._config = cls.DEFAULT_CONFIG.copy()
                return cls._config.copy()
    
    @classmethod
    def save_config(cls) -> None:
        """保存配置（线程安全）"""
        with cls._lock:
            if cls._config is None:
                cls._config = cls.DEFAULT_CONFIG.copy()
            
            try:
                with open(cls.CONFIG_FILE, 'w', encoding='utf-8') as f:
                    json.dump(cls._config, f, ensure_ascii=False, indent=2)
            except Exception as e:
                print(f"保存配置时出错: {str(e)}")
    
    @classmethod
    def get(cls, key: str, default=None):
        """获取配置项"""
        config = cls.load_config()
        return config.get(key, default)
    
    @classmethod
    def set(cls, key: str, value) -> None:
        """设置配置项"""
        with cls._lock:
            config = cls.load_config()
            config[key] = value
            cls._config = config
            cls.save_config()
    
    @classmethod
    def reset_to_defaults(cls) -> None:
        """重置为默认配置"""
        with cls._lock:
            cls._config = cls.DEFAULT_CONFIG.copy()
            cls.save_config()
    
    @classmethod
    def migrate_from_old_version(cls) -> bool:
        """从旧版本迁移数据"""
        try:
            old_dir = Path(os.path.expanduser("~")) / ".clipboard_assistant"
            if old_dir.exists() and not cls.HISTORY_FILE.exists():
                # 迁移旧数据
                old_history = old_dir / "data" / "history.json"
                if old_history.exists():
                    import shutil
                    cls.ensure_directories()
                    shutil.copy2(old_history, cls.HISTORY_FILE)
                    print(f"已迁移旧版本数据从 {old_history}")
                    return True
        except Exception as e:
            print(f"迁移旧版本数据时出错: {str(e)}")
        return False