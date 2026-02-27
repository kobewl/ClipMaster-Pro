import sys
import os

# PyInstaller 路径处理
def get_resource_path(relative_path):
    """获取资源路径（兼容PyInstaller打包后的路径）"""
    if hasattr(sys, '_MEIPASS'):
        # PyInstaller 打包后的临时目录
        return os.path.join(sys._MEIPASS, relative_path)
    return os.path.join(os.path.abspath("."), relative_path)

# 添加src目录到Python路径
src_path = get_resource_path('src')
if src_path not in sys.path:
    sys.path.insert(0, src_path)

# 设置高DPI支持（必须在创建QApplication之前）
os.environ["QT_ENABLE_HIGHDPI_SCALING"] = "1"
os.environ["QT_AUTO_SCREEN_SCALE_FACTOR"] = "1"
os.environ["QT_SCALE_FACTOR"] = "1"

from PyQt6.QtWidgets import QApplication
from PyQt6.QtCore import Qt
from PyQt6.QtGui import QFont

from views.main_window import MainWindow
from controllers.clipboard_controller import ClipboardController
from config.settings import Settings
from utils.logger import logger


def setup_application():
    """配置应用程序"""
    # 创建应用
    app = QApplication(sys.argv)
    app.setApplicationName(Settings.APP_NAME)
    app.setApplicationVersion(Settings.APP_VERSION)
    app.setOrganizationName("ClipMaster")
    
    # 设置全局字体
    font = QFont("Segoe UI", 10)
    font.setStyleHint(QFont.StyleHint.SansSerif)
    app.setFont(font)
    
    # 注意：高DPI缩放已通过环境变量设置，不需要在这里调用
    
    return app


def main():
    """程序入口"""
    try:
        # 迁移旧版本数据
        Settings.migrate_from_old_version()
        
        # 确保必要的目录存在
        Settings.ensure_directories()
        
        # 配置应用
        app = setup_application()
        
        # 获取系统剪贴板
        clipboard = app.clipboard()
        clipboard_controller = ClipboardController(clipboard)
        
        # 创建主窗口
        window = MainWindow(clipboard_controller)
        
        # 检查是否最小化启动
        if not Settings.get("minimize_to_tray", False):
            window.show()
        else:
            window.hide()
            logger.info("程序已最小化到托盘启动")
        
        logger.info(f"{Settings.APP_NAME} v{Settings.APP_VERSION} 启动成功")
        
        # 运行应用
        exit_code = app.exec()
        
        # 清理资源
        logger.info(f"{Settings.APP_NAME} 正在关闭...")
        
        sys.exit(exit_code)
        
    except Exception as e:
        logger.error(f"程序启动失败: {str(e)}")
        print(f"错误: {str(e)}")
        import traceback
        traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    main()