"""
Global Hotkey Controller using Windows RegisterHotKey API with keyboard library fallback.

WHY NOT keyboard.add_hotkey?
  keyboard.add_hotkey registers callbacks that execute inside the
  WH_KEYBOARD_LL hook thread. Emitting Qt signals or doing ANY work
  that may block causes the hook callback to exceed Windows' ~200 ms
  timeout, at which point Windows silently uninstalls the hook.  Once
  the hook is gone, ALL hotkeys and key monitoring stop working.

RegisterHotKey is the correct Windows API for global hotkeys:
  - Does NOT use low-level keyboard hooks at all.
  - Windows posts WM_HOTKEY messages to the thread's message queue.
  - Qt's event loop processes them natively.
  - A QAbstractNativeEventFilter intercepts and dispatches them.
  - Immune to hook timeouts — will never randomly stop working.

FALLBACK:
  If RegisterHotKey fails, we fall back to keyboard.add_hotkey but use
  a safe wrapper that queues callbacks to the main thread to avoid timeout.
"""

import platform
import ctypes
import ctypes.wintypes as wt
from PyQt6.QtCore import QObject, pyqtSignal, QAbstractNativeEventFilter, QTimer
from PyQt6.QtWidgets import QApplication
from utils.logger import logger

# Try to import keyboard library for Windows fallback
try:
    import keyboard
    _KEYBOARD_AVAILABLE = True
    _KEYBOARD_IMPORT_ERROR = None
except ImportError:
    _KEYBOARD_AVAILABLE = False
    _KEYBOARD_IMPORT_ERROR = "keyboard not installed"

try:
    from pynput import keyboard as pynput_keyboard
    _PYNPUT_AVAILABLE = True
    _PYNPUT_IMPORT_ERROR = None
except ImportError:
    pynput_keyboard = None
    _PYNPUT_AVAILABLE = False
    _PYNPUT_IMPORT_ERROR = "pynput not installed"

_IS_WINDOWS = platform.system() == "Windows"
_IS_MAC = platform.system() == "Darwin"

WM_HOTKEY = 0x0312
MOD_ALT = 0x0001
MOD_CONTROL = 0x0002
MOD_SHIFT = 0x0004
MOD_WIN = 0x0008

_VK_MAP = {
    # Letters
    "a": 0x41, "b": 0x42, "c": 0x43, "d": 0x44, "e": 0x45,
    "f": 0x46, "g": 0x47, "h": 0x48, "i": 0x49, "j": 0x4A,
    "k": 0x4B, "l": 0x4C, "m": 0x4D, "n": 0x4E, "o": 0x4F,
    "p": 0x50, "q": 0x51, "r": 0x52, "s": 0x53, "t": 0x54,
    "u": 0x55, "v": 0x56, "w": 0x57, "x": 0x58, "y": 0x59,
    "z": 0x5A,
    # Numbers
    "0": 0x30, "1": 0x31, "2": 0x32, "3": 0x33, "4": 0x34,
    "5": 0x35, "6": 0x36, "7": 0x37, "8": 0x38, "9": 0x39,
    # Function keys
    "f1": 0x70, "f2": 0x71, "f3": 0x72, "f4": 0x73,
    "f5": 0x74, "f6": 0x75, "f7": 0x76, "f8": 0x77,
    "f9": 0x78, "f10": 0x79, "f11": 0x7A, "f12": 0x7B,
    "f13": 0x7C, "f14": 0x7D, "f15": 0x7E, "f16": 0x7F,
    "f17": 0x80, "f18": 0x81, "f19": 0x82, "f20": 0x83,
    "f21": 0x84, "f22": 0x85, "f23": 0x86, "f24": 0x87,
    # Navigation
    "space": 0x20, "enter": 0x0D, "return": 0x0D, "tab": 0x09,
    "escape": 0x1B, "esc": 0x1B, "backspace": 0x08,
    "delete": 0x2E, "del": 0x2E, "insert": 0x2D, "ins": 0x2D,
    "home": 0x24, "end": 0x23,
    "pageup": 0x21, "page up": 0x21, "pgup": 0x21,
    "pagedown": 0x22, "page down": 0x22, "pgdn": 0x22,
    "up": 0x26, "down": 0x28, "left": 0x25, "right": 0x27,
    # Special characters (top row)
    "`": 0xC0, "~": 0xC0,  # Grave accent
    "-": 0xBD, "_": 0xBD,  # Minus
    "=": 0xBB, "+": 0xBB,  # Equals
    "[": 0xDB, "{": 0xDB,  # Left bracket
    "]": 0xDD, "}": 0xDD,  # Right bracket
    "\\": 0xDC, "|": 0xDC,  # Backslash
    ";": 0xBA, ":": 0xBA,  # Semicolon
    "'": 0xDE, '\"': 0xDE,  # Quote
    ",": 0xBC, "<": 0xBC,  # Comma
    ".": 0xBE, ">": 0xBE,  # Period
    "/": 0xBF, "?": 0xBF,  # Slash
    # Numpad
    "num0": 0x60, "num1": 0x61, "num2": 0x62, "num3": 0x63,
    "num4": 0x64, "num5": 0x65, "num6": 0x66, "num7": 0x67,
    "num8": 0x68, "num9": 0x69,
    "numpad0": 0x60, "numpad1": 0x61, "numpad2": 0x62, "numpad3": 0x63,
    "numpad4": 0x64, "numpad5": 0x65, "numpad6": 0x66, "numpad7": 0x67,
    "numpad8": 0x68, "numpad9": 0x69,
    "multiply": 0x6A, "add": 0x6B, "separator": 0x6C,
    "subtract": 0x6D, "decimal": 0x6E, "divide": 0x6F,
    # Other useful keys
    "print": 0x2C, "printscreen": 0x2C, "prtsc": 0x2C,
    "scroll": 0x91, "scrolllock": 0x91,
    "pause": 0x13, "break": 0x13,
    "caps": 0x14, "capslock": 0x14,
    "numlock": 0x90,
}

if _IS_WINDOWS:
    _user32 = ctypes.WinDLL("user32", use_last_error=True)
else:
    _user32 = None


def _parse_hotkey(key_sequence: str):
    """Parse 'Ctrl+Shift+C' into (modifiers, vk_code).
    Returns (0, 0) if parsing fails."""
    parts = [p.strip().lower() for p in key_sequence.split("+")]
    modifiers = 0
    vk = 0
    for part in parts:
        if part in ("ctrl", "control"):
            modifiers |= MOD_CONTROL
        elif part == "shift":
            modifiers |= MOD_SHIFT
        elif part == "alt":
            modifiers |= MOD_ALT
        elif part in ("win", "windows", "meta"):
            modifiers |= MOD_WIN
        elif part:
            vk = _VK_MAP.get(part, 0)
            if not vk and len(part) == 1:
                vk = ord(part.upper())
    return modifiers, vk


def _to_keyboard_format(key_sequence: str) -> str:
    """Convert 'Ctrl+Shift+C' to 'ctrl+shift+c' format for keyboard library."""
    parts = [_normalize_key_token(p) for p in key_sequence.split("+")]
    return "+".join(parts)


def _to_pynput_format(key_sequence: str) -> str:
    """Convert hotkeys to pynput format such as '<ctrl>+<shift>+c'."""
    converted = []
    for raw_part in key_sequence.split("+"):
        part = _normalize_key_token(raw_part)
        if not part:
            continue

        special = {
            "ctrl": "<ctrl>",
            "alt": "<alt>",
            "shift": "<shift>",
            "command": "<cmd>",
            "windows": "<cmd>" if _IS_MAC else "<super>",
            "escape": "<esc>",
            "enter": "<enter>",
            "tab": "<tab>",
            "space": "<space>",
            "backspace": "<backspace>",
            "delete": "<delete>",
            "home": "<home>",
            "end": "<end>",
            "pageup": "<page_up>",
            "pagedown": "<page_down>",
            "left": "<left>",
            "right": "<right>",
            "up": "<up>",
            "down": "<down>",
            "`": "`",
        }
        converted.append(special.get(part, part))

    return "+".join(converted)


def _normalize_key_token(part: str) -> str:
    token = part.strip().lower()
    if not token:
        return ""

    aliases = {
        "control": "ctrl",
        "option": "alt",
        "cmd": "command",
        "command": "command",
        "meta": "command" if _IS_MAC else "windows",
        "win": "windows",
        "windows": "windows",
        "esc": "escape",
        "return": "enter",
        "pageup": "page up",
        "pagedown": "page down",
        "pgup": "page up",
        "pgdn": "page down",
        "spacebar": "space",
        "plus": "+",
        "comma": ",",
        "period": ".",
        "slash": "/",
        "semicolon": ";",
        "apostrophe": "'",
        "backtick": "`",
        "grave": "`",
        "quoteleft": "`",
        "⌘": "command",
        "⎋": "escape",
        "·": "`" if _IS_MAC else "·",
    }
    return aliases.get(token, token)


class _POINT(ctypes.Structure):
    _fields_ = [("x", ctypes.c_long), ("y", ctypes.c_long)]


class _MSG(ctypes.Structure):
    _fields_ = [
        ("hwnd", wt.HWND),
        ("message", wt.UINT),
        ("wParam", wt.WPARAM),
        ("lParam", wt.LPARAM),
        ("time", wt.DWORD),
        ("pt", _POINT),
    ]


class _HotkeyFilter(QAbstractNativeEventFilter):
    """Intercepts WM_HOTKEY from the Windows message queue."""

    def __init__(self, dispatch_fn):
        super().__init__()
        self._dispatch = dispatch_fn

    def nativeEventFilter(self, eventType, message):
        try:
            # Support multiple message type names across Qt versions
            event_type_str = str(eventType)
            if "windows" in event_type_str.lower() and "MSG" in event_type_str:
                msg = _MSG.from_address(int(message))
                if msg.message == WM_HOTKEY:
                    hotkey_id = int(msg.wParam)
                    logger.debug(f"WM_HOTKEY received: id=0x{hotkey_id:X}")
                    self._dispatch(hotkey_id)
                    return True, 0
        except Exception as e:
            logger.error(f"Error in nativeEventFilter: {e}")
        return False, 0


class HotkeyController(QObject):
    """Global hotkey controller backed by Windows RegisterHotKey API
    with keyboard library fallback."""

    on_hotkey_triggered = pyqtSignal(str)

    def __init__(self, parent=None):
        super().__init__(parent)

        self._next_id = 0xBF00
        self._id_to_key: dict[int, str] = {}
        self._key_to_id: dict[str, int] = {}
        self.callbacks: dict[str, object] = {}

        # Track which hotkeys use fallback (keyboard library)
        self._fallback_hotkeys: dict[str, object] = {}  # norm -> keyboard hotkey handle
        self._pynput_hotkeys: dict[str, object] = {}
        self._pynput_listener = None

        # Install native event filter for RegisterHotKey
        self._filter = _HotkeyFilter(self._on_wm_hotkey)
        app = QApplication.instance()
        if app:
            app.installNativeEventFilter(self._filter)
            logger.info("Native event filter installed")

        self.on_hotkey_triggered.connect(self._handle_signal)

        # Pending callbacks for fallback mode (thread-safe queue)
        self._pending_callbacks = []
        self._callback_timer = QTimer(self)
        self._callback_timer.timeout.connect(self._process_pending_callbacks)
        self._callback_timer.start(50)  # Process every 50ms

    def _convert_key_sequence(self, qkey_sequence: str) -> str:
        if not qkey_sequence:
            return ""
        normalized_parts = [
            _normalize_key_token(part) for part in qkey_sequence.split("+")
        ]
        return "+".join(part.replace(" ", "") for part in normalized_parts if part)

    def register_shortcut(self, key_sequence: str, callback) -> bool:
        """Register a global hotkey. Returns True on success."""
        try:
            if not key_sequence:
                return False

            norm = self._convert_key_sequence(key_sequence)
            logger.info(f"Registering hotkey: '{key_sequence}' (norm='{norm}')")

            # Unregister existing if any
            if norm in self._key_to_id or norm in self._fallback_hotkeys:
                self.unregister_shortcut(key_sequence)

            # Try Windows RegisterHotKey first (Windows only)
            if _IS_WINDOWS and _user32:
                modifiers, vk = _parse_hotkey(norm)
                if vk:
                    hk_id = self._next_id
                    self._next_id += 1

                    ok = _user32.RegisterHotKey(None, hk_id, modifiers, vk)
                    if ok:
                        self._id_to_key[hk_id] = norm
                        self._key_to_id[norm] = hk_id
                        self.callbacks[norm] = callback
                        logger.info(f"✓ Registered via RegisterHotKey: {key_sequence} (id=0x{hk_id:X}, mod=0x{modifiers:X}, vk=0x{vk:X})")
                        return True
                    else:
                        err = ctypes.get_last_error()
                        logger.warning(f"RegisterHotKey failed for '{key_sequence}': win32 error {err}")

            # macOS / Linux fallback via pynput (does not require admin)
            if not _IS_WINDOWS and _PYNPUT_AVAILABLE:
                try:
                    self._ensure_pynput_listener()
                    parsed = pynput_keyboard.HotKey.parse(_to_pynput_format(norm))
                    hotkey = pynput_keyboard.HotKey(
                        parsed, lambda n=norm: self._queue_callback(n)
                    )
                    self._pynput_hotkeys[norm] = hotkey
                    self.callbacks[norm] = callback
                    logger.info(f"✓ Registered via pynput: {key_sequence} (format={_to_pynput_format(norm)})")
                    return True
                except Exception as e:
                    logger.error(f"pynput hotkey registration failed for '{key_sequence}': {e}")
            elif not _IS_WINDOWS:
                logger.error(f"pynput unavailable for '{key_sequence}': {_PYNPUT_IMPORT_ERROR}")

            # Fallback to keyboard library
            if _IS_WINDOWS and _KEYBOARD_AVAILABLE:
                try:
                    kb_format = _to_keyboard_format(norm)
                    # Use a wrapper to queue callback to main thread
                    def make_wrapper(n):
                        def wrapper():
                            self._queue_callback(n)
                        return wrapper

                    handle = keyboard.add_hotkey(kb_format, make_wrapper(norm), suppress=False)
                    if handle:
                        self._fallback_hotkeys[norm] = handle
                        self.callbacks[norm] = callback
                        logger.info(f"✓ Registered via keyboard library: {key_sequence} (format={kb_format})")
                        return True
                except Exception as e:
                    logger.error(f"keyboard.add_hotkey failed for '{key_sequence}': {e}")
            elif _IS_WINDOWS:
                logger.error(f"keyboard fallback unavailable for '{key_sequence}': {_KEYBOARD_IMPORT_ERROR}")

            logger.error(f"✗ Failed to register hotkey: {key_sequence}")
            return False

        except Exception as e:
            logger.error(f"Error registering hotkey '{key_sequence}': {e}")
            return False

    def _queue_callback(self, norm: str):
        """Queue a callback to be executed on the main thread."""
        self._pending_callbacks.append(norm)

    def _process_pending_callbacks(self):
        """Process pending callbacks (called on main thread via timer)."""
        while self._pending_callbacks:
            norm = self._pending_callbacks.pop(0)
            self.on_hotkey_triggered.emit(norm)

    def unregister_shortcut(self, key_sequence: str) -> bool:
        """Unregister a global hotkey."""
        try:
            if not key_sequence:
                return False

            norm = self._convert_key_sequence(key_sequence)
            logger.info(f"Unregistering hotkey: '{key_sequence}' (norm='{norm}')")

            # Try to unregister from Windows API
            hk_id = self._key_to_id.pop(norm, None)
            if hk_id is not None:
                self._id_to_key.pop(hk_id, None)
                self.callbacks.pop(norm, None)
                if _IS_WINDOWS and _user32:
                    _user32.UnregisterHotKey(None, hk_id)
                logger.info(f"✓ Unregistered from RegisterHotKey: {key_sequence}")
                return True

            # Try to unregister from keyboard library fallback
            handle = self._fallback_hotkeys.pop(norm, None)
            if handle is not None:
                self.callbacks.pop(norm, None)
                if _KEYBOARD_AVAILABLE:
                    try:
                        keyboard.remove_hotkey(handle)
                    except Exception as e:
                        logger.warning(f"Error removing keyboard hotkey: {e}")
                logger.info(f"✓ Unregistered from keyboard library: {key_sequence}")
                return True

            hotkey = self._pynput_hotkeys.pop(norm, None)
            if hotkey is not None:
                self.callbacks.pop(norm, None)
                if not self._pynput_hotkeys:
                    self._stop_pynput_listener()
                logger.info(f"✓ Unregistered from pynput: {key_sequence}")
                return True

            return False

        except Exception as e:
            logger.error(f"Error unregistering hotkey '{key_sequence}': {e}")
            return False

    def unregister_all(self):
        """Unregister all hotkeys."""
        logger.info("Unregistering all hotkeys...")

        # Unregister Windows API hotkeys
        for hk_id in list(self._id_to_key.keys()):
            try:
                if _IS_WINDOWS and _user32:
                    _user32.UnregisterHotKey(None, hk_id)
            except Exception as e:
                logger.warning(f"UnregisterHotKey 0x{hk_id:X} error: {e}")

        # Unregister keyboard library hotkeys
        if _KEYBOARD_AVAILABLE:
            for handle in self._fallback_hotkeys.values():
                try:
                    keyboard.remove_hotkey(handle)
                except Exception as e:
                    logger.warning(f"Error removing keyboard hotkey: {e}")

        self._stop_pynput_listener()

        self._id_to_key.clear()
        self._key_to_id.clear()
        self._fallback_hotkeys.clear()
        self._pynput_hotkeys.clear()
        self.callbacks.clear()
        logger.info("All hotkeys unregistered")

    def _on_wm_hotkey(self, hotkey_id: int):
        """Called from the native event filter on the main Qt thread."""
        norm = self._id_to_key.get(hotkey_id)
        logger.debug(f"_on_wm_hotkey: id=0x{hotkey_id:X}, norm={norm}")
        if norm:
            self.on_hotkey_triggered.emit(norm)

    def _handle_signal(self, key_sequence: str):
        """Handle hotkey trigger signal."""
        callback = self.callbacks.get(key_sequence)
        logger.debug(f"_handle_signal: key_sequence={key_sequence}, has_callback={callback is not None}")
        if callback:
            try:
                callback()
            except Exception as e:
                logger.error(f"Hotkey callback error for '{key_sequence}': {e}")
        else:
            logger.warning(f"No callback found for key_sequence: {key_sequence}")

    def is_registered(self, key_sequence: str) -> bool:
        """Check if a hotkey is registered."""
        norm = self._convert_key_sequence(key_sequence)
        return (
            norm in self._key_to_id
            or norm in self._fallback_hotkeys
            or norm in self._pynput_hotkeys
        )

    def _ensure_pynput_listener(self):
        if self._pynput_listener is not None:
            return
        if not _PYNPUT_AVAILABLE:
            raise RuntimeError("pynput is unavailable")

        def on_press(key):
            listener = self._pynput_listener
            if listener is None:
                return
            canonical = listener.canonical(key)
            for hotkey in list(self._pynput_hotkeys.values()):
                hotkey.press(canonical)

        def on_release(key):
            listener = self._pynput_listener
            if listener is None:
                return
            canonical = listener.canonical(key)
            for hotkey in list(self._pynput_hotkeys.values()):
                hotkey.release(canonical)

        self._pynput_listener = pynput_keyboard.Listener(
            on_press=on_press,
            on_release=on_release,
        )
        self._pynput_listener.start()
        logger.info("pynput hotkey listener started")

    def _stop_pynput_listener(self):
        if self._pynput_listener is None:
            return
        try:
            self._pynput_listener.stop()
        except Exception as e:
            logger.warning(f"Error stopping pynput listener: {e}")
        self._pynput_listener = None
