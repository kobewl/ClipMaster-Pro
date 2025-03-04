class MainStyle:
    """主要样式定义 - 亮色主题"""
    
    MAIN_WIDGET = """
        #mainWidget {
            background-color: #ffffff;
            border: 1px solid #e0e0e0;
            border-radius: 12px;
        }
    """
    
    TITLE_BAR = """
        #titleBar {
            background-color: #f8f9fa;
            border-top-left-radius: 12px;
            border-top-right-radius: 12px;
            padding: 8px;
        }
    """
    
    TITLE_LABEL = """
        #titleLabel {
            font-family: 'Segoe UI', Arial, sans-serif;
            font-size: 16px;
            font-weight: bold;
            color: #1a73e8;
            padding: 8px;
        }
    """
    
    SEARCH_CONTAINER = """
        #searchContainer {
            background-color: #f8f9fa;
            border: 1px solid #e0e0e0;
            border-radius: 24px;
            padding: 6px 16px;
            margin: 8px;
        }
        
        #searchContainer QLineEdit {
            border: none;
            background: transparent;
            padding: 4px 8px;
            font-size: 14px;
            color: #202124;
            min-width: 240px;
        }
        
        #searchContainer QLineEdit:focus {
            outline: none;
        }
        
        #searchContainer QLineEdit::placeholder {
            color: #5f6368;
        }
    """
    
    TOP_BUTTON = """
        #topButton {
            border-radius: 20px;
            font-size: 16px;
            padding: 8px;
            min-width: 40px;
            min-height: 40px;
            background-color: #f8f9fa;
            border: 1px solid #e0e0e0;
            color: #5f6368;
        }
        
        #topButton:checked {
            background-color: #e8f0fe;
            color: #1a73e8;
            border-color: #1a73e8;
        }
        
        #topButton:hover {
            background-color: #f1f3f4;
        }
    """
    
    HISTORY_LIST = """
        #historyList {
            border: none;
            background-color: transparent;
            padding: 8px;
        }
        
        #historyList::item {
            padding: 12px 16px;
            border-radius: 8px;
            margin: 4px 0;
            font-size: 14px;
            color: #202124;
            background-color: #f8f9fa;
            border: 1px solid transparent;
        }
        
        #historyList::item:hover {
            background-color: #f1f3f4;
            border-color: #e0e0e0;
        }
        
        #historyList::item:selected {
            background-color: #e8f0fe;
            color: #1a73e8;
            border-color: #1a73e8;
        }
    """
    
    SEPARATOR = """
        #separator {
            background-color: #e0e0e0;
            height: 1px;
            margin: 8px 16px;
        }
    """
    
    CLOSE_BUTTON = """
        #closeButton {
            border: none;
            border-radius: 16px;
            font-size: 20px;
            font-weight: bold;
            color: #5f6368;
            background-color: transparent;
        }
        
        #closeButton:hover {
            background-color: #fce8e6;
            color: #d93025;
        }
        
        #closeButton:pressed {
            background-color: #fad2cf;
            color: #d93025;
        }
    """
    
    THEME_BUTTON = """
        #themeButton {
            border-radius: 20px;
            font-size: 16px;
            padding: 8px;
            min-width: 40px;
            min-height: 40px;
            background-color: #f8f9fa;
            border: 1px solid #e0e0e0;
            color: #5f6368;
        }
        
        #themeButton:hover {
            background-color: #f1f3f4;
        }
    """
    
    SETTINGS_BUTTON = """
        #settingsButton {
            border-radius: 20px;
            font-size: 16px;
            padding: 8px;
            min-width: 40px;
            min-height: 40px;
            background-color: #f8f9fa;
            border: 1px solid #e0e0e0;
            color: #5f6368;
        }
        
        #settingsButton:hover {
            background-color: #f1f3f4;
        }
    """
    
    @classmethod
    def get_all_styles(cls) -> str:
        """获取所有样式"""
        return "\n".join([
            cls.MAIN_WIDGET,
            cls.TITLE_BAR,
            cls.TITLE_LABEL,
            cls.SEARCH_CONTAINER,
            cls.TOP_BUTTON,
            cls.HISTORY_LIST,
            cls.SEPARATOR,
            cls.CLOSE_BUTTON,
            cls.THEME_BUTTON,
            cls.SETTINGS_BUTTON
        ])


class DarkStyle:
    """暗黑模式样式定义"""
    
    MAIN_WIDGET = """
        #mainWidget {
            background-color: #202124;
            border: 1px solid #3c4043;
            border-radius: 12px;
        }
    """
    
    TITLE_BAR = """
        #titleBar {
            background-color: #2d2e30;
            border-top-left-radius: 12px;
            border-top-right-radius: 12px;
            padding: 8px;
        }
    """
    
    TITLE_LABEL = """
        #titleLabel {
            font-family: 'Segoe UI', Arial, sans-serif;
            font-size: 16px;
            font-weight: bold;
            color: #8ab4f8;
            padding: 8px;
        }
    """
    
    SEARCH_CONTAINER = """
        #searchContainer {
            background-color: #303134;
            border: 1px solid #3c4043;
            border-radius: 24px;
            padding: 6px 16px;
            margin: 8px;
        }
        
        #searchContainer QLineEdit {
            border: none;
            background: transparent;
            padding: 4px 8px;
            font-size: 14px;
            color: #e8eaed;
            min-width: 240px;
        }
        
        #searchContainer QLineEdit:focus {
            outline: none;
        }
        
        #searchContainer QLineEdit::placeholder {
            color: #9aa0a6;
        }
    """
    
    TOP_BUTTON = """
        #topButton {
            border-radius: 20px;
            font-size: 16px;
            padding: 8px;
            min-width: 40px;
            min-height: 40px;
            background-color: #303134;
            border: 1px solid #3c4043;
            color: #9aa0a6;
        }
        
        #topButton:checked {
            background-color: #174ea6;
            color: #e8eaed;
            border-color: #8ab4f8;
        }
        
        #topButton:hover {
            background-color: #3c4043;
        }
    """
    
    HISTORY_LIST = """
        #historyList {
            border: none;
            background-color: transparent;
            padding: 8px;
        }
        
        #historyList::item {
            padding: 12px 16px;
            border-radius: 8px;
            margin: 4px 0;
            font-size: 14px;
            color: #e8eaed;
            background-color: #303134;
            border: 1px solid transparent;
        }
        
        #historyList::item:hover {
            background-color: #3c4043;
            border-color: #5f6368;
        }
        
        #historyList::item:selected {
            background-color: #174ea6;
            color: #e8eaed;
            border-color: #8ab4f8;
        }
    """
    
    SEPARATOR = """
        #separator {
            background-color: #3c4043;
            height: 1px;
            margin: 8px 16px;
        }
    """
    
    CLOSE_BUTTON = """
        #closeButton {
            border: none;
            border-radius: 16px;
            font-size: 20px;
            font-weight: bold;
            color: #9aa0a6;
            background-color: transparent;
        }
        
        #closeButton:hover {
            background-color: #5c2b29;
            color: #f28b82;
        }
        
        #closeButton:pressed {
            background-color: #501d1a;
            color: #f28b82;
        }
    """
    
    THEME_BUTTON = """
        #themeButton {
            border-radius: 20px;
            font-size: 16px;
            padding: 8px;
            min-width: 40px;
            min-height: 40px;
            background-color: #303134;
            border: 1px solid #3c4043;
            color: #9aa0a6;
        }
        
        #themeButton:hover {
            background-color: #3c4043;
        }
    """
    
    SETTINGS_BUTTON = """
        #settingsButton {
            border-radius: 20px;
            font-size: 16px;
            padding: 8px;
            min-width: 40px;
            min-height: 40px;
            background-color: #303134;
            border: 1px solid #3c4043;
            color: #9aa0a6;
        }
        
        #settingsButton:hover {
            background-color: #3c4043;
        }
    """
    
    @classmethod
    def get_all_styles(cls) -> str:
        """获取所有样式"""
        return "\n".join([
            cls.MAIN_WIDGET,
            cls.TITLE_BAR,
            cls.TITLE_LABEL,
            cls.SEARCH_CONTAINER,
            cls.TOP_BUTTON,
            cls.HISTORY_LIST,
            cls.SEPARATOR,
            cls.CLOSE_BUTTON,
            cls.THEME_BUTTON,
            cls.SETTINGS_BUTTON
        ])


class StyleManager:
    """样式管理器"""
    
    @staticmethod
    def get_style(is_dark_mode: bool = False) -> str:
        """获取当前主题的样式"""
        return DarkStyle.get_all_styles() if is_dark_mode else MainStyle.get_all_styles() 