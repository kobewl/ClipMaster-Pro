"""
Source Tracker - Track the origin of clipboard content.

Extracts source URL from HTML clipboard data and
retrieves window information for copied content.
"""

import platform
import re
from urllib.parse import urlparse
from typing import Optional, Dict
from dataclasses import dataclass
from utils.logger import logger


@dataclass
class SourceInfo:
    """Source information for clipboard content."""
    url: str = ""  # Source URL (if from web)
    title: str = ""  # Window title or page title
    app_name: str = ""  # Application name
    domain: str = ""  # Domain name (extracted from URL)
    type: str = "unknown"  # "web", "application", "file", "unknown"

    def is_web(self) -> bool:
        return self.type == "web" and bool(self.url)

    def display_name(self) -> str:
        """Get a human-readable source name."""
        if self.is_web():
            return self.domain or self.title or self.url[:30]
        elif self.app_name:
            return self.app_name
        elif self.title:
            return self.title
        return "未知来源"

    def to_dict(self) -> Dict:
        return {
            'url': self.url,
            'title': self.title,
            'app_name': self.app_name,
            'domain': self.domain,
            'type': self.type
        }

    @classmethod
    def from_dict(cls, data: Dict) -> 'SourceInfo':
        return cls(
            url=data.get('url', ''),
            title=data.get('title', ''),
            app_name=data.get('app_name', ''),
            domain=data.get('domain', ''),
            type=data.get('type', 'unknown')
        )


class SourceTracker:
    """Track and extract source information for clipboard content."""

    @staticmethod
    def extract_from_html(html: str) -> SourceInfo:
        """Extract source URL from HTML clipboard data."""
        info = SourceInfo(type="web")

        if not html:
            return info

        # Try to find SourceURL comment (Windows/Chrome format)
        # Format: <!--Source: https://example.com/page-->
        source_match = re.search(r'<!--\s*[Ss]ource:\s*(https?://[^\s>]+)\s*-->', html)
        if source_match:
            info.url = source_match.group(1)
            info.domain = SourceTracker._extract_domain(info.url)
            return info

        # Try to find base tag
        base_match = re.search(r'<base\s+[^>]*href=["\'](https?://[^"\']+)["\']', html, re.IGNORECASE)
        if base_match:
            info.url = base_match.group(1)
            info.domain = SourceTracker._extract_domain(info.url)
            return info

        # Try to find any link with href
        link_match = re.search(r'<a\s+[^>]*href=["\'](https?://[^"\']+)["\']', html, re.IGNORECASE)
        if link_match:
            info.url = link_match.group(1)
            info.domain = SourceTracker._extract_domain(info.url)
            return info

        return info

    # ── macOS helpers ─────────────────────────────────────────────────

    @staticmethod
    def _get_mac_frontmost_app() -> tuple[str, str]:
        """Return (app_name, bundle_id) for the frontmost app on macOS.
        Uses NSWorkspace — does NOT require Accessibility permission."""
        try:
            from AppKit import NSWorkspace
            app = NSWorkspace.sharedWorkspace().frontmostApplication()
            name = app.localizedName()
            bundle = app.bundleIdentifier()
            return name or "", bundle or ""
        except Exception:
            return "", ""

    @staticmethod
    def _get_mac_window_title() -> str:
        """Try to get the frontmost window title via Accessibility.
        May fail if permission is not granted — returns empty string."""
        try:
            from ApplicationServices import (
                AXUIElementCopyAttributeValue,
                AXUIElementCreateSystemWide,
                kAXFocusedApplicationAttribute,
                kAXTitleAttribute,
            )
            system = AXUIElementCreateSystemWide()
            err, app = AXUIElementCopyAttributeValue(
                system, kAXFocusedApplicationAttribute, None
            )
            if err != 0 or app is None:
                return ""
            err, title = AXUIElementCopyAttributeValue(
                app, kAXTitleAttribute, None
            )
            if err == 0 and title:
                return str(title)
        except Exception:
            pass
        return ""

    @staticmethod
    def _get_linux_active_window() -> tuple[str, str]:
        """Return (app_name, title) for the active window on Linux (X11/Wayland)."""
        try:
            # Try xdotool first (most common)
            import subprocess
            result = subprocess.run(
                ["xdotool", "getactivewindow", "getwindowname"],
                capture_output=True, text=True, timeout=2
            )
            if result.returncode == 0:
                title = result.stdout.strip()
                # Also try to get the WM_CLASS (app name)
                cls_result = subprocess.run(
                    ["xdotool", "getactivewindow", "getwindowclassname"],
                    capture_output=True, text=True, timeout=2
                )
                app_name = cls_result.stdout.strip() if cls_result.returncode == 0 else ""
                return app_name, title
        except Exception:
            pass

        try:
            # Fallback: try reading from _NET_ACTIVE_WINDOW via xprop
            import subprocess
            result = subprocess.run(
                ["xprop", "-root", "_NET_ACTIVE_WINDOW"],
                capture_output=True, text=True, timeout=2
            )
            if result.returncode == 0:
                line = result.stdout.strip()
                if "0x" in line:
                    wid = line.split("0x")[-1].strip()
                    wid_hex = "0x" + wid
                    wm_result = subprocess.run(
                        ["xprop", "-id", wid_hex, "WM_CLASS"],
                        capture_output=True, text=True, timeout=2
                    )
                    if wm_result.returncode == 0:
                        # WM_CLASS output format: WM_CLASS(STRING) = "class", "Class"
                        raw = wm_result.stdout.strip()
                        if '"' in raw:
                            parts = raw.split('"')
                            app_name = parts[1] if len(parts) > 1 else ""
                            return app_name, ""
        except Exception:
            pass

        return "", ""

    # ── Windows helpers ───────────────────────────────────────────────

    @staticmethod
    def _get_win_active_window() -> tuple[str, str, str]:
        """Return (app_name, title, domain_hint) for the active window on Windows."""
        try:
            import ctypes
            import ctypes.wintypes as wt

            hwnd = ctypes.windll.user32.GetForegroundWindow()
            if not hwnd:
                return "", "", ""

            title = ""
            length = ctypes.windll.user32.GetWindowTextLengthW(hwnd)
            if length > 0:
                buffer = ctypes.create_unicode_buffer(length + 1)
                ctypes.windll.user32.GetWindowTextW(hwnd, buffer, length + 1)
                title = buffer.value

            app_name = ""
            pid = wt.DWORD()
            ctypes.windll.user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))

            try:
                import psutil
                process = psutil.Process(pid.value)
                app_name = process.name()
                exe_path = process.exe()
                if exe_path:
                    app_mapping = {
                        'chrome.exe': 'Google Chrome',
                        'firefox.exe': 'Mozilla Firefox',
                        'msedge.exe': 'Microsoft Edge',
                        'opera.exe': 'Opera',
                        'brave.exe': 'Brave Browser',
                        'code.exe': 'Visual Studio Code',
                        'notepad.exe': 'Notepad',
                        'notepad++.exe': 'Notepad++',
                        'sublime_text.exe': 'Sublime Text',
                        'pycharm64.exe': 'PyCharm',
                        'idea64.exe': 'IntelliJ IDEA',
                        'devenv.exe': 'Visual Studio',
                        'winword.exe': 'Microsoft Word',
                        'excel.exe': 'Microsoft Excel',
                        'powerpnt.exe': 'Microsoft PowerPoint',
                        'outlook.exe': 'Microsoft Outlook',
                        'teams.exe': 'Microsoft Teams',
                        'slack.exe': 'Slack',
                        'discord.exe': 'Discord',
                        'telegram.exe': 'Telegram',
                        'wechat.exe': 'WeChat',
                        'qq.exe': 'QQ',
                        'dingtalk.exe': 'DingTalk',
                        'feishu.exe': 'Feishu',
                    }
                    exe_name = exe_path.split('\\')[-1].lower()
                    if exe_name in app_mapping:
                        app_name = app_mapping[exe_name]
                    else:
                        app_name = exe_name.replace('.exe', '').title()
            except Exception as e:
                logger.debug(f"Could not get process info: {e}")

            # Detect browser domain from title
            domain = ""
            if app_name in ['Google Chrome', 'Microsoft Edge', 'Brave Browser', 'Opera']:
                title_parts = title.split(' - ')
                if len(title_parts) >= 2:
                    browser_names = ['Google Chrome', 'Microsoft Edge', 'Brave', 'Opera']
                    if title_parts[-1] in browser_names:
                        domain = title_parts[-2]
                    else:
                        domain = title_parts[-1]

            return app_name, title, domain
        except Exception as e:
            logger.error(f"Error getting active window info: {e}")
            return "", "", ""

    @staticmethod
    def get_active_window_info() -> SourceInfo:
        """Get information about the currently active window."""
        info = SourceInfo(type="application")
        system = platform.system()

        if system == "Windows":
            app_name, title, domain = SourceTracker._get_win_active_window()
            info.app_name = app_name
            info.title = title
            info.domain = domain

        elif system == "Darwin":
            app_name, bundle_id = SourceTracker._get_mac_frontmost_app()
            info.app_name = app_name
            # Try to get window title via Accessibility (may fail gracefully)
            info.title = SourceTracker._get_mac_window_title()
            if not info.app_name and bundle_id:
                # Fallback: derive name from bundle ID
                info.app_name = bundle_id.split('.')[-1].replace('-', ' ').title()

        elif system == "Linux":
            app_name, title = SourceTracker._get_linux_active_window()
            info.app_name = app_name
            info.title = title

        return info

    @staticmethod
    def _extract_domain(url: str) -> str:
        """Extract domain name from URL."""
        try:
            parsed = urlparse(url)
            domain = parsed.netloc
            # Remove www. prefix
            if domain.startswith('www.'):
                domain = domain[4:]
            return domain
        except Exception:
            return ""

    @classmethod
    def track_source(cls, html: str = None) -> SourceInfo:
        """Track source from clipboard content and active window."""
        # First try to get from HTML
        if html:
            info = cls.extract_from_html(html)
            if info.is_web():
                # Also get window info for the title
                window_info = cls.get_active_window_info()
                if window_info.title:
                    info.title = window_info.title
                if not info.app_name and window_info.app_name:
                    info.app_name = window_info.app_name
                return info

        # Fall back to window info
        return cls.get_active_window_info()
