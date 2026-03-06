import platform


SYSTEM = platform.system()
IS_WINDOWS = SYSTEM == "Windows"
IS_MACOS = SYSTEM == "Darwin"
IS_LINUX = SYSTEM == "Linux"

# First macOS release keeps clipboard history and global hotkeys, but
# disables the Windows-centric input monitoring and source tracking stack.
SUPPORTS_GLOBAL_HOTKEYS = True
SUPPORTS_INPUT_MONITOR = IS_WINDOWS
SUPPORTS_SOURCE_TRACKING = IS_WINDOWS
SUPPORTS_NATIVE_CLIPBOARD_PROBE = IS_WINDOWS


def paste_shortcut() -> str:
    if IS_MACOS:
        return "command+v"
    return "ctrl+v"
