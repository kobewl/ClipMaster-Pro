import sys
from PyQt6.QtWidgets import QApplication
from views.main_window import MainWindow
from controllers.clipboard_controller import ClipboardController
from config.settings import Settings
from utils.logger import logger

def main():
    """程序入口"""
    try:
        # 初始化应用
        app = QApplication(sys.argv)
        app.setApplicationName(Settings.APP_NAME)
        
        # 确保必要的目录存在
        Settings.ensure_directories()
        
        # 获取系统剪贴板
        clipboard = app.clipboard()
        clipboard_controller = ClipboardController(clipboard)
        
        # 创建主窗口
        window = MainWindow(clipboard_controller)
        window.show()
        
        logger.info(f"{Settings.APP_NAME} 启动成功")
        sys.exit(app.exec())
        
    except Exception as e:
        logger.error(f"程序启动失败: {str(e)}")
        sys.exit(1)

if __name__ == "__main__":
    main() 