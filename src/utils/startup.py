import os
import sys
import platform
from pathlib import Path

if platform.system() == "Windows":
    import winreg
else:
    winreg = None

from config.settings import Settings
from utils.logger import logger


class StartupManager:
    """开机自启动管理器。

    支持两种运行模式：
      - PyInstaller 打包后：sys.executable 就是应用本身，直接启动。
      - 源码运行：sys.executable 是 Python 解释器，需要拼接脚本路径。
    """

    _BUNDLE_ID = "com.kobewl.clipmasterpro"

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
    def _is_frozen(cls) -> bool:
        """判断当前是否为 PyInstaller 等打包后的可执行文件。"""
        return getattr(sys, "frozen", False)

    @classmethod
    def _get_launch_info(cls) -> tuple[str, list[str], str]:
        """获取启动所需信息。

        Returns:
            (exe_path, extra_args, working_dir)
            - exe_path:  要执行的可执行文件绝对路径
            - extra_args: 额外参数列表（源码运行时为脚本路径）
            - working_dir: 建议的工作目录
        """
        exe = sys.executable
        args: list[str] = []
        work_dir = os.path.abspath(os.getcwd())

        if cls._is_frozen():
            # PyInstaller 等打包场景：sys.executable 就是应用本身
            return exe, args, work_dir

        # 源码运行场景：需要找到入口脚本
        # 策略：先尝试 run.py（项目根目录），再回退到 src/main.py
        candidates = []

        # 从当前工作目录或 __main__ 模块推断项目根目录
        project_root = None

        # 尝试从 sys.argv[0] 推断（如 python /path/to/run.py）
        if sys.argv and len(sys.argv) > 0:
            argv0 = os.path.abspath(sys.argv[0])
            if argv0.endswith("run.py"):
                project_root = os.path.dirname(argv0)
            elif argv0.endswith(("main.py", "__main__.py")):
                # 可能是 python src/main.py 或 python -m src.main
                parent = os.path.dirname(argv0)
                if os.path.basename(parent) == "src":
                    project_root = os.path.dirname(parent)

        # 如果 argv 推断失败，尝试从当前工作目录向上查找 run.py
        if project_root is None:
            cwd = os.path.abspath(os.getcwd())
            for search_dir in [cwd, os.path.dirname(cwd)]:
                run_py = os.path.join(search_dir, "run.py")
                if os.path.isfile(run_py):
                    project_root = search_dir
                    break

        if project_root is None:
            # 最终 fallback：无法推断项目根目录，直接用当前目录
            logger.warning("无法推断项目根目录，开机启动可能失效")
            project_root = work_dir

        run_py = os.path.join(project_root, "run.py")
        if os.path.isfile(run_py):
            args = [run_py]
            work_dir = project_root
        else:
            # run.py 不存在，尝试 src/main.py
            main_py = os.path.join(project_root, "src", "main.py")
            if os.path.isfile(main_py):
                args = [main_py]
                work_dir = project_root
            else:
                logger.warning("未找到 run.py 或 src/main.py，开机启动可能失效")

        return exe, args, work_dir

    @classmethod
    def set_startup(cls, enable: bool) -> bool:
        """设置或取消开机自启动。"""
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
    def is_startup_enabled(cls) -> bool:
        """检查当前是否已设置开机自启动。"""
        try:
            if cls.is_windows():
                key = winreg.OpenKey(
                    winreg.HKEY_CURRENT_USER,
                    r"Software\Microsoft\Windows\CurrentVersion\Run",
                    0,
                    winreg.KEY_QUERY_VALUE,
                )
                try:
                    winreg.QueryValueEx(key, Settings.APP_NAME)
                    return True
                except FileNotFoundError:
                    return False
                finally:
                    winreg.CloseKey(key)

            elif cls.is_mac():
                plist_path = (
                    Path.home() / "Library" / "LaunchAgents" / f"{cls._BUNDLE_ID}.plist"
                )
                return plist_path.exists()

            elif cls.is_linux():
                desktop_path = (
                    Path.home()
                    / ".config"
                    / "autostart"
                    / f"{Settings.APP_NAME.lower().replace(' ', '')}.desktop"
                )
                return desktop_path.exists()

            return False
        except Exception as e:
            logger.error(f"检查开机自启动状态时发生错误: {e}")
            return False

    # ── Windows ───────────────────────────────────────────────────────

    @classmethod
    def _set_windows_startup(cls, enable: bool) -> bool:
        try:
            exe, args, _ = cls._get_launch_info()
            app_name = Settings.APP_NAME

            key = winreg.OpenKey(
                winreg.HKEY_CURRENT_USER,
                r"Software\Microsoft\Windows\CurrentVersion\Run",
                0,
                winreg.KEY_SET_VALUE | winreg.KEY_QUERY_VALUE,
            )

            if enable:
                if args:
                    # 源码运行："python.exe" "run.py"
                    cmd = f'"{exe}" "{"\" \"".join(args)}"'
                else:
                    # 打包后：直接启动 exe
                    cmd = f'"{exe}"'
                winreg.SetValueEx(key, app_name, 0, winreg.REG_SZ, cmd)
                logger.info(f"已添加 {app_name} 到 Windows 启动项: {cmd}")
            else:
                try:
                    winreg.DeleteValue(key, app_name)
                    logger.info(f"已从 Windows 启动项移除 {app_name}")
                except FileNotFoundError:
                    pass

            winreg.CloseKey(key)
            return True

        except Exception as e:
            logger.error(f"设置 Windows 开机自启动时发生错误: {e}")
            return False

    # ── macOS ─────────────────────────────────────────────────────────

    @classmethod
    def _set_mac_startup(cls, enable: bool) -> bool:
        try:
            exe, args, work_dir = cls._get_launch_info()
            app_name = Settings.APP_NAME

            launch_agents_dir = Path.home() / "Library" / "LaunchAgents"
            launch_agents_dir.mkdir(parents=True, exist_ok=True)

            plist_path = launch_agents_dir / f"{cls._BUNDLE_ID}.plist"

            if enable:
                # 构建 ProgramArguments 数组
                prog_args_lines = "\n".join(
                    f"        <string>{item}</string>"
                    for item in ([exe] + list(args))
                )

                plist_content = (
                    '<?xml version="1.0" encoding="UTF-8"?>\n'
                    '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" '
                    '"http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n'
                    '<plist version="1.0">\n'
                    '<dict>\n'
                    '    <key>Label</key>\n'
                    f'    <string>{cls._BUNDLE_ID}</string>\n'
                    '    <key>ProgramArguments</key>\n'
                    '    <array>\n'
                    f'{prog_args_lines}\n'
                    '    </array>\n'
                    '    <key>RunAtLoad</key>\n'
                    '    <true/>\n'
                    '    <key>WorkingDirectory</key>\n'
                    f'    <string>{work_dir}</string>\n'
                    '</dict>\n'
                    '</plist>\n'
                )
                plist_path.write_text(plist_content, encoding="utf-8")
                logger.info(
                    f"已添加 {app_name} 到 macOS 启动项: {plist_path}"
                )
            else:
                if plist_path.exists():
                    plist_path.unlink()
                    logger.info(f"已从 macOS 启动项移除 {app_name}")

            return True

        except Exception as e:
            logger.error(f"设置 macOS 开机自启动时发生错误: {e}")
            return False

    # ── Linux ─────────────────────────────────────────────────────────

    @classmethod
    def _set_linux_startup(cls, enable: bool) -> bool:
        try:
            exe, args, work_dir = cls._get_launch_info()
            app_name = Settings.APP_NAME
            app_id = app_name.lower().replace(" ", "")

            autostart_dir = Path.home() / ".config" / "autostart"
            autostart_dir.mkdir(parents=True, exist_ok=True)

            desktop_path = autostart_dir / f"{app_id}.desktop"

            if enable:
                if args:
                    cmd = f'"{exe}" "{"\" \"".join(args)}"'
                else:
                    cmd = f'"{exe}"'

                desktop_content = f"""[Desktop Entry]
Type=Application
Name={app_name}
Exec={cmd}
Path={work_dir}
Terminal=false
X-GNOME-Autostart-enabled=true
"""
                desktop_path.write_text(desktop_content, encoding="utf-8")
                desktop_path.chmod(0o755)
                logger.info(f"已添加 {app_name} 到 Linux 启动项")
            else:
                if desktop_path.exists():
                    desktop_path.unlink()
                    logger.info(f"已从 Linux 启动项移除 {app_name}")

            return True

        except Exception as e:
            logger.error(f"设置 Linux 开机自启动时发生错误: {e}")
            return False
