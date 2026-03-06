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

    @staticmethod
    def get_active_window_info() -> SourceInfo:
        """Get information about the currently active window."""
        info = SourceInfo(type="application")

        if platform.system() != "Windows":
            return info

        try:
            import ctypes
            import ctypes.wintypes as wt

            # Get foreground window
            hwnd = ctypes.windll.user32.GetForegroundWindow()
            if not hwnd:
                return info

            # Get window title
            length = ctypes.windll.user32.GetWindowTextLengthW(hwnd)
            if length > 0:
                buffer = ctypes.create_unicode_buffer(length + 1)
                ctypes.windll.user32.GetWindowTextW(hwnd, buffer, length + 1)
                info.title = buffer.value

            # Get process ID
            pid = wt.DWORD()
            ctypes.windll.user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))

            # Get process name
            try:
                import psutil
                process = psutil.Process(pid.value)
                info.app_name = process.name()
                # Try to get the executable path for better identification
                exe_path = process.exe()
                if exe_path:
                    # Map common executables to friendly names
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
                        info.app_name = app_mapping[exe_name]
                    elif not info.app_name:
                        info.app_name = exe_name.replace('.exe', '').title()
            except Exception as e:
                logger.debug(f"Could not get process info: {e}")

            # Detect browser and extract URL from title
            if info.app_name in ['Google Chrome', 'Microsoft Edge', 'Brave Browser', 'Opera']:
                # Try to extract URL from window title pattern
                # Common patterns:
                # "Page Title - Website Name"
                # "Page Title - Website Name - Browser Name"
                title_parts = info.title.split(' - ')
                if len(title_parts) >= 2:
                    # Last part might be browser name
                    browser_names = ['Google Chrome', 'Microsoft Edge', 'Brave', 'Opera']
                    if title_parts[-1] in browser_names:
                        site_name = title_parts[-2]
                    else:
                        site_name = title_parts[-1]
                    info.domain = site_name

        except Exception as e:
            logger.error(f"Error getting active window info: {e}")

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
