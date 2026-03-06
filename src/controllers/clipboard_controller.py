import base64
import ctypes
import ctypes.wintypes as wt
import platform

from PyQt6.QtCore import QByteArray, QBuffer, QObject, Qt, QTimer, QUrl, pyqtSignal
from PyQt6.QtGui import QClipboard, QImage
from PyQt6.QtWidgets import QApplication

from models.clipboard_item import ClipboardItem, ContentType
from services.clipboard_service import ClipboardService
from utils.logger import logger
from utils.source_tracker import SourceTracker

_IS_WINDOWS = platform.system() == "Windows"
if _IS_WINDOWS:
    _user32 = ctypes.WinDLL("user32", use_last_error=True)
    _kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    _shell32 = ctypes.WinDLL("shell32", use_last_error=True)

    _user32.OpenClipboard.argtypes = [wt.HWND]
    _user32.OpenClipboard.restype = wt.BOOL
    _user32.CloseClipboard.argtypes = []
    _user32.CloseClipboard.restype = wt.BOOL
    _user32.IsClipboardFormatAvailable.argtypes = [wt.UINT]
    _user32.IsClipboardFormatAvailable.restype = wt.BOOL
    _user32.GetClipboardData.argtypes = [wt.UINT]
    _user32.GetClipboardData.restype = wt.HANDLE
    _user32.GetClipboardSequenceNumber.argtypes = []
    _user32.GetClipboardSequenceNumber.restype = wt.DWORD
    _user32.RegisterClipboardFormatW.argtypes = [wt.LPCWSTR]
    _user32.RegisterClipboardFormatW.restype = wt.UINT

    _kernel32.GlobalLock.argtypes = [wt.HGLOBAL]
    _kernel32.GlobalLock.restype = wt.LPVOID
    _kernel32.GlobalUnlock.argtypes = [wt.HGLOBAL]
    _kernel32.GlobalUnlock.restype = wt.BOOL
    _kernel32.GlobalSize.argtypes = [wt.HGLOBAL]
    _kernel32.GlobalSize.restype = ctypes.c_size_t

    _shell32.DragQueryFileW.argtypes = [wt.HANDLE, wt.UINT, wt.LPWSTR, wt.UINT]
    _shell32.DragQueryFileW.restype = wt.UINT

    _CF_UNICODETEXT = 13
    _CF_HDROP = 15
    _CF_HTML = _user32.RegisterClipboardFormatW("HTML Format")
else:
    _user32 = None
    _kernel32 = None
    _shell32 = None
    _CF_UNICODETEXT = 0
    _CF_HDROP = 0
    _CF_HTML = 0


class ClipboardController(QObject):
    """Clipboard monitor and copy helper."""

    history_updated = pyqtSignal()
    item_added = pyqtSignal(object)

    def __init__(self, clipboard: QClipboard):
        super().__init__()
        self.clipboard = clipboard
        self.service = ClipboardService()
        self._last_clipboard_data = None
        self._last_clipboard_sequence = self._get_clipboard_sequence_number()

        self._debounce_timer = QTimer(self)
        self._debounce_timer.setSingleShot(True)
        self._debounce_timer.timeout.connect(self._process_clipboard_change)

        # Some Windows apps update clipboard formats lazily. Poll as a fallback.
        self._poll_timer = QTimer(self)
        self._poll_timer.timeout.connect(self._poll_clipboard)
        self._poll_timer.start(350)

        self.clipboard.dataChanged.connect(self._on_clipboard_changed)

        self.service.item_added.connect(self.item_added.emit)
        self.service.history_changed.connect(self.history_updated.emit)
        self.history_updated.emit()

    def _on_clipboard_changed(self):
        """Process clipboard changes with debounce."""
        self._debounce_timer.stop()
        self._debounce_timer.start(120)

    def _poll_clipboard(self):
        """Fallback for apps that do not emit a reliable clipboard notification."""
        sequence = self._get_clipboard_sequence_number()
        if sequence and sequence == self._last_clipboard_sequence:
            return

        signature = self._build_clipboard_signature()
        if signature and signature != self._last_clipboard_data:
            self._debounce_timer.stop()
            self._debounce_timer.start(120)

    def _process_clipboard_change(self):
        """Read clipboard content and add it to history."""
        try:
            native_snapshot = self._get_windows_clipboard_snapshot()
            if native_snapshot:
                self._process_native_snapshot(native_snapshot)
                self._last_clipboard_sequence = self._get_clipboard_sequence_number()
                return

            mime_data = self.clipboard.mimeData()
            if mime_data is None:
                self._last_clipboard_sequence = self._get_clipboard_sequence_number()
                return

            signature = self._build_clipboard_signature(mime_data)
            if not signature or signature == self._last_clipboard_data:
                self._last_clipboard_sequence = self._get_clipboard_sequence_number()
                return

            html_content = mime_data.html() if mime_data.hasHtml() else None
            source_info = SourceTracker.track_source(html_content)
            metadata = {"source": source_info.to_dict()}

            added = False
            if mime_data.hasImage():
                added = self._handle_image_clipboard(metadata)
            elif mime_data.hasUrls():
                added = self._handle_file_clipboard(mime_data.urls(), metadata)
            elif mime_data.hasHtml():
                html = mime_data.html()
                text = mime_data.text() or html
                metadata["html"] = html
                added = self.service.add_item(
                    content=text,
                    content_type=ContentType.HTML,
                    metadata=metadata,
                )
            else:
                text = mime_data.text()
                if text and text.strip():
                    added = self.service.add_item(
                        text,
                        ContentType.TEXT,
                        metadata=metadata,
                    )

            if added:
                self._last_clipboard_data = signature
                self._last_clipboard_sequence = self._get_clipboard_sequence_number()

        except Exception as e:
            logger.error(f"Error processing clipboard change: {e}")

    def _process_native_snapshot(self, snapshot):
        """Use the Windows clipboard API for apps that Qt misses."""
        snapshot_type = snapshot.get("type")
        content = snapshot.get("content")
        if not content:
            return False

        signature = f"{snapshot_type}:{content}"
        if signature == self._last_clipboard_data:
            return False

        source_info = SourceTracker.get_active_window_info()
        metadata = {
            "source": source_info.to_dict(),
            "native_clipboard": True,
        }

        added = False
        if snapshot_type == "files":
            metadata["files"] = content
            metadata["count"] = len(content)
            added = self.service.add_item(
                content="\n".join(content),
                content_type=ContentType.FILE,
                metadata=metadata,
            )
        elif snapshot_type == "html":
            html = content
            metadata["html"] = html
            added = self.service.add_item(
                content=snapshot.get("text") or html,
                content_type=ContentType.HTML,
                metadata=metadata,
            )
        elif snapshot_type == "text":
            if isinstance(content, str) and content.strip():
                added = self.service.add_item(
                    content,
                    ContentType.TEXT,
                    metadata=metadata,
                )

        if added:
            self._last_clipboard_data = signature
        return added

    def _build_clipboard_signature(self, mime_data=None):
        """Create a lightweight fingerprint for duplicate suppression."""
        native_snapshot = self._get_windows_clipboard_snapshot()
        if native_snapshot and native_snapshot.get("content"):
            content = native_snapshot["content"]
            if isinstance(content, list):
                content = "|".join(content)
            return f"{native_snapshot['type']}:{content}"

        if mime_data is None:
            mime_data = self.clipboard.mimeData()
        if mime_data is None:
            return None

        if mime_data.hasImage():
            image = self.clipboard.image()
            if not image.isNull():
                return f"image:{image.width()}x{image.height()}:{image.cacheKey()}"

        if mime_data.hasUrls():
            urls = [url.toString() for url in mime_data.urls()]
            if urls:
                return "urls:" + "|".join(urls)

        if mime_data.hasHtml():
            html = mime_data.html()
            if html:
                return "html:" + html

        text = mime_data.text()
        if text:
            return "text:" + text

        return None

    def _get_clipboard_sequence_number(self):
        if not _IS_WINDOWS or not _user32:
            return 0

        try:
            return int(_user32.GetClipboardSequenceNumber())
        except Exception:
            return 0

    def _get_windows_clipboard_snapshot(self):
        """Read clipboard content directly via Win32 for better IDE compatibility."""
        if not _IS_WINDOWS or not _user32:
            return None

        if not _user32.OpenClipboard(None):
            return None

        try:
            file_snapshot = self._read_windows_files()
            if file_snapshot:
                return {"type": "files", "content": file_snapshot}

            html_snapshot = self._read_windows_html()
            if html_snapshot:
                return html_snapshot

            text_snapshot = self._read_windows_unicode_text()
            if text_snapshot:
                return {"type": "text", "content": text_snapshot}
        except Exception as e:
            logger.debug(f"Failed to read Windows clipboard directly: {e}")
        finally:
            _user32.CloseClipboard()

        return None

    def _read_windows_unicode_text(self):
        if not _user32.IsClipboardFormatAvailable(_CF_UNICODETEXT):
            return None

        handle = _user32.GetClipboardData(_CF_UNICODETEXT)
        if not handle:
            return None

        locked = _kernel32.GlobalLock(handle)
        if not locked:
            return None

        try:
            text = ctypes.wstring_at(locked)
            return text or None
        finally:
            _kernel32.GlobalUnlock(handle)

    def _read_windows_html(self):
        if not _CF_HTML or not _user32.IsClipboardFormatAvailable(_CF_HTML):
            return None

        handle = _user32.GetClipboardData(_CF_HTML)
        if not handle:
            return None

        locked = _kernel32.GlobalLock(handle)
        if not locked:
            return None

        try:
            size = _kernel32.GlobalSize(handle)
            raw = ctypes.string_at(locked, size)
            html = raw.decode("utf-8", errors="ignore").rstrip("\x00")
            if not html:
                return None

            return {
                "type": "html",
                "content": html,
                "text": self._extract_text_from_html_clipboard(html),
            }
        finally:
            _kernel32.GlobalUnlock(handle)

    def _read_windows_files(self):
        if not _user32.IsClipboardFormatAvailable(_CF_HDROP):
            return None

        handle = _user32.GetClipboardData(_CF_HDROP)
        if not handle:
            return None

        count = _shell32.DragQueryFileW(handle, 0xFFFFFFFF, None, 0)
        if not count:
            return None

        files = []
        for index in range(count):
            length = _shell32.DragQueryFileW(handle, index, None, 0)
            buffer = ctypes.create_unicode_buffer(length + 1)
            _shell32.DragQueryFileW(handle, index, buffer, length + 1)
            files.append(buffer.value)
        return files or None

    def _extract_text_from_html_clipboard(self, html):
        start_marker = "StartFragment:"
        end_marker = "EndFragment:"
        if start_marker in html and end_marker in html:
            try:
                start = int(html.split(start_marker, 1)[1][:10])
                end = int(html.split(end_marker, 1)[1][:10])
                fragment = html[start:end]
                cleaned = fragment.replace("\r", "").replace("\n", " ").strip()
                return cleaned or html
            except Exception:
                return html
        return html

    def _handle_image_clipboard(self, metadata=None):
        """Store clipboard image as base64."""
        try:
            image = self.clipboard.image()
            if image.isNull():
                return False

            buffer_image = QApplication.clipboard().pixmap().toImage()
            scaled_image = buffer_image.scaled(
                800,
                600,
                Qt.AspectRatioMode.KeepAspectRatio,
                Qt.TransformationMode.SmoothTransformation,
            )

            byte_array = QByteArray()
            buffer_stream = QBuffer(byte_array)
            buffer_stream.open(QBuffer.OpenModeFlag.WriteOnly)
            scaled_image.save(buffer_stream, "JPEG", 85)

            base64_data = base64.b64encode(byte_array.data()).decode("utf-8")

            item_metadata = {
                "width": image.width(),
                "height": image.height(),
                "format": "jpeg",
            }
            if metadata:
                item_metadata.update(metadata)

            return self.service.add_item(
                content=f"data:image/jpeg;base64,{base64_data}",
                content_type=ContentType.IMAGE,
                metadata=item_metadata,
            )
        except Exception as e:
            logger.error(f"Error handling image clipboard content: {e}")
            return False

    def _handle_file_clipboard(self, urls, metadata=None):
        """Store copied local file paths."""
        try:
            file_paths = [url.toLocalFile() for url in urls if url.isLocalFile()]
            if not file_paths:
                return False

            item_metadata = {"files": file_paths, "count": len(file_paths)}
            if metadata:
                item_metadata.update(metadata)

            return self.service.add_item(
                content="\n".join(file_paths),
                content_type=ContentType.FILE,
                metadata=item_metadata,
            )
        except Exception as e:
            logger.error(f"Error handling file clipboard content: {e}")
            return False

    def copy_text(self, text: str) -> None:
        try:
            self.clipboard.setText(text)
            self._last_clipboard_data = self._build_clipboard_signature()
            self._last_clipboard_sequence = self._get_clipboard_sequence_number()
        except Exception as e:
            logger.error(f"Error copying text: {e}")

    def copy_item(self, item: ClipboardItem) -> None:
        try:
            if item.content_type == ContentType.IMAGE:
                try:
                    if item.content.startswith("data:image"):
                        base64_data = item.content.split(",", 1)[1]
                        image_data = base64.b64decode(base64_data)
                        image = QImage()
                        image.loadFromData(image_data)
                        self.clipboard.setImage(image)
                    else:
                        self.clipboard.setText(item.content)
                except Exception as e:
                    logger.error(f"Error copying image: {e}")
                    self.clipboard.setText(item.content)
            elif item.content_type == ContentType.FILE:
                from PyQt6.QtCore import QMimeData

                mime_data = QMimeData()
                urls = [QUrl.fromLocalFile(f) for f in item.metadata.get("files", [])]
                mime_data.setUrls(urls)
                self.clipboard.setMimeData(mime_data)
            else:
                self.clipboard.setText(item.content)

            self._last_clipboard_data = self._build_clipboard_signature()
            self._last_clipboard_sequence = self._get_clipboard_sequence_number()
        except Exception as e:
            logger.error(f"Error copying item: {e}")

    def clear_history(self, keep_favorites: bool = True) -> None:
        try:
            self.service.clear_history(keep_favorites)
        except Exception as e:
            logger.error(f"Error clearing history: {e}")

    def delete_item(self, content_hash: str) -> None:
        try:
            self.service.delete_item(content_hash)
        except Exception as e:
            logger.error(f"Error deleting item: {e}")

    def toggle_favorite(self, content_hash: str) -> None:
        try:
            self.service.toggle_favorite(content_hash)
        except Exception as e:
            logger.error(f"Error toggling favorite: {e}")

    def get_history(self, limit: int = 100, offset: int = 0, search_text: str = None, favorites_only: bool = False):
        return self.service.get_history(limit, offset, search_text, favorites_only)

    def get_count(self) -> int:
        return self.service.get_count()

    def export_history(self, file_path: str) -> bool:
        try:
            return self.service.export_history(file_path)
        except Exception as e:
            logger.error(f"Error exporting history: {e}")
            return False

    def import_history(self, file_path: str) -> bool:
        try:
            return self.service.import_history(file_path)
        except Exception as e:
            logger.error(f"Error importing history: {e}")
            return False

    def update_settings(self, max_history: int = None, retention_days: int = None):
        if max_history is not None:
            self.service.set_max_history(max_history)
        if retention_days is not None:
            self.service.set_retention_days(retention_days)
