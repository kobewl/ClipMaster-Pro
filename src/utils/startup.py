import os
import sys
import platform
from pathlib import Path
import winreg
import shutil

from config.settings import Settings
from utils.logger import logger

class StartupManager:
    """启动管理器，用于设置开机自启动"""
    
    @staticmethod
    def is_windows() -> bool:
        """检查是否为Windows系统"""
        return platform.system() == "Windows"
    
    @staticmethod
    def is_mac() -> bool:
        """检查是否为macOS系统"""
        return platform.system() == "Darwin"
    
    @staticmethod
    def is_linux() -> bool:
        """检查是否为Linux系统"""
        return platform.system() == "Linux"
    
    @classmethod
    def set_startup(cls, enable: bool) -> bool:
        """设置开机自启动"""
        try:
            if cls.is_windows():
                return cls._set_windows_startup(enable)
            elif cls.is_mac():
                return cls._set_mac_startup(enable)
            elif cls.is_linux():
                return cls._set_linux_startup(enable)
            else:
                logger.warning(f"不支持的操作系统: {platform.system()}")
                return False
        except Exception as e:
            logger.error(f"设置开机自启动时发生错误: {str(e)}")
            return False
    
    @classmethod
    def _set_windows_startup(cls, enable: bool) -> bool:
        """设置Windows开机自启动"""
        try:
            # 获取应用程序路径
            app_path = sys.executable
            app_name = Settings.APP_NAME
            
            # 打开注册表
            key = winreg.OpenKey(
                winreg.HKEY_CURRENT_USER,
                r"Software\Microsoft\Windows\CurrentVersion\Run",
                0,
                winreg.KEY_SET_VALUE | winreg.KEY_QUERY_VALUE
            )
            
            if enable:
                # 添加到启动项
                winreg.SetValueEx(key, app_name, 0, winreg.REG_SZ, f'"{app_path}"')
                logger.info(f"已添加 {app_name} 到Windows启动项")
            else:
                # 从启动项中移除
                try:
                    winreg.DeleteValue(key, app_name)
                    logger.info(f"已从Windows启动项中移除 {app_name}")
                except FileNotFoundError:
                    # 如果键不存在，忽略错误
                    pass
            
            winreg.CloseKey(key)
            return True
            
        except Exception as e:
            logger.error(f"设置Windows开机自启动时发生错误: {str(e)}")
            return False
    
    @classmethod
    def _set_mac_startup(cls, enable: bool) -> bool:
        """设置macOS开机自启动"""
        try:
            # 获取应用程序路径
            app_path = sys.executable
            app_name = Settings.APP_NAME
            
            # 用户启动项目录
            launch_agents_dir = Path.home() / "Library" / "LaunchAgents"
            launch_agents_dir.mkdir(parents=True, exist_ok=True)
            
            # plist文件路径
            plist_path = launch_agents_dir / f"com.{app_name.lower()}.plist"
            
            if enable:
                # 创建plist文件
                plist_content = f"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.{app_name.lower()}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{app_path}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"""
                plist_path.write_text(plist_content)
                logger.info(f"已添加 {app_name} 到macOS启动项")
            else:
                # 删除plist文件
                if plist_path.exists():
                    plist_path.unlink()
                    logger.info(f"已从macOS启动项中移除 {app_name}")
            
            return True
            
        except Exception as e:
            logger.error(f"设置macOS开机自启动时发生错误: {str(e)}")
            return False
    
    @classmethod
    def _set_linux_startup(cls, enable: bool) -> bool:
        """设置Linux开机自启动"""
        try:
            # 获取应用程序路径
            app_path = sys.executable
            app_name = Settings.APP_NAME
            
            # 用户自启动目录
            autostart_dir = Path.home() / ".config" / "autostart"
            autostart_dir.mkdir(parents=True, exist_ok=True)
            
            # 桌面文件路径
            desktop_path = autostart_dir / f"{app_name.lower()}.desktop"
            
            if enable:
                # 创建桌面文件
                desktop_content = f"""[Desktop Entry]
Type=Application
Name={app_name}
Exec={app_path}
Terminal=false
X-GNOME-Autostart-enabled=true
"""
                desktop_path.write_text(desktop_content)
                # 设置权限
                desktop_path.chmod(0o755)
                logger.info(f"已添加 {app_name} 到Linux启动项")
            else:
                # 删除桌面文件
                if desktop_path.exists():
                    desktop_path.unlink()
                    logger.info(f"已从Linux启动项中移除 {app_name}")
            
            return True
            
        except Exception as e:
            logger.error(f"设置Linux开机自启动时发生错误: {str(e)}")
            return False 