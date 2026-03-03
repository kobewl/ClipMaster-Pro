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
_LOCAL_DEBOUNCE_MS = 20     # Faster response for local matching
_AI_DEBOUNCE_MS = 400       # Slightly longer for AI to reduce API calls
_MIN_LOCAL_CHARS = 2        # Lower threshold for faster triggering
_MAX_LOCAL_ITEMS = 30       # More items for better matching
_AUTO_DISMISS_MS = 8000     # auto-dismiss overlay after 8 s
_MAX_AI_PREDICTION_LEN = 50  # Limit AI prediction length


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

        # Real-time AI prediction timer (triggered after typing pause)
        self._ai_realtime_timer = QTimer(self)
        self._ai_realtime_timer.setSingleShot(True)
        self._ai_realtime_timer.setInterval(600)
        self._ai_realtime_timer.timeout.connect(self._try_realtime_ai_prediction)

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
        self._ai_realtime_timer.stop()
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

        # Also schedule AI prediction if no local match found after delay
        if not self._showing and len(context) >= 5:
            self._ai_realtime_timer.stop()
            self._ai_realtime_timer.start()

    def _do_local_match(self):
        """Improved local matching with better relevance scoring."""
        context = self.input_monitor.get_context()
        if not context or self._cooldown or self._showing:
            return

        lines = context.split("\n")
        current_line = (lines[-1] if lines else context)

        if len(current_line.strip()) < _MIN_LOCAL_CHARS:
            return

        current_lower = current_line.lower().strip()
        best = ""
        best_score = 0

        for content in self._clip_cache:
            content_stripped = content.strip()
            if len(content_stripped) <= len(current_line.strip()):
                continue

            content_lower = content_stripped.lower()

            # Priority 1: Exact prefix match (user typed beginning of cached content)
            if content_lower.startswith(current_lower):
                remainder = content_stripped[len(current_line):]
                if len(remainder) >= 2:
                    # Score based on how much of the content matches + base score
                    match_ratio = len(current_lower) / len(content_lower)
                    score = 2000 + int(match_ratio * 1000) + len(remainder)
                    if score > best_score:
                        best = remainder
                        best_score = score
                continue

            # Priority 2: Partial suffix/prefix match (typed ending of one line matches start of another)
            line_len = len(current_lower)
            cnt_len = len(content_lower)

            # Try matching last 2-10 characters of input with start of clipboard content
            for match_len in range(min(10, line_len), min(1, line_len) - 1, -1):
                if match_len < 2:
                    break
                if current_lower[-match_len:] == content_lower[:match_len]:
                    remainder = content_stripped[match_len:]
                    if len(remainder) >= 2:
                        # Higher score for longer matches
                        score = 500 + match_len * 10 + len(remainder)
                        if score > best_score and best_score < 2000:
                            best = remainder
                            best_score = score
                    break

            # Priority 3: Check if clipboard contains the current line somewhere in middle
            if current_lower in content_lower:
                idx = content_lower.find(current_lower)
                remainder = content_stripped[idx + len(current_line):]
                if len(remainder) >= 2:
                    score = 300 + len(remainder)
                    if score > best_score and best_score < 500:
                        best = remainder
                        best_score = score

        # Only show if score meets threshold
        if best and len(best) >= 2 and best_score >= 300:
            logger.debug(f"Local match found: score={best_score}, text='{best[:30]}...'")
            self._pred_context = context
            self._pred_full = best
            self._show(best)

    # ── Word boundary → AI prediction ────────────────────────────────

    def _try_realtime_ai_prediction(self):
        """Try AI prediction during active typing (not just at word boundaries)."""
        if self._cooldown or self._ai_busy or self._showing:
            return

        context = self.input_monitor.get_context()
        if not context or len(context.strip()) < 5:
            return

        lines = context.split("\n")
        current_line = lines[-1].strip() if lines else context.strip()

        # Only trigger if we have a substantial partial word
        # (e.g., "hello wo" - we can predict "rld")
        words = current_line.split()
        if not words:
            return

        last_word = words[-1]
        # Only predict if last word is partially typed (3+ chars) and not too long
        if len(last_word) < 3 or len(last_word) > 20:
            return

        # Don't trigger too frequently
        if hasattr(self, '_last_ai_time'):
            import time
            if time.time() - self._last_ai_time < 3:  # Max once per 3 seconds
                return

        items = self.clipboard_service.get_history(limit=8)
        text_items = [it for it in items if it.content_type.value == "text"]
        if not text_items:
            return

        self._gen += 1
        gen = self._gen
        self._ai_busy = True
        import time
        self._last_ai_time = time.time()
        self.ai_service.request_prediction(context, text_items, gen)
        logger.debug(f"Realtime AI prediction triggered for: '{last_word}'")

    def _on_word_completed(self, context: str):
        """Trigger AI prediction with smarter filtering."""
        if self._cooldown or self._ai_busy:
            return
        if self._showing and len(self._pred_full) > 5:
            return
        if not self._clip_cache:
            return

        # Only trigger AI if we have meaningful context
        lines = context.split("\n")
        current_line = lines[-1].strip() if lines else context.strip()

        # Skip if current line is too short or just whitespace
        if len(current_line) < 3:
            return

        # Skip if context is just special characters
        if not any(c.isalnum() for c in current_line):
            return

        # Get fresh history items
        self._gen += 1
        gen = self._gen
        items = self.clipboard_service.get_history(limit=10)
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

        # Filter out [NONE] responses
        if cleaned.startswith("[NONE]") or cleaned == "[NONE":
            return

        # Validate prediction isn't just repeating context
        current_context = self.input_monitor.get_context()
        current_line = current_context.split("\n")[-1] if "\n" in current_context else current_context

        # Don't show if prediction is already in the current line
        if cleaned.lower() in current_line.lower():
            logger.debug(f"AI prediction '{cleaned}' already in context, discarding")
            return

        # Don't show if prediction is too similar to what user just typed
        last_words = current_line.split()[-3:] if current_line.split() else []
        last_part = " ".join(last_words).lower()
        if cleaned.lower().startswith(last_part) or last_part.endswith(cleaned.lower()[:10]):
            logger.debug(f"AI prediction too similar to context, discarding")
            return

        self._pred_context = current_context
        self._pred_full = cleaned
        logger.info(f"AI prediction ready: '{cleaned[:40]}...'")
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
