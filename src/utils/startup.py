import platform
import sys
from pathlib import Path

from config.settings import Settings
from utils.logger import logger


class StartupManager:
    """Cross-platform startup manager."""

    @staticmethod
    def is_windows() -> bool:
        return platform.system() == "Windows"

    @staticmethod
    def is_mac() -> bool:
        return platform.system() == "Darwin"

    @staticmethod
    def is_linux() -> bool:
        return platform.system() == "Linux"

    @classmethod
    def set_startup(cls, enable: bool) -> bool:
        try:
            if cls.is_windows():
                return cls._set_windows_startup(enable)
            if cls.is_mac():
                return cls._set_mac_startup(enable)
            if cls.is_linux():
                return cls._set_linux_startup(enable)

            logger.warning(f"Unsupported platform for startup: {platform.system()}")
            return False
        except Exception as e:
            logger.error(f"Failed to update startup setting: {e}")
            return False

    @classmethod
    def _set_windows_startup(cls, enable: bool) -> bool:
        try:
            import winreg

            app_path = sys.executable
            app_name = Settings.APP_NAME

            key = winreg.OpenKey(
                winreg.HKEY_CURRENT_USER,
                r"Software\Microsoft\Windows\CurrentVersion\Run",
                0,
                winreg.KEY_SET_VALUE | winreg.KEY_QUERY_VALUE,
            )

            if enable:
                winreg.SetValueEx(key, app_name, 0, winreg.REG_SZ, f'"{app_path}"')
                logger.info(f"Startup enabled for {app_name} on Windows")
            else:
                try:
                    winreg.DeleteValue(key, app_name)
                except FileNotFoundError:
                    pass
                logger.info(f"Startup disabled for {app_name} on Windows")

            winreg.CloseKey(key)
            return True
        except Exception as e:
            logger.error(f"Failed to update Windows startup: {e}")
            return False

    @classmethod
    def _set_mac_startup(cls, enable: bool) -> bool:
        try:
            app_path = sys.executable
            app_name = Settings.APP_NAME
            bundle_id = f"com.clipmasterpro.{app_name.lower().replace(' ', '')}"

            launch_agents_dir = Path.home() / "Library" / "LaunchAgents"
            launch_agents_dir.mkdir(parents=True, exist_ok=True)
            plist_path = launch_agents_dir / f"{bundle_id}.plist"

            if enable:
                plist_content = f"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{bundle_id}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{app_path}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"""
                plist_path.write_text(plist_content, encoding="utf-8")
                logger.info(f"Startup enabled for {app_name} on macOS")
            else:
                if plist_path.exists():
                    plist_path.unlink()
                logger.info(f"Startup disabled for {app_name} on macOS")

            return True
        except Exception as e:
            logger.error(f"Failed to update macOS startup: {e}")
            return False

    @classmethod
    def _set_linux_startup(cls, enable: bool) -> bool:
        try:
            app_path = sys.executable
            app_name = Settings.APP_NAME

            autostart_dir = Path.home() / ".config" / "autostart"
            autostart_dir.mkdir(parents=True, exist_ok=True)
            desktop_path = autostart_dir / f"{app_name.lower()}.desktop"

            if enable:
                desktop_content = f"""[Desktop Entry]
Type=Application
Name={app_name}
Exec={app_path}
Terminal=false
X-GNOME-Autostart-enabled=true
"""
                desktop_path.write_text(desktop_content, encoding="utf-8")
                desktop_path.chmod(0o755)
                logger.info(f"Startup enabled for {app_name} on Linux")
            else:
                if desktop_path.exists():
                    desktop_path.unlink()
                logger.info(f"Startup disabled for {app_name} on Linux")

            return True
        except Exception as e:
            logger.error(f"Failed to update Linux startup: {e}")
            return False
