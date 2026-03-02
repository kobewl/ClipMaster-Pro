"""
Prediction Engine - Multi-level real-time text prediction.

SAFETY-FIRST design:
  - NO keyboard.add_hotkey / keyboard.remove_hotkey — avoids lock contention
    with the WH_KEYBOARD_LL hook thread that destabilised Windows.
  - Tab / Esc detection is done through InputMonitor's deque-polling, which
    runs entirely on the main thread.
  - Auto-dismiss after 8 seconds prevents stuck overlays.
  - keyboard.write runs in a background thread.
"""

import threading
import keyboard
from PyQt6.QtCore import QObject, QTimer, pyqtSignal

from services.ai_service import AIService
from services.clipboard_service import ClipboardService
from controllers.input_monitor import InputMonitor
from views.components.prediction_overlay import PredictionOverlay
from config.settings import Settings
from utils.logger import logger


_CACHE_REFRESH_MS = 5000
_LOCAL_DEBOUNCE_MS = 30
_MIN_LOCAL_CHARS = 3
_MAX_LOCAL_ITEMS = 20
_AUTO_DISMISS_MS = 8000     # auto-dismiss overlay after 8 s


class PredictionEngine(QObject):
    """Coordinates local matching, AI prediction, and overlay display."""

    status_changed = pyqtSignal(str)

    def __init__(self, clipboard_service: ClipboardService, parent=None):
        super().__init__(parent)
        self.clipboard_service = clipboard_service

        self.ai_service = AIService()
        self.input_monitor = InputMonitor(ai_debounce_ms=300)
        self.overlay = PredictionOverlay()

        self._showing = False
        self._pred_context = ""
        self._pred_full = ""
        self._gen = 0
        self._cooldown = False
        self._ai_busy = False

        # Clipboard cache (refreshed periodically, not per keystroke)
        self._clip_cache: list = []
        self._cache_timer = QTimer(self)
        self._cache_timer.setInterval(_CACHE_REFRESH_MS)
        self._cache_timer.timeout.connect(self._refresh_cache)

        # Local match debounce
        self._local_timer = QTimer(self)
        self._local_timer.setSingleShot(True)
        self._local_timer.setInterval(_LOCAL_DEBOUNCE_MS)
        self._local_timer.timeout.connect(self._do_local_match)

        # Auto-dismiss safety timer
        self._dismiss_timer = QTimer(self)
        self._dismiss_timer.setSingleShot(True)
        self._dismiss_timer.setInterval(_AUTO_DISMISS_MS)
        self._dismiss_timer.timeout.connect(self._do_dismiss)

        # Wire signals — ALL on the main thread via InputMonitor's polling
        self.input_monitor.typing_changed.connect(self._on_typing_changed)
        self.input_monitor.word_completed.connect(self._on_word_completed)
        self.input_monitor.tab_pressed.connect(self._do_accept)
        self.input_monitor.esc_pressed.connect(self._do_dismiss)
        self.ai_service.prediction_ready.connect(self._on_ai_result)
        self.ai_service.prediction_error.connect(self._on_ai_error)

        if self.ai_service.is_configured():
            self.start()

    # ── Lifecycle ────────────────────────────────────────────────────

    def start(self):
        if not self.ai_service.is_configured():
            logger.warning("AI service not configured; prediction engine idle")
            return
        self._refresh_cache()
        self._cache_timer.start()
        self.input_monitor.start()
        self.status_changed.emit("AI prediction ON")
        logger.info("Prediction engine started")

    def stop(self):
        self.input_monitor.stop()
        self._cache_timer.stop()
        self._local_timer.stop()
        self._dismiss_timer.stop()
        self._do_dismiss()
        self.status_changed.emit("AI prediction OFF")
        logger.info("Prediction engine stopped")

    def reload_settings(self):
        was_running = self.input_monitor._enabled
        if was_running:
            self.stop()
        self.ai_service.reload_settings()
        ai = Settings.get("ai", {})
        self.input_monitor.set_pause_delay(ai.get("trigger_delay", 300))
        if self.ai_service.is_configured():
            self.start()

    # ── Clipboard cache ──────────────────────────────────────────────

    def _refresh_cache(self):
        try:
            items = self.clipboard_service.get_history(limit=_MAX_LOCAL_ITEMS)
            self._clip_cache = [
                it.content.strip()
                for it in items
                if it.content_type.value == "text" and it.content.strip()
            ]
        except Exception as e:
            logger.error(f"Failed to refresh clipboard cache: {e}")

    # ── Every keystroke ──────────────────────────────────────────────

    def _on_typing_changed(self, context: str):
        if self._cooldown:
            return

        # Progressive trimming
        if self._showing and self._pred_context and self._pred_full:
            if len(context) >= len(self._pred_context):
                new_chars = context[len(self._pred_context):]
                remaining = self._pred_full

                if new_chars and remaining:
                    match_len = 0
                    for i, ch in enumerate(new_chars):
                        if i < len(remaining) and ch == remaining[i]:
                            match_len += 1
                        else:
                            break

                    if match_len > 0 and match_len == len(new_chars):
                        self._pred_full = remaining[match_len:]
                        self._pred_context = context
                        if self._pred_full:
                            self.overlay.update_prediction(self._pred_full)
                            return
                        else:
                            self._do_dismiss()
                            return
                    else:
                        self._do_dismiss()
            else:
                self._do_dismiss()

        # Schedule debounced local matching
        self._local_timer.stop()
        self._local_timer.start()

    def _do_local_match(self):
        context = self.input_monitor.get_context()
        if not context or self._cooldown or self._showing:
            return

        lines = context.split("\n")
        current_line = (lines[-1] if lines else context).strip()

        if len(current_line) < _MIN_LOCAL_CHARS:
            return

        current_lower = current_line.lower()
        best = ""
        best_score = 0

        for content in self._clip_cache:
            if len(content) <= len(current_line):
                continue
            content_lower = content.lower()

            if content_lower.startswith(current_lower):
                remainder = content[len(current_line):]
                score = len(remainder) + 1000
                if score > best_score:
                    best = remainder
                    best_score = score
                continue

            line_len = len(current_line)
            cnt_len = len(content)
            for frac in (1.0, 0.75, 0.5, 0.25):
                ol = min(int(line_len * frac), cnt_len)
                if ol < _MIN_LOCAL_CHARS:
                    break
                if current_lower[-ol:] == content_lower[:ol]:
                    remainder = content[ol:]
                    score = ol * 2
                    if remainder and score > best_score and best_score < 1000:
                        best = remainder
                        best_score = score
                    break

        if best and len(best) >= 2:
            self._pred_context = context
            self._pred_full = best
            self._show(best)

    # ── Word boundary → AI prediction ────────────────────────────────

    def _on_word_completed(self, context: str):
        if self._cooldown or self._ai_busy:
            return
        if self._showing and len(self._pred_full) > 5:
            return
        if not self._clip_cache:
            return

        self._gen += 1
        gen = self._gen
        items = self.clipboard_service.get_history(limit=15)
        text_items = [it for it in items if it.content_type.value == "text"]
        if not text_items:
            return

        self._ai_busy = True
        self.ai_service.request_prediction(context, text_items, gen)

    def _on_ai_result(self, text: str):
        self._ai_busy = False
        if not text or not text.strip() or self._cooldown or self._showing:
            return
        cleaned = text.strip()
        if cleaned.startswith("[NONE]"):
            return
        current_context = self.input_monitor.get_context()
        self._pred_context = current_context
        self._pred_full = cleaned
        self._show(cleaned)

    def _on_ai_error(self, _err: str):
        self._ai_busy = False

    # ── Show / Accept / Dismiss ──────────────────────────────────────

    def _show(self, text: str):
        if self._showing and self.overlay.get_prediction() == text:
            return
        self._showing = True
        self.input_monitor.set_intercept_tab(True)
        self.overlay.show_prediction(text)
        # Auto-dismiss safety net
        self._dismiss_timer.stop()
        self._dismiss_timer.start()

    def _do_accept(self):
        if not self._showing:
            return
        prediction = self._pred_full
        self._do_dismiss()
        if not prediction:
            return

        self._cooldown = True
        self.input_monitor.suppress()

        def _write_in_bg():
            try:
                keyboard.write(prediction, delay=0.005)
            except Exception as e:
                logger.error(f"keyboard.write failed: {e}")
            finally:
                delay = max(int(len(prediction) * 8), 400)
                QTimer.singleShot(delay, self._end_cooldown)

        threading.Thread(target=_write_in_bg, daemon=True).start()

    def _end_cooldown(self):
        self._cooldown = False
        self.input_monitor.unsuppress()

    def _do_dismiss(self):
        if not self._showing and not self.overlay.isVisible():
            return
        self._showing = False
        self._pred_context = ""
        self._pred_full = ""
        self.input_monitor.set_intercept_tab(False)
        self._dismiss_timer.stop()
        self.overlay.hide_prediction()
