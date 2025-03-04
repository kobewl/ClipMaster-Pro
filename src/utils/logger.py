import logging
from datetime import datetime
from pathlib import Path
from config.settings import Settings

def setup_logger():
    """配置日志系统"""
    Settings.ensure_directories()
    
    log_file = Settings.LOG_DIR / f"clipboard_{datetime.now().strftime('%Y%m%d')}.log"
    
    logging.basicConfig(
        level=logging.INFO,
        format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
        handlers=[
            logging.FileHandler(log_file, encoding='utf-8'),
            logging.StreamHandler()
        ]
    )
    
    return logging.getLogger(Settings.APP_NAME)

logger = setup_logger() 