"""
Input Monitor - Captures global keyboard input to build a typing context buffer.

CRITICAL DESIGN: The keyboard hook callback (`_on_raw_key`) runs inside
Windows' low-level hook thread (WH_KEYBOARD_LL).  It MUST return in
< 200 ms or Windows silently unhooks it, destabilising the whole system.

Therefore the callback does ONLY `deque.append(name)` — zero Qt interaction,
zero mutex, zero cross-thread anything.  A QTimer polls the deque every 16 ms
(~60 fps) on the main thread and does all processing there.
"""

from collections import deque
import keyboard as kb_lib
from PyQt6.QtCore import QObject, pyqtSignal, QTimer
from utils.logger import logger


_WORD_BOUNDARY = frozenset({
    "space", "enter", ".", ",", ";", ":", "!", "?",
    "/", "\\", "(", ")", "[", "]", "{", "}", "-",
    "'", '"', "=", "+", "@", "#",
})

_IGNORED = frozenset({
    "ctrl", "alt", "shift", "left windows", "right windows",
    "left ctrl", "right ctrl", "left alt", "right alt",
    "left shift", "right shift",
    "caps lock", "num lock", "scroll lock",
    "print screen", "insert", "pause",
    "home", "end", "page up", "page down",
    "up", "down", "left", "right",
    "f1", "f2", "f3", "f4", "f5", "f6",
    "f7", "f8", "f9", "f10", "f11", "f12",
    "delete",
})

_POLL_INTERVAL_MS = 16   # ~60 fps


class InputMonitor(QObject):
    """Watches global keystrokes using a safe deque-polling architecture."""

    typing_changed = pyqtSignal(str)
    word_completed = pyqtSignal(str)

    # Emitted on main thread when Tab / Esc are detected
    tab_pressed = pyqtSignal()
    esc_pressed = pyqtSignal()

    def __init__(self, ai_debounce_ms: int = 300, parent=None):
        super().__init__(parent)
        self._buffer: list = []
        self._max_buf = 500
        self._enabled = False
        self._suppressed = False
        self._hook = None

        # Whether Tab should be treated as "accept prediction"
        self._intercept_tab = False

        # Thread-safe FIFO: hook thread appends, main thread pops
        self._key_deque: deque = deque(maxlen=2000)

        # Polling timer drains the deque on the main thread
        self._poll_timer = QTimer(self)
        self._poll_timer.setInterval(_POLL_INTERVAL_MS)
        self._poll_timer.timeout.connect(self._poll_keys)

        # Debounce for AI predictions (after word boundary)
        self._ai_timer = QTimer(self)
        self._ai_timer.setSingleShot(True)
        self._ai_timer.setInterval(ai_debounce_ms)
        self._ai_timer.timeout.connect(self._fire_word_completed)

        # Idle timer — fires if user stops typing (even mid-word)
        self._idle_timer = QTimer(self)
        self._idle_timer.setSingleShot(True)
        self._idle_timer.setInterval(800)
        self._idle_timer.timeout.connect(self._fire_word_completed)

    # ── Public API ───────────────────────────────────────────────────

    def start(self):
        if self._enabled:
            return
        self._enabled = True
        try:
            self._hook = kb_lib.on_press(self._on_raw_key, suppress=False)
        except Exception as e:
            logger.error(f"Failed to install keyboard hook: {e}")
            self._enabled = False
            return
        self._poll_timer.start()
        logger.info("Input monitor started (deque-polling mode)")

    def stop(self):
        if not self._enabled:
            return
        self._enabled = False
        self._poll_timer.stop()
        if self._hook is not None:
            try:
                kb_lib.unhook(self._hook)
            except Exception:
                pass
            self._hook = None
        self._ai_timer.stop()
        self._idle_timer.stop()
        self._key_deque.clear()
        logger.info("Input monitor stopped")

    def suppress(self):
        self._suppressed = True

    def unsuppress(self):
        self._suppressed = False

    def clear_buffer(self):
        self._buffer.clear()

    def get_context(self) -> str:
        return "".join(self._buffer)

    def set_pause_delay(self, ms: int):
        self._ai_timer.setInterval(max(ms, 100))

    def set_intercept_tab(self, intercept: bool):
        self._intercept_tab = intercept

    # ── Keyboard hook (background thread — ABSOLUTE MINIMUM work) ────

    def _on_raw_key(self, event):
        """Called in the WH_KEYBOARD_LL hook thread.
        MUST return ASAP — only append to deque."""
        if not self._enabled:
            return
        name = event.name if event.name else ""
        if name:
            self._key_deque.append(name)

    # ── Main-thread polling ──────────────────────────────────────────

    def _poll_keys(self):
        """Drain the deque and process keys safely on the main thread."""
        if self._suppressed:
            self._key_deque.clear()
            return

        processed = 0
        while self._key_deque and processed < 50:
            try:
                key_name = self._key_deque.popleft()
            except IndexError:
                break
            processed += 1
            self._handle_key(key_name)

    # ── Key processing (main thread only) ────────────────────────────

    def _handle_key(self, key_name: str):
        name = key_name.lower()

        if name in _IGNORED:
            return

        # Tab / Esc — emit signals for PredictionEngine
        if name == "tab":
            if self._intercept_tab:
                self.tab_pressed.emit()
            return

        if name == "escape":
            self.esc_pressed.emit()
            self._buffer.clear()
            return

        if name == "backspace":
            if self._buffer:
                self._buffer.pop()
            self._emit_changed()
            return

        is_boundary = name in _WORD_BOUNDARY

        if name == "enter":
            self._buffer.append("\n")
        elif name == "space":
            self._buffer.append(" ")
        elif len(key_name) == 1:
            self._buffer.append(key_name)
        else:
            return

        if len(self._buffer) > self._max_buf:
            self._buffer = self._buffer[-self._max_buf:]

        self._emit_changed()

        self._idle_timer.stop()
        self._idle_timer.start()

        if is_boundary:
            self._ai_timer.stop()
            self._ai_timer.start()

    # ── Signal helpers ───────────────────────────────────────────────

    def _emit_changed(self):
        ctx = "".join(self._buffer)
        if ctx:
            self.typing_changed.emit(ctx)

    def _fire_word_completed(self):
        self._ai_timer.stop()
        self._idle_timer.stop()
        ctx = "".join(self._buffer)
        if len(ctx.strip()) >= 3:
            self.word_completed.emit(ctx)
