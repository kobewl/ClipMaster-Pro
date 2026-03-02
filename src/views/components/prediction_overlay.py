"""
Prediction Overlay - A translucent, always-on-top popup that shows AI
predictions near the text caret.  Does NOT steal focus.

On Windows the caret position is obtained via GetGUIThreadInfo();
falls back to the mouse cursor position.
"""

import platform
from PyQt6.QtWidgets import (
    QWidget, QLabel, QHBoxLayout,
    QGraphicsDropShadowEffect,
)
from PyQt6.QtCore import Qt, QPoint
from PyQt6.QtGui import QColor, QGuiApplication

from utils.logger import logger

# ── Windows caret helper ─────────────────────────────────────────────

def _get_caret_pos_win():
    """Return (x, y) screen coords of the text caret on Windows."""
    try:
        import ctypes
        import ctypes.wintypes as wt

        class GUITHREADINFO(ctypes.Structure):
            _fields_ = [
                ("cbSize",       wt.DWORD),
                ("flags",        wt.DWORD),
                ("hwndActive",   wt.HWND),
                ("hwndFocus",    wt.HWND),
                ("hwndCapture",  wt.HWND),
                ("hwndMenuOwner", wt.HWND),
                ("hwndMoveSize", wt.HWND),
                ("hwndCaret",    wt.HWND),
                ("rcCaret",      wt.RECT),
            ]

        info = GUITHREADINFO()
        info.cbSize = ctypes.sizeof(GUITHREADINFO)

        if ctypes.windll.user32.GetGUIThreadInfo(0, ctypes.byref(info)):
            if info.hwndCaret:
                pt = wt.POINT(info.rcCaret.left, info.rcCaret.bottom)
                ctypes.windll.user32.ClientToScreen(
                    info.hwndCaret, ctypes.byref(pt)
                )
                return (pt.x, pt.y)

        # Fallback: mouse cursor position
        pt = wt.POINT()
        ctypes.windll.user32.GetCursorPos(ctypes.byref(pt))
        return (pt.x, pt.y)
    except Exception:
        return None


def get_caret_position():
    if platform.system() == "Windows":
        return _get_caret_pos_win()
    return None


# ── Overlay Widget ───────────────────────────────────────────────────

class PredictionOverlay(QWidget):
    """Frameless, translucent popup for showing AI predictions."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self._prediction = ""
        self._setup_window()
        self._setup_ui()

    # ── Window config ────────────────────────────────────────────────

    def _setup_window(self):
        self.setWindowFlags(
            Qt.WindowType.FramelessWindowHint
            | Qt.WindowType.WindowStaysOnTopHint
            | Qt.WindowType.Tool
            | Qt.WindowType.WindowDoesNotAcceptFocus
        )
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
        self.setFocusPolicy(Qt.FocusPolicy.NoFocus)

    # ── UI ───────────────────────────────────────────────────────────

    def _setup_ui(self):
        root = QHBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)

        self._container = QWidget()
        self._container.setObjectName("predOverlay")
        inner = QHBoxLayout(self._container)
        inner.setContentsMargins(14, 8, 14, 8)
        inner.setSpacing(10)

        self._text = QLabel()
        self._text.setObjectName("predText")
        self._text.setWordWrap(True)
        self._text.setMaximumWidth(420)
        inner.addWidget(self._text)

        self._hint = QLabel("Tab")
        self._hint.setObjectName("predHint")
        inner.addWidget(self._hint)

        root.addWidget(self._container)

        shadow = QGraphicsDropShadowEffect()
        shadow.setBlurRadius(24)
        shadow.setOffset(0, 4)
        shadow.setColor(QColor(0, 0, 0, 80))
        self._container.setGraphicsEffect(shadow)

        self._apply_style()

    def _apply_style(self):
        self.setStyleSheet("""
            #predOverlay {
                background-color: rgba(24, 24, 28, 235);
                border: 1px solid rgba(255, 255, 255, 0.08);
                border-radius: 10px;
            }
            #predText {
                color: rgba(230, 230, 230, 0.92);
                font-size: 13px;
                font-family: 'Segoe UI', 'Microsoft YaHei', sans-serif;
            }
            #predHint {
                color: rgba(76, 194, 255, 0.85);
                font-size: 11px;
                font-family: 'Segoe UI Semibold', monospace;
                padding: 2px 8px;
                background-color: rgba(76, 194, 255, 0.12);
                border-radius: 4px;
            }
        """)

    # ── Public API ───────────────────────────────────────────────────

    def show_prediction(self, text: str):
        self._prediction = text
        display = text if len(text) <= 80 else text[:77] + "..."
        self._text.setText(display)
        self.adjustSize()
        self._position_near_caret()
        if not self.isVisible():
            self.show()

    def update_prediction(self, text: str):
        """Update displayed text without repositioning (for streaming)."""
        self._prediction = text
        display = text if len(text) <= 80 else text[:77] + "..."
        self._text.setText(display)
        self.adjustSize()

    def hide_prediction(self):
        self._prediction = ""
        self.hide()

    def get_prediction(self) -> str:
        return self._prediction

    # ── Positioning ──────────────────────────────────────────────────

    def _position_near_caret(self):
        pos = get_caret_position()
        if not pos:
            return

        x, y = pos[0], pos[1] + 6  # small gap below caret

        # Keep the popup on-screen
        screen = QGuiApplication.screenAt(QPoint(pos[0], pos[1]))
        if screen:
            geo = screen.availableGeometry()
            if x + self.width() > geo.right():
                x = geo.right() - self.width() - 8
            if y + self.height() > geo.bottom():
                y = pos[1] - self.height() - 6  # show above caret
            x = max(x, geo.left())
            y = max(y, geo.top())

        self.move(x, y)
